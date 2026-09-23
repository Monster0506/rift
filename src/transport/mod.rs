use std::io::{BufRead, Write};

use serde::Serialize;

/// Reject a framed body larger than this instead of allocating on an
/// attacker/bug-controlled `Content-Length` from an untrusted peer.
pub const MAX_FRAME_LEN: usize = 256 * 1024 * 1024;

/// Serialize `msg` as JSON and return it as one `Content-Length: N\r\n\r\n<body>`
/// buffer, ready to hand to a writer (or a channel to one on another thread).
pub fn frame_message<T: Serialize>(msg: &T) -> std::io::Result<Vec<u8>> {
    let body = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut framed = Vec::with_capacity(header.len() + body.len());
    framed.extend_from_slice(header.as_bytes());
    framed.extend_from_slice(&body);
    Ok(framed)
}

/// Serialize `msg` as JSON, write `Content-Length: N\r\n\r\n` then the body, then flush.
pub fn write_framed<W: Write, T: Serialize>(writer: &mut W, msg: &T) -> std::io::Result<()> {
    let framed = frame_message(msg)?;
    writer.write_all(&framed)?;
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
#[path = "tests.rs"]
mod tests;
