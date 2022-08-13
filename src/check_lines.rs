use rustpython_parser::ast::Location;

use crate::checks::Check;
use crate::checks::CheckKind::LineTooLong;

pub fn check_lines(contents: &str) -> Vec<Check> {
    contents
        .lines()
        .enumerate()
        .filter_map(|(row, line)| {
            if line.len() > 79 {
                Some(Check {
                    kind: LineTooLong,
                    location: Location::new(row + 1, 79 + 1),
                })
            } else {
                None
            }
        })
        .collect()
}
