use super::haystack::BufferHaystackContext;
use super::*;
use crate::buffer::api::BufferView;
use crate::character::Character;
use crate::search::find_iter;

// --- Mock Buffer Implementation ---

struct MockBuffer {
    chars: Vec<Character>,
    line_starts: Vec<usize>,
}

impl MockBuffer {
    fn new(lines: &[&str]) -> Self {
        let mut chars = Vec::new();
        let mut line_starts = Vec::new();
        let mut offset = 0;

        for line in lines.iter() {
            line_starts.push(offset);
            for c in line.chars() {
                chars.push(Character::from(c));
                offset += 1;
            }
            chars.push(Character::Newline);
            offset += 1;
        }

        Self { chars, line_starts }
    }
}

impl BufferView for MockBuffer {
    fn len(&self) -> usize {
        self.chars.len()
    }

    fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    fn line_start(&self, line: usize) -> usize {
        self.line_starts
            .get(line)
            .copied()
            .unwrap_or(self.chars.len())
    }

    type CharIter<'a> = std::iter::Copied<std::slice::Iter<'a, Character>>;

    fn iter_at(&self, pos: usize) -> Self::CharIter<'_> {
        if pos >= self.chars.len() {
            return self.chars[self.chars.len()..].iter().copied();
        }
        self.chars[pos..].iter().copied()
    }

    type ChunkIter<'a> = std::iter::Once<&'a [Character]>;

    fn iter_chunks_at(&self, pos: usize) -> Self::ChunkIter<'_> {
        if pos >= self.chars.len() {
            std::iter::once(&self.chars[self.chars.len()..])
        } else {
            std::iter::once(&self.chars[pos..])
        }
    }

    fn revision(&self) -> u64 {
        0
    }
}

// --- Tests ---

#[test]
fn test_find_next_forward_simple() {
    let buffer = MockBuffer::new(&["hello world", "another line"]);

    let res = find_next(&buffer, 0, "world", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let _m = res.unwrap();

    let res = find_next(&buffer, 0, "hello", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..5);
}

#[test]
fn test_find_next_forward_next_line() {
    let buffer = MockBuffer::new(&["line one", "line two"]);

    let res = find_next(&buffer, 0, "two", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 14..17);
}

#[test]
fn test_find_next_forward_wrap() {
    let buffer = MockBuffer::new(&["first", "second", "third"]);

    let start_pos = 6;
    let res = find_next(&buffer, start_pos, "first", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..5);
}

#[test]
fn test_find_next_backward_simple() {
    let buffer = MockBuffer::new(&["hello world"]);

    let res = find_next(&buffer, 10, "hello", SearchDirection::Backward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 0..5);
}

#[test]
fn test_find_next_backward_wrap() {
    let buffer = MockBuffer::new(&["first", "second"]);

    let res = find_next(&buffer, 0, "second", SearchDirection::Backward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 6..12);
}

#[test]
fn test_find_next_backward_same_line() {
    let buffer = MockBuffer::new(&["foo bar baz"]);

    let res = find_next(&buffer, 8, "bar", SearchDirection::Backward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 4..7);
}

#[test]
fn test_unicode_offsets() {
    let buffer = MockBuffer::new(&["Héllo world"]);

    let res = find_next(&buffer, 0, "world", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 6..11);

    let res = find_next(&buffer, 0, "é", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 1..2);
}

#[test]
fn test_unicode_word_search_no_panic() {
    let buffer = MockBuffer::new(&["héllo wörld café"]);

    let (_m, _) = find_next(&buffer, 0, r"\w+", SearchDirection::Forward).unwrap();

    let (all, _) = find_all(&buffer, r"\w+").unwrap();
    let _words: Vec<_> = all.iter().map(|m| m.range.clone()).collect();
}

#[test]
fn test_lookbehind_non_ascii_no_panic() {
    let buffer = MockBuffer::new(&["äbc"]);

    let (m, _) = find_next(&buffer, 0, r"(?<!x)b", SearchDirection::Forward).unwrap();
    assert_eq!(m.expect("(?<!x)b should match").range, 1..2);

    let (_all, _) = find_all(&buffer, r"(?<!q).").unwrap();
}

#[test]
fn test_multiline_search() {
    let buffer = MockBuffer::new(&["line one", "line two"]);

    let res = find_next(&buffer, 0, "one\\nline", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 5..13);
}

#[test]
fn test_multiline_wrap() {
    let buffer = MockBuffer::new(&["A", "B", "C"]);
    let res = find_next(&buffer, 4, "A\\nB", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let _m = res.unwrap();
}

#[test]
fn test_case_sensitivity() {
    let buffer = MockBuffer::new(&["Hello"]);

    let res = find_next(&buffer, 0, "hello", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());

    let res = find_next(&buffer, 0, "HELLO", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_none());
}

#[test]
fn test_regex_anchors() {
    let buffer = MockBuffer::new(&["foo bar", "baz qux"]);

    let res = find_next(&buffer, 0, "^baz", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 8..11);

    let res = find_next(&buffer, 0, "bar$", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_some());
    let m = res.unwrap();
    assert_eq!(m.range, 4..7);
}

#[test]
fn test_no_match() {
    let buffer = MockBuffer::new(&["hello world"]);
    let res = find_next(&buffer, 0, "xyz", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_none());
}

#[test]
fn test_empty_buffer() {
    let buffer = MockBuffer::new(&[]);
    let res = find_next(&buffer, 0, "abc", SearchDirection::Forward)
        .unwrap()
        .0;
    assert!(res.is_none());
}
#[test]
fn test_find_iter_incremental() {
    let buffer = MockBuffer::new(&["hello world", "hello again"]);
    let context = BufferHaystackContext::new(&buffer);

    let mut iter = find_iter(&context, "hello").unwrap();

    let m1 = iter.next().unwrap();
    assert_eq!(m1.range, 0..5);

    let m2 = iter.next().unwrap();
    assert_eq!(m2.range, 12..17);

    assert!(iter.next().is_none());
}
#[test]
fn test_find_all_incremental_integration() {
    let buffer = MockBuffer::new(&["fn foo() {", "    return;", "}"]);

    let (matches, _) = find_all(&buffer, "fn.*").unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].range, 0..10);

    let query = "fn.*\\n.*return";
    let (matches2, _) = find_all(&buffer, query).unwrap();
    assert_eq!(matches2.len(), 1);
}

