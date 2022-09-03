use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::parser;

use crate::autofix::apply_fixes;
use crate::check_ast::check_ast;
use crate::check_lines::check_lines;
use crate::checks::{Check, LintSource};
use crate::message::Message;
use crate::settings::Settings;
use crate::{cache, fs};

pub fn check_path(
    path: &Path,
    settings: &Settings,
    mode: &cache::Mode,
    autofix: bool,
) -> Result<Vec<Message>> {
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
        .any(|check_code| matches!(check_code.lint_source(), LintSource::AST))
    {
        let path = path.to_string_lossy();
        let python_ast = parser::parse_program(&contents, &path)?;
        checks.extend(check_ast(&python_ast, &contents, settings, &path));
    }

    // Run the lines-based checks.
    check_lines(&mut checks, &contents, settings);

    // Apply autofix.
    if autofix {
        apply_fixes(&mut checks, &contents, path)?;
    }

    // Convert to messages.
    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| Message {
            kind: check.kind,
            fixed: check.fixed,
            location: check.location,
            filename: path.to_string_lossy().to_string(),
        })
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
    fn e402() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/E402.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E402]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![Message {
            kind: CheckKind::ModuleImportNotAtTopOfFile,
            location: Location::new(20, 1),
            filename: "./resources/test/fixtures/E402.py".to_string(),
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e501() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/E501.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E501]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![Message {
            kind: CheckKind::LineTooLong,
            location: Location::new(5, 89),
            filename: "./resources/test/fixtures/E501.py".to_string(),
            fixed: false,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f401() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F401.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F401]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::UnusedImport("logging.handlers".to_string()),
                location: Location::new(12, 1),
                filename: "./resources/test/fixtures/F401.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UnusedImport("functools".to_string()),
                location: Location::new(3, 1),
                filename: "./resources/test/fixtures/F401.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UnusedImport("collections.OrderedDict".to_string()),
                location: Location::new(4, 1),
                filename: "./resources/test/fixtures/F401.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f403() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F403.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F403]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::ImportStarUsage,
                location: Location::new(1, 1),
                filename: "./resources/test/fixtures/F403.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::ImportStarUsage,
                location: Location::new(2, 1),
                filename: "./resources/test/fixtures/F403.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }
    #[test]
    fn f541() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F541.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F541]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(4, 7),
                filename: "./resources/test/fixtures/F541.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(5, 7),
                filename: "./resources/test/fixtures/F541.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(7, 7),
                filename: "./resources/test/fixtures/F541.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f631() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F631.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F631]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::AssertTuple,
                location: Location::new(1, 1),
                filename: "./resources/test/fixtures/F631.py".to_string(),
            },
            Message {
                kind: CheckKind::AssertTuple,
                location: Location::new(2, 1),
                filename: "./resources/test/fixtures/F631.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f634() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F634.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F634]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::IfTuple,
                location: Location::new(1, 1),
                filename: "./resources/test/fixtures/F634.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::IfTuple,
                location: Location::new(7, 5),
                filename: "./resources/test/fixtures/F634.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f704() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F704.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F704]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::YieldOutsideFunction,
                location: Location::new(6, 5),
                filename: "./resources/test/fixtures/F704.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::YieldOutsideFunction,
                location: Location::new(9, 1),
                filename: "./resources/test/fixtures/F704.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::YieldOutsideFunction,
                location: Location::new(10, 1),
                filename: "./resources/test/fixtures/F704.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f706() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F706.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F706]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::ReturnOutsideFunction,
                location: Location::new(6, 5),
                filename: "./resources/test/fixtures/F706.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::ReturnOutsideFunction,
                location: Location::new(9, 1),
                filename: "./resources/test/fixtures/F706.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f707() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F707.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F707]),
            },
            &cache::Mode::None,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::DefaultExceptNotLast,
                location: Location::new(3, 1),
                filename: "./resources/test/fixtures/F707.py".to_string(),
            },
            Message {
                kind: CheckKind::DefaultExceptNotLast,
                location: Location::new(10, 1),
                filename: "./resources/test/fixtures/F707.py".to_string(),
            },
            Message {
                kind: CheckKind::DefaultExceptNotLast,
                location: Location::new(19, 1),
                filename: "./resources/test/fixtures/F707.py".to_string(),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f821() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F821.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F821]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::UndefinedName("self".to_string()),
                location: Location::new(2, 12),
                filename: "./resources/test/fixtures/F821.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UndefinedName("self".to_string()),
                location: Location::new(6, 13),
                filename: "./resources/test/fixtures/F821.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UndefinedName("self".to_string()),
                location: Location::new(10, 9),
                filename: "./resources/test/fixtures/F821.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UndefinedName("numeric_string".to_string()),
                location: Location::new(21, 12),
                filename: "./resources/test/fixtures/F821.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f822() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F822.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F822]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![Message {
            kind: CheckKind::UndefinedExport("b".to_string()),
            location: Location::new(3, 1),
            filename: "./resources/test/fixtures/F822.py".to_string(),
            fixed: false,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f823() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F823.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F823]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![Message {
            kind: CheckKind::UndefinedLocal("my_var".to_string()),
            location: Location::new(6, 5),
            filename: "./resources/test/fixtures/F823.py".to_string(),
            fixed: false,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f831() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F831.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F831]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(1, 25),
                filename: "./resources/test/fixtures/F831.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(5, 28),
                filename: "./resources/test/fixtures/F831.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(9, 27),
                filename: "./resources/test/fixtures/F831.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f841() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F841.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F841]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::UnusedVariable("e".to_string()),
                location: Location::new(3, 1),
                filename: "./resources/test/fixtures/F841.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UnusedVariable("z".to_string()),
                location: Location::new(16, 5),
                filename: "./resources/test/fixtures/F841.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f901() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F901.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F901]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::RaiseNotImplemented,
                location: Location::new(2, 5),
                filename: "./resources/test/fixtures/F901.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::RaiseNotImplemented,
                location: Location::new(6, 5),
                filename: "./resources/test/fixtures/F901.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn r0205() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/R0205.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::R0205]),
            },
            &cache::Mode::None,
            false,
        )?;
        let expected = vec![
            Message {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(5, 9),
                filename: "./resources/test/fixtures/R0205.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UselessObjectInheritance("C".to_string()),
                location: Location::new(9, 12),
                filename: "./resources/test/fixtures/R0205.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UselessObjectInheritance("D".to_string()),
                location: Location::new(14, 5),
                filename: "./resources/test/fixtures/R0205.py".to_string(),
                fixed: false,
            },
            Message {
                kind: CheckKind::UselessObjectInheritance("E".to_string()),
                location: Location::new(21, 13),
                filename: "./resources/test/fixtures/R0205.py".to_string(),
                fixed: false,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }
}
