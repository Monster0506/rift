//! Portable `Instant`/`SystemTime`: std's panics on wasm32 ("time not
//! implemented on this platform"); `web_time` is an API-compatible drop-in.

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_arch = "wasm32")]
pub use web_time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
mod tests {
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
}
