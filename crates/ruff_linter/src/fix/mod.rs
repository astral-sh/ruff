use std::collections::BTreeSet;

use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Edit, Fix, IsolationLevel, SourceMap};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Locator;
use crate::linter::FixTable;
use crate::message::Message;
use crate::registry::Rule;
use crate::settings::types::UnsafeFixes;

pub(crate) mod codemods;
pub(crate) mod edits;
pub(crate) mod snippet;

pub(crate) struct FixResult {
    /// The resulting source code, after applying all fixes.
    pub(crate) code: String,
    /// The number of fixes applied for each [`Rule`].
    pub(crate) fixes: FixTable,
    /// Source map for the fixed source code.
    pub(crate) source_map: SourceMap,
}

/// Fix errors in a file, and write the fixed source code to disk.
pub(crate) fn fix_file(
    messages: &[Message],
    locator: &Locator,
    unsafe_fixes: UnsafeFixes,
) -> Option<FixResult> {
    let required_applicability = unsafe_fixes.required_applicability();

    let mut with_fixes = messages
        .iter()
        .filter(|message| {
            message
                .fix()
                .is_some_and(|fix| fix.applies(required_applicability))
        })
        .peekable();

    if with_fixes.peek().is_none() {
        None
    } else {
        Some(apply_fixes(with_fixes, locator))
    }
}

/// Apply a series of fixes.
fn apply_fixes<'a>(
    diagnostics: impl Iterator<Item = &'a Message>,
    locator: &'a Locator<'a>,
) -> FixResult {
    let mut output = String::with_capacity(locator.len());
    let mut last_pos: Option<TextSize> = None;
    let mut applied: BTreeSet<&Edit> = BTreeSet::default();
    let mut isolated: FxHashSet<u32> = FxHashSet::default();
    let mut fixed = FxHashMap::default();
    let mut source_map = SourceMap::default();

    for (rule, fix) in diagnostics
        .filter_map(|msg| msg.to_rule().map(|rule| (rule, msg)))
        .filter_map(|(rule, diagnostic)| diagnostic.fix().map(|fix| (rule, fix)))
        .sorted_by(|(rule1, fix1), (rule2, fix2)| cmp_fix(*rule1, *rule2, fix1, fix2))
    {
        let mut edits = fix
            .edits()
            .iter()
            .filter(|edit| !applied.contains(edit))
            .peekable();

        // If the fix contains at least one new edit, enforce isolation and positional requirements.
        if let Some(first) = edits.peek() {
            // If this fix requires isolation, and we've already applied another fix in the
            // same isolation group, skip it.
            if let IsolationLevel::Group(id) = fix.isolation() {
                if !isolated.insert(id) {
                    continue;
                }
            }

            // If this fix overlaps with a fix we've already applied, skip it.
            if last_pos.is_some_and(|last_pos| last_pos >= first.start()) {
                continue;
            }
        }

        let mut applied_edits = Vec::with_capacity(fix.edits().len());
        for edit in edits {
            // Add all contents from `last_pos` to `fix.location`.
            let slice = locator.slice(TextRange::new(last_pos.unwrap_or_default(), edit.start()));
            output.push_str(slice);

            // Add the start source marker for the patch.
            source_map.push_start_marker(edit, output.text_len());

            // Add the patch itself.
            output.push_str(edit.content().unwrap_or_default());

            // Add the end source marker for the added patch.
            source_map.push_end_marker(edit, output.text_len());

            // Track that the edit was applied.
            last_pos = Some(edit.end());
            applied_edits.push(edit);
        }

        applied.extend(applied_edits.drain(..));
        *fixed.entry(rule).or_default() += 1;
    }

    // Add the remaining content.
    let slice = locator.after(last_pos.unwrap_or_default());
    output.push_str(slice);

    FixResult {
        code: output,
        fixes: fixed,
        source_map,
    }
}

