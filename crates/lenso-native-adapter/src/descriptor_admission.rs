//! Compile-time composition for source-authored provided admission overrides.

const KEY: &[u8] = b"\"default_admission\"";

pub const fn joined_len(parts: &[&str]) -> usize {
    let mut length = 0;
    let mut index = 0;
    while index < parts.len() {
        length += parts[index].len();
        index += 1;
    }
    length
}

pub const fn join<const N: usize>(parts: &[&str]) -> [u8; N] {
    let mut output = [0; N];
    let mut offset = 0;
    let mut part = 0;
    while part < parts.len() {
        let bytes = parts[part].as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            output[offset] = bytes[index];
            offset += 1;
            index += 1;
        }
        part += 1;
    }
    assert!(offset == N, "descriptor composition length mismatch");
    output
}

pub const fn text(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(value) => value,
        Err(_) => panic!("generated descriptor is not UTF-8"),
    }
}

const fn matches(bytes: &[u8], offset: usize, expected: &[u8]) -> bool {
    if offset + expected.len() > bytes.len() {
        return false;
    }
    let mut index = 0;
    while index < expected.len() {
        if bytes[offset + index] != expected[index] {
            return false;
        }
        index += 1;
    }
    true
}

// The input is one generated provided-capability fragment, never a configuration
// Schema. Locate its top-level field so nested extension data cannot be rewritten.
const fn admission_range(fragment: &str) -> (usize, usize) {
    let bytes = fragment.as_bytes();
    let mut index = 0;
    let mut depth = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index += 2;
                    } else if bytes[index] == b'"' {
                        break;
                    } else {
                        index += 1;
                    }
                }
                assert!(index < bytes.len(), "unterminated provided fragment string");
                if depth == 1 && index + 1 - start == KEY.len() && matches(bytes, start, KEY) {
                    index += 1;
                    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                        index += 1;
                    }
                    assert!(
                        index < bytes.len() && bytes[index] == b':',
                        "invalid admission field"
                    );
                    index += 1;
                    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                        index += 1;
                    }
                    assert!(
                        index < bytes.len() && bytes[index] == b'{',
                        "admission must be an object"
                    );
                    let value_start = index;
                    let mut value_depth = 1;
                    index += 1;
                    while index < bytes.len() {
                        if bytes[index] == b'{' {
                            value_depth += 1;
                        } else if bytes[index] == b'}' {
                            value_depth -= 1;
                            if value_depth == 0 {
                                return (value_start, index + 1);
                            }
                        } else if bytes[index] == b'"' {
                            index += 1;
                            while index < bytes.len() && bytes[index] != b'"' {
                                if bytes[index] == b'\\' {
                                    index += 1;
                                }
                                index += 1;
                            }
                        }
                        index += 1;
                    }
                    panic!("unterminated admission object");
                }
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                assert!(depth > 0, "unbalanced provided fragment");
                depth -= 1;
            }
            _ => {}
        }
        index += 1;
    }
    panic!("generated provided fragment has no default_admission");
}

pub const fn admission_len(fragment: &str, admission: &str) -> usize {
    let (start, end) = admission_range(fragment);
    fragment.len() - (end - start) + admission.len()
}

pub const fn with_admission<const N: usize>(fragment: &str, admission: &str) -> [u8; N] {
    let (start, end) = admission_range(fragment);
    let bytes = fragment.as_bytes();
    let replacement = admission.as_bytes();
    let mut output = [0; N];
    let mut offset = 0;
    while offset < start {
        output[offset] = bytes[offset];
        offset += 1;
    }
    let mut index = 0;
    while index < replacement.len() {
        output[offset] = replacement[index];
        offset += 1;
        index += 1;
    }
    index = end;
    while index < bytes.len() {
        output[offset] = bytes[index];
        offset += 1;
        index += 1;
    }
    assert!(offset == N, "admission composition length mismatch");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_preserves_identity_and_nested_data() {
        const SOURCE: &str = r#"{"nested":{"default_admission":{"queue_capacity":7,"max_concurrency":8}},"default_admission":{"queue_capacity":0,"max_concurrency":1},"capability_id":"example.endpoint@1"}"#;
        const POLICY: &str = r#"{"max_concurrency":2,"queue_capacity":16}"#;
        const BYTES: [u8; admission_len(SOURCE, POLICY)] = with_admission(SOURCE, POLICY);
        let result: serde_json::Value = serde_json::from_str(text(&BYTES)).unwrap();
        assert_eq!(result["default_admission"]["queue_capacity"], 16);
        assert_eq!(result["default_admission"]["max_concurrency"], 2);
        assert_eq!(result["nested"]["default_admission"]["max_concurrency"], 8);
        assert_eq!(result["capability_id"], "example.endpoint@1");
    }
}
