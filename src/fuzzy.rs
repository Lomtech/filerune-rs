//! Portierung von FuzzyMatch.swift — identische Bonusstruktur, damit die
//! Trefferreihenfolge exakt der SwiftUI-Fassung entspricht.

/// Subsequenz-Match mit Positionsboni. `None`, wenn nicht alle Query-Zeichen
/// der Reihe nach vorkommen.
pub fn score(query: &str, target: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.to_lowercase().chars().collect();
    let t: Vec<char> = target.to_lowercase().chars().collect();
    if q.is_empty() || t.is_empty() {
        return None;
    }

    const SEPARATORS: [char; 5] = ['-', '_', '.', ' ', '/'];

    let mut ti = 0usize;
    let mut qi = 0usize;
    let mut score = 0i32;
    let mut prev_match: i64 = -2;

    while qi < q.len() && ti < t.len() {
        if q[qi] == t[ti] {
            let mut bonus = 1;
            if ti == 0 {
                bonus += 30;
            }
            if ti > 0 && SEPARATORS.contains(&t[ti - 1]) {
                bonus += 20;
            }
            if ti as i64 == prev_match + 1 {
                bonus += 10;
            }
            score += bonus;
            prev_match = ti as i64;
            qi += 1;
        }
        ti += 1;
    }
    if qi != q.len() {
        return None;
    }
    // Längere Namen leicht abwerten, damit der kurze exakte Treffer gewinnt.
    Some(score - (target.chars().count() as i32) / 20)
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn prefix_beats_mid_string() {
        assert!(score("cargo", "Cargo.toml").unwrap() > score("cargo", "my-cargo-notes").unwrap());
    }

    #[test]
    fn separator_start_gets_bonus() {
        // "toml" nach dem Punkt bekommt den Separator-Bonus.
        assert!(score("toml", "Cargo.toml").unwrap() > score("toml", "atomlike").unwrap());
    }

    #[test]
    fn non_subsequence_is_none() {
        assert!(score("zzz", "Cargo.toml").is_none());
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "irgendwas"), Some(0));
    }

    #[test]
    fn is_case_insensitive() {
        assert!(score("CARGO", "cargo.toml").is_some());
    }
}
