use std::borrow::Cow;
use std::collections::BTreeSet;

use itertools::Itertools;
use ropey::RopeBuilder;
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::autofix::{Fix, Patch};
use crate::checks::Check;
use crate::source_code_locator::SourceCodeLocator;

// TODO(charlie): The model here is awkward because `Apply` is only relevant at
// higher levels in the execution flow.
#[derive(Hash)]
pub enum Mode {
    Generate,
    Apply,
    None,
}

impl Mode {
    /// Return `true` if a patch should be generated under the given `Mode`.
    pub fn patch(&self) -> bool {
        match &self {
            Mode::Generate => true,
            Mode::Apply => true,
            Mode::None => false,
        }
    }
}

impl From<bool> for Mode {
    fn from(value: bool) -> Self {
        match value {
            true => Mode::Apply,
            false => Mode::None,
        }
    }
}

/// Auto-fix errors in a file, and write the fixed source code to disk.
pub fn fix_file<'a>(
    checks: &'a mut [Check],
    locator: &'a SourceCodeLocator<'a>,
) -> Option<Cow<'a, str>> {
    if checks.iter().all(|check| check.fix.is_none()) {
        return None;
    }

    Some(apply_fixes(
        checks.iter_mut().filter_map(|check| check.fix.as_mut()),
        locator,
    ))
}

/// Apply a series of fixes.
fn apply_fixes<'a>(
    fixes: impl Iterator<Item = &'a mut Fix>,
    locator: &'a SourceCodeLocator<'a>,
) -> Cow<'a, str> {
    let mut output = RopeBuilder::new();
    let mut last_pos: Location = Location::new(1, 0);
    let mut applied: BTreeSet<&Patch> = BTreeSet::default();

    for fix in fixes.sorted_by_key(|fix| fix.patch.location) {
        // If we already applied an identical fix as part of another correction, skip
        // any re-application.
        if applied.contains(&fix.patch) {
            fix.applied = true;
            continue;
        }

        // Best-effort approach: if this fix overlaps with a fix we've already applied,
        // skip it.
        if last_pos > fix.patch.location {
            continue;
        }

        // Add all contents from `last_pos` to `fix.patch.location`.
        let slice = locator.slice_source_code_range(&Range {
            location: last_pos,
            end_location: fix.patch.location,
        });
        output.append(&slice);

        // Add the patch itself.
        output.append(&fix.patch.content);

        // Track that the fix was applied.
        last_pos = fix.patch.end_location;
        applied.insert(&fix.patch);
        fix.applied = true;
    }

    // Add the remaining content.
    let slice = locator.slice_source_code_at(&last_pos);
    output.append(&slice);

    Cow::from(output.finish())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::ast::Location;

    use crate::autofix::fixer::apply_fixes;
    use crate::autofix::{Fix, Patch};
    use crate::SourceCodeLocator;

    #[test]
    fn empty_file() -> Result<()> {
        let mut fixes = vec![];
        let locator = SourceCodeLocator::new("");
        let actual = apply_fixes(fixes.iter_mut(), &locator);
        let expected = "";

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn apply_single_replacement() -> Result<()> {
        let mut fixes = vec![Fix {
            patch: Patch {
                content: "Bar".to_string(),
                location: Location::new(1, 8),
                end_location: Location::new(1, 14),
            },
            applied: false,
        }];
        let locator = SourceCodeLocator::new(
            "class A(object):
        ...
",
        );
        let actual = apply_fixes(fixes.iter_mut(), &locator);

        let expected = "class A(Bar):
        ...
";

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn apply_single_removal() -> Result<()> {
        let mut fixes = vec![Fix {
            patch: Patch {
                content: "".to_string(),
                location: Location::new(1, 7),
                end_location: Location::new(1, 15),
            },
            applied: false,
        }];
        let locator = SourceCodeLocator::new(
            "class A(object):
        ...
",
        );
        let actual = apply_fixes(fixes.iter_mut(), &locator);

        let expected = "class A:
        ...
";

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn apply_double_removal() -> Result<()> {
        let mut fixes = vec![
            Fix {
                patch: Patch {
                    content: "".to_string(),
                    location: Location::new(1, 7),
                    end_location: Location::new(1, 16),
                },
                applied: false,
            },
            Fix {
                patch: Patch {
                    content: "".to_string(),
                    location: Location::new(1, 16),
                    end_location: Location::new(1, 23),
                },
                applied: false,
            },
        ];
        let locator = SourceCodeLocator::new(
            "class A(object, object):
        ...
",
        );
        let actual = apply_fixes(fixes.iter_mut(), &locator);

        let expected = "class A:
        ...
";

        assert_eq!(actual, expected);

        Ok(())
    }

    #[test]
    fn ignore_overlapping_fixes() -> Result<()> {
        let mut fixes = vec![
            Fix {
                patch: Patch {
                    content: "".to_string(),
                    location: Location::new(1, 7),
                    end_location: Location::new(1, 15),
                },
                applied: false,
            },
            Fix {
                patch: Patch {
                    content: "ignored".to_string(),
                    location: Location::new(1, 9),
                    end_location: Location::new(1, 11),
                },
                applied: false,
            },
        ];
        let locator = SourceCodeLocator::new(
            "class A(object):
    ...
",
        );
        let actual = apply_fixes(fixes.iter_mut(), &locator);

        let expected = "class A:
    ...
";

        assert_eq!(actual, expected);

        Ok(())
    }
}
