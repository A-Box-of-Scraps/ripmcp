use super::super::{
    Provider,
    callback::{Browser, Callback},
    credentials::Vault,
    invalid,
    network::Network,
    store::{SecureStore, StoreFuture},
};
use crate::{
    deadline::Deadline,
    mcp::{CancellationToken, Operation},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use url::Url;

#[derive(Default)]
pub struct Memory {
    pub data: Mutex<BTreeMap<String, Vec<u8>>>,
    pub writes: AtomicUsize,
    pub fail_write: AtomicUsize,
    pub fail_read: AtomicBool,
}

impl SecureStore for Memory {
    fn read<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<Vec<u8>>> {
        Box::pin(async move {
            if self.fail_read.load(Ordering::SeqCst) {
                return Err(invalid());
            }
            Ok(self.data.lock().unwrap().get(key).cloned())
        })
    }
    fn write<'a>(&'a self, key: &'a str, value: &'a [u8], _: bool) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let number = self.writes.fetch_add(1, Ordering::SeqCst) + 1;
            if number == self.fail_write.load(Ordering::SeqCst) {
                return Err(invalid());
            }
            self.data
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_vec());
            Ok(())
        })
    }
}

pub struct Fixture {
    pub root: TempDir,
    pub endpoint: Url,
    pub settings: Arc<Mutex<Settings>>,
    worker: JoinHandle<()>,
    pub memory: Arc<Memory>,
}

pub struct Settings {
    pub stall_path: Option<String>,
    pub resource: Value,
    pub authorization: Value,
    pub token: Value,
    pub token_status: &'static str,
    pub challenge: Option<String>,
    pub replies: BTreeMap<String, (String, String)>,
    pub requests: Vec<String>,
}

impl Fixture {
    pub async fn new() -> Self {
        let listener: TcpListener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base: String = format!("http://{}", listener.local_addr().unwrap());
        let endpoint: Url = Url::parse(&format!("{base}/mcp")).unwrap();
        let settings: Arc<Mutex<Settings>> = Arc::new(Mutex::new(Settings {
            stall_path: None,
            resource: json!({"resource": endpoint.as_str(), "authorization_servers": [base], "scopes_supported": ["tools:read"]}),
            authorization: json!({"issuer": base, "authorization_endpoint": format!("{base}/authorize"),
                "token_endpoint": format!("{base}/token"), "response_types_supported": ["code"],
                "code_challenge_methods_supported": ["S256"], "client_id_metadata_document_supported": true,
                "authorization_response_iss_parameter_supported": true}),
            token: json!({"access_token": "secret-access", "token_type": "Bearer", "refresh_token": "secret-refresh", "expires_in": 3600}),
            token_status: "200 OK",
            challenge: None,
            replies: BTreeMap::new(),
            requests: Vec::new(),
        }));
        let shared: Arc<Mutex<Settings>> = settings.clone();
        let worker: JoinHandle<()> = tokio::spawn(serve(listener, shared));
        Self {
            root: tempfile::tempdir().unwrap(),
            endpoint,
            settings,
            worker,
            memory: Arc::new(Memory::default()),
        }
    }

    pub fn provider(&self) -> Provider {
        let mut network: Network = Network::new().unwrap();
        network.loopback = true;
        Provider {
            endpoint: self.endpoint.clone(),
            network,
            vault: Vault::isolated(self.memory.clone(), self.root.path().to_path_buf()),
        }
    }

