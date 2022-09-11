use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::parser;

use crate::autofix::fixer;
use crate::autofix::fixer::fix_file;
use crate::check_ast::check_ast;
use crate::check_lines::check_lines;
use crate::checks::{Check, LintSource};
use crate::message::Message;
use crate::settings::Settings;
use crate::{cache, fs};

fn check_path(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> Result<Vec<Check>> {
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
        checks.extend(check_ast(&python_ast, &contents, settings, autofix, &path));
    }

    // Run the lines-based checks.
    check_lines(&mut checks, &contents, settings);

    Ok(checks)
}

pub fn lint_path(
    path: &Path,
    settings: &Settings,
    mode: &cache::Mode,
    autofix: &fixer::Mode,
) -> Result<Vec<Message>> {
    let metadata = path.metadata()?;

    // Check the cache.
    if let Some(messages) = cache::get(path, &metadata, settings, autofix, mode) {
        debug!("Cache hit for: {}", path.to_string_lossy());
        return Ok(messages);
    }

    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Generate checks.
    let mut checks = check_path(path, settings, autofix)?;

    // Apply autofix.
    if matches!(autofix, fixer::Mode::Apply) {
        fix_file(&mut checks, &contents, path)?;
    };

    // Convert to messages.
    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| Message {
            kind: check.kind,
            fixed: check.fix.map(|fix| fix.applied).unwrap_or_default(),
            location: check.location,
            filename: path.to_string_lossy().to_string(),
        })
        .collect();
    cache::set(path, &metadata, settings, autofix, &messages, mode);

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::ast::Location;

    use crate::autofix::fixer;
    use crate::checks::{Check, CheckCode, CheckKind, Fix, RejectedCmpop};
    use crate::linter::check_path;
    use crate::settings;

    #[test]
    fn e402() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E402.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E402]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::ModuleImportNotAtTopOfFile,
            location: Location::new(20, 1),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e501() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E501.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E501]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::LineTooLong,
            location: Location::new(5, 89),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e711() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E711.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E711]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::NoneComparison(RejectedCmpop::Eq),
                location: Location::new(1, 11),
                fix: None,
            },
            Check {
                kind: CheckKind::NoneComparison(RejectedCmpop::NotEq),
                location: Location::new(4, 4),
                fix: None,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e712() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E712.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E712]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::TrueFalseComparison(true, RejectedCmpop::Eq),
                location: Location::new(1, 11),
                fix: None,
            },
            Check {
                kind: CheckKind::TrueFalseComparison(false, RejectedCmpop::NotEq),
                location: Location::new(4, 4),
                fix: None,
            },
            Check {
                kind: CheckKind::TrueFalseComparison(false, RejectedCmpop::NotEq),
                location: Location::new(7, 11),
                fix: None,
            },
            Check {
                kind: CheckKind::TrueFalseComparison(true, RejectedCmpop::NotEq),
                location: Location::new(7, 20),
                fix: None,
            },
        ];

        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e713() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E713.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E713]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::NotInTest,
            location: Location::new(2, 12),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e714() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E714.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E714]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::NotIsTest,
            location: Location::new(1, 13),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e731() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E731.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E731]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::DoNotAssignLambda,
                location: Location::new(3, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::DoNotAssignLambda,
                location: Location::new(5, 1),
                fix: None,
            },
        ];

        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn e741() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/E741.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::E741]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(3, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("I".to_string()),
                location: Location::new(4, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("O".to_string()),
                location: Location::new(5, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(6, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(8, 4),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(9, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(10, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(11, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(16, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(20, 8),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(25, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(26, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(30, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(33, 9),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(34, 9),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(40, 8),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("I".to_string()),
                location: Location::new(40, 14),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(44, 8),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("I".to_string()),
                location: Location::new(44, 16),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(48, 9),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("I".to_string()),
                location: Location::new(48, 14),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(57, 16),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(66, 20),
                fix: None,
            },
            Check {
                kind: CheckKind::AmbiguousVariableName("l".to_string()),
                location: Location::new(71, 1),
                fix: None,
            },
        ];

        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f401() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F401.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F401]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::UnusedImport("functools".to_string()),
                location: Location::new(3, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::UnusedImport("collections.OrderedDict".to_string()),
                location: Location::new(5, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::UnusedImport("logging.handlers".to_string()),
                location: Location::new(13, 1),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F403.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F403]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::ImportStarUsage,
                location: Location::new(1, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::ImportStarUsage,
                location: Location::new(2, 1),
                fix: None,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f404() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F404.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F404]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::LateFutureImport,
            location: Location::new(7, 1),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f407() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F407.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F407]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::FutureFeatureNotDefined("non_existent_feature".to_string()),
            location: Location::new(2, 1),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f541() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F541.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F541]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(4, 7),
                fix: None,
            },
            Check {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(5, 7),
                fix: None,
            },
            Check {
                kind: CheckKind::FStringMissingPlaceholders,
                location: Location::new(7, 7),
                fix: None,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f601() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F601.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F601]),
            },
            &fixer::Mode::Generate,
        )?;
        let expected = vec![
            Check {
                kind: CheckKind::MultiValueRepeatedKeyLiteral,
                location: Location::new(3, 6),
                fix: None,
            },
            Check {
                kind: CheckKind::MultiValueRepeatedKeyLiteral,
                location: Location::new(9, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::MultiValueRepeatedKeyLiteral,
                location: Location::new(11, 7),
                fix: None,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f602() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F602.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F602]),
            },
            &fixer::Mode::Generate,
        )?;
        let expected = vec![Check {
            kind: CheckKind::MultiValueRepeatedKeyVariable("a".to_string()),
            location: Location::new(5, 5),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f622() -> Result<()> {
        let actual = check_path(
            Path::new("./resources/test/fixtures/F622.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F622]),
            },
            &fixer::Mode::Generate,
        )?;
        let expected = vec![Check {
            kind: CheckKind::TwoStarredExpressions,
            location: Location::new(1, 1),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f631() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F631.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F631]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::AssertTuple,
                location: Location::new(1, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::AssertTuple,
                location: Location::new(2, 1),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F634.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F634]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::IfTuple,
                location: Location::new(1, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::IfTuple,
                location: Location::new(7, 5),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F704.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F704]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::YieldOutsideFunction,
                location: Location::new(6, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::YieldOutsideFunction,
                location: Location::new(9, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::YieldOutsideFunction,
                location: Location::new(10, 1),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F706.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F706]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::ReturnOutsideFunction,
                location: Location::new(6, 5),
                fix: None,
            },
            Check {
                kind: CheckKind::ReturnOutsideFunction,
                location: Location::new(9, 1),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F707.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F707]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::DefaultExceptNotLast,
                location: Location::new(3, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::DefaultExceptNotLast,
                location: Location::new(10, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::DefaultExceptNotLast,
                location: Location::new(19, 1),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F821.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F821]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::UndefinedName("self".to_string()),
                location: Location::new(2, 12),
                fix: None,
            },
            Check {
                kind: CheckKind::UndefinedName("self".to_string()),
                location: Location::new(6, 13),
                fix: None,
            },
            Check {
                kind: CheckKind::UndefinedName("self".to_string()),
                location: Location::new(10, 9),
                fix: None,
            },
            Check {
                kind: CheckKind::UndefinedName("numeric_string".to_string()),
                location: Location::new(21, 12),
                fix: None,
            },
            Check {
                kind: CheckKind::UndefinedName("Bar".to_string()),
                location: Location::new(58, 5),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F822.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F822]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::UndefinedExport("b".to_string()),
            location: Location::new(3, 1),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f823() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F823.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F823]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![Check {
            kind: CheckKind::UndefinedLocal("my_var".to_string()),
            location: Location::new(6, 5),
            fix: None,
        }];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn f831() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F831.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F831]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(1, 25),
                fix: None,
            },
            Check {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(5, 28),
                fix: None,
            },
            Check {
                kind: CheckKind::DuplicateArgumentName,
                location: Location::new(9, 27),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F841.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F841]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::UnusedVariable("e".to_string()),
                location: Location::new(3, 1),
                fix: None,
            },
            Check {
                kind: CheckKind::UnusedVariable("z".to_string()),
                location: Location::new(16, 5),
                fix: None,
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
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/F901.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::F901]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::RaiseNotImplemented,
                location: Location::new(2, 25),
                fix: None,
            },
            Check {
                kind: CheckKind::RaiseNotImplemented,
                location: Location::new(6, 11),
                fix: None,
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn r001() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/R001.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::R001]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(5, 9),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(5, 8),
                    end: Location::new(5, 16),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(10, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(9, 8),
                    end: Location::new(11, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(16, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(15, 8),
                    end: Location::new(18, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(24, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(22, 8),
                    end: Location::new(25, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(31, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(29, 8),
                    end: Location::new(32, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(37, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(36, 8),
                    end: Location::new(39, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(45, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(43, 8),
                    end: Location::new(47, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(53, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(51, 8),
                    end: Location::new(55, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(61, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(59, 8),
                    end: Location::new(63, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(69, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(67, 8),
                    end: Location::new(71, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(75, 12),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(75, 10),
                    end: Location::new(75, 18),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(79, 9),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(79, 9),
                    end: Location::new(79, 17),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(84, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(84, 5),
                    end: Location::new(85, 5),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(92, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(91, 6),
                    end: Location::new(92, 11),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(98, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(98, 5),
                    end: Location::new(100, 5),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("B".to_string()),
                location: Location::new(108, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(107, 6),
                    end: Location::new(108, 11),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(114, 13),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(114, 12),
                    end: Location::new(114, 20),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(119, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(118, 8),
                    end: Location::new(120, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(125, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(124, 8),
                    end: Location::new(126, 2),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::UselessObjectInheritance("A".to_string()),
                location: Location::new(131, 5),
                fix: Some(Fix {
                    content: "".to_string(),
                    start: Location::new(130, 8),
                    end: Location::new(133, 2),
                    applied: false,
                }),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }

    #[test]
    fn r002() -> Result<()> {
        let mut actual = check_path(
            Path::new("./resources/test/fixtures/R002.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                select: BTreeSet::from([CheckCode::R002]),
            },
            &fixer::Mode::Generate,
        )?;
        actual.sort_by_key(|check| check.location);
        let expected = vec![
            Check {
                kind: CheckKind::NoAssertEquals,
                location: Location::new(1, 5),
                fix: Some(Fix {
                    content: "assertEqual".to_string(),
                    start: Location::new(1, 6),
                    end: Location::new(1, 18),
                    applied: false,
                }),
            },
            Check {
                kind: CheckKind::NoAssertEquals,
                location: Location::new(2, 5),
                fix: Some(Fix {
                    content: "assertEqual".to_string(),
                    start: Location::new(2, 6),
                    end: Location::new(2, 18),
                    applied: false,
                }),
            },
        ];
        assert_eq!(actual.len(), expected.len());
        for i in 0..actual.len() {
            assert_eq!(actual[i], expected[i]);
        }

        Ok(())
    }
}
