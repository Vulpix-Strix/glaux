//! `Content-Length` framing of JSON-RPC messages.

use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest body a frame may announce. A server that claims more is
/// treated as broken instead of being allowed to exhaust memory.
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// Why a frame could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("the headers have no Content-Length")]
    MissingLength,
    #[error("malformed header `{0}`")]
    BadHeader(String),
    #[error("a frame of {0} bytes exceeds the {MAX_FRAME_BYTES} byte limit")]
    TooLarge(usize),
    #[error("the stream ended inside a frame")]
    Truncated,
    #[error("the body is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// Reads the next message, or `None` when the stream ends between frames.
///
/// Headers other than `Content-Length` are accepted and ignored.
pub async fn read_frame<R>(reader: &mut R) -> Result<Option<Value>, FrameError>
where
    R: AsyncBufRead + Unpin,
{
    let mut length = None;
    let mut first = true;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).await?;
        if read == 0 {
            return if first {
                Ok(None)
            } else {
                Err(FrameError::Truncated)
            };
        }
        first = false;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(FrameError::BadHeader(line.to_owned()));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| FrameError::BadHeader(line.to_owned()))?,
            );
        }
    }
    let length = length.ok_or(FrameError::MissingLength)?;
    if length > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge(length));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            FrameError::Truncated
        } else {
            FrameError::Io(error)
        }
    })?;
    Ok(Some(serde_json::from_slice(&body)?))
}

/// Writes one message with its header and flushes it.
pub async fn write_frame<W>(writer: &mut W, message: &Value) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(message)?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests;
