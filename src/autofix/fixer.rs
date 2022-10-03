use std::fs;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
use rustpython_parser::ast::Location;

use crate::checks::{Check, Fix};

#[derive(Hash)]
pub enum Mode {
    Generate,
    Apply,
    None,
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
pub fn fix_file(checks: &mut [Check], contents: &str, path: &Path) -> Result<()> {
    if checks.iter().all(|check| check.fix.is_none()) {
        return Ok(());
    }

    let output = apply_fixes(
        checks.iter_mut().filter_map(|check| check.fix.as_mut()),
        contents,
    );

    fs::write(path, output).map_err(|e| e.into())
}

/// Apply a series of fixes.
fn apply_fixes<'a>(fixes: impl Iterator<Item = &'a mut Fix>, contents: &str) -> String {
    let lines: Vec<&str> = contents.lines().collect();

    let mut output = "".to_string();
    let mut last_pos: Location = Location::new(0, 0);

    for fix in fixes.sorted_by_key(|fix| fix.location) {
        // Best-effort approach: if this fix overlaps with a fix we've already applied, skip it.
        if last_pos > fix.location {
            continue;
        }

        if fix.location.row() > last_pos.row() {
            if last_pos.row() > 0 || last_pos.column() > 0 {
                output.push_str(&lines[last_pos.row() - 1][last_pos.column() - 1..]);
                output.push('\n');
            }
            for line in &lines[last_pos.row()..fix.location.row() - 1] {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str(&lines[fix.location.row() - 1][..fix.location.column() - 1]);
            output.push_str(&fix.content);
        } else {
            output.push_str(
                &lines[last_pos.row() - 1][last_pos.column() - 1..fix.location.column() - 1],
            );
            output.push_str(&fix.content);
        }

        last_pos = fix.end_location;
        fix.applied = true;
    }

    if last_pos.row() > 0
        && (last_pos.row() - 1) < lines.len()
        && (last_pos.row() > 0 || last_pos.column() > 0)
    {
        output.push_str(&lines[last_pos.row() - 1][last_pos.column() - 1..]);
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
    use crate::checks::Fix;

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
            content: "Bar".to_string(),
            location: Location::new(1, 9),
            end_location: Location::new(1, 15),
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
            content: "".to_string(),
            location: Location::new(1, 8),
            end_location: Location::new(1, 16),
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
                content: "".to_string(),
                location: Location::new(1, 8),
                end_location: Location::new(1, 17),
                applied: false,
            },
            Fix {
                content: "".to_string(),
                location: Location::new(1, 17),
                end_location: Location::new(1, 24),
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
                content: "".to_string(),
                location: Location::new(1, 8),
                end_location: Location::new(1, 16),
                applied: false,
            },
            Fix {
                content: "ignored".to_string(),
                location: Location::new(1, 10),
                end_location: Location::new(1, 12),
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