    pub fn issuer(&self) -> String {
        self.settings.lock().unwrap().authorization["issuer"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn count(&self, path: &str) -> usize {
        self.settings
            .lock()
            .unwrap()
            .requests
            .iter()
            .filter(|request| request.split_whitespace().nth(1) == Some(path))
            .count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.worker.abort();
    }
}

impl Settings {
    fn reply(&mut self, request: String) -> String {
        let path: String = request.split_whitespace().nth(1).unwrap().to_owned();
        self.requests.push(request);
        if let Some((status, body)) = self.replies.get(&path) {
            return response(status, body, "");
        }
        match path.as_str() {
            "/mcp" => response(
                "401 Unauthorized",
                "{}",
                &self
                    .challenge
                    .as_ref()
                    .map(|value| format!("WWW-Authenticate: {value}\r\n"))
                    .unwrap_or_default(),
            ),
            "/.well-known/oauth-protected-resource/mcp"
            | "/.well-known/oauth-protected-resource" => {
                response("200 OK", &self.resource.to_string(), "")
            }
            "/.well-known/oauth-authorization-server" | "/.well-known/openid-configuration" => {
                response("200 OK", &self.authorization.to_string(), "")
            }
            "/token" => response(self.token_status, &self.token.to_string(), ""),
            _ => response("404 Not Found", "{}", ""),
        }
    }
}

fn response(status: &str, body: &str, headers: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
        body.len()
    )
}

async fn read(stream: &mut TcpStream) -> String {
    let mut bytes: Vec<u8> = Vec::new();
    loop {
        let mut buffer: [u8; 1024] = [0; 1024];
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&buffer[..count]);
        if complete(&bytes) {
            return String::from_utf8(bytes).unwrap();
        }
        assert!(bytes.len() < 65536);
    }
}

fn complete(bytes: &[u8]) -> bool {
    let Some(end): Option<usize> = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers: &str = std::str::from_utf8(&bytes[..end]).unwrap();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value): (&str, &str) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    bytes.len() >= end + 4 + length
}

pub fn operation() -> Operation {
    Operation::new(
        Deadline::new(Duration::from_secs(5)),
        CancellationToken::new(),
    )
}

pub async fn callback() -> Callback {
    let listener: TcpListener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let redirect: String = format!("http://{}/oauth/callback", listener.local_addr().unwrap());
    Callback::new(listener, redirect).unwrap()
}

pub struct TestBrowser {
    pub issuer: String,
    pub change: BTreeMap<String, String>,
    pub urls: Mutex<Vec<Url>>,
    pub logout: Option<Arc<Provider>>,
}

impl TestBrowser {
    pub fn new(issuer: String) -> Self {
        Self {
            issuer,
            change: BTreeMap::new(),
            urls: Mutex::new(Vec::new()),
            logout: None,
        }
    }
}

impl Browser for TestBrowser {
    fn open<'a>(&'a self, url: &'a Url, operation: &'a Operation) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.urls.lock().unwrap().push(url.clone());
            if let Some(provider) = &self.logout {
                provider.logout(operation).await?;
            }
            let query: BTreeMap<String, String> = url.query_pairs().into_owned().collect();
            let mut redirect: Url = Url::parse(&query["redirect_uri"]).unwrap();
            let mut fields: BTreeMap<String, String> = BTreeMap::from([
                ("state".to_owned(), query["state"].clone()),
                ("code".to_owned(), "secret-code".to_owned()),
                ("iss".to_owned(), self.issuer.clone()),
            ]);
            fields.extend(self.change.clone());
            fields.retain(|_, value| value != "REMOVE");
            redirect.query_pairs_mut().extend_pairs(fields);
            let mut stream: TcpStream = TcpStream::connect(("127.0.0.1", redirect.port().unwrap()))
                .await
                .unwrap();
            let request: String = format!(
                "GET {}?{} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
                redirect.path(),
                redirect.query().unwrap(),
                redirect.port().unwrap()
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            Ok(())
        })
    }
}

async fn serve(listener: TcpListener, shared: Arc<Mutex<Settings>>) {
    while let Ok((mut stream, _)) = listener.accept().await {
        let request: String = read(&mut stream).await;
        let (reply, stall): (String, bool) = {
            let mut settings: std::sync::MutexGuard<'_, Settings> = shared.lock().unwrap();
            let stall = request.split_whitespace().nth(1) == settings.stall_path.as_deref();
            (settings.reply(request), stall)
        };
        if stall {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        let _ = stream.write_all(reply.as_bytes()).await;
    }
}
