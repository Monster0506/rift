use super::Debounced;
use crate::time::Instant;
use std::time::Duration;

const DELAY: Duration = Duration::from_millis(40);

#[test]
fn rearming_pushes_the_deadline_back() {
    let mut debounced = Debounced::new(DELAY);
    let start = Instant::now();
    debounced.arm(1, start);
    debounced.arm(1, start + Duration::from_millis(30));

    assert!(!debounced.is_due(1, start + DELAY));
    assert!(debounced.is_due(1, start + Duration::from_millis(70)));
    assert_eq!(
        debounced.deadline(1),
        Some(start + Duration::from_millis(70))
    );
}

#[test]
fn due_lists_only_elapsed_keys() {
    let mut debounced = Debounced::new(DELAY);
    let start = Instant::now();
    debounced.arm(1, start);
    debounced.arm(2, start + DELAY);

    assert_eq!(debounced.due(start + DELAY), vec![1]);
}

#[test]
fn starting_a_job_disarms_and_cancel_returns_it() {
    let mut debounced = Debounced::new(DELAY);
    let start = Instant::now();
    debounced.arm(1, start);
    debounced.start(1, 7);

    assert!(!debounced.is_armed(1));
    assert_eq!(debounced.deadline(1), None);
    assert_eq!(debounced.cancel(1), Some(7));
    assert_eq!(debounced.cancel(1), None);
}

#[test]
fn finishing_a_stale_job_keeps_the_current_one() {
    let mut debounced = Debounced::new(DELAY);
    debounced.start(1, 7);
    debounced.finish(1, 3);
    assert_eq!(debounced.cancel(1), Some(7));
}

#[test]
fn finishing_the_job_keeps_a_rearmed_deadline() {
    let mut debounced = Debounced::new(DELAY);
    let start = Instant::now();
    debounced.start(1, 7);
    debounced.arm(1, start);
    debounced.finish(1, 7);

    assert_eq!(debounced.deadline(1), Some(start + DELAY));
    assert_eq!(debounced.cancel(1), None);
}
