use pretty_assertions::assert_eq;
use serde_json::json;
use tokio::io::{AsyncWriteExt, BufReader, duplex};

use super::*;

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn read_all(bytes: &[u8]) -> Result<Vec<Value>, FrameError> {
    block_on(async {
        let mut reader = BufReader::new(bytes);
        let mut out = Vec::new();
        while let Some(message) = read_frame(&mut reader).await? {
            out.push(message);
        }
        Ok(out)
    })
}

fn framed(body: &str) -> Vec<u8> {
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

#[test]
fn a_written_frame_reads_back() {
    let message =
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"é": "ñ😀"}});
    let read = block_on(async {
        let (mut tx, rx) = duplex(1024);
        write_frame(&mut tx, &message).await.unwrap();
        drop(tx);
        read_frame(&mut BufReader::new(rx)).await.unwrap()
    });
    assert_eq!(read, Some(message));
}

#[test]
fn length_counts_bytes_not_characters() {
    let body = r#"{"t":"ñ😀"}"#;
    assert_ne!(body.len(), body.chars().count());
    assert_eq!(read_all(&framed(body)).unwrap(), vec![json!({"t": "ñ😀"})]);
}

#[test]
fn several_frames_and_extra_headers_are_accepted() {
    let mut bytes =
        b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}"
            .to_vec();
    bytes.extend(framed(r#"{"a":1}"#));
    assert_eq!(read_all(&bytes).unwrap(), vec![json!({}), json!({"a": 1})]);
}

#[test]
fn a_frame_split_into_tiny_chunks_is_reassembled() {
    let bytes = framed(r#"{"method":"x","params":[1,2,3]}"#);
    let read = block_on(async move {
        let (mut tx, rx) = duplex(8);
        let writer = tokio::spawn(async move {
            for chunk in bytes.chunks(3) {
                tx.write_all(chunk).await.unwrap();
                tokio::task::yield_now().await;
            }
        });
        let message = read_frame(&mut BufReader::new(rx)).await.unwrap();
        writer.await.unwrap();
        message
    });
    assert_eq!(read, Some(json!({"method": "x", "params": [1, 2, 3]})));
}

#[test]
fn an_empty_stream_is_a_clean_end() {
    assert_eq!(read_all(b"").unwrap(), Vec::<Value>::new());
}

#[test]
fn truncation_is_an_error_not_an_end() {
    assert!(matches!(
        read_all(b"Content-Length: 10\r\n\r\n{}"),
        Err(FrameError::Truncated)
    ));
    assert!(matches!(
        read_all(b"Content-Length: 10\r\n"),
        Err(FrameError::Truncated)
    ));
}

#[test]
fn broken_headers_are_errors() {
    assert!(matches!(
        read_all(b"X: 1\r\n\r\n{}"),
        Err(FrameError::MissingLength)
    ));
    assert!(matches!(
        read_all(b"Content-Length: abc\r\n\r\n"),
        Err(FrameError::BadHeader(_))
    ));
    assert!(matches!(
        read_all(b"no colon here\r\n\r\n"),
        Err(FrameError::BadHeader(_))
    ));
}

#[test]
fn invalid_json_is_an_error() {
    assert!(matches!(
        read_all(&framed("{not json")),
        Err(FrameError::Json(_))
    ));
}

#[test]
fn an_oversized_frame_is_refused_before_allocating() {
    let header = format!("Content-Length: {}\r\n\r\n", MAX_FRAME_BYTES + 1);
    assert!(matches!(
        read_all(header.as_bytes()),
        Err(FrameError::TooLarge(_))
    ));
}
