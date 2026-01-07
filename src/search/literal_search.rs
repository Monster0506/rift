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

    let pattern_chars: Vec<char> = pattern.chars().collect();
    let pattern_len = pattern_chars.len();
    let first_char = pattern_chars[0];

    let mut iter = view.iter_at(start_pos);
    let mut current_pos = start_pos;

    while let Some(c) = iter.next() {
        if c.to_char_lossy() == first_char {
            // Potential match, check strict equality
            let match_start = current_pos;
            let mut is_match = true;

            // Check remaining chars
            let mut check_iter = iter.clone();

            for &pat_char in pattern_chars.iter().skip(1) {
                if let Some(next_c) = check_iter.next() {
                    if next_c.to_char_lossy() != pat_char {
                        is_match = false;
                        break;
                    }
                } else {
                    // End of buffer before pattern finished
                    is_match = false;
                    break;
                }
            }

            if is_match {
                // Return range in code-points
                let match_end = match_start + pattern_len;
                return Some(SearchMatch {
                    range: match_start..match_end,
                });
            }
        }

        current_pos += 1; // Increment by 1 code point
    }

    None
}
