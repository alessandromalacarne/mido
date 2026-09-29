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
