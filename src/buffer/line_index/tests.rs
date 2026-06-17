use super::*;
use crate::character::Character;

fn chars(s: &str) -> Vec<Character> {
    s.chars().map(Character::from).collect()
}

#[test]
fn test_new_line_index() {
    let idx = LineIndex::new();
    assert_eq!(idx.line_count(), 1);
    assert_eq!(idx.len(), 0);
    assert!(idx.is_empty());
}

#[test]
fn test_insert_basic() {
    let mut idx = LineIndex::new();
    idx.insert(0, &chars("Hello"));
    assert_eq!(idx.len(), 5);
    assert_eq!(idx.line_count(), 1);
    assert_eq!(idx.get_start(0), Some(0));
}

#[test]
fn test_insert_newlines() {
    let mut idx = LineIndex::new();
    idx.insert(0, &chars("Line 1\nLine 2"));
    assert_eq!(idx.line_count(), 2);
    assert_eq!(idx.get_start(0), Some(0));
    assert_eq!(idx.get_start(1), Some(7)); // "Line 1\n" is 7 chars
}

#[test]
fn test_get_end() {
    let mut idx = LineIndex::new();
    // "Line 1\nLine 2"
    idx.insert(0, &chars("Line 1\nLine 2"));
    let total_len = idx.len();

    // Line 0: "Line 1" (len 6). Newline at 6.
    // get_end returns position of newline (exclusive end of content)
    assert_eq!(idx.get_end(0, total_len), Some(6));

    // Line 1: "Line 2" (len 6). End of buffer at 13.
    assert_eq!(idx.get_end(1, total_len), Some(13));

    assert_eq!(idx.get_end(2, total_len), None);
}

#[test]
fn test_get_line_at() {
    let mut idx = LineIndex::new();
    idx.insert(0, &chars("A\nB\nC"));
    // 0: 'A', 1: '\n' -> Line 0
    // 2: 'B', 3: '\n' -> Line 1
    // 4: 'C'          -> Line 2

    assert_eq!(idx.get_line_at(0), 0);
    assert_eq!(idx.get_line_at(1), 0); // Newline belongs to line 0
    assert_eq!(idx.get_line_at(2), 1);
    assert_eq!(idx.get_line_at(3), 1);
    assert_eq!(idx.get_line_at(4), 2);
}

#[test]
fn test_delete() {
    let mut idx = LineIndex::new();
    idx.insert(0, &chars("Line 1\nLine 2"));
    // Delete "\nLine " (indices 6 to 11)
    // "Line 12"
    idx.delete(6, 6);

    assert_eq!(idx.line_count(), 1);
    let bytes = idx.bytes_range(0..idx.len());
    assert_eq!(bytes, b"Line 12");
}

#[test]
fn test_char_access() {
    let mut idx = LineIndex::new();
    idx.insert(0, &chars("Hello"));
    assert_eq!(idx.char_at(0), Character::from('H'));
    assert_eq!(idx.char_at(4), Character::from('o'));

    let range = idx.bytes_range(1..4);
    assert_eq!(range, b"ell");
}

/// Independent oracle: line-start char offsets computed directly from a
/// materialized char vector.
fn oracle_starts(text: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, &c) in text.iter().enumerate() {
        if c == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Hammer the incremental line-start cache against the oracle across a long
/// deterministic sequence of inserts, deletes, and replaces.
#[test]
fn test_incremental_line_cache_matches_oracle_under_random_edits() {
    // Tiny deterministic LCG so the sequence is reproducible without a dep.
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut rng = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };

    let mut idx = LineIndex::new();
    let mut model: Vec<char> = Vec::new();
    let alphabet = ['a', 'b', '\n', 'c', '\n'];

    for step in 0..4000 {
        let len = model.len();
        let op = rng() % 10;
        if op < 6 || len == 0 {
            // Insert 1-4 chars (often containing newlines) at a random offset.
            let pos = if len == 0 { 0 } else { rng() % (len + 1) };
            let n = 1 + rng() % 4;
            let text: Vec<char> = (0..n).map(|_| alphabet[rng() % alphabet.len()]).collect();
            idx.insert(pos, &chars(&text.iter().collect::<String>()));
            model.splice(pos..pos, text);
        } else if op < 9 {
            // Delete a random range.
            let pos = rng() % len;
            let n = 1 + rng() % 4.min(len - pos).max(1);
            let n = n.min(len - pos);
            idx.delete(pos, n);
            model.drain(pos..pos + n);
        } else {
            // Replace a random range with new (possibly multi-line) text.
            let pos = rng() % len;
            let count = (1 + rng() % 3).min(len - pos);
            let n = 1 + rng() % 3;
            let text: Vec<char> = (0..n).map(|_| alphabet[rng() % alphabet.len()]).collect();
            idx.replace(pos, count, &chars(&text.iter().collect::<String>()));
            model.splice(pos..pos + count, text);
        }

        // Every line start matches the oracle, and the line count agrees.
        let expected = oracle_starts(&model);
        assert_eq!(
            idx.line_count(),
            expected.len(),
            "line count mismatch at step {step}"
        );
        for (line, &want) in expected.iter().enumerate() {
            assert_eq!(
                idx.get_start(line),
                Some(want),
                "get_start({line}) at step {step}"
            );
        }
        // get_line_at at several probe positions matches the oracle.
        let total = model.len();
        for probe in [0, total / 3, total / 2, total.saturating_sub(1), total] {
            let want = expected.partition_point(|&s| s <= probe).saturating_sub(1);
            assert_eq!(
                idx.get_line_at(probe),
                want,
                "get_line_at({probe}) at step {step}"
            );
        }
    }
}
