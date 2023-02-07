use std::collections::BTreeSet;

use itertools::Itertools;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Locator;

pub mod helpers;

/// Auto-fix errors in a file, and write the fixed source code to disk.
pub fn fix_file(diagnostics: &[Diagnostic], locator: &Locator) -> Option<(String, usize)> {
    if diagnostics.iter().all(|check| check.fix.is_none()) {
        return None;
    }

    Some(apply_fixes(
        diagnostics.iter().filter_map(|check| check.fix.as_ref()),
        locator,
    ))
}

/// Apply a series of fixes.
fn apply_fixes<'a>(
    fixes: impl Iterator<Item = &'a Fix>,
    locator: &'a Locator<'a>,
) -> (String, usize) {
    let mut output = String::with_capacity(locator.len());
    let mut last_pos: Location = Location::new(1, 0);
    let mut applied: BTreeSet<&Fix> = BTreeSet::default();
    let mut num_fixed: usize = 0;

    for fix in fixes.sorted_by_key(|fix| fix.location) {
        // If we already applied an identical fix as part of another correction, skip
        // any re-application.
        if applied.contains(&fix) {
            num_fixed += 1;
            continue;
        }

        // Best-effort approach: if this fix overlaps with a fix we've already applied,
        // skip it.
        if last_pos > fix.location {
            continue;
        }

        // Add all contents from `last_pos` to `fix.location`.
        let slice = locator.slice_source_code_range(&Range::new(last_pos, fix.location));
        output.push_str(slice);

        // Add the patch itself.
        output.push_str(&fix.content);

        // Track that the fix was applied.
        last_pos = fix.end_location;
        applied.insert(fix);
        num_fixed += 1;
    }

    // Add the remaining content.
    let slice = locator.slice_source_code_at(last_pos);
    output.push_str(slice);

    (output, num_fixed)
}

/// Apply a single fix.
pub(crate) fn apply_fix(fix: &Fix, locator: &Locator) -> String {
    let mut output = String::with_capacity(locator.len());

    // Add all contents from `last_pos` to `fix.location`.
    let slice = locator.slice_source_code_range(&Range::new(Location::new(1, 0), fix.location));
    output.push_str(slice);

    // Add the patch itself.
    output.push_str(&fix.content);

    // Add the remaining content.
    let slice = locator.slice_source_code_at(fix.end_location);
    output.push_str(slice);

    output
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::Location;

    use crate::autofix::{apply_fix, apply_fixes};
    use crate::fix::Fix;
    use crate::source_code::Locator;

    #[test]
    fn empty_file() {
        let fixes = vec![];
        let locator = Locator::new(r#""#);
        let (contents, fixed) = apply_fixes(fixes.iter(), &locator);
        assert_eq!(contents, "");
        assert_eq!(fixed, 0);
    }

    #[test]
    fn apply_one_replacement() {
        let fixes = vec![Fix {
            content: "Bar".to_string(),
            location: Location::new(1, 8),
            end_location: Location::new(1, 14),
        }];
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
        assert_eq!(fixed, 1);
    }

    #[test]
    fn apply_one_removal() {
        let fixes = vec![Fix {
            content: String::new(),
            location: Location::new(1, 7),
            end_location: Location::new(1, 15),
        }];
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
        assert_eq!(fixed, 1);
    }

    #[test]
    fn apply_two_removals() {
        let fixes = vec![
            Fix {
                content: String::new(),
                location: Location::new(1, 7),
                end_location: Location::new(1, 16),
            },
            Fix {
                content: String::new(),
                location: Location::new(1, 16),
                end_location: Location::new(1, 23),
            },
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
        assert_eq!(fixed, 2);
    }

    #[test]
    fn ignore_overlapping_fixes() {
        let fixes = vec![
            Fix {
                content: String::new(),
                location: Location::new(1, 7),
                end_location: Location::new(1, 15),
            },
            Fix {
                content: "ignored".to_string(),
                location: Location::new(1, 9),
                end_location: Location::new(1, 11),
            },
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
        assert_eq!(fixed, 1);
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
