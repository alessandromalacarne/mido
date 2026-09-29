use std::collections::BTreeSet;

/// `difflib.get_close_matches` in spirit: the closest known key, or the list.
pub fn suggestion(key: &str, known: &BTreeSet<&str>) -> String {
    match closest_match(key, known) {
        Some(close) => format!("did you mean `{close}`?"),
        None => format!("known keys: {}", joined(known)),
    }
}

pub fn joined(known: &BTreeSet<&str>) -> String {
    known.iter().copied().collect::<Vec<_>>().join(", ")
}

/// The best match at or above `cutoff`, ties broken by sorted order.
pub fn closest_match(key: &str, known: &BTreeSet<&str>) -> Option<String> {
    let mut best: Option<(f64, &str)> = None;
    for candidate in known {
        let score = ratio(key, candidate);
        if score < 0.6 {
            continue;
        }
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, candidate));
        }
    }
    best.map(|(_, candidate)| candidate.to_string())
}

/// `SequenceMatcher.ratio` in the mean: `2M / T`, with the matches taken from
/// the longest common subsequence — close enough to pick the same key at the
/// 0.6 cutoff, at a fraction of the machinery.
pub fn ratio(left: &str, right: &str) -> f64 {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let matches = longest_common_subsequence(&left, &right);
    2.0 * matches as f64 / (left.len() + right.len()) as f64
}

fn longest_common_subsequence(left: &[char], right: &[char]) -> usize {
    let mut previous = vec![0usize; right.len() + 1];
    let mut current = vec![0usize; right.len() + 1];

    for left_item in left {
        for (index, right_item) in right.iter().enumerate() {
            current[index + 1] = if left_item == right_item {
                previous[index] + 1
            } else {
                previous[index + 1].max(current[index])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn known() -> BTreeSet<&'static str> {
        ["mi_min", "cognitive_max", "min_mi"].into_iter().collect()
    }

    #[test]
    fn a_near_miss_key_is_named_back() {
        assert_eq!(
            suggestion("min_mi", &["mi_min", "cognitive_max"].into_iter().collect()),
            "did you mean `mi_min`?"
        );
    }

    #[test]
    fn a_far_key_lists_what_is_known() {
        let suggestion = suggestion("totally_unrelated_option", &known());

        assert!(suggestion.starts_with("known keys: "));
        assert!(suggestion.contains("mi_min"));
    }

    #[test]
    fn identical_strings_score_one() {
        assert_eq!(ratio("mi_min", "mi_min"), 1.0);
        assert_eq!(ratio("", ""), 1.0);
        assert_eq!(ratio("abc", "xyz"), 0.0);
    }

    #[test]
    fn ratio_is_the_difflib_mean() {
        // "mi_min" vs "min_mi": LCS "mi_mi" is 5 of 12 characters.
        assert!((ratio("mi_min", "min_mi") - 10.0 / 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_best_match_wins_over_a_merely_close_one() {
        let known: BTreeSet<&str> = ["tests", "test", "lint"].into_iter().collect();

        assert_eq!(closest_match("test", &known).as_deref(), Some("test"));
    }
}
