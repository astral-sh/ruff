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

fn check_path(
    path: &Path,
    contents: &str,
    settings: &Settings,
    autofix: &fixer::Mode,
) -> Result<Vec<Check>> {
    // Aggregate all checks.
    let mut checks: Vec<Check> = vec![];

    // Run the AST-based checks.
    if settings
        .select
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::AST))
    {
        let python_ast = parser::parse_program(contents, "<filename>")?;
        checks.extend(check_ast(&python_ast, contents, settings, autofix, path));
    }

    // Run the lines-based checks.
    check_lines(&mut checks, contents, settings);

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
    let mut checks = check_path(path, &contents, settings, autofix)?;

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

    use crate::autofix::fixer;
    use crate::checks::{Check, CheckCode};
    use crate::fs;
    use crate::linter;
    use crate::settings;

    fn check_path(
        path: &Path,
        settings: &settings::Settings,
        autofix: &fixer::Mode,
    ) -> Result<Vec<Check>> {
        let contents = fs::read_file(path)?;
        linter::check_path(path, &contents, settings, autofix)
    }

    #[test]
    fn e402() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E402.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E402]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e501() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E501.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E501]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e711() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E711.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E711]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e712() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E712.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E712]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e713() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E713.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E713]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e721() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E721.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E721]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e722() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E722.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E722]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e714() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E714.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E714]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e731() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E731.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E731]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e741() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E741.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E741]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e742() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E742.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E742]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e743() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E743.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::E743]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f401() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F401.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F401]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f403() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F403.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F403]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f404() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F404.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F404]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f406() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F406.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F406]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f407() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F407.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F407]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f541() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F541.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F541]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f601() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F601.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F601]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f602() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F602.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F602]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f622() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F622.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F622]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f631() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F631.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F631]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f632() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F632.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F632]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f633() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F633.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F633]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f634() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F634.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F634]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f701() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F701.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F701]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f702() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F702.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F702]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f704() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F704.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F704]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f706() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F706.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F706]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f707() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F707.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F707]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f722() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F722.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F722]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f821() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F821.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F821]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f822() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F822.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F822]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f823() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F823.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F823]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f831() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F831.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F831]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f841() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F841.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F841]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f901() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F901.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F901]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn r001() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/R001.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::R001]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn r002() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/R002.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::R002]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn init() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/__init__.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F821, CheckCode::F822]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn future_annotations() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/future_annotations.py"),
            &settings::Settings {
                line_length: 88,
                exclude: vec![],
                extend_exclude: vec![],
                select: BTreeSet::from([CheckCode::F401, CheckCode::F821]),
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
