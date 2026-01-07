use crate::buffer::api::BufferView;
use crate::search::SearchMatch;

/// Tier 1: Literal Search
///
/// Fast scan for simple string literals without regex compilation overhead.
/// Uses the BufferView iterator directly to find matches.
pub fn find_literal<'a, V>(view: &'a V, pattern: &str, start_pos: usize) -> Option<SearchMatch>
where
    V: BufferView,
{
    if pattern.is_empty() {
        return Some(SearchMatch {
            range: start_pos..start_pos,
        });
    }

    // Smart Case Logic:
    // If pattern contains any uppercase char, it's case-sensitive.
    // If pattern is all lowercase, it's case-insensitive.
    // Note: We check the original pattern string for uppercase.
    let case_sensitive = pattern.chars().any(char::is_uppercase);

    if case_sensitive {
        find_literal_exact(view, pattern, start_pos)
    } else {
        find_literal_ignore_case(view, pattern, start_pos)
    }
}

fn find_literal_exact<'a, V>(view: &'a V, pattern: &str, start_pos: usize) -> Option<SearchMatch>
where
    V: BufferView,
{
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let pattern_len = pattern_chars.len();
    let first_char = pattern_chars[0];

    let mut chunk_iter = view.iter_chunks_at(start_pos);
    let mut current_pos = start_pos;

    while let Some(chunk) = chunk_iter.next() {
        for (idx, &c) in chunk.iter().enumerate() {
            // to_char_lossy is cheap for Unicode/Byte chars
            let ch = c.to_char_lossy();
            if ch == first_char {
                let match_start = current_pos + idx;
                let remaining_in_chunk = chunk.len() - idx;

                if remaining_in_chunk >= pattern_len {
                    // Fast path
                    let slice = &chunk[idx..idx + pattern_len];
                    let mut match_found = true;
                    for (i, &sc) in slice.iter().skip(1).enumerate() {
                        if sc.to_char_lossy() != pattern_chars[i + 1] {
                            match_found = false;
                            break;
                        }
                    }
                    if match_found {
                        return Some(SearchMatch {
                            range: match_start..match_start + pattern_len,
                        });
                    }
                } else {
                    // Slow path
                    if check_match_slow(view, match_start, &pattern_chars) {
                        return Some(SearchMatch {
                            range: match_start..match_start + pattern_len,
                        });
                    }
                }
            }
        }
        current_pos += chunk.len();
    }
    None
}

fn find_literal_ignore_case<'a, V>(
    view: &'a V,
    pattern: &str,
    start_pos: usize,
) -> Option<SearchMatch>
where
    V: BufferView,
{
    // Pre-process pattern to lowercase for comparison
    // Note: one char in pattern could map to multiple in lowercase (e.g. German sharp s).
    // For simplicity/performance in Tier 1, we assume 1-to-1 or 1-to-many char mapping is handled by equality.
    // Wait, simple iteration assumes 1-to-1 code points mostly.
    // If pattern "SS" matches "ß", lengths differ.
    // Tier 1 literal search is "literal". "ß" != "SS".
    // We just compare to_lowercase() == to_lowercase().
    // We assume pattern length in chars matches target length in chars for simplicity,
    // OR we just match sequence.
    // Let's use standard char iteration.

    // Optimization: If pattern is ASCII, use ASCII fast path.
    if pattern.is_ascii() {
        return find_literal_ignore_case_ascii(view, pattern, start_pos);
    }

    // Unicode slow path
    let pattern_lower_str = pattern.to_lowercase();
    let pattern_lower: Vec<char> = pattern_lower_str.chars().collect();
    let first_char_lower = pattern_lower[0];
    // This is tricky because `to_lowercase` returns iterator.
    // And "First char" in haystack might expand to multiple?
    // Let's rely on simple char-by-char lowercase comparison. If lengths diverge, it's not a "literal" match in the simple sense.
    // But safely, we should fallback to regex for complex unicode casing?
    // No, user wants speed.
    // Let's implement simple 1-to-1 check where possible.

    let mut chunk_iter = view.iter_chunks_at(start_pos);
    let mut current_pos = start_pos;

    while let Some(chunk) = chunk_iter.next() {
        for (idx, &c) in chunk.iter().enumerate() {
            let ch = c.to_char_lossy();
            // Check if starts match
            // We can't easily check one-to-many here without buffering.
            // Given the constraints and "Literal" tier definition, we only support
            // queries where character counts match (simple case insensitivity).
            // If pattern is "foo", we match "Foo".Lengths same.

            if ch.to_lowercase().next() == Some(first_char_lower) {
                // Check rest
                let match_start = current_pos + idx;
                if check_match_ignore_case(view, match_start, &pattern_lower) {
                    return Some(SearchMatch {
                        range: match_start..match_start + pattern.chars().count(),
                    });
                }
            }
        }
        current_pos += chunk.len();
    }
    None
}

