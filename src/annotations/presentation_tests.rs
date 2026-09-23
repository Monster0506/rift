use super::*;

#[test]
fn presentation_round_trips_through_json() {
    let p = Presentation::with_face(FaceRef::new("diag.error"))
        .with_adornment(Adornment::new("E", Placement::Trailing))
        .with_priority(5);
    let json = serde_json::to_string(&p).unwrap();
    let back: Presentation = serde_json::from_str(&json).unwrap();
    assert_eq!(p, back);
}

#[test]
fn style_override_carries_color() {
    let s = StyleOverride {
        fg: Some(Color::Red),
        bold: true,
        ..Default::default()
    };
    let back: StyleOverride = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back.fg, Some(Color::Red));
    assert!(back.bold);
}
