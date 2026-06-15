use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Viewport {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    pub token: String,
    pub client_name: String,
    pub viewport: Viewport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputKeyParams {
    pub key: String,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResizeParams {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderUpdateParams {
    pub screen: String,
    pub last_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndingParams {
    pub reason: String,
}

pub const ERR_UNAUTHORIZED: i64 = -32003;

pub fn b64_encode(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
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

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
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
mod tests {
    use super::*;

    #[test]
    fn initialize_params_round_trip() {
        let p = InitializeParams {
            token: "abc".into(),
            client_name: "rift-frontend".into(),
            viewport: Viewport {
                rows: 48,
                cols: 220,
            },
        };
        let j = serde_json::to_string(&p).unwrap();
        let back: InitializeParams = serde_json::from_str(&j).unwrap();
        assert_eq!(back.viewport.cols, 220);
    }

    #[test]
    fn render_update_round_trip() {
        let u = RenderUpdateParams {
            screen: "AAAA".into(),
            last_seq: 7,
        };
        let j = serde_json::to_string(&u).unwrap();
        let back: RenderUpdateParams = serde_json::from_str(&j).unwrap();
        assert_eq!(back.last_seq, 7);
    }

    #[test]
    fn b64_round_trip() {
        let bytes = b"\x1b[2J\x1b[H";
        let enc = b64_encode(bytes);
        let dec = b64_decode(&enc).unwrap();
        assert_eq!(dec, bytes);
    }

    #[test]
    fn b64_padding() {
        assert_eq!(b64_decode(&b64_encode(b"a")).unwrap(), b"a");
        assert_eq!(b64_decode(&b64_encode(b"ab")).unwrap(), b"ab");
        assert_eq!(b64_decode(&b64_encode(b"abc")).unwrap(), b"abc");
    }
}
