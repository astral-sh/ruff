use rustpython_parser::ast::Location;

use crate::checks::{Check, CheckKind};
use crate::settings::Settings;

pub fn check_lines(contents: &str, settings: &Settings) -> Vec<Check> {
    contents
        .lines()
        .enumerate()
        .filter_map(|(row, line)| {
            if settings.select.contains(CheckKind::LineTooLong.code())
                && line.len() > settings.line_length
            {
                let chunks: Vec<&str> = line.split_whitespace().collect();
                if !(chunks.len() == 1 || (chunks.len() == 2 && chunks[0] == "#")) {
                    return Some(Check {
                        kind: CheckKind::LineTooLong,
                        location: Location::new(row + 1, settings.line_length + 1),
                    });
                }
            }

            None
        })
        .collect()
}
