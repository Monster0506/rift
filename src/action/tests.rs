//! Tests for the action module

use super::*;

#[test]
fn test_motion_equals() {
    assert_eq!(Motion::Left, Motion::Left);
    assert_ne!(Motion::Left, Motion::Right);
}

#[test]
fn test_action_equals() {
    assert_eq!(Action::Move(Motion::Left), Action::Move(Motion::Left));
    assert_ne!(Action::Move(Motion::Left), Action::Move(Motion::Right));
}
