//! Bounded case-insensitive filename fallback matching.

pub(super) fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.to_lowercase().chars().collect::<Vec<_>>();
    let name = name.to_lowercase().chars().collect::<Vec<_>>();
    let (mut p, mut n, mut star, mut retry) = (0, 0, None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            retry += 1;
            n = retry;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|character| *character == '*')
}