#[test]
fn test_find_all_empty_literal_pattern_terminates_with_no_matches() {
    let buffer = MockBuffer::new(&["hello world", "hello again"]);
    let (matches, _) = find_all(&buffer, "/").unwrap();
    assert!(matches.is_empty());
}

#[test]
fn test_find_all_empty_pattern_with_flags_terminates_with_no_matches() {
    let buffer = MockBuffer::new(&["hello world"]);
    let (matches, _) = find_all(&buffer, "/i").unwrap();
    assert!(matches.is_empty());
}

#[test]
fn test_large_file_search_performance_with_cache() {
    use crate::buffer::byte_map::ByteLineMap;
    use crate::document::Document;

    let mut doc = Document::new(1).unwrap();

    let line = "This is a line of text to simulate a file content.\n";
    let mut large_text = String::with_capacity(1_000_000);
    for _ in 0..20_000 {
        large_text.push_str(line);
    }
    doc.insert_str(&large_text).unwrap();

    {
        let buffer = &doc.buffer;
        let mut current_byte_offset = 0;
        let mut current_char_offset = 0;
        let mut line_starts = vec![0];
        let mut line_char_starts = vec![0];

        for c in buffer.iter() {
            current_byte_offset += c.len_utf8();
            current_char_offset += 1;
            if c == crate::character::Character::Newline {
                line_starts.push(current_byte_offset);
                line_char_starts.push(current_char_offset);
            }
        }

        if let Some(cell) = buffer.byte_line_map() {
            *cell.borrow_mut() = Some(ByteLineMap::new(
                line_starts,
                line_char_starts,
                buffer.revision(),
            ));
        }
    }
    let start = std::time::Instant::now();
    let result = doc.perform_search("nonexistent_string_12345", SearchDirection::Forward, false);
    let duration = start.elapsed();
    eprintln!("[perf] warm-cache literal search took {:?}", duration);

    let max_duration = if cfg!(debug_assertions) { 250 } else { 15 };

    assert!(
        duration.as_millis() < max_duration,
        "Search took too long: {:?} (expected <{}ms with cache)",
        duration,
        max_duration
    );
    assert!(result.unwrap().0.is_none());
}

