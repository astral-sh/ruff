//! Infrastructure for providing "Did you mean..?" suggestions to attach to diagnostics.
//!
//! This is a Levenshtein implementation that is mainly ported from the implementation
//! CPython uses to provide suggestions in its own exception messages.
//! The tests similarly owe much to CPython's test suite.
//! Many thanks to Pablo Galindo Salgado and others for implementing the original
//! feature in CPython!

use crate::Db;
use crate::types::{Type, all_members};

use indexmap::IndexSet;

/// Given a type and an unresolved member name, find the best suggestion for a member name
/// that is similar to the unresolved member name.
///
/// This function is used to provide suggestions for subdiagnostics attached to
/// `unresolved-attribute`, `unresolved-import`, and `unresolved-reference` diagnostics.
pub(crate) fn find_best_suggestion_for_unresolved_member<'db>(
    db: &'db dyn Db,
    obj: Type<'db>,
    unresolved_member: &str,
    hide_underscored_suggestions: HideUnderscoredSuggestions,
) -> Option<&'db str> {
    find_best_suggestion(
        all_members(db, obj)
            .iter()
            .map(ruff_python_ast::name::Name::as_str),
        unresolved_member,
        hide_underscored_suggestions,
    )
}

/// Whether to hide suggestions that start with an underscore.
///
/// If the typo itself starts with an underscore, this policy is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HideUnderscoredSuggestions {
    Yes,
    No,
}

impl HideUnderscoredSuggestions {
    const fn is_no(self) -> bool {
        matches!(self, HideUnderscoredSuggestions::No)
    }
}

fn find_best_suggestion<'db, O, I>(
    options: O,
    unresolved_member: &str,
    hide_underscored_suggestions: HideUnderscoredSuggestions,
) -> Option<&'db str>
where
    O: IntoIterator<IntoIter = I>,
    I: ExactSizeIterator<Item = &'db str>,
{
    if unresolved_member.is_empty() {
        return None;
    }

    let options = options.into_iter();

    // Don't spend a *huge* amount of time computing suggestions if there are many candidates.
    // This limit is fairly arbitrary and can be adjusted as needed.
    if options.len() > 4096 {
        return None;
    }

    // Filter out the unresolved member itself.
    // Otherwise (due to our implementation of implicit instance attributes),
    // we end up giving bogus suggestions like this:
    //
    // ```python
    // class Foo:
    //     _attribute = 42
    //     def bar(self):
    //         print(self.attribute)  # error: unresolved attribute `attribute`; did you mean `attribute`?
    // ```
    let options = options.filter(|name| *name != unresolved_member);

    let mut options: IndexSet<&'db str> =
        if hide_underscored_suggestions.is_no() || unresolved_member.starts_with('_') {
            options.collect()
        } else {
            options.filter(|name| !name.starts_with('_')).collect()
        };
    options.sort_unstable();
    find_best_suggestion_impl(options, unresolved_member)
}

