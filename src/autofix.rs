use std::fs;
use std::path::Path;

use anyhow::Result;
use rustpython_parser::ast::Location;

use crate::checks::Check;

// TODO(charlie): This should take Vec<Fix>.
// TODO(charlie): Add tests.
pub fn apply_fixes(checks: &mut Vec<Check>, contents: &str, path: &Path) -> Result<()> {
    if checks.iter().all(|check| check.fix.is_none()) {
        return Ok(());
    }

    let lines: Vec<&str> = contents.lines().collect();

    let mut last_pos: Location = Location::new(0, 0);
    let mut output = "".to_string();

    for check in checks {
        if let Some(fix) = &check.fix {
            if last_pos.row() > fix.start.row()
                || (last_pos.row()) == fix.start.row() && last_pos.column() > fix.start.column()
            {
                continue;
            }

            if fix.start.row() > last_pos.row() {
                if last_pos.row() > 0 || last_pos.column() > 0 {
                    output.push_str(&lines[last_pos.row() - 1][last_pos.column() - 1..]);
                    output.push('\n');
                }
                for line in &lines[last_pos.row()..fix.start.row() - 1] {
                    output.push_str(line);
                    output.push('\n');
                }
                output.push_str(&lines[fix.start.row() - 1][..fix.start.column() - 1]);
                output.push_str(&fix.content);
            } else {
                output.push_str(
                    &lines[last_pos.row() - 1][last_pos.column() - 1..fix.start.column() - 1],
                );
                output.push_str(&fix.content);
            }

            last_pos = fix.end;
            check.fixed = true;
        }
    }

    if last_pos.row() > 0 || last_pos.column() > 0 {
        output.push_str(&lines[last_pos.row() - 1][last_pos.column() - 1..]);
        output.push('\n');
    }
    for line in &lines[last_pos.row()..] {
        output.push_str(line);
        output.push('\n');
    }

    fs::write(path, output).map_err(|e| e.into())
}
