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
    /// Escalate to `InvariantTier::Deep` every `deep_every` steps; 0 disables it.
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

    let mut keys = Vec::with_capacity(config.steps);
    for step in 0..config.steps {
        let key = random_key(&mut rng);
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

    fn panic_on_failure(seed_desc: &str, failure: FuzzFailure) -> ! {
        let Some(violation) = failure.violations.first().cloned() else {
            panic!(
                "{seed_desc}: failed via a tick error, which shrink doesn't cover yet: {:?}",
                failure.tick_error
            );
        };

        let original_step = failure.step;
        let original_keys = failure.keys.len();

        let minimal = ddmin(failure.keys, |keys| reproduces_area(keys, violation.area));

        panic!(
            "\nInvariant failure:\n  {}\n  {}\n\nOriginal:\n  {seed_desc}, step={original_step}, keys={original_keys}\n\nShrunk:\n  keys={}\n\nReproducer:\n  {}\n",
            violation.area,
            violation.detail,
            minimal.len(),
            bracket_list(&minimal),
        );
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