fn find_best_suggestion_impl<'db>(
    options: IndexSet<&'db str>,
    unresolved_member: &str,
) -> Option<&'db str> {
    let mut best_suggestion = None;

    for member in options {
        let mut max_distance =
            (member.chars().count() + unresolved_member.chars().count() + 3) * MOVE_COST / 6;

        if let Some((_, best_distance)) = best_suggestion {
            if best_distance > 0 {
                max_distance = max_distance.min(best_distance - 1);
            }
        }

        let current_distance = levenshtein_distance(unresolved_member, member, max_distance);
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

    let char_a_lowercase = char_a.to_lowercase();
    let char_b_lowercase = char_b.to_lowercase();

    if char_a_lowercase.len() == char_b_lowercase.len()
        && char_a_lowercase.zip(char_b_lowercase).all(|(a, b)| a == b)
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

/// The cost of a Levenshtein insertion, deletion, or substitution.
/// It should be the same as `CharacterMatch::None` cast to a `usize`.
///
/// This is used instead of the conventional unit cost to give these differences a higher cost than
/// casing differences, which CPython assigns a cost of 1.
const MOVE_COST: usize = CharacterMatch::None as usize;

/// Returns the [Levenshtein edit distance] between strings `string_a` and `string_b`.
/// Uses the [Wagner-Fischer algorithm] to speed up the calculation.
///
/// [Levenshtein edit distance]: https://en.wikipedia.org/wiki/Levenshtein_distance
/// [Wagner-Fischer algorithm]: https://en.wikipedia.org/wiki/Wagner%E2%80%93Fischer_algorithm
fn levenshtein_distance(string_a: &str, string_b: &str, max_cost: usize) -> usize {
    if string_a == string_b {
        return 0;
    }

    let string_a_chars: Vec<char> = string_a.chars().collect();
    let string_b_chars: Vec<char> = string_b.chars().collect();

    // Trim away common affixes
    let pre = string_a_chars
        .iter()
        .zip(string_b_chars.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let string_a_chars = &string_a_chars[pre..];
    let string_b_chars = &string_b_chars[pre..];

    // Trim away common suffixes
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

    // Short-circuit if either string is empty after trimming affixes/suffixes
    if string_a_len == 0 || string_b_len == 0 {
        return MOVE_COST * (string_a_len + string_b_len);
    }

    // `string_a` should refer to the shorter of the two strings.
    // This enables us to create a smaller buffer in the main loop below.
    if string_b_chars.len() < string_a_chars.len() {
        std::mem::swap(&mut string_a_chars, &mut string_b_chars);
        std::mem::swap(&mut string_a_len, &mut string_b_len);
    }

    // Quick fail if a match is impossible.
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
    #[test_case(["noise", "more_noise", "a", "bc", "bluchin"], "bluchin"; "test for additional characters")]
    #[test_case(["noise", "more_noise", "a", "bc", "blech"], "blech"; "test for substituted characters")]
    #[test_case(["noise", "more_noise", "a", "bc", "blch"], "blch"; "test for eliminated characters")]
    #[test_case(["blach", "bluc"], "blach"; "substitutions are preferred over eliminations")]
    #[test_case(["blach", "bluchi"], "blach"; "substitutions are preferred over additions")]
    #[test_case(["blucha", "bluc"], "bluc"; "eliminations are preferred over additions")]
    #[test_case(["Luch", "fluch", "BLuch"], "BLuch"; "case changes are preferred over substitutions")]
    fn test_good_suggestions<const T: usize>(candidate_list: [&str; T], expected_suggestion: &str) {
        let suggestion =
            find_best_suggestion(candidate_list, "bluch", HideUnderscoredSuggestions::No);
        assert_eq!(suggestion, Some(expected_suggestion));
    }

    /// Test ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4080-L4099>
    #[test]
    fn underscored_names_not_suggested_if_hide_policy_set_to_yes() {
        let suggestion = find_best_suggestion(["bluch"], "bluch", HideUnderscoredSuggestions::Yes);
        if let Some(suggestion) = suggestion {
            panic!(
                "Expected no suggestions for `bluch` due to `HideUnderscoredSuggestions::Yes` but `{suggestion}` was suggested"
            );
        }
    }

    /// Test ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4080-L4099>
    #[test_case("_blach")]
    #[test_case("_luch")]
    fn underscored_names_are_suggested_if_hide_policy_set_to_yes_when_typo_is_underscored(
        typo: &str,
    ) {
        let suggestion = find_best_suggestion(["_bluch"], typo, HideUnderscoredSuggestions::Yes);
        assert_eq!(suggestion, Some("_bluch"));
    }

    /// Test ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4080-L4099>
    #[test_case("_luch")]
    #[test_case("_bluch")]
    fn non_underscored_names_always_suggested_even_if_typo_underscored(typo: &str) {
        let suggestion = find_best_suggestion(["bluch"], typo, HideUnderscoredSuggestions::Yes);
        assert_eq!(suggestion, Some("bluch"));
    }

    /// This asserts that we do not offer silly suggestions for very small names.
    /// The test is ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4108-L4120>
    #[test_case("b")]
    #[test_case("v")]
    #[test_case("m")]
    #[test_case("py")]
    fn test_bad_suggestions_do_not_trigger_for_small_names(typo: &str) {
        let candidates = ["vvv", "mom", "w", "id", "pytho"];
        let suggestion = find_best_suggestion(candidates, typo, HideUnderscoredSuggestions::No);
        if let Some(suggestion) = suggestion {
            panic!("Expected no suggestions for `{typo}` but `{suggestion}` was suggested");
        }
    }

    /// Test ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4101-L4106>
    #[test]
    fn test_no_suggestion_for_very_different_attribute() {
        assert_eq!(
            find_best_suggestion(
                ["blech"],
                "somethingverywrong",
                HideUnderscoredSuggestions::No
            ),
            None
        );
    }

    /// These tests are from the Levenshtein Wikipedia article, updated to match CPython's
    /// implementation (just doubling the score to accommodate the MOVE_COST)
    #[test_case("kitten", "sitting", 6)]
    #[test_case("uninformed", "uniformed", 2)]
    #[test_case("flaw", "lawn", 4)]
    fn test_levenshtein_distance_calculation_wikipedia_examples(
        string_a: &str,
        string_b: &str,
        expected_distance: usize,
    ) {
        assert_eq!(
            levenshtein_distance(string_a, string_b, usize::MAX),
            expected_distance
        );
    }

    /// Test ported from <https://github.com/python/cpython/blob/6eb6c5dbfb528bd07d77b60fd71fd05d81d45c41/Lib/test/test_traceback.py#L4670-L4697>
    #[test_case("", "", 0)]
    #[test_case("", "a", 2)]
    #[test_case("a", "A", 1)]
    #[test_case("Apple", "Aple", 2)]
    #[test_case("Banana", "B@n@n@", 6)]
    #[test_case("Cherry", "Cherry!", 2)]
    #[test_case("---0---", "------", 2)]
    #[test_case("abc", "y", 6)]
    #[test_case("aa", "bb", 4)]
    #[test_case("aaaaa", "AAAAA", 5)]
    #[test_case("wxyz", "wXyZ", 2)]
    #[test_case("wxyz", "wXyZ123", 8)]
    #[test_case("Python", "Java", 12)]
    #[test_case("Java", "C#", 8)]
    #[test_case("AbstractFoobarManager", "abstract_foobar_manager", 3+2*2)]
    #[test_case("CPython", "PyPy", 10)]
    #[test_case("CPython", "pypy", 11)]
    #[test_case("AttributeError", "AttributeErrop", 2)]
    #[test_case("AttributeError", "AttributeErrorTests", 10)]
    #[test_case("ABA", "AAB", 4)]
    fn test_levenshtein_distance_calculation_cpython_examples(
        string_a: &str,
        string_b: &str,
        expected_distance: usize,
    ) {
        assert_eq!(
            levenshtein_distance(string_a, string_b, 4044),
            expected_distance
        );
    }
}
