use std::collections::BTreeSet;

use itertools::Itertools;
use rustc_hash::FxHashMap;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::linter::FixTable;
use crate::registry::Diagnostic;
use crate::source_code::Locator;

pub mod helpers;

/// Auto-fix errors in a file, and write the fixed source code to disk.
pub fn fix_file(diagnostics: &[Diagnostic], locator: &Locator) -> Option<(String, FixTable)> {
    if diagnostics.iter().all(|check| check.fix.is_none()) {
        None
    } else {
        Some(apply_fixes(diagnostics.iter(), locator))
    }
}

/// Apply a series of fixes.
fn apply_fixes<'a>(
    diagnostics: impl Iterator<Item = &'a Diagnostic>,
    locator: &'a Locator<'a>,
) -> (String, FixTable) {
    let mut output = String::with_capacity(locator.len());
    let mut last_pos: Location = Location::new(1, 0);
    let mut applied: BTreeSet<&Fix> = BTreeSet::default();
    let mut fixed = FxHashMap::default();

    for (rule, fix) in diagnostics
        .filter_map(|diagnostic| {
            diagnostic
                .fix
                .as_ref()
                .map(|fix| (diagnostic.kind.rule(), fix))
        })
        .sorted_by_key(|(.., fix)| fix.location)
    {
        // If we already applied an identical fix as part of another correction, skip
        // any re-application.
        if applied.contains(&fix) {
            *fixed.entry(rule).or_default() += 1;
            continue;
        }

        // Best-effort approach: if this fix overlaps with a fix we've already applied,
        // skip it.
        if last_pos > fix.location {
            continue;
        }

        // Add all contents from `last_pos` to `fix.location`.
        let slice = locator.slice(&Range::new(last_pos, fix.location));
        output.push_str(slice);

        // Add the patch itself.
        output.push_str(&fix.content);

        // Track that the fix was applied.
        last_pos = fix.end_location;
        applied.insert(fix);
        *fixed.entry(rule).or_default() += 1;
    }

    // Add the remaining content.
    let slice = locator.skip(last_pos);
    output.push_str(slice);

    (output, fixed)
}

/// Apply a single fix.
pub(crate) fn apply_fix(fix: &Fix, locator: &Locator) -> String {
    let mut output = String::with_capacity(locator.len());

    // Add all contents from `last_pos` to `fix.location`.
    let slice = locator.slice(&Range::new(Location::new(1, 0), fix.location));
    output.push_str(slice);

    // Add the patch itself.
    output.push_str(&fix.content);

    // Add the remaining content.
    let slice = locator.skip(fix.end_location);
    output.push_str(slice);

    output
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::Location;

    use crate::autofix::{apply_fix, apply_fixes};
    use crate::fix::Fix;
    use crate::registry::Diagnostic;
    use crate::rules::pycodestyle::rules::NoNewLineAtEndOfFile;

    use crate::source_code::Locator;

    #[test]
    fn empty_file() {
        let fixes: Vec<Diagnostic> = vec![];
        let locator = Locator::new(r#""#);
        let (contents, fixed) = apply_fixes(fixes.iter(), &locator);
        assert_eq!(contents, "");
        assert_eq!(fixed.values().sum::<usize>(), 0);
    }

    impl From<Fix> for Diagnostic {
        fn from(fix: Fix) -> Self {
            Diagnostic {
                // The choice of rule here is arbitrary.
                kind: NoNewLineAtEndOfFile.into(),
                location: fix.location,
                end_location: fix.end_location,
                fix: Some(fix),
                parent: None,
            }
        }
    }

    #[test]
    fn apply_one_replacement() {
        let fixes: Vec<Diagnostic> = vec![Fix {
            content: "Bar".to_string(),
            location: Location::new(1, 8),
            end_location: Location::new(1, 14),
        }
        .into()];
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let (contents, fixed) = apply_fixes(fixes.iter(), &locator);
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
        let fixes: Vec<Diagnostic> = vec![Fix {
            content: String::new(),
            location: Location::new(1, 7),
            end_location: Location::new(1, 15),
        }
        .into()];
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let (contents, fixed) = apply_fixes(fixes.iter(), &locator);
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
        let fixes: Vec<Diagnostic> = vec![
            Fix {
                content: String::new(),
                location: Location::new(1, 7),
                end_location: Location::new(1, 16),
            }
            .into(),
            Fix {
                content: String::new(),
                location: Location::new(1, 16),
                end_location: Location::new(1, 23),
            }
            .into(),
        ];
        let locator = Locator::new(
            r#"
class A(object, object):
    ...
"#
            .trim(),
        );
        let (contents, fixed) = apply_fixes(fixes.iter(), &locator);

        assert_eq!(
            contents,
            r#"
class A:
    ...
"#
            .trim()
        );
        assert_eq!(fixed.values().sum::<usize>(), 2);
    }

    #[test]
    fn ignore_overlapping_fixes() {
        let fixes: Vec<Diagnostic> = vec![
            Fix {
                content: String::new(),
                location: Location::new(1, 7),
                end_location: Location::new(1, 15),
            }
            .into(),
            Fix {
                content: "ignored".to_string(),
                location: Location::new(1, 9),
                end_location: Location::new(1, 11),
            }
            .into(),
        ];
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let (contents, fixed) = apply_fixes(fixes.iter(), &locator);
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

    #[test]
    fn apply_single_fix() {
        let locator = Locator::new(
            r#"
class A(object):
    ...
"#
            .trim(),
        );
        let contents = apply_fix(
            &Fix {
                content: String::new(),
                location: Location::new(1, 7),
                end_location: Location::new(1, 15),
            },
            &locator,
        );
        assert_eq!(
            contents,
            r#"
class A:
    ...
"#
            .trim()
        );
    }
}
