use super::*;

#[test]
fn instant_now_is_monotonic() {
    let a = Instant::now();
    let b = Instant::now();
    assert!(b >= a);
}

#[test]
fn instant_elapsed_reflects_a_sleep() {
    let start = Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(5));
    assert!(start.elapsed() >= std::time::Duration::from_millis(5));
}

#[test]
fn system_time_now_is_after_unix_epoch() {
    assert!(SystemTime::now().duration_since(UNIX_EPOCH).is_ok());
}
