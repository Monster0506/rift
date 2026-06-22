use std::io::{BufRead, Write};

use serde::Serialize;

/// Reject a framed body larger than this instead of allocating on an
/// attacker/bug-controlled `Content-Length` from an untrusted peer.
pub const MAX_FRAME_LEN: usize = 256 * 1024 * 1024;

/// Serialize `msg` as JSON, write `Content-Length: N\r\n\r\n` then the body, then flush.
pub fn write_framed<W: Write, T: Serialize>(writer: &mut W, msg: &T) -> std::io::Result<()> {
    let body = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes())?;
    writer.write_all(&body)?;
    writer.flush()
}

/// Read headers until a blank line, parse `Content-Length`, read exactly N bytes.
/// Returns `InvalidData` if no `Content-Length` header is found.
pub fn read_framed<R: BufRead>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "EOF reading headers",
                ))
            }
            Err(e) => return Err(e),
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                content_length = Some(n);
            }
        }
    }

    let n = content_length.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "missing Content-Length header",
        )
    })?;
    if n > MAX_FRAME_LEN {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Content-Length {n} exceeds max frame size {MAX_FRAME_LEN}"),
        ));
    }

    let mut body = vec![0u8; n];
    reader.read_exact(&mut body)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_framed_message() {
        let msg = serde_json::json!({"method": "ping", "params": {"x": 42}});
        let mut buf: Vec<u8> = Vec::new();
        write_framed(&mut buf, &msg).unwrap();
        let mut reader = std::io::BufReader::new(Cursor::new(buf));
        let body = read_framed(&mut reader).unwrap();
        let got: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(got["params"]["x"], 42);
    }

    #[test]
    fn header_format_is_correct() {
        let msg = serde_json::json!({});
        let mut buf: Vec<u8> = Vec::new();
        write_framed(&mut buf, &msg).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("Content-Length: 2\r\n\r\n"));
    }

    #[test]
    fn read_framed_rejects_missing_header() {
        let data = b"no-header\r\n\r\n{}";
        let mut reader = std::io::BufReader::new(Cursor::new(data.as_ref()));
        let err = read_framed(&mut reader).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_framed_rejects_oversized_content_length_without_allocating() {
        // A malicious/buggy peer claims a huge body but sends none; this must
        // error out before `vec![0u8; n]` tries to allocate gigabytes.
        let data = b"Content-Length: 999999999999\r\n\r\n";
        let mut reader = std::io::BufReader::new(Cursor::new(data.as_ref()));
        let err = read_framed(&mut reader).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
