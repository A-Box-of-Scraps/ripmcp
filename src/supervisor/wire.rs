use super::{incompatible, unavailable};
use crate::error::Error;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) const VERSION: u32 = 3;
pub(super) const FRAME_LIMIT: usize = 4096;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Hello {
    pub protocol: u32,
    pub build: String,
    pub nonce: String,
    pub context: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Ping,
    Shutdown,
    Local {
        request: Box<super::LocalRequest>,
        budget: std::time::Duration,
        sent: std::time::Duration,
    },
    Status {
        cwd: std::path::PathBuf,
    },
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Reply {
    Ready,
    Pong,
    Stopping,
    Incompatible,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Response {
    Success(String),
    Failure {
        kind: crate::error::ErrorKind,
        message: String,
    },
}

pub(super) fn now() -> std::time::Duration {
    let time: rustix::time::Timespec =
        rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    std::time::Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

pub(super) async fn read<T: DeserializeOwned>(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<T, Error> {
    read_limit(stream, FRAME_LIMIT).await
}

pub(super) async fn read_limit<T: DeserializeOwned>(
    stream: &mut (impl AsyncRead + Unpin),
    limit: usize,
) -> Result<T, Error> {
    let size = stream.read_u32().await.map_err(|_| unavailable())? as usize;
    if size == 0 || size > limit {
        return Err(incompatible());
    }
    let mut bytes: Vec<u8> = vec![0; size];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| unavailable())?;
    serde_json::from_slice(&bytes).map_err(|_| incompatible())
}

pub(super) async fn write<T: Serialize>(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &T,
) -> Result<(), Error> {
    write_limit(stream, value, FRAME_LIMIT).await
}

pub(super) async fn write_limit<T: Serialize>(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &T,
    limit: usize,
) -> Result<(), Error> {
    let bytes: Vec<u8> = serde_json::to_vec(value).map_err(|_| incompatible())?;
    if bytes.is_empty() || bytes.len() > limit {
        return Err(incompatible());
    }
    stream
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|_| unavailable())?;
    stream.write_all(&bytes).await.map_err(|_| unavailable())
}

pub(super) const DATA_LIMIT: usize = 64 * 1024 * 1024;