fn find_literal_ignore_case_ascii<'a, V>(
    view: &'a V,
    pattern: &str,
    start_pos: usize,
) -> Option<SearchMatch>
where
    V: BufferView,
{
    let pattern_bytes = pattern.as_bytes();
    let pattern_len = pattern_bytes.len();
    let first_byte = pattern_bytes[0].to_ascii_lowercase();

    let mut chunk_iter = view.iter_chunks_at(start_pos);
    let mut current_pos = start_pos;

    while let Some(chunk) = chunk_iter.next() {
        for (idx, &c) in chunk.iter().enumerate() {
            let ch = c.to_char_lossy();
            if ch.is_ascii() {
                let lower = ch.to_ascii_lowercase();
                if lower as u8 == first_byte {
                    let match_start = current_pos + idx;
                    let remaining = chunk.len() - idx;
                    if remaining >= pattern_len {
                        let slice = &chunk[idx..idx + pattern_len];
                        let mut match_found = true;
                        for (i, &sc) in slice.iter().skip(1).enumerate() {
                            let sc_char = sc.to_char_lossy();
                            if !sc_char.is_ascii()
                                || sc_char.to_ascii_lowercase() as u8
                                    != pattern_bytes[i + 1].to_ascii_lowercase()
                            {
                                match_found = false;
                                break;
                            }
                        }
                        if match_found {
                            return Some(SearchMatch {
                                range: match_start..match_start + pattern_len,
                            });
                        }
                    } else {
                        if check_match_ignore_case_ascii(view, match_start, pattern_bytes) {
                            return Some(SearchMatch {
                                range: match_start..match_start + pattern_len,
                            });
                        }
                    }
                }
            }
        }
        current_pos += chunk.len();
    }
    None
}

fn check_match_slow<V: BufferView>(view: &V, start: usize, pattern: &[char]) -> bool {
    let mut iter = view.iter_at(start);
    for &pc in pattern {
        if let Some(c) = iter.next() {
            if c.to_char_lossy() != pc {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

fn check_match_ignore_case<V: BufferView>(view: &V, start: usize, pattern_lower: &[char]) -> bool {
    let mut iter = view.iter_at(start);
    for &pc in pattern_lower {
        if let Some(c) = iter.next() {
            // This is slightly incorrect if one char maps to multiple, but fits Tier 1 "simple" heuristic
            if c.to_char_lossy().to_lowercase().next() != Some(pc) {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

fn check_match_ignore_case_ascii<V: BufferView>(
    view: &V,
    start: usize,
    pattern_bytes: &[u8],
) -> bool {
    let mut iter = view.iter_at(start);
    for &pb in pattern_bytes {
        if let Some(c) = iter.next() {
            let ch = c.to_char_lossy();
            if !ch.is_ascii() || ch.to_ascii_lowercase() as u8 != pb.to_ascii_lowercase() {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}
