//! Availability limits for engine answers.
//!
//! An engine can return a list larger than the UI can paint, or a JSON document
//! deeper than a recursive parser should walk. Refusing those with a named
//! availability error is better than allocating without bound or reporting an
//! empty store.

/// The deepest object/array nesting berth will parse.
pub const MAX_DEPTH: usize = 64;
/// The largest list berth will accept from one store.
pub const MAX_ENTRIES: usize = 5_000;

/// Whether `bytes` nests objects/arrays deeper than `max`.
///
/// This is a byte scanner, not a parser: it only needs to know the depth, and
/// it must not be fooled by brackets inside strings.
pub fn nesting_exceeds(bytes: &[u8], max: usize) -> bool {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > max {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    false
}

/// Whether the top-level JSON array in `bytes` has more than `max` entries.
///
/// A non-array answer returns false; the caller's typed parse will reject it.
pub fn array_len_exceeds(bytes: &[u8], max: usize) -> bool {
    let Some(start) = bytes.iter().position(|byte| !byte.is_ascii_whitespace()) else {
        return false;
    };
    if bytes[start] != b'[' {
        return false;
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut entries = 1usize;
    let mut any_value = false;

    for &byte in &bytes[start..] {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            any_value = true;
            continue;
        }

        match byte {
            b'"' => {
                in_string = true;
                any_value = true;
            }
            b'[' | b'{' => {
                depth += 1;
                if depth > 1 {
                    any_value = true;
                }
            }
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                any_value = true;
            }
            b',' if depth == 1 => {
                entries += 1;
                if entries > max {
                    return true;
                }
            }
            byte if !byte.is_ascii_whitespace() => any_value = true,
            _ => {}
        }
    }

    any_value && entries > max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_is_counted_at_the_limit() {
        let ok = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(!nesting_exceeds(ok.as_bytes(), MAX_DEPTH));

        let too_deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert!(nesting_exceeds(too_deep.as_bytes(), MAX_DEPTH));
    }

    #[test]
    fn brackets_inside_strings_do_not_count() {
        let json = br#"{"message":"[[[[[[[[[["}"#;
        assert!(!nesting_exceeds(json, 2));
    }

    #[test]
    fn escaped_quotes_do_not_end_a_string() {
        let json = br#"{"message":"a \" [ still a string"}"#;
        assert!(!nesting_exceeds(json, 2));
    }

    #[test]
    fn array_length_stops_at_the_cap() {
        let small = br#"[1,2,3]"#;
        assert!(!array_len_exceeds(small, 3));
        assert!(array_len_exceeds(small, 2));
        let empty = br#"[]"#;
        assert!(!array_len_exceeds(empty, 0));
    }

    #[test]
    fn nested_arrays_do_not_inflate_the_top_level_count() {
        let json = br#"[[1,2,3],[4,5,6]]"#;
        assert!(!array_len_exceeds(json, 2));
        assert!(array_len_exceeds(json, 1));
    }

    #[test]
    fn commas_inside_strings_are_not_separators() {
        let json = br#"["a,b","c,d"]"#;
        assert!(!array_len_exceeds(json, 2));
        assert!(array_len_exceeds(json, 1));
    }
}
