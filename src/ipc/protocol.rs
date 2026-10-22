use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Viewport {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InitializeParams {
    pub(crate) token: String,
    pub(crate) client_name: String,
    pub(crate) viewport: Viewport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InputKeyParams {
    pub(crate) key: String,
    pub(crate) seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResizeParams {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RenderUpdateParams {
    pub(crate) screen: String,
    pub(crate) last_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionEndingParams {
    pub(crate) reason: String,
}

pub(crate) const ERR_UNAUTHORIZED: i64 = -32003;

#[must_use]
pub(crate) fn b64_encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[must_use]
pub(crate) fn b64_decode(s: &str) -> Option<Vec<u8>> {
    const fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let s = s.as_bytes();
    if !s.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut i = 0;
    while i < s.len() {
        let v0 = val(s[i])?;
        let v1 = val(s[i + 1])?;
        out.push((v0 << 2) | (v1 >> 4));
        if s[i + 2] != b'=' {
            let v2 = val(s[i + 2])?;
            out.push(((v1 & 0xf) << 4) | (v2 >> 2));
            if s[i + 3] != b'=' {
                let v3 = val(s[i + 3])?;
                out.push(((v2 & 0x3) << 6) | v3);
            }
        }
        i += 4;
    }
    Some(out)
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod protocol_tests;
