//! Infrastructure for providing "Did you mean..?" suggestions to attach to diagnostics.
//!
//! This is a Levenshtein implementation that is mainly ported from the implementation
//! CPython uses to provide suggestions in its own exception messages.
//! The tests similarly owe much to CPython's test suite.
//! Many thanks to Pablo Galindo Salgado and others for implementing the original
//! feature in CPython!

use crate::Db;
use crate::types::{Type, all_members};

use ruff_python_ast::name::Name;

/// Given a type and an unresolved member name, find the best suggestion for a member name
/// that is similar to the unresolved member name.
///
/// This function is used to provide suggestions for subdiagnostics attached to
/// `unresolved-attribute`, `unresolved-import`, and `unresolved-reference` diagnostics.
pub(crate) fn find_best_suggestion_for_unresolved_member<'db>(
    db: &'db dyn Db,
    obj: Type<'db>,
    unresolved_member: &str,
) -> Option<Name> {
    find_best_suggestion(all_members(db, obj), unresolved_member)
}

/// The cost of a Levenshtein insertion, deletion, or substitution.
///
/// This is used instead of the conventional unit cost to give these differences a higher cost than
/// casing differences, which CPython assigns a cost of 1.
const MOVE_COST: usize = 2;

fn find_best_suggestion(
    options: impl IntoIterator<Item = Name>,
    unresolved_member: &str,
) -> Option<Name> {
    if unresolved_member.is_empty() {
        return None;
    }

    let mut best_suggestion = None;
    for member in options {
        let mut max_distance = (member.len() + unresolved_member.len() + 3) * MOVE_COST / 6;
        if let Some((_, best_distance)) = best_suggestion {
            if best_distance > 0 {
                max_distance = max_distance.min(best_distance - 1);
            }
        }
        let current_distance = levenshtein(unresolved_member, &member, max_distance);
        let max_distance = (unresolved_member.len() + member.len() + 3) / 3;
        if current_distance > max_distance {
            continue;
        }
        if best_suggestion
            .as_ref()
            .is_none_or(|(_, best_score)| &current_distance < best_score)
        {
            best_suggestion = Some((member, current_distance));
        }
    }
    best_suggestion.map(|(suggestion, _)| suggestion)
}

/// Determine the "cost" of converting `string_a` to `string_b`.
fn substitution_cost(char_a: char, char_b: char) -> CharacterMatch {
    if char_a == char_b {
        return CharacterMatch::Exact;
    }

    if char_a
        .to_lowercase()
        .zip(char_b.to_lowercase())
        .all(|(a, b)| a == b)
    {
        return CharacterMatch::CaseInsensitive;
    }

    CharacterMatch::None
}

/// The result of comparing two characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharacterMatch {
    Exact,
    CaseInsensitive,
    None,
}

