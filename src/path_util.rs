/// Canonical form for any user-facing hierarchical path used as a row key
/// (pages, galleries, files, menu items). Trims whitespace, strips leading and
/// trailing slashes, collapses interior duplicate slashes, and lowercases.
///
/// The whisper / `paths/children` queries assume this canonical form, so all
/// writes must go through this function to stay searchable.
pub fn normalize(raw: &str) -> String {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(trimmed.len());
    let mut prev_slash = false;
    for ch in trimmed.chars() {
        if ch == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
            out.push('/');
        } else {
            prev_slash = false;
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
        }
    }
    out
}

/// Same as [`normalize`] but normalizes a prefix used in a LIKE query: returns
/// either an empty string (root) or a value that ends with `/`.
pub fn normalize_prefix(raw: &str) -> String {
    let n = normalize(raw);
    if n.is_empty() { n } else { format!("{n}/") }
}
