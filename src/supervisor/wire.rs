use super::{incompatible, unavailable};
use crate::error::Error;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) const VERSION: u32 = 1;
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
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Reply {
    Ready,
    Pong,
    Stopping,
    Incompatible,
}

pub(super) async fn read<T: DeserializeOwned>(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<T, Error> {
    let size = stream.read_u32().await.map_err(|_| unavailable())? as usize;
    if size == 0 || size > FRAME_LIMIT {
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
    let bytes: Vec<u8> = serde_json::to_vec(value).map_err(|_| incompatible())?;
    if bytes.is_empty() || bytes.len() > FRAME_LIMIT {
        return Err(incompatible());
    }
    stream
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|_| unavailable())?;
    stream.write_all(&bytes).await.map_err(|_| unavailable())
}
