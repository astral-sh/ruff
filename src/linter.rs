use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::parser;

use crate::check_ast::check_ast;
use crate::check_lines::check_lines;
use crate::checks::{Check, CheckKind, LintSource};
use crate::message::Message;
use crate::settings::Settings;
use crate::{cache, fs};

pub fn check_path(path: &Path, settings: &Settings, mode: &cache::Mode) -> Result<Vec<Message>> {
    // Check the cache.
    if let Some(messages) = cache::get(path, settings, mode) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(messages);
    }

    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Aggregate all checks.
    let mut checks: Vec<Check> = vec![];

    // Run the AST-based checks.
    if settings
        .select
        .iter()
        .any(|check_code| matches!(CheckKind::new(check_code).lint_source(), LintSource::AST))
    {
        let python_ast = parser::parse_program(&contents)?;
        checks.extend(check_ast(&python_ast, settings));
    }

    // Run the lines-based checks.
    if settings
        .select
        .iter()
        .any(|check_code| matches!(CheckKind::new(check_code).lint_source(), LintSource::Lines))
    {
        checks.extend(check_lines(&contents, settings));
    }

    // Convert to messages.
    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| Message {
            kind: check.kind,
            location: check.location,
            filename: path.to_string_lossy().to_string(),
        })
        .filter(|message| !message.is_inline_ignored())
        .collect();
    cache::set(path, settings, &messages, mode);

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::ast::Location;

    use crate::checks::{CheckCode, CheckKind};
    use crate::linter::check_path;
    use crate::message::Message;
    use crate::{cache, settings};

    #[test]
    fn duplicate_argument_name() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/duplicate_argument_name.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F831]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(1, 25),
                filename: "./resources/test/src/duplicate_argument_name.py".to_string(),
            },
            Message {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(5, 28),
                filename: "./resources/test/src/duplicate_argument_name.py".to_string(),
            },
            Message {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(9, 27),
                filename: "./resources/test/src/duplicate_argument_name.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f_string_missing_placeholders() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/f_string_missing_placeholders.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F541]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(4, 7),
                filename: "./resources/test/src/f_string_missing_placeholders.py".to_string(),
            },
            Message {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(5, 7),
                filename: "./resources/test/src/f_string_missing_placeholders.py".to_string(),
            },
            Message {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(7, 7),
                filename: "./resources/test/src/f_string_missing_placeholders.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn if_tuple() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/if_tuple.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F634]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::IfTuple,
                location: Location::new(1, 1),
                filename: "./resources/test/src/if_tuple.py".to_string(),
            },
            Message {
                kind: CheckKind::IfTuple,
                location: Location::new(7, 5),
                filename: "./resources/test/src/if_tuple.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn import_star_usage() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/import_star_usage.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F403]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::ImportStarUsage,
                location: Location::new(1, 1),
                filename: "./resources/test/src/import_star_usage.py".to_string(),
            },
            Message {
                kind: CheckKind::ImportStarUsage,
                location: Location::new(2, 1),
                filename: "./resources/test/src/import_star_usage.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn return_outside_function() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/return_outside_function.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F706]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::ReturnOutsideFunction,
                location: Location::new(6, 5),
                filename: "./resources/test/src/return_outside_function.py".to_string(),
            },
            Message {
                kind: CheckKind::ReturnOutsideFunction,
                location: Location::new(9, 1),
                filename: "./resources/test/src/return_outside_function.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn raise_not_implemented() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/raise_not_implemented.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F901]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::RaiseNotImplemented,
                location: Location::new(2, 5),
                filename: "./resources/test/src/raise_not_implemented.py".to_string(),
            },
            Message {
                kind: CheckKind::RaiseNotImplemented,
                location: Location::new(6, 5),
                filename: "./resources/test/src/raise_not_implemented.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn line_too_long() -> Result<()> {
        let actual = check_path(
            &Path::new("./resources/test/src/line_too_long.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E501]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![Message {
            kind: CheckKind::LineTooLong,
            location: Location::new(5, 89),
            filename: "./resources/test/src/line_too_long.py".to_string(),
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }
}