/// Compare two fixes.
fn cmp_fix(rule1: Rule, rule2: Rule, fix1: &Fix, fix2: &Fix) -> std::cmp::Ordering {
    // Always apply `RedefinedWhileUnused` before `UnusedImport`, as the latter can end up fixing
    // the former. But we can't apply this just for `RedefinedWhileUnused` and `UnusedImport` because it violates
    // `< is transitive: a < b and b < c implies a < c. The same must hold for both == and >.`
    // See https://github.com/astral-sh/ruff/issues/12469#issuecomment-2244392085
    match (rule1, rule2) {
        (Rule::RedefinedWhileUnused, Rule::RedefinedWhileUnused) => std::cmp::Ordering::Equal,
        (Rule::RedefinedWhileUnused, _) => std::cmp::Ordering::Less,
        (_, Rule::RedefinedWhileUnused) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
    // Apply fixes in order of their start position.
    .then_with(|| fix1.min_start().cmp(&fix2.min_start()))
    // Break ties in the event of overlapping rules, for some specific combinations.
    .then_with(|| match (&rule1, &rule2) {
        // Apply `MissingTrailingPeriod` fixes before `NewLineAfterLastParagraph` fixes.
        (Rule::MissingTrailingPeriod, Rule::NewLineAfterLastParagraph) => std::cmp::Ordering::Less,
        (Rule::NewLineAfterLastParagraph, Rule::MissingTrailingPeriod) => {
            std::cmp::Ordering::Greater
        }
        // Apply `IfElseBlockInsteadOfDictGet` fixes before `IfElseBlockInsteadOfIfExp` fixes.
        (Rule::IfElseBlockInsteadOfDictGet, Rule::IfElseBlockInsteadOfIfExp) => {
            std::cmp::Ordering::Less
        }
        (Rule::IfElseBlockInsteadOfIfExp, Rule::IfElseBlockInsteadOfDictGet) => {
            std::cmp::Ordering::Greater
        }
        _ => std::cmp::Ordering::Equal,
    })
}

#[cfg(test)]
mod tests {
    use ruff_diagnostics::{Diagnostic, Edit, Fix, SourceMarker};
    use ruff_source_file::SourceFileBuilder;
    use ruff_text_size::{Ranged, TextSize};

    use crate::Locator;
    use crate::fix::{FixResult, apply_fixes};
    use crate::message::Message;
    use crate::rules::pycodestyle::rules::MissingNewlineAtEndOfFile;

    fn create_diagnostics(
        filename: &str,
        source: &str,
        edit: impl IntoIterator<Item = Edit>,
    ) -> Vec<Message> {
        edit.into_iter()
            .map(|edit| {
                // The choice of rule here is arbitrary.
                let diagnostic = Diagnostic::new(MissingNewlineAtEndOfFile, edit.range());
                Message::from_diagnostic(
                    diagnostic.with_fix(Fix::safe_edit(edit)),
                    SourceFileBuilder::new(filename, source).finish(),
                    None,
                )
            })
            .collect()
    }

    #[test]
    fn empty_file() {
        let locator = Locator::new(r"");
        let diagnostics = create_diagnostics("<filename>", locator.contents(), []);
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(code, "");
        assert_eq!(fixes.values().sum::<usize>(), 0);
        assert!(source_map.markers().is_empty());
    }

    #[test]
    fn apply_one_insertion() {
        let locator = Locator::new(
            r#"
import os

print("hello world")
"#
            .trim(),
        );
        let diagnostics = create_diagnostics(
            "<filename>",
            locator.contents(),
            [Edit::insertion(
                "import sys\n".to_string(),
                TextSize::new(10),
            )],
        );
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r#"
import os
import sys

print("hello world")
"#
            .trim()
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker::new(10.into(), 10.into()),
                SourceMarker::new(10.into(), 21.into()),
            ]
        );
    }

    #[test]
    fn apply_one_replacement() {
        let locator = Locator::new(
            r"
class A(object):
    ...
"
            .trim(),
        );
        let diagnostics = create_diagnostics(
            "<filename>",
            locator.contents(),
            [Edit::replacement(
                "Bar".to_string(),
                TextSize::new(8),
                TextSize::new(14),
            )],
        );
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r"
class A(Bar):
    ...
"
            .trim(),
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker::new(8.into(), 8.into()),
                SourceMarker::new(14.into(), 11.into()),
            ]
        );
    }

    #[test]
    fn apply_one_removal() {
        let locator = Locator::new(
            r"
class A(object):
    ...
"
            .trim(),
        );
        let diagnostics = create_diagnostics(
            "<filename>",
            locator.contents(),
            [Edit::deletion(TextSize::new(7), TextSize::new(15))],
        );
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r"
class A:
    ...
"
            .trim()
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker::new(7.into(), 7.into()),
                SourceMarker::new(15.into(), 7.into()),
            ]
        );
    }

    #[test]
    fn apply_two_removals() {
        let locator = Locator::new(
            r"
class A(object, object, object):
    ...
"
            .trim(),
        );
        let diagnostics = create_diagnostics(
            "<filename>",
            locator.contents(),
            [
                Edit::deletion(TextSize::from(8), TextSize::from(16)),
                Edit::deletion(TextSize::from(22), TextSize::from(30)),
            ],
        );
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);

        assert_eq!(
            code,
            r"
class A(object):
    ...
"
            .trim()
        );
        assert_eq!(fixes.values().sum::<usize>(), 2);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker::new(8.into(), 8.into()),
                SourceMarker::new(16.into(), 8.into()),
                SourceMarker::new(22.into(), 14.into()),
                SourceMarker::new(30.into(), 14.into()),
            ]
        );
    }

    #[test]
    fn ignore_overlapping_fixes() {
        let locator = Locator::new(
            r"
class A(object):
    ...
"
            .trim(),
        );
        let diagnostics = create_diagnostics(
            "<filename>",
            locator.contents(),
            [
                Edit::deletion(TextSize::from(7), TextSize::from(15)),
                Edit::replacement("ignored".to_string(), TextSize::from(9), TextSize::from(11)),
            ],
        );
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r"
class A:
    ...
"
            .trim(),
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker::new(7.into(), 7.into()),
                SourceMarker::new(15.into(), 7.into()),
            ]
        );
    }
}
