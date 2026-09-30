//! Minimal deterministic keybind fuzzer: feeds a headless `Editor` a stream
//! of pseudo-random keys and calls `assert_invariants` after each one

use super::backend::ReplayBackend;
use crate::editor::Editor;
use crate::error::RiftError;
use crate::invariants::{InvariantTier, Violation};
use crate::key::Key;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

const KEY_CHARS: &[char] = &[
    'h', 'j', 'k', 'l', 'w', 'b', 'e', 'W', 'B', 'E', '0', '$', '^', 'g', 'G', 'i', 'a', 'o', 'O',
    'I', 'A', 'x', 'X', 's', 'S', 'r', 'R', 'c', 'C', 'd', 'D', 'y', 'Y', 'p', 'P', 'u', 'v', 'V',
    'm', '/', '?', 'n', 'N', ':', '.', 'z', 'Z', '~', '{', '}', '(', ')', '[', ']', '%', '"', '*',
    '#', '\'', '1', '2', '3', '4', '5', ' ',
];

const CTRL_BYTES: &[u8] = b"vwraeudfboinp";

fn random_key(rng: &mut Rng) -> Key {
    match rng.below(20) {
        0..=13 => Key::Char(KEY_CHARS[rng.below(KEY_CHARS.len())]),
        14 => Key::Escape,
        15 => Key::Enter,
        16 => Key::Backspace,
        17 => Key::Ctrl(CTRL_BYTES[rng.below(CTRL_BYTES.len())]),
        18 => match rng.below(4) {
            0 => Key::ArrowUp,
            1 => Key::ArrowDown,
            2 => Key::ArrowLeft,
            _ => Key::ArrowRight,
        },
        _ => Key::Tab,
    }
}

pub struct FuzzConfig {
    pub seed: u64,
    pub steps: usize,
    pub tier: InvariantTier,
    pub deep_every: usize,
    pub rows: u16,
    pub cols: u16,
}

impl Default for FuzzConfig {
    fn default() -> Self {
        Self {
            seed: 0x5EED,
            steps: 2000,
            tier: InvariantTier::Fast,
            deep_every: 200,
            rows: 24,
            cols: 80,
        }
    }
}

pub struct FuzzFailure {
    pub step: usize,
    pub keys: Vec<Key>,
    pub violations: Vec<Violation>,
    pub tick_error: Option<RiftError>,
}

pub fn run(config: &FuzzConfig) -> Result<usize, FuzzFailure> {
    let mut rng = Rng::new(config.seed);
    let backend = ReplayBackend::new(std::io::sink(), config.rows, config.cols);
    let mut ed = Editor::with_file(backend, None).expect("fresh headless editor");

    let trace = std::env::var_os("RIFT_FUZZ_TRACE").is_some();

    let mut keys = Vec::with_capacity(config.steps);
    for step in 0..config.steps {
        let key = random_key(&mut rng);
        if trace {
            eprintln!("step={step} key={key:?}");
        }
        keys.push(key.clone());
        ed.term.push_keys(std::iter::once(key));

        if let Err(tick_error) = ed.tick() {
            return Err(FuzzFailure {
                step,
                keys,
                violations: Vec::new(),
                tick_error: Some(tick_error),
            });
        }

        let tier = if config.deep_every > 0 && (step + 1) % config.deep_every == 0 {
            InvariantTier::Deep
        } else {
            config.tier
        };
        let violations = ed.assert_invariants(tier);
        if !violations.is_empty() {
            return Err(FuzzFailure {
                step,
                keys,
                violations,
                tick_error: None,
            });
        }
    }

    Ok(config.steps)
}

#[cfg(test)]
mod tests {
    use super::super::shrink::{bracket_list, ddmin, reproduces_area};
    use super::*;

    fn format_failure(seed_desc: &str, failure: FuzzFailure) -> String {
        let Some(violation) = failure.violations.first().cloned() else {
            return format!(
                "{seed_desc}: failed via a tick error, which shrink doesn't cover yet: {:?}\n",
                failure.tick_error
            );
        };

        let original_step = failure.step;
        let original_keys = failure.keys.len();

        let minimal = ddmin(failure.keys, |keys| reproduces_area(keys, violation.area));

        format!(
            "\nInvariant failure:\n  {}\n  {}\n\nOriginal:\n  {seed_desc}, step={original_step}, keys={original_keys}\n\nShrunk:\n  keys={}\n\nReproducer:\n  {}\n",
            violation.area,
            violation.detail,
            minimal.len(),
            bracket_list(&minimal),
        )
    }

    fn panic_on_failure(seed_desc: &str, failure: FuzzFailure) -> ! {
        panic!("{}", format_failure(seed_desc, failure));
    }

    #[test]
    #[ignore]
    fn single_seed_fuzz_probe() {
        use std::panic::{self, AssertUnwindSafe};

        let seed: u64 = std::env::var("RIFT_FUZZ_SEED")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let steps: usize = std::env::var("RIFT_FUZZ_STEPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5000);

        let config = FuzzConfig {
            seed,
            steps,
            tier: InvariantTier::Standard,
            deep_every: 100,
            ..FuzzConfig::default()
        };

        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| run(&config)));
        panic::set_hook(previous_hook);

        match outcome {
            Ok(Ok(_)) => println!("RIFT_FUZZ_RESULT: OK seed={seed}"),
            Ok(Err(failure)) => {
                let seed_desc = format!("seed={seed}");
                let previous_hook = panic::take_hook();
                panic::set_hook(Box::new(|_| {}));
                let report =
                    panic::catch_unwind(AssertUnwindSafe(|| format_failure(&seed_desc, failure)))
                        .unwrap_or_else(|_| {
                            format!(
                                "{seed_desc}: found a real invariant violation, but shrinking \
                                 it crashed (ddmin's replay retriggered a panic); reporting \
                                 unshrunk\n"
                            )
                        });
                panic::set_hook(previous_hook);
                println!("RIFT_FUZZ_RESULT: VIOLATION\n{report}");
            }
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "<non-string panic payload>".to_string());
                println!("RIFT_FUZZ_RESULT: CRASH seed={seed}: {message}");
            }
        }
    }

    #[test]
    fn short_random_session_holds_its_invariants() {
        let config = FuzzConfig {
            steps: 500,
            ..FuzzConfig::default()
        };
        if let Err(failure) = run(&config) {
            panic_on_failure(&format!("seed={}", config.seed), failure);
        }
    }

    #[test]
    #[ignore]
    fn long_multi_seed_sweep_holds_its_invariants() {
        for seed in 0..20u64 {
            let config = FuzzConfig {
                seed,
                steps: 5000,
                tier: InvariantTier::Standard,
                deep_every: 100,
                ..FuzzConfig::default()
            };
            if let Err(failure) = run(&config) {
                panic_on_failure(&format!("seed={seed}"), failure);
            }
        }
    }
}
