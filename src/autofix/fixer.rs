use std::collections::BTreeSet;

use itertools::Itertools;
use rustpython_parser::ast::Location;

use crate::autofix::Fix;
use crate::autofix::Patch;
use crate::checks::Check;

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
pub fn fix_file(checks: &mut [Check], contents: &str) -> Option<String> {
    if checks.iter().all(|check| check.fix.is_none()) {
        return None;
    }

    Some(apply_fixes(
        checks.iter_mut().filter_map(|check| check.fix.as_mut()),
        contents,
    ))
}

/// Apply a series of fixes.
fn apply_fixes<'a>(fixes: impl Iterator<Item = &'a mut Fix>, contents: &str) -> String {
    let lines: Vec<&str> = contents.lines().collect();

    let mut output: String = Default::default();
    let mut last_pos: Location = Default::default();
    let mut applied: BTreeSet<&Patch> = Default::default();

    for fix in fixes.sorted_by_key(|fix| fix.patch.location) {
        // If we already applied an identical fix as part of another correction, skip any
        // re-application.
        if applied.contains(&fix.patch) {
            fix.applied = true;
            continue;
        }

        // Best-effort approach: if this fix overlaps with a fix we've already applied, skip it.
        if last_pos > fix.patch.location {
            continue;
        }

        if fix.patch.location.row() > last_pos.row() {
            if last_pos.row() > 0 || last_pos.column() > 0 {
                output.push_str(&lines[last_pos.row() - 1][last_pos.column()..]);
                output.push('\n');
            }
            for line in &lines[last_pos.row()..fix.patch.location.row() - 1] {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str(&lines[fix.patch.location.row() - 1][..fix.patch.location.column()]);
            output.push_str(&fix.patch.content);
        } else {
            output.push_str(
                &lines[last_pos.row() - 1][last_pos.column()..fix.patch.location.column()],
            );
            output.push_str(&fix.patch.content);
        }
        last_pos = fix.patch.end_location;

        applied.insert(&fix.patch);
        fix.applied = true;
    }

    if last_pos.row() > 0
        && (last_pos.row() - 1) < lines.len()
        && (last_pos.row() > 0 || last_pos.column() > 0)
    {
        output.push_str(&lines[last_pos.row() - 1][last_pos.column()..]);
        output.push('\n');
    }
    if last_pos.row() < lines.len() {
        for line in &lines[last_pos.row()..] {
            output.push_str(line);
            output.push('\n');
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::ast::Location;

    use crate::autofix::fixer::apply_fixes;
    use crate::autofix::Fix;
    use crate::autofix::Patch;

    #[test]
    fn empty_file() -> Result<()> {
        let mut fixes = vec![];
        let actual = apply_fixes(fixes.iter_mut(), "");
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
        let actual = apply_fixes(
            fixes.iter_mut(),
            "class A(object):
        ...
",
        );

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
        let actual = apply_fixes(
            fixes.iter_mut(),
            "class A(object):
        ...
",
        );

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
        let actual = apply_fixes(
            fixes.iter_mut(),
            "class A(object, object):
        ...
",
        );

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
        let actual = apply_fixes(
            fixes.iter_mut(),
            "class A(object):
    ...
",
        );

        let expected = "class A:
    ...
";

        assert_eq!(actual, expected);

        Ok(())
    }
}
