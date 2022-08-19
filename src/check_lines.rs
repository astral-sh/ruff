use rustpython_parser::ast::Location;

use crate::checks::Check;
use crate::checks::CheckKind::LineTooLong;
use crate::settings::MAX_LINE_LENGTH;

pub fn check_lines(contents: &str) -> Vec<Check> {
    contents
        .lines()
        .enumerate()
        .filter_map(|(row, line)| {
            if line.len() > *MAX_LINE_LENGTH {
                let chunks: Vec<&str> = line.split_whitespace().collect();
                if chunks.len() == 1 || (chunks.len() == 2 && chunks[0] == "#") {
                    None
                } else {
                    Some(Check {
                        kind: LineTooLong,
                        location: Location::new(row + 1, MAX_LINE_LENGTH + 1),
                    })
                }
            } else {
                None
            }
        })
        .collect()
}