/// Returns the [Levenshtein edit distance] between strings `string_a` and `string_b`.
/// Uses the [Wagner-Fischer algorithm] to speed up the calculation.
///
/// [Levenshtein edit distance]: https://en.wikipedia.org/wiki/Levenshtein_distance
/// [Wagner-Fischer algorithm]: https://en.wikipedia.org/wiki/Wagner%E2%80%93Fischer_algorithm
fn levenshtein(string_a: &str, string_b: &str, max_cost: usize) -> usize {
    if string_a == string_b {
        return 0;
    }

    let string_a_chars: Vec<_> = string_a.chars().collect();
    let string_b_chars: Vec<_> = string_b.chars().collect();

    let pre = string_a_chars
        .iter()
        .zip(string_b_chars.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let string_a_chars = &string_a_chars[pre..];
    let string_b_chars = &string_b_chars[pre..];

    let post = string_a_chars
        .iter()
        .rev()
        .zip(string_b_chars.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    let mut string_a_chars = &string_a_chars[..string_a_chars.len() - post];
    let mut string_b_chars = &string_b_chars[..string_b_chars.len() - post];

    let mut string_a_len = string_a_chars.len();
    let mut string_b_len = string_b_chars.len();

    if string_a_len == 0 || string_b_len == 0 {
        return MOVE_COST * (string_a_len + string_b_len);
    }

    // Prefer a shorter buffer
    if string_b_chars.len() < string_a_chars.iter().len() {
        std::mem::swap(&mut string_a_chars, &mut string_b_chars);
        std::mem::swap(&mut string_a_len, &mut string_b_len);
    }

    if (string_b_len - string_a_len) * MOVE_COST > max_cost {
        return max_cost + 1;
    }

    let mut row = vec![0; string_a_len];
    for (i, v) in (MOVE_COST..MOVE_COST * (string_a_len + 1))
        .step_by(MOVE_COST)
        .enumerate()
    {
        row[i] = v;
    }

    let mut result = 0;

    for (b_index, b_char) in string_b_chars
        .iter()
        .copied()
        .enumerate()
        .take(string_b_len)
    {
        result = b_index * MOVE_COST;
        let mut distance = result;
        let mut minimum = usize::MAX;
        for index in 0..string_a_len {
            let substitute = distance + substitution_cost(b_char, string_a_chars[index]) as usize;
            distance = row[index];
            let insert_delete = result.min(distance) + MOVE_COST;
            result = insert_delete.min(substitute);

            row[index] = result;
            if result < minimum {
                minimum = result;
            }
        }

        if minimum > max_cost {
            return max_cost + 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    /// Given a list of candidates, this test asserts that the best suggestion
    /// for the typo `bluch` is what we'd expect.
    ///
    /// This test is ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4037-L4078>
    #[test_case(&["noise", "more_noise", "a", "bc", "bluchin"], "bluchin"; "test for additional characters")]
    #[test_case(&["noise", "more_noise", "a", "bc", "blech"], "blech"; "test for substituted characters")]
    #[test_case(&["noise", "more_noise", "a", "bc", "blch"], "blch"; "test for eliminated characters")]
    #[test_case(&["blach", "bluc"], "blach"; "substitutions are preferred over eliminations")]
    #[test_case(&["blach", "bluchi"], "blach"; "substitutions are preferred over additions")]
    #[test_case(&["blucha", "bluc"], "bluc"; "eliminations are preferred over additions")]
    #[test_case(&["Luch", "fluch", "BLuch"], "BLuch"; "case changes are preferred over additions")]
    fn test_good_suggestions(candidate_list: &[&str], expected_suggestion: &str) {
        let candidates: Vec<Name> = candidate_list.iter().copied().map(Name::from).collect();
        let suggestion = find_best_suggestion(candidates, "bluch");
        assert_eq!(suggestion.as_deref(), Some(expected_suggestion));
    }

    /// This asserts that we do not offer silly suggestions for very small names.
    /// The test is ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4108-L4120>
    #[test_case("b")]
    #[test_case("v")]
    #[test_case("m")]
    #[test_case("py")]
    fn test_bad_suggestions_do_not_trigger_for_small_names(typo: &str) {
        let candidates = ["vvv", "mom", "w", "id", "pytho"].map(Name::from);
        let suggestion = find_best_suggestion(candidates, typo);
        if let Some(suggestion) = suggestion {
            panic!("Expected no suggestions for `{typo}` but `{suggestion}` was suggested");
        }
    }

    // These tests are from the Levenshtein Wikipedia article, updated to match CPython's
    // implementation (just doubling the score to accommodate the MOVE_COST)
    #[test_case("kitten", "sitting", 6)]
    #[test_case("uninformed", "uniformed", 2)]
    #[test_case("flaw", "lawn", 4)]
    fn test_levenshtein_distance_calculation(
        string_a: &str,
        string_b: &str,
        expected_distance: usize,
    ) {
        assert_eq!(
            levenshtein(string_a, string_b, usize::MAX),
            expected_distance
        );
    }
}
