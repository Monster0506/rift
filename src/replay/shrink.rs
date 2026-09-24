use super::backend::ReplayBackend;
use crate::editor::Editor;
use crate::invariants::InvariantTier;
use crate::ipc::key_notation::vim_to_key;
use crate::key::Key;

pub fn reproduces_area(keys: &[Key], target_area: &'static str) -> bool {
    let backend = ReplayBackend::new(std::io::sink(), 24, 80);
    let Ok(mut ed) = Editor::with_file(backend, None) else {
        return false;
    };

    for key in keys {
        ed.term.push_keys(std::iter::once(key.clone()));
        if ed.tick().is_err() {
            return false;
        }
        if ed
            .assert_invariants(InvariantTier::Deep)
            .iter()
            .any(|v| v.area == target_area)
        {
            return true;
        }
    }
    false
}

pub fn ddmin(mut keys: Vec<Key>, reproduces: impl Fn(&[Key]) -> bool) -> Vec<Key> {
    assert!(
        reproduces(&keys),
        "ddmin: the initial input must already reproduce the failure"
    );

    let mut chunk_count = 2usize;
    while keys.len() >= 2 {
        let chunk_size = keys.len().div_ceil(chunk_count);
        let mut reduced = false;

        for chunk_start in (0..keys.len()).step_by(chunk_size) {
            let chunk_end = (chunk_start + chunk_size).min(keys.len());

            let complement: Vec<Key> = keys[..chunk_start]
                .iter()
                .chain(keys[chunk_end..].iter())
                .cloned()
                .collect();
            if !complement.is_empty() && reproduces(&complement) {
                keys = complement;
                chunk_count = (chunk_count - 1).max(2);
                reduced = true;
                break;
            }

            let chunk: Vec<Key> = keys[chunk_start..chunk_end].to_vec();
            if chunk.len() < keys.len() && reproduces(&chunk) {
                keys = chunk;
                chunk_count = 2;
                reduced = true;
                break;
            }
        }

        if !reduced {
            if chunk_count >= keys.len() {
                break;
            }
            chunk_count = (chunk_count * 2).min(keys.len());
        }
    }

    let mut i = 0;
    while i < keys.len() {
        let mut candidate = keys.clone();
        candidate.remove(i);
        if !candidate.is_empty() && reproduces(&candidate) {
            keys = candidate;
        } else {
            i += 1;
        }
    }

    keys
}

pub fn format_keys(keys: &[Key]) -> (String, String) {
    let notation: String = keys.iter().cloned().map(vim_to_key).collect();
    let literal = format!("{keys:?}");
    (notation, literal)
}

pub fn bracket_list(keys: &[Key]) -> String {
    let tokens: Vec<String> = keys.iter().cloned().map(vim_to_key).collect();
    format!("[{}]", tokens.join(", "))
}

#[cfg(test)]
#[path = "shrink_tests.rs"]
mod tests;
