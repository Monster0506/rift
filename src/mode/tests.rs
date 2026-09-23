use super::Mode;

#[test]
fn visual_variants_report_as_visual_string() {
    assert_eq!(Mode::Visual.as_str(), "visual");
    assert_eq!(Mode::VisualLine.as_str(), "visual");
    assert_eq!(Mode::VisualBlock.as_str(), "visual");
}

#[test]
fn is_visual_true_only_for_visual_variants() {
    assert!(Mode::Visual.is_visual());
    assert!(Mode::VisualLine.is_visual());
    assert!(Mode::VisualBlock.is_visual());
    assert!(!Mode::Normal.is_visual());
    assert!(!Mode::OperatorPending.is_visual());
    assert!(!Mode::Insert.is_visual());
}
