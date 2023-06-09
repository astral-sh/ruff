use std::collections::BTreeSet;

use itertools::Itertools;
use nohash_hasher::IntSet;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Edit, Fix, IsolationLevel};
use ruff_python_ast::source_code::Locator;

use crate::autofix::source_map::SourceMap;
use crate::linter::FixTable;
use crate::registry::{AsRule, Rule};

pub(crate) mod codemods;
pub(crate) mod edits;
pub(crate) mod source_map;

pub(crate) struct FixResult {
    /// The resulting source code, after applying all fixes.
    pub(crate) code: String,
    /// The number of fixes applied for each [`Rule`].
    pub(crate) fixes: FixTable,
    /// Source map for the fixed source code.
    pub(crate) source_map: SourceMap,
}

/// Auto-fix errors in a file, and write the fixed source code to disk.
pub(crate) fn fix_file(diagnostics: &[Diagnostic], locator: &Locator) -> Option<FixResult> {
    let mut with_fixes = diagnostics
        .iter()
        .filter(|diag| diag.fix.is_some())
        .peekable();

    if with_fixes.peek().is_none() {
        None
    } else {
        Some(apply_fixes(with_fixes, locator))
    }
}

/// Apply a series of fixes.
fn apply_fixes<'a>(
    diagnostics: impl Iterator<Item = &'a Diagnostic>,
    locator: &'a Locator<'a>,
) -> FixResult {
    let mut output = String::with_capacity(locator.len());
    let mut last_pos: Option<TextSize> = None;
    let mut applied: BTreeSet<&Edit> = BTreeSet::default();
    let mut isolated: IntSet<u32> = IntSet::default();
    let mut fixed = FxHashMap::default();
    let mut source_map = SourceMap::default();

    for (rule, fix) in diagnostics
        .filter_map(|diagnostic| {
            diagnostic
                .fix
                .as_ref()
                .map(|fix| (diagnostic.kind.rule(), fix))
        })
        .sorted_by(|(rule1, fix1), (rule2, fix2)| cmp_fix(*rule1, *rule2, fix1, fix2))
    {
        // If we already applied an identical fix as part of another correction, skip
        // any re-application.
        if fix.edits().iter().all(|edit| applied.contains(edit)) {
            *fixed.entry(rule).or_default() += 1;
            continue;
        }

        // Best-effort approach: if this fix overlaps with a fix we've already applied,
        // skip it.
        if last_pos.map_or(false, |last_pos| {
            fix.min_start()
                .map_or(false, |fix_location| last_pos >= fix_location)
        }) {
            continue;
        }

        // If this fix requires isolation, and we've already applied another fix in the
        // same isolation group, skip it.
        if let IsolationLevel::Group(id) = fix.isolation() {
            if !isolated.insert(id) {
                continue;
            }
        }

        for edit in fix
            .edits()
            .iter()
            .sorted_unstable_by_key(|edit| edit.start())
        {
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
            applied.insert(edit);
        }

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
    fix1.min_start()
        .cmp(&fix2.min_start())
        .then_with(|| match (&rule1, &rule2) {
            // Apply `EndsInPeriod` fixes before `NewLineAfterLastParagraph` fixes.
            (Rule::EndsInPeriod, Rule::NewLineAfterLastParagraph) => std::cmp::Ordering::Less,
            (Rule::NewLineAfterLastParagraph, Rule::EndsInPeriod) => std::cmp::Ordering::Greater,
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
    use ruff_text_size::TextSize;

    use ruff_diagnostics::Diagnostic;
    use ruff_diagnostics::Edit;
    use ruff_diagnostics::Fix;
    use ruff_python_ast::source_code::Locator;

    use crate::autofix::source_map::SourceMarker;
    use crate::autofix::{apply_fixes, FixResult};
    use crate::rules::pycodestyle::rules::MissingNewlineAtEndOfFile;

    #[allow(deprecated)]
    fn create_diagnostics(edit: impl IntoIterator<Item = Edit>) -> Vec<Diagnostic> {
        edit.into_iter()
            .map(|edit| Diagnostic {
                // The choice of rule here is arbitrary.
                kind: MissingNewlineAtEndOfFile.into(),
                range: edit.range(),
                fix: Some(Fix::unspecified(edit)),
                parent: None,
            })
            .collect()
    }

    #[test]
    fn empty_file() {
        let locator = Locator::new(r#""#);
        let diagnostics = create_diagnostics([]);
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
        let diagnostics = create_diagnostics([Edit::insertion(
            "import sys\n".to_string(),
            TextSize::new(10),
        )]);
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
                SourceMarker {
                    source: 10.into(),
                    dest: 10.into(),
                },
                SourceMarker {
                    source: 10.into(),
                    dest: 21.into(),
                },
            ]
        );
    }

    #[test]
    fn apply_one_replacement() {
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let diagnostics = create_diagnostics([Edit::replacement(
            "Bar".to_string(),
            TextSize::new(8),
            TextSize::new(14),
        )]);
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r#"
class A(Bar):
    ...
"#
            .trim(),
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker {
                    source: 8.into(),
                    dest: 8.into(),
                },
                SourceMarker {
                    source: 14.into(),
                    dest: 11.into(),
                },
            ]
        );
    }

    #[test]
    fn apply_one_removal() {
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let diagnostics = create_diagnostics([Edit::deletion(TextSize::new(7), TextSize::new(15))]);
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r#"
class A:
    ...
"#
            .trim()
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker {
                    source: 7.into(),
                    dest: 7.into()
                },
                SourceMarker {
                    source: 15.into(),
                    dest: 7.into()
                }
            ]
        );
    }

    #[test]
    fn apply_two_removals() {
        let locator = Locator::new(
            r#"
class A(object, object, object):
    ...
"#
            .trim(),
        );
        let diagnostics = create_diagnostics([
            Edit::deletion(TextSize::from(8), TextSize::from(16)),
            Edit::deletion(TextSize::from(22), TextSize::from(30)),
        ]);
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);

        assert_eq!(
            code,
            r#"
class A(object):
    ...
"#
            .trim()
        );
        assert_eq!(fixes.values().sum::<usize>(), 2);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker {
                    source: 8.into(),
                    dest: 8.into()
                },
                SourceMarker {
                    source: 16.into(),
                    dest: 8.into()
                },
                SourceMarker {
                    source: 22.into(),
                    dest: 14.into(),
                },
                SourceMarker {
                    source: 30.into(),
                    dest: 14.into(),
                }
            ]
        );
    }

    #[test]
    fn ignore_overlapping_fixes() {
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let diagnostics = create_diagnostics([
            Edit::deletion(TextSize::from(7), TextSize::from(15)),
            Edit::replacement("ignored".to_string(), TextSize::from(9), TextSize::from(11)),
        ]);
        let FixResult {
            code,
            fixes,
            source_map,
        } = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            code,
            r#"
class A:
    ...
"#
            .trim(),
        );
        assert_eq!(fixes.values().sum::<usize>(), 1);
        assert_eq!(
            source_map.markers(),
            &[
                SourceMarker {
                    source: 7.into(),
                    dest: 7.into(),
                },
                SourceMarker {
                    source: 15.into(),
                    dest: 7.into(),
                }
            ]
        );
    }
}
