use std::path::PathBuf;

/// Minimal glob: supports `*` (any chars in segment) and `**` (any path segment)
pub fn glob_match(pattern: &str, path: &PathBuf) -> bool {
    let path_str = path.to_string_lossy();
    glob_match_str(pattern, &path_str)
}

fn glob_match_str(pattern: &str, s: &str) -> bool {
    // Strip leading "./"
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let s = s.strip_prefix("./").unwrap_or(s);

    let pat_parts: Vec<&str> = pattern.split('/').collect();
    let str_parts: Vec<&str> = s.split('/').collect();
    glob_parts(&pat_parts, &str_parts)
}

fn glob_parts(pat: &[&str], s: &[&str]) -> bool {
    match (pat.first(), s.first()) {
        (None, None) => true,
        (Some(&"**"), _) => {
            // ** matches zero or more segments
            for i in 0..=s.len() {
                if glob_parts(&pat[1..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        (Some(p), Some(seg)) if segment_match(p, seg) => glob_parts(&pat[1..], &s[1..]),
        _ => false,
    }
}

fn segment_match(pattern: &str, s: &str) -> bool {
    // Simple * wildcard within a single path segment
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == s;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut remaining = s;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == parts.len() - 1 {
            return remaining.ends_with(part);
        } else {
            if let Some(pos) = remaining.find(part) {
                remaining = &remaining[pos + part.len()..];
            } else {
                return false;
            }
        }
    }
    true
}
