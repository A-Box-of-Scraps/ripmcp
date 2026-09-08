use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub enum Reply {
    Bytes(Vec<u8>),
    Close,
    Stall,
    Disconnect,
}

pub struct Http {
    pub address: SocketAddr,
    pub requests: Receiver<Vec<u8>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}

impl Http {
    pub fn new(replies: Vec<Reply>) -> Self {
        let listener: TcpListener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address: SocketAddr = listener.local_addr().unwrap();
        let stop: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let stopping: Arc<AtomicBool> = Arc::clone(&stop);
        let (sender, requests): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();
        let worker: JoinHandle<io::Result<()>> =
            thread::spawn(move || serve(listener, replies, sender, &stopping));
        Self {
            address,
            requests,
            stop,
            worker: Some(worker),
        }
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.worker
            .take()
            .unwrap()
            .join()
            .expect("HTTP fixture panicked")
    }
}

impl Drop for Http {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _: thread::Result<io::Result<()>> = worker.join();
        }
    }
}

fn serve(
    listener: TcpListener,
    replies: Vec<Reply>,
    sender: Sender<Vec<u8>>,
    stop: &AtomicBool,
) -> io::Result<()> {
    for reply in replies {
        let Some(mut stream): Option<TcpStream> = accept(&listener, stop)? else {
            return Ok(());
        };
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        stream.set_write_timeout(Some(Duration::from_millis(200)))?;
        let request: Vec<u8> = request(&mut stream)?;
        sender
            .send(request)
            .map_err(|_| io::Error::other("request receiver closed"))?;
        match reply {
            Reply::Bytes(bytes) => stream.write_all(&bytes)?,
            Reply::Close => {}
            Reply::Disconnect => disconnect(&mut stream, &sender)?,
            Reply::Stall => {
                let started: Instant = Instant::now();
                while !stop.load(Ordering::Relaxed) && started.elapsed() < Duration::from_secs(2) {
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
    Ok(())
}

fn disconnect(stream: &mut TcpStream, sender: &Sender<Vec<u8>>) -> io::Result<()> {
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n: waiting\n\n")?;
    let mut byte: [u8; 1] = [0];
    match stream.read(&mut byte) {
        Ok(0) => (),
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
            ) => {}
        Err(error) => return Err(error),
        _ => return Err(io::Error::other("expected stream cancellation")),
    }
    sender
        .send(b"disconnected".to_vec())
        .map_err(|_| io::Error::other("request receiver closed"))
}

fn accept(listener: &TcpListener, stop: &AtomicBool) -> io::Result<Option<TcpStream>> {
    let started: Instant = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => return Ok(Some(stream)),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error),
        }
        if started.elapsed() > Duration::from_secs(2) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "no fixture request",
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(None)
}

fn request(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut chunk: [u8; 1024] = [0; 1024];
    let started: Instant = Instant::now();
    while bytes.len() < 65536 && started.elapsed() < Duration::from_secs(2) {
        let count = stream.read(&mut chunk)?;
        if count == 0 {
            return Err(io::Error::other("incomplete fixture request"));
        }
        bytes.extend_from_slice(&chunk[..count]);
        if complete(&bytes)? {
            return Ok(bytes);
        }
    }
    Err(io::Error::other("fixture request limit exceeded"))
}

fn complete(bytes: &[u8]) -> io::Result<bool> {
    let Some(end): Option<usize> = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
        return Ok(false);
    };
    let headers: &str = std::str::from_utf8(&bytes[..end]).map_err(io::Error::other)?;
    let mut length: usize = 0;
    for line in headers.lines() {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().map_err(io::Error::other)?;
        }
    }
    if length > 65536 {
        return Err(io::Error::other("fixture body limit exceeded"));
    }
    Ok(bytes.len() >= end + 4 + length)
}

pub fn json(status: &str, body: &str) -> Reply {
    Reply::Bytes(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes())
}
