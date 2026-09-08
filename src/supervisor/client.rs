use super::endpoint::{Endpoint, Record};
use super::wire::{self, Hello, Reply, Request};
use super::{incompatible, invalid, unavailable};
use crate::error::Error;
use crate::mcp::Operation;
use crate::storage::Paths;
use std::fs::File;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::net::UnixStream;

pub struct Connection {
    stream: UnixStream,
}

impl Connection {
    pub async fn local(
        mut self,
        request: super::LocalRequest,
        operation: &Operation,
    ) -> Result<serde_json::Value, Error> {
        let request: Request = Request::Local {
            request: Box::new(request),
            budget: operation.remaining()?,
            sent: wire::now(),
        };
        self.data(request, operation).await
    }

    pub async fn status(
        mut self,
        cwd: std::path::PathBuf,
        operation: &Operation,
    ) -> Result<serde_json::Value, Error> {
        self.data(Request::Status { cwd }, operation).await
    }

    async fn data(
        &mut self,
        request: Request,
        operation: &Operation,
    ) -> Result<serde_json::Value, Error> {
        operation
            .run(async {
                wire::write_limit(&mut self.stream, &request, wire::DATA_LIMIT).await?;
                let response: wire::Response =
                    wire::read_limit(&mut self.stream, wire::DATA_LIMIT).await?;
                match response {
                    wire::Response::Success(json) => {
                        crate::json::parse(json.as_bytes()).map_err(|_| incompatible())
                    }
                    wire::Response::Failure { kind, message } => Err(Error {
                        kind,
                        message: std::borrow::Cow::Owned(message),
                    }),
                }
            })
            .await
    }
    pub async fn existing(paths: &Paths, operation: &Operation) -> Result<Option<Self>, Error> {
        operation.remaining()?;
        let Some(endpoint): Option<Endpoint> = Endpoint::open(paths, false)? else {
            return Ok(None);
        };
        operation.run(connect(&endpoint)).await
    }

    pub async fn ensure(
        paths: &Paths,
        executable: &Path,
        operation: &Operation,
    ) -> Result<Self, Error> {
        operation.remaining()?;
        super::guardian::check_support()?;
        let endpoint: Endpoint = Endpoint::open(paths, true)?.ok_or_else(invalid)?;
        let lock: File = endpoint.lock(".bootstrap.lock")?;
        operation
            .run(async {
                acquire(&lock).await?;
                if let Some(connection) = connect(&endpoint).await? {
                    return Ok(connection);
                }
                let mut child: Launch = Launch::spawn(paths, executable)?;
                let connection: Connection = child.ready(&endpoint).await?;
                child.detach();
                Ok(connection)
            })
            .await
    }

    pub async fn ping(mut self, operation: &Operation) -> Result<(), Error> {
        self.exchange(Request::Ping, Reply::Pong, operation).await
    }

    pub async fn shutdown(mut self, operation: &Operation) -> Result<(), Error> {
        self.exchange(Request::Shutdown, Reply::Stopping, operation)
            .await
    }

    async fn exchange(
        &mut self,
        request: Request,
        expected: Reply,
        operation: &Operation,
    ) -> Result<(), Error> {
        operation
            .run(async {
                wire::write(&mut self.stream, &request).await?;
                let reply: Reply = wire::read(&mut self.stream).await?;
                if reply != expected {
                    return Err(incompatible());
                }
                Ok(())
            })
            .await
    }
}

async fn connect(endpoint: &Endpoint) -> Result<Option<Connection>, Error> {
    let record: Option<Record> = endpoint.record()?;
    if endpoint.metadata()?.is_none() {
        return Ok(None);
    }
    let record: Record = record.ok_or_else(invalid)?;
    endpoint.verify(&record)?;
    let mut stream: UnixStream = match UnixStream::connect(endpoint.path()).await {
        Ok(stream) => stream,
        Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => return Ok(None),
        Err(_) => return Err(unavailable()),
    };
    Endpoint::authenticate(&stream, Some(record.pid))?;
    let hello: Hello = Hello {
        protocol: wire::VERSION,
        build: env!("CARGO_PKG_VERSION").to_owned(),
        nonce: record.nonce.as_str().to_owned(),
        context: endpoint.context.clone(),
    };
    wire::write(&mut stream, &hello).await?;
    let reply: Reply = wire::read(&mut stream).await?;
    if reply != Reply::Ready {
        return Err(incompatible());
    }
    Ok(Some(Connection { stream }))
}

pub(super) async fn acquire(file: &File) -> Result<(), Error> {
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Err(std::fs::TryLockError::Error(_)) => return Err(invalid()),
        }
    }
}

struct Launch(Option<Child>);

impl Launch {
    async fn ready(&mut self, endpoint: &Endpoint) -> Result<Connection, Error> {
        loop {
            if self
                .0
                .as_mut()
                .ok_or_else(unavailable)?
                .try_wait()
                .map_err(crate::storage::io_error)?
                .is_some()
            {
                return Err(unavailable());
            }
            if let Some(connection) = connect(endpoint).await? {
                return Ok(connection);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn spawn(paths: &Paths, executable: &Path) -> Result<Self, Error> {
        use std::os::unix::process::CommandExt;
        let mut command: Command = Command::new(executable);
        command
            .arg("__supervisor")
            .current_dir("/")
            .env_clear()
            .envs(paths.environment())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Detach only the supervisor, never the shared MCP child, from CLI signals.
        unsafe {
            command.pre_exec(|| rustix::process::setsid().map(|_| ()).map_err(Into::into));
        }
        command
            .spawn()
            .map(|child| Self(Some(child)))
            .map_err(crate::storage::io_error)
    }

    fn detach(&mut self) {
        if let Some(mut child) = self.0.take() {
            std::thread::spawn(move || {
                let _: std::io::Result<std::process::ExitStatus> = child.wait();
            });
        }
    }
}

impl Drop for Launch {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _: std::io::Result<()> = child.kill();
            let _: std::io::Result<std::process::ExitStatus> = child.wait();
        }
    }
}
