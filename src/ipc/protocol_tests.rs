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