/// Complex searches over a ~1 MB buffer must finish under 1s, run on a worker
/// thread so a true hang fails the budget instead of blocking the test.
#[test]
fn test_complex_query_search_within_time_budget() {
    use std::sync::mpsc;
    use std::sync::Arc;
    use std::time::Duration;

    let line = "This is a line of text to simulate a file content.";
    let lines = vec![line; 20_000];
    let buffer = Arc::new(MockBuffer::new(&lines));

    let queries: &[(&str, &str)] = &[
        ("regex digit class", r"nonexistent_\d+"),
        ("regex char class", r"zzz[a-z0-9]+qqq"),
        ("regex alternation", r"(foo|bar|baz)_nonexistent"),
        ("regex dot-star", r"simulate.*nonexistent"),
        ("anchored regex", r"^nonexistent.*content$"),
        ("negative lookahead", r"content(?>!\.)nonexistent"),
        ("negative lookbehind", r"(?<!x)nonexistent_zzz"),
        ("backreference", r"(\w+)_\1_nonexistent"),
    ];

    let budget = Duration::from_secs(1);

    for (label, query) in queries {
        let buf = Arc::clone(&buffer);
        let q = (*query).to_string();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let matched = find_next(&*buf, 0, &q, SearchDirection::Forward)
                .expect("search should not error")
                .0
                .is_some();
            let _ = tx.send(matched);
        });

        match rx.recv_timeout(budget) {
            Ok(matched) => {
                assert!(
                    !matched,
                    "{label} ({query}) unexpectedly matched in the buffer"
                );
                handle.join().unwrap();
            }
            Err(_) => panic!(
                "{label} ({query}) did not finish within {budget:?} \
                 (catastrophic backtracking / O(N^2) regression?)"
            ),
        }
    }
}

/// `find_all` (used by `:s` substitution and search highlight/count) must also
/// stay fast on a freshly-loaded ~1 MB file with a *cold* cache.
#[test]
fn test_complex_find_all_within_time_budget() {
    use crate::document::Document;
    use std::time::Duration;

    let mut doc = Document::new(1).unwrap();
    let line = "This is a line of text to simulate a file content.\n";
    let mut text = String::with_capacity(1_000_000);
    for _ in 0..20_000 {
        text.push_str(line);
    }
    doc.insert_str(&text).unwrap();

    let queries: &[(&str, &str)] = &[
        ("regex digit class", r"nonexistent_\d+"),
        ("regex char class", r"zzz[a-z0-9]+qqq"),
        ("regex alternation", r"(foo|bar|baz)_nonexistent"),
        ("regex dot-star", r"simulate.*nonexistent"),
        ("anchored regex", r"^nonexistent.*content$"),
        ("negative lookahead", r"content(?>!\.)nonexistent"),
        ("negative lookbehind", r"(?<!x)nonexistent_zzz"),
        ("backreference", r"(\w+)_\1_nonexistent"),
    ];

    let budget = Duration::from_secs(1);

    for (label, query) in queries {
        let start = std::time::Instant::now();
        let (matches, _) = doc
            .find_all_matches(query)
            .expect("find_all should not error");
        let elapsed = start.elapsed();
        assert!(
            matches.is_empty(),
            "{label} ({query}) unexpectedly matched in the buffer"
        );
        assert!(
            elapsed < budget,
            "find_all for {label} ({query}) took {elapsed:?}, exceeds {budget:?} \
             (O(N^2) line_start / streaming-haystack regression?)"
        );
    }
}

#[test]
fn test_smartcase_uppercase_no_hang_find_next() {
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let buffer = MockBuffer::new(&["a"]);
        let result = find_next(&buffer, 0, "A", SearchDirection::Forward);
        tx.send(result).unwrap();
    });

    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(result) => {
            let (m, _stats) = result.unwrap();
            assert!(
                m.is_none(),
                "Uppercase 'A' should not match lowercase 'a' with smartcase"
            );
        }
        Err(_) => {
            panic!("HANG DETECTED: find_next with smartcase uppercase query hung for >2s");
        }
    }

    handle.join().unwrap();
}

