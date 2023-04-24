use std::collections::BTreeSet;

use itertools::Itertools;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Location;

use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::types::Range;

use crate::linter::FixTable;
use crate::registry::{AsRule, Rule};

pub mod actions;

/// Auto-fix errors in a file, and write the fixed source code to disk.
pub fn fix_file(diagnostics: &[Diagnostic], locator: &Locator) -> Option<(String, FixTable)> {
    let mut with_fixes = diagnostics
        .iter()
        .filter(|diag| !diag.fix.is_empty())
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
) -> (String, FixTable) {
    let mut output = String::with_capacity(locator.len());
    let mut last_pos: Option<Location> = None;
    let mut applied: BTreeSet<&Edit> = BTreeSet::default();
    let mut fixed = FxHashMap::default();

    for (rule, fix) in diagnostics
        .filter_map(|diagnostic| {
            if diagnostic.fix.is_empty() {
                None
            } else {
                Some((diagnostic.kind.rule(), &diagnostic.fix))
            }
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
            fix.min_location()
                .map_or(false, |fix_location| last_pos >= fix_location)
        }) {
            continue;
        }

        for edit in fix.edits() {
            // Add all contents from `last_pos` to `fix.location`.
            let slice = locator.slice(Range::new(last_pos.unwrap_or_default(), edit.location()));
            output.push_str(slice);

            // Add the patch itself.
            output.push_str(edit.content().unwrap_or_default());

            // Track that the edit was applied.
            last_pos = Some(edit.end_location());
            applied.insert(edit);
        }

        *fixed.entry(rule).or_default() += 1;
    }

    // Add the remaining content.
    let slice = locator.after(last_pos.unwrap_or_default());
    output.push_str(slice);

    (output, fixed)
}

/// Compare two fixes.
fn cmp_fix(rule1: Rule, rule2: Rule, fix1: &Fix, fix2: &Fix) -> std::cmp::Ordering {
    fix1.min_location()
        .cmp(&fix2.min_location())
        .then_with(|| match (&rule1, &rule2) {
            // Apply `EndsInPeriod` fixes before `NewLineAfterLastParagraph` fixes.
            (Rule::EndsInPeriod, Rule::NewLineAfterLastParagraph) => std::cmp::Ordering::Less,
            (Rule::NewLineAfterLastParagraph, Rule::EndsInPeriod) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        })
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::Location;

    use ruff_diagnostics::Diagnostic;
    use ruff_diagnostics::Edit;
    use ruff_python_ast::source_code::Locator;

    use crate::autofix::apply_fixes;
    use crate::rules::pycodestyle::rules::MissingNewlineAtEndOfFile;

    fn create_diagnostics(edit: impl IntoIterator<Item = Edit>) -> Vec<Diagnostic> {
        edit.into_iter()
            .map(|edit| Diagnostic {
                // The choice of rule here is arbitrary.
                kind: MissingNewlineAtEndOfFile.into(),
                location: edit.location(),
                end_location: edit.end_location(),
                fix: edit.into(),
                parent: None,
            })
            .collect()
    }

    #[test]
    fn empty_file() {
        let locator = Locator::new(r#""#);
        let diagnostics = create_diagnostics([]);
        let (contents, fixed) = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(contents, "");
        assert_eq!(fixed.values().sum::<usize>(), 0);
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
            Location::new(1, 8),
            Location::new(1, 14),
        )]);
        let (contents, fixed) = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            contents,
            r#"
class A(Bar):
    ...
"#
            .trim(),
        );
        assert_eq!(fixed.values().sum::<usize>(), 1);
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
        let diagnostics =
            create_diagnostics([Edit::deletion(Location::new(1, 7), Location::new(1, 15))]);
        let (contents, fixed) = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            contents,
            r#"
class A:
    ...
"#
            .trim()
        );
        assert_eq!(fixed.values().sum::<usize>(), 1);
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
            Edit::deletion(Location::new(1, 8), Location::new(1, 16)),
            Edit::deletion(Location::new(1, 22), Location::new(1, 30)),
        ]);
        let (contents, fixed) = apply_fixes(diagnostics.iter(), &locator);

        assert_eq!(
            contents,
            r#"
class A(object):
    ...
"#
            .trim()
        );
        assert_eq!(fixed.values().sum::<usize>(), 2);
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
            Edit::deletion(Location::new(1, 7), Location::new(1, 15)),
            Edit::replacement(
                "ignored".to_string(),
                Location::new(1, 9),
                Location::new(1, 11),
            ),
        ]);
        let (contents, fixed) = apply_fixes(diagnostics.iter(), &locator);
        assert_eq!(
            contents,
            r#"
class A:
    ...
"#
            .trim(),
        );
        assert_eq!(fixed.values().sum::<usize>(), 1);
    }
}
