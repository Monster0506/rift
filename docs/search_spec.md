# Rift Search Specification

This document outlines the regular expression syntax and features supported by Rift's search engine.

## 1. General Syntax

Search patterns are entered in the format:
`pattern/flags`

*   **Pattern**: The regex to match.
*   **Flags**: Optional single-character flags modifying the search behavior.

### Special Characters
The following characters have special meaning and must be escaped with `\` to be matched literally:
`.` `*` `+` `?` `^` `$` `|` `(` `)` `[` `]` `{` `}` `\`

All other characters match themselves literally.

**Note on Dot (`.`):**
By default, `.` matches any character except newline. Use the `s` (dotall) flag to make `.` match newlines.

### Case Sensitivity
*   **Default (Smartcase)**: Case-insensitive if the pattern contains only lowercase letters. Case-sensitive if the pattern contains any uppercase letters.
*   **Overrides**: Can be explicitly set using the `i` (ignore-case) or `c` (case-sensitive) flags.

## 2. Quantifiers

Quantifiers specify how many times the preceding atom (character, group, or character class) should match.

| Quantifier | Meaning | Greedy? | Example |
| :--- | :--- | :--- | :--- |
| `*` | 0 or more | Yes | `a*` matches "", "a", "aa"... |
| `+` | 1 or more | Yes | `a+` matches "a", "aa"... |
| `?` | 0 or 1 | Yes (prefers 1) | `a?` matches "" or "a", preferring "a" |
| `{n}` | Exactly *n* | — | `a{3}` matches "aaa" |
| `{n,m}` | *n* to *m* | Yes | `a{2,4}` matches "aa", "aaa", "aaaa" |
| `{n,}` | *n* or more | Yes | `a{2,}` matches "aa", "aaa"... |
| `{,m}` | 0 to *m* | Yes | `a{,3}` matches "", "a", "aa", "aaa" |
| `*?` | 0 or more | No | `a*?` matches minimal characters |
| `+?` | 1 or more | No | `a+?` matches minimal characters |
| `??` | 0 or 1 | No | `a??` prefers 0 matches |
| `{n,m}?` | *n* to *m* | No | `a{2,4}?` matches "aa" before "aaa" |

## 3. Character Classes

### Standard Classes
| Class | Matches |
| :--- | :--- |
| `\d` | Digit `[0-9]` |
| `\D` | Non-digit |
| `\w` | Word character `[a-zA-Z0-9_]` (ASCII by default) |
| `\W` | Non-word character |
| `\s` | Whitespace `[ \t\r\n\f\v]` |
| `\S` | Non-whitespace |

### Extended Classes
| Class | Matches |
| :--- | :--- |
| `\l` | Lowercase character |
| `\L` | Non-lowercase character |
| `\u` | Uppercase character |
| `\U` | Non-uppercase character |
| `\x` | Hexadecimal digit |
| `\X` | Non-hexadecimal digit |
| `\o` | Octal digit |
| `\O` | Non-octal digit |
| `\h` | Head of word character (start of a word) |
| `\H` | Non-head of word character |
| `\p` | Punctuation `[!"#$%&'()*+,\-./:;<=>?@\[\\\]^_`{|}~]` |
| `\P` | Non-punctuation |
| `\a` | Alphanumeric `[a-zA-Z0-9]` |
| `\A` | Non-alphanumeric |

### Unicode Support
*   **Default**: `\w`, `\d`, `\s`, `\h` match ASCII characters only.
*   **With `u` flag**: These classes include Unicode characters (e.g., `\w` matches accented characters).

### Character Sets
Custom character sets and ranges (e.g., `[a-z]`, `[^0-9]`) are supported.

**Note on Escaping in Character Classes:**
In character classes, special meaning is different. For example, `[\]]` matches a literal `]`, and `[a\-z]` matches `a`, `\`, or `-`.

## 4. Anchors and Boundaries

Anchors assert a position without matching characters (zero-width).

| Anchor | Meaning |
| :--- | :--- |
| `^` | Start of string (or start of line in multiline mode) |
| `$` | End of string (or end of line in multiline mode) |
| `\<` | Start of word |
| `\>` | End of word |
| `\b` | Word boundary (matches at `\<` or `\>`) |
| `\zs` | Sets the start of the match (everything before is excluded from the result) |
| `\ze` | Sets the end of the match (everything after is excluded from the result) |

### Position Anchors
These anchors match at a specific position in the buffer. They are zero-width assertions and do not consume characters.

| Anchor | Meaning | Example |
| :--- | :--- | :--- |
| `\%nl` | Matches anywhere on line *n* (1-indexed). | `\%5lfoo` matches "foo" only if it appears on line 5. |
| `\%nc` | Matches at column *n* (1-indexed). | `\%5cfoo` matches "foo" starting at column 5. |
| `\%#` | Matches at the current cursor position. | `\%#foo` matches "foo" starting exactly under the cursor. |

### Word Boundaries Explained
*   `\<`: Matches the position where a word starts (preceded by non-word, followed by word char).
*   `\>`: Matches the position where a word ends (preceded by word char, followed by non-word).
*   `\b`: Matches at either `\<` or `\>`.

Word boundaries `\<` and `\>` use the same character definition as `\w` (`[a-zA-Z0-9_]`). With the `u` flag, both adapt to Unicode.

## 5. Flags

Flags are appended after the pattern delimiter (e.g., `pattern/flags`).

| Flag | Name | Description |
| :--- | :--- | :--- |
| `i` | ignore-case | Case-insensitive matching (overrides smartcase). |
| `c` | case-sensitive | Case-sensitive matching (overrides smartcase). |
| `m` | multiline | `^` and `$` match line boundaries (`\n`), not just the start/end of the entire buffer. |
| `s` | dotall | `.` matches newlines (including end-of-line). |
| `x` | verbose | Whitespace and `#` comments in the pattern are ignored. Literal spaces must be escaped (e.g., `\ ` or `[ ]`). |
| `g` | global | Match all occurrences (used for find-all or replace operations). |
| `u` | unicode | Enables Unicode support for character classes (`\w`, `\d`, etc.). |

**Verbose Mode Examples (`x` flag):**
*   `/foo bar/x` matches "foobar" (space is ignored).
*   `/foo\ bar/x` matches "foo bar" (space is escaped).
*   `/foo[ ]bar/x` matches "foo bar" (space in bracket).

## 6. Escape Sequences

| Sequence | Matches |
| :--- | :--- |
| `\n` | Newline (LF) |
| `\t` | Tab |
| `\r` | Carriage return (CR) |
| `\f` | Form feed |
| `\v` | Vertical tab |
| `\\` | Literal backslash |

## 7. Groups, Alternation, and Assertions

*   **Alternation**: `pattern1|pattern2` matches either *pattern1* or *pattern2*.
*   **Grouping**: `(pattern)` groups part of the regex and captures it.
*   **Named Capture**: `(?<name>pattern)` captures the group with a specific name.
*   **Non-Capturing Group**: `(?:pattern)` groups without capturing.
*   **Backreferences**: `\1` through `\9` refer to captured groups 1-9. `\0` refers to the entire match.

### Lookaround Assertions
Lookarounds assert that what follows or precedes the current position matches a pattern, without including it in the match result.

| Assertion | Type | Meaning |
| :--- | :--- | :--- |
| `(?>=foo)` | Positive Lookahead | Matches if followed by "foo". |
| `(?>!foo)` | Negative Lookahead | Matches if **not** followed by "foo". |
| `(?<=foo)` | Positive Lookbehind | Matches if preceded by "foo". |
| `(?<!foo)` | Negative Lookbehind | Matches if **not** preceded by "foo". |