#[test]
fn test_smartcase_uppercase_no_hang_find_all() {
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let buffer = MockBuffer::new(&["a"]);
        let result = find_all(&buffer, "A");
        tx.send(result).unwrap();
    });

    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(result) => {
            let (matches, _stats) = result.unwrap();
            assert!(
                matches.is_empty(),
                "Uppercase 'A' should not match lowercase 'a' with smartcase"
            );
        }
        Err(_) => {
            panic!("HANG DETECTED: find_all with smartcase uppercase query hung for >2s");
        }
    }

    handle.join().unwrap();
}

#[test]
fn test_smartcase_uppercase_no_hang_with_document() {
    use crate::document::Document;
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let mut doc = Document::new(1).unwrap();
        doc.insert_str("a").unwrap();

        let result = doc.perform_search("A", SearchDirection::Forward, false);
        tx.send(result).unwrap();
    });

    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(result) => {
            let (m, _stats) = result.unwrap();
            assert!(
                m.is_none(),
                "Uppercase 'A' should not match lowercase 'a' with smartcase"
            );
        }
        Err(_) => {
            panic!("HANG DETECTED: Document.perform_search with smartcase uppercase query hung for >2s");
        }
    }

    handle.join().unwrap();
}

#[test]
fn test_smartcase_coding_no_hang_with_document() {
    use crate::document::Document;
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let mut doc = Document::new(1).unwrap();
        doc.insert_str("some text content here\n").unwrap();

        let result = doc.perform_search("Coding", SearchDirection::Forward, false);
        tx.send(result).unwrap();
    });

    match rx.recv_timeout(Duration::from_secs(1)) {
        Ok(result) => {
            let (m, _stats) = result.unwrap();
            assert!(
                m.is_none(),
                "'Coding' should not match lowercase content with smartcase"
            );
        }
        Err(_) => {
            panic!("HANG DETECTED: Document.perform_search for 'Coding' hung for >2s");
        }
    }

    handle.join().unwrap();
}

#[test]
fn test_smartcase_various_letters_no_hang() {
    use std::sync::mpsc;
    use std::time::Duration;

    let test_cases: Vec<(&[&str], &str)> = vec![
        (&["a"], "A"),
        (&["b"], "B"),
        (&["z"], "Z"),
        (&["abc"], "ABC"),
        (&["hello world"], "HELLO"),
        (&["coding"], "Coding"),
    ];

    for (lines, query) in test_cases {
        let query_owned = query.to_string();
        let lines_owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let (tx, rx) = mpsc::channel();

        let handle = std::thread::spawn(move || {
            let line_refs: Vec<&str> = lines_owned.iter().map(|s| s.as_str()).collect();
            let buffer = MockBuffer::new(&line_refs);
            let result = find_next(&buffer, 0, &query_owned, SearchDirection::Forward);
            tx.send(result).unwrap();
        });

        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(result) => {
                assert!(
                    result.is_ok(),
                    "Search for '{}' in {:?} should not error",
                    query,
                    lines
                );
            }
            Err(_) => {
                panic!(
                    "HANG DETECTED: find_next for query '{}' in buffer {:?} hung for >2s",
                    query, lines
                );
            }
        }

        handle.join().unwrap();
    }
}

#[test]
fn compile_regex_plain_pattern_applies_smartcase() {
    let (regex, _) = compile_regex("hello.*world").expect("should compile");
    let matches: Vec<_> = regex.find_all("HELLO there WORLD").collect();
    assert_eq!(
        matches.len(),
        1,
        "smartcase should make this match case-insensitively"
    );
}

#[test]
fn compile_regex_interior_slash_is_not_treated_as_rift_format() {
    let (regex, _) = compile_regex("foo.*/bar").expect("should compile");
    let matches: Vec<_> = regex.find_all("foo123/bar").collect();
    assert_eq!(
        matches.len(),
        1,
        "interior slash should be part of the pattern, not a flag delimiter"
    );
}
