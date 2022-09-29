use std::path::Path;

use anyhow::Result;
use log::debug;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::{lexer, parser};

use crate::autofix::fixer;
use crate::autofix::fixer::fix_file;
use crate::check_ast::check_ast;
use crate::check_lines::check_lines;
use crate::checks::{Check, CheckCode, CheckKind, LintSource};
use crate::message::Message;
use crate::noqa::add_noqa;
use crate::settings::Settings;
use crate::{cache, fs, noqa};

/// Collect tokens up to and including the first error.
fn tokenize(contents: &str) -> Vec<LexResult> {
    let mut tokens: Vec<LexResult> = vec![];
    for tok in lexer::make_tokenizer(contents) {
        let is_err = tok.is_err();
        tokens.push(tok);
        if is_err {
            break;
        }
    }
    tokens
}

fn check_path(
    path: &Path,
    contents: &str,
    tokens: Vec<LexResult>,
    noqa_line_for: &[usize],
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
        match parser::parse_program_tokens(tokens, "<filename>") {
            Ok(python_ast) => {
                checks.extend(check_ast(&python_ast, contents, settings, autofix, path))
            }
            Err(parse_error) => {
                if settings.select.contains(&CheckCode::E999) {
                    checks.push(Check::new(
                        CheckKind::SyntaxError(parse_error.error.to_string()),
                        parse_error.location,
                    ))
                }
            }
        }
    }

    // Run the lines-based checks.
    check_lines(&mut checks, contents, noqa_line_for, settings, autofix);

    // Create path ignores.
    if !checks.is_empty() && !settings.per_file_ignores.is_empty() {
        let ignores = fs::ignores_from_path(path, &settings.per_file_ignores)?;
        if !ignores.is_empty() {
            return Ok(checks
                .into_iter()
                .filter(|check| !ignores.contains(check.kind.code()))
                .collect());
        }
    }

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

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(&contents);

    // Determine the noqa line for every line in the source.
    let noqa_line_for = noqa::extract_noqa_line_for(&tokens);

    // Generate checks.
    let mut checks = check_path(path, &contents, tokens, &noqa_line_for, settings, autofix)?;

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

pub fn add_noqa_to_path(path: &Path, settings: &Settings) -> Result<usize> {
    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(&contents);

    // Determine the noqa line for every line in the source.
    let noqa_line_for = noqa::extract_noqa_line_for(&tokens);

    // Generate checks.
    let checks = check_path(
        path,
        &contents,
        tokens,
        &noqa_line_for,
        settings,
        &fixer::Mode::None,
    )?;

    add_noqa(&checks, &contents, &noqa_line_for, path)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use rustpython_parser::lexer::LexResult;

    use crate::autofix::fixer;
    use crate::checks::{Check, CheckCode};
    use crate::linter;
    use crate::linter::tokenize;
    use crate::settings;
    use crate::{fs, noqa};

    fn check_path(
        path: &Path,
        settings: &settings::Settings,
        autofix: &fixer::Mode,
    ) -> Result<Vec<Check>> {
        let contents = fs::read_file(path)?;
        let tokens: Vec<LexResult> = tokenize(&contents);
        let noqa_line_for = noqa::extract_noqa_line_for(&tokens);
        linter::check_path(path, &contents, tokens, &noqa_line_for, settings, autofix)
    }

    #[test]
    fn e402() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E402.py"),
            &settings::Settings::for_rule(CheckCode::E402),
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
            &settings::Settings::for_rule(CheckCode::E501),
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
            &settings::Settings::for_rule(CheckCode::E711),
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
            &settings::Settings::for_rule(CheckCode::E712),
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
            &settings::Settings::for_rule(CheckCode::E713),
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
            &settings::Settings::for_rule(CheckCode::E721),
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
            &settings::Settings::for_rule(CheckCode::E722),
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
            &settings::Settings::for_rule(CheckCode::E714),
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
            &settings::Settings::for_rule(CheckCode::E731),
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
            &settings::Settings::for_rule(CheckCode::E741),
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
            &settings::Settings::for_rule(CheckCode::E742),
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
            &settings::Settings::for_rule(CheckCode::E743),
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
            &settings::Settings::for_rule(CheckCode::F401),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f402() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F402.py"),
            &settings::Settings::for_rule(CheckCode::F402),
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
            &settings::Settings::for_rule(CheckCode::F403),
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
            &settings::Settings::for_rule(CheckCode::F404),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f405() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F405.py"),
            &settings::Settings::for_rule(CheckCode::F405),
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
            &settings::Settings::for_rule(CheckCode::F406),
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
            &settings::Settings::for_rule(CheckCode::F407),
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
            &settings::Settings::for_rule(CheckCode::F541),
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
            &settings::Settings::for_rule(CheckCode::F601),
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
            &settings::Settings::for_rule(CheckCode::F602),
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
            &settings::Settings::for_rule(CheckCode::F622),
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
            &settings::Settings::for_rule(CheckCode::F631),
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
            &settings::Settings::for_rule(CheckCode::F632),
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
            &settings::Settings::for_rule(CheckCode::F633),
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
            &settings::Settings::for_rule(CheckCode::F634),
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
            &settings::Settings::for_rule(CheckCode::F701),
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
            &settings::Settings::for_rule(CheckCode::F702),
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
            &settings::Settings::for_rule(CheckCode::F704),
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
            &settings::Settings::for_rule(CheckCode::F706),
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
            &settings::Settings::for_rule(CheckCode::F707),
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
            &settings::Settings::for_rule(CheckCode::F722),
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
            &settings::Settings::for_rule(CheckCode::F821),
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
            &settings::Settings::for_rule(CheckCode::F822),
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
            &settings::Settings::for_rule(CheckCode::F823),
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
            &settings::Settings::for_rule(CheckCode::F831),
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
            &settings::Settings::for_rule(CheckCode::F841),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn f841_dummy_variable_rgx() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/F841.py"),
            &settings::Settings {
                dummy_variable_rgx: Regex::new(r"^z$").unwrap(),
                ..settings::Settings::for_rule(CheckCode::F841)
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
            &settings::Settings::for_rule(CheckCode::F901),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn m001() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/M001.py"),
            &settings::Settings::for_rules(vec![CheckCode::M001, CheckCode::E501, CheckCode::F841]),
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
            &settings::Settings::for_rule(CheckCode::R001),
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
            &settings::Settings::for_rule(CheckCode::R002),
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
            &settings::Settings::for_rules(vec![CheckCode::F821, CheckCode::F822]),
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
            &settings::Settings::for_rules(vec![CheckCode::F401, CheckCode::F821]),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn e999() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/E999.py"),
            &settings::Settings::for_rule(CheckCode::E999),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn a001() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/A001.py"),
            &settings::Settings::for_rule(CheckCode::A001),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn a002() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/A002.py"),
            &settings::Settings::for_rule(CheckCode::A002),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn a003() -> Result<()> {
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/A003.py"),
            &settings::Settings::for_rule(CheckCode::A003),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
