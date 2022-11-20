use std::fs::write;
use std::io;
use std::io::Write;
use std::path::Path;

use anyhow::Result;
#[cfg(not(target_family = "wasm"))]
use log::debug;
use rustpython_ast::{Mod, Suite};
use rustpython_parser::error::ParseError;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::parser::Mode;
use rustpython_parser::{lexer, parser};

use crate::ast::types::Range;
use crate::autofix::fixer;
use crate::autofix::fixer::fix_file;
use crate::check_ast::check_ast;
use crate::check_imports::check_imports;
use crate::check_lines::check_lines;
use crate::check_tokens::check_tokens;
use crate::checks::{Check, CheckCode, CheckKind, LintSource};
use crate::code_gen::SourceGenerator;
use crate::directives::Directives;
use crate::message::{Message, Source};
use crate::noqa::add_noqa;
use crate::settings::Settings;
use crate::source_code_locator::SourceCodeLocator;
use crate::{cache, directives, fs};

/// Collect tokens up to and including the first error.
pub(crate) fn tokenize(contents: &str) -> Vec<LexResult> {
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

/// Parse a full Python program from its tokens.
pub(crate) fn parse_program_tokens(
    lxr: Vec<LexResult>,
    source_path: &str,
) -> Result<Suite, ParseError> {
    parser::parse_tokens(lxr, Mode::Module, source_path).map(|top| match top {
        Mod::Module { body, .. } => body,
        _ => unreachable!(),
    })
}

pub(crate) fn check_path(
    path: &Path,
    contents: &str,
    tokens: Vec<LexResult>,
    locator: &SourceCodeLocator,
    directives: &Directives,
    settings: &Settings,
    autofix: &fixer::Mode,
) -> Result<Vec<Check>> {
    // Aggregate all checks.
    let mut checks: Vec<Check> = vec![];

    // Run the token-based checks.
    let use_tokens = settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::Tokens));
    if use_tokens {
        check_tokens(&mut checks, locator, &tokens, settings, autofix);
    }

    // Run the AST-based checks.
    let use_ast = settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::AST));
    let use_imports = settings
        .enabled
        .iter()
        .any(|check_code| matches!(check_code.lint_source(), LintSource::Imports));
    if use_ast || use_imports {
        match parse_program_tokens(tokens, "<filename>") {
            Ok(python_ast) => {
                if use_ast {
                    checks.extend(check_ast(&python_ast, locator, settings, autofix, path));
                }
                if use_imports {
                    checks.extend(check_imports(
                        &python_ast,
                        locator,
                        &directives.isort_exclusions,
                        settings,
                        autofix,
                    ));
                }
            }
            Err(parse_error) => {
                if settings.enabled.contains(&CheckCode::E999) {
                    checks.push(Check::new(
                        CheckKind::SyntaxError(parse_error.error.to_string()),
                        Range {
                            location: parse_error.location,
                            end_location: parse_error.location,
                        },
                    ))
                }
            }
        }
    }

    // Run the lines-based checks.
    check_lines(
        &mut checks,
        contents,
        &directives.noqa_line_for,
        settings,
        autofix,
    );

    // Create path ignores.
    if !checks.is_empty() && !settings.per_file_ignores.is_empty() {
        let ignores = fs::ignores_from_path(path, &settings.per_file_ignores)?;
        if !ignores.is_empty() {
            return Ok(checks
                .into_iter()
                .filter(|check| !ignores.contains(&check.kind.code()))
                .collect());
        }
    }

    Ok(checks)
}

pub fn lint_stdin(
    path: &Path,
    stdin: &str,
    settings: &Settings,
    autofix: &fixer::Mode,
) -> Result<Vec<Message>> {
    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(stdin);

    // Initialize the SourceCodeLocator (which computes offsets lazily).
    let locator = SourceCodeLocator::new(stdin);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        &directives::Flags::from_settings(settings),
    );

    // Generate checks.
    let mut checks = check_path(
        path,
        stdin,
        tokens,
        &locator,
        &directives,
        settings,
        autofix,
    )?;

    // Apply autofix, write results to stdout.
    if matches!(autofix, fixer::Mode::Apply) {
        match fix_file(&mut checks, &locator) {
            None => io::stdout().write_all(stdin.as_bytes()),
            Some(contents) => io::stdout().write_all(contents.as_bytes()),
        }?;
    }

    // Convert to messages.
    Ok(checks
        .into_iter()
        .map(|check| {
            let filename = path.to_string_lossy().to_string();
            let source = if settings.show_source {
                Some(Source::from_check(&check, &locator))
            } else {
                None
            };
            Message::from_check(check, filename, source)
        })
        .collect())
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

    // Initialize the SourceCodeLocator (which computes offsets lazily).
    let locator = SourceCodeLocator::new(&contents);

    // Determine the noqa and isort exclusions.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        &directives::Flags::from_settings(settings),
    );

    // Generate checks.
    let mut checks = check_path(
        path,
        &contents,
        tokens,
        &locator,
        &directives,
        settings,
        autofix,
    )?;

    // Apply autofix.
    if matches!(autofix, fixer::Mode::Apply) {
        if let Some(fixed_contents) = fix_file(&mut checks, &locator) {
            write(path, fixed_contents.as_ref())?;
        }
    };

    // Convert to messages.
    let messages: Vec<Message> = checks
        .into_iter()
        .map(|check| {
            let filename = path.to_string_lossy().to_string();
            let source = if settings.show_source {
                Some(Source::from_check(&check, &locator))
            } else {
                None
            };
            Message::from_check(check, filename, source)
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

    // Initialize the SourceCodeLocator (which computes offsets lazily).
    let locator = SourceCodeLocator::new(&contents);

    // Extract the `# noqa` and `# isort: skip` directives from the source.
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        &directives::Flags::from_settings(settings),
    );

    // Generate checks.
    let checks = check_path(
        path,
        &contents,
        tokens,
        &locator,
        &directives,
        settings,
        &fixer::Mode::None,
    )?;

    add_noqa(&checks, &contents, &directives.noqa_line_for, path)
}

pub fn autoformat_path(path: &Path) -> Result<()> {
    // Read the file from disk.
    let contents = fs::read_file(path)?;

    // Tokenize once.
    let tokens: Vec<LexResult> = tokenize(&contents);

    // Generate the AST.
    let python_ast = parse_program_tokens(tokens, "<filename>")?;
    let mut generator: SourceGenerator = Default::default();
    generator.unparse_suite(&python_ast)?;
    write(path, generator.generate()?)?;

    Ok(())
}

#[cfg(test)]
pub fn test_path(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> Result<Vec<Check>> {
    let contents = fs::read_file(path)?;
    let tokens: Vec<LexResult> = tokenize(&contents);
    let locator = SourceCodeLocator::new(&contents);
    let directives = directives::extract_directives(
        &tokens,
        &locator,
        &directives::Flags::from_settings(settings),
    );
    check_path(
        path,
        &contents,
        tokens,
        &locator,
        &directives,
        settings,
        autofix,
    )
}

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use regex::Regex;
    use test_case::test_case;

    use crate::autofix::fixer;
    use crate::checks::CheckCode;
    use crate::linter::test_path;
    use crate::settings;

    #[test_case(CheckCode::A001, Path::new("A001.py"); "A001")]
    #[test_case(CheckCode::A002, Path::new("A002.py"); "A002")]
    #[test_case(CheckCode::A003, Path::new("A003.py"); "A003")]
    #[test_case(CheckCode::B002, Path::new("B002.py"); "B002")]
    #[test_case(CheckCode::B003, Path::new("B003.py"); "B003")]
    #[test_case(CheckCode::B004, Path::new("B004.py"); "B004")]
    #[test_case(CheckCode::B005, Path::new("B005.py"); "B005")]
    #[test_case(CheckCode::B006, Path::new("B006_B008.py"); "B006")]
    #[test_case(CheckCode::B007, Path::new("B007.py"); "B007")]
    #[test_case(CheckCode::B008, Path::new("B006_B008.py"); "B008")]
    #[test_case(CheckCode::B009, Path::new("B009_B010.py"); "B009")]
    #[test_case(CheckCode::B010, Path::new("B009_B010.py"); "B010")]
    #[test_case(CheckCode::B011, Path::new("B011.py"); "B011")]
    #[test_case(CheckCode::B012, Path::new("B012.py"); "B012")]
    #[test_case(CheckCode::B013, Path::new("B013.py"); "B013")]
    #[test_case(CheckCode::B014, Path::new("B014.py"); "B014")]
    #[test_case(CheckCode::B015, Path::new("B015.py"); "B015")]
    #[test_case(CheckCode::B016, Path::new("B016.py"); "B016")]
    #[test_case(CheckCode::B017, Path::new("B017.py"); "B017")]
    #[test_case(CheckCode::B018, Path::new("B018.py"); "B018")]
    #[test_case(CheckCode::B019, Path::new("B019.py"); "B019")]
    #[test_case(CheckCode::B020, Path::new("B020.py"); "B020")]
    #[test_case(CheckCode::B021, Path::new("B021.py"); "B021")]
    #[test_case(CheckCode::B022, Path::new("B022.py"); "B022")]
    #[test_case(CheckCode::B024, Path::new("B024.py"); "B024")]
    #[test_case(CheckCode::B025, Path::new("B025.py"); "B025")]
    #[test_case(CheckCode::B026, Path::new("B026.py"); "B026")]
    #[test_case(CheckCode::B027, Path::new("B027.py"); "B027")]
    #[test_case(CheckCode::BLE001, Path::new("BLE.py"); "BLE001")]
    #[test_case(CheckCode::C400, Path::new("C400.py"); "C400")]
    #[test_case(CheckCode::C401, Path::new("C401.py"); "C401")]
    #[test_case(CheckCode::C402, Path::new("C402.py"); "C402")]
    #[test_case(CheckCode::C403, Path::new("C403.py"); "C403")]
    #[test_case(CheckCode::C404, Path::new("C404.py"); "C404")]
    #[test_case(CheckCode::C405, Path::new("C405.py"); "C405")]
    #[test_case(CheckCode::C406, Path::new("C406.py"); "C406")]
    #[test_case(CheckCode::C408, Path::new("C408.py"); "C408")]
    #[test_case(CheckCode::C409, Path::new("C409.py"); "C409")]
    #[test_case(CheckCode::C410, Path::new("C410.py"); "C410")]
    #[test_case(CheckCode::C411, Path::new("C411.py"); "C411")]
    #[test_case(CheckCode::C413, Path::new("C413.py"); "C413")]
    #[test_case(CheckCode::C414, Path::new("C414.py"); "C414")]
    #[test_case(CheckCode::C415, Path::new("C415.py"); "C415")]
    #[test_case(CheckCode::C416, Path::new("C416.py"); "C416")]
    #[test_case(CheckCode::C417, Path::new("C417.py"); "C417")]
    #[test_case(CheckCode::D100, Path::new("D.py"); "D100")]
    #[test_case(CheckCode::D101, Path::new("D.py"); "D101")]
    #[test_case(CheckCode::D102, Path::new("D.py"); "D102")]
    #[test_case(CheckCode::D103, Path::new("D.py"); "D103")]
    #[test_case(CheckCode::D104, Path::new("D.py"); "D104")]
    #[test_case(CheckCode::D105, Path::new("D.py"); "D105")]
    #[test_case(CheckCode::D106, Path::new("D.py"); "D106")]
    #[test_case(CheckCode::D107, Path::new("D.py"); "D107")]
    #[test_case(CheckCode::D201, Path::new("D.py"); "D201")]
    #[test_case(CheckCode::D202, Path::new("D.py"); "D202")]
    #[test_case(CheckCode::D203, Path::new("D.py"); "D203")]
    #[test_case(CheckCode::D204, Path::new("D.py"); "D204")]
    #[test_case(CheckCode::D205, Path::new("D.py"); "D205")]
    #[test_case(CheckCode::D206, Path::new("D.py"); "D206")]
    #[test_case(CheckCode::D207, Path::new("D.py"); "D207")]
    #[test_case(CheckCode::D208, Path::new("D.py"); "D208")]
    #[test_case(CheckCode::D209, Path::new("D.py"); "D209")]
    #[test_case(CheckCode::D210, Path::new("D.py"); "D210")]
    #[test_case(CheckCode::D211, Path::new("D.py"); "D211")]
    #[test_case(CheckCode::D212, Path::new("D.py"); "D212")]
    #[test_case(CheckCode::D213, Path::new("D.py"); "D213")]
    #[test_case(CheckCode::D214, Path::new("sections.py"); "D214")]
    #[test_case(CheckCode::D215, Path::new("sections.py"); "D215")]
    #[test_case(CheckCode::D300, Path::new("D.py"); "D300")]
    #[test_case(CheckCode::D400, Path::new("D.py"); "D400")]
    #[test_case(CheckCode::D402, Path::new("D.py"); "D402")]
    #[test_case(CheckCode::D403, Path::new("D.py"); "D403")]
    #[test_case(CheckCode::D404, Path::new("D.py"); "D404")]
    #[test_case(CheckCode::D405, Path::new("sections.py"); "D405")]
    #[test_case(CheckCode::D406, Path::new("sections.py"); "D406")]
    #[test_case(CheckCode::D407, Path::new("sections.py"); "D407")]
    #[test_case(CheckCode::D408, Path::new("sections.py"); "D408")]
    #[test_case(CheckCode::D409, Path::new("sections.py"); "D409")]
    #[test_case(CheckCode::D410, Path::new("sections.py"); "D410")]
    #[test_case(CheckCode::D411, Path::new("sections.py"); "D411")]
    #[test_case(CheckCode::D412, Path::new("sections.py"); "D412")]
    #[test_case(CheckCode::D413, Path::new("sections.py"); "D413")]
    #[test_case(CheckCode::D414, Path::new("sections.py"); "D414")]
    #[test_case(CheckCode::D415, Path::new("D.py"); "D415")]
    #[test_case(CheckCode::D416, Path::new("D.py"); "D416")]
    #[test_case(CheckCode::D417, Path::new("sections.py"); "D417_0")]
    #[test_case(CheckCode::D417, Path::new("canonical_numpy_examples.py"); "D417_1")]
    #[test_case(CheckCode::D417, Path::new("canonical_google_examples.py"); "D417_2")]
    #[test_case(CheckCode::D418, Path::new("D.py"); "D418")]
    #[test_case(CheckCode::D419, Path::new("D.py"); "D419")]
    #[test_case(CheckCode::E402, Path::new("E402.py"); "E402")]
    #[test_case(CheckCode::E501, Path::new("E501.py"); "E501")]
    #[test_case(CheckCode::E711, Path::new("E711.py"); "E711")]
    #[test_case(CheckCode::E712, Path::new("E712.py"); "E712")]
    #[test_case(CheckCode::E713, Path::new("E713.py"); "E713")]
    #[test_case(CheckCode::E714, Path::new("E714.py"); "E714")]
    #[test_case(CheckCode::E721, Path::new("E721.py"); "E721")]
    #[test_case(CheckCode::E722, Path::new("E722.py"); "E722")]
    #[test_case(CheckCode::E731, Path::new("E731.py"); "E731")]
    #[test_case(CheckCode::E741, Path::new("E741.py"); "E741")]
    #[test_case(CheckCode::E742, Path::new("E742.py"); "E742")]
    #[test_case(CheckCode::E743, Path::new("E743.py"); "E743")]
    #[test_case(CheckCode::E999, Path::new("E999.py"); "E999")]
    #[test_case(CheckCode::F401, Path::new("F401_0.py"); "F401_0")]
    #[test_case(CheckCode::F401, Path::new("F401_1.py"); "F401_1")]
    #[test_case(CheckCode::F401, Path::new("F401_2.py"); "F401_2")]
    #[test_case(CheckCode::F401, Path::new("F401_3.py"); "F401_3")]
    #[test_case(CheckCode::F401, Path::new("F401_4.py"); "F401_4")]
    #[test_case(CheckCode::F401, Path::new("F401_5.py"); "F401_5")]
    #[test_case(CheckCode::F401, Path::new("F401_6.py"); "F401_6")]
    #[test_case(CheckCode::F402, Path::new("F402.py"); "F402")]
    #[test_case(CheckCode::F403, Path::new("F403.py"); "F403")]
    #[test_case(CheckCode::F404, Path::new("F404.py"); "F404")]
    #[test_case(CheckCode::F405, Path::new("F405.py"); "F405")]
    #[test_case(CheckCode::F406, Path::new("F406.py"); "F406")]
    #[test_case(CheckCode::F407, Path::new("F407.py"); "F407")]
    #[test_case(CheckCode::F541, Path::new("F541.py"); "F541")]
    #[test_case(CheckCode::F601, Path::new("F601.py"); "F601")]
    #[test_case(CheckCode::F602, Path::new("F602.py"); "F602")]
    #[test_case(CheckCode::F622, Path::new("F622.py"); "F622")]
    #[test_case(CheckCode::F631, Path::new("F631.py"); "F631")]
    #[test_case(CheckCode::F632, Path::new("F632.py"); "F632")]
    #[test_case(CheckCode::F633, Path::new("F633.py"); "F633")]
    #[test_case(CheckCode::F634, Path::new("F634.py"); "F634")]
    #[test_case(CheckCode::F701, Path::new("F701.py"); "F701")]
    #[test_case(CheckCode::F702, Path::new("F702.py"); "F702")]
    #[test_case(CheckCode::F704, Path::new("F704.py"); "F704")]
    #[test_case(CheckCode::F706, Path::new("F706.py"); "F706")]
    #[test_case(CheckCode::F707, Path::new("F707.py"); "F707")]
    #[test_case(CheckCode::F722, Path::new("F722.py"); "F722")]
    #[test_case(CheckCode::F821, Path::new("F821_0.py"); "F821_0")]
    #[test_case(CheckCode::F821, Path::new("F821_1.py"); "F821_1")]
    #[test_case(CheckCode::F821, Path::new("F821_2.py"); "F821_2")]
    #[test_case(CheckCode::F821, Path::new("F821_3.py"); "F821_3")]
    #[test_case(CheckCode::F821, Path::new("F821_4.py"); "F821_4")]
    #[test_case(CheckCode::F821, Path::new("F821_5.py"); "F821_5")]
    #[test_case(CheckCode::F822, Path::new("F822.py"); "F822")]
    #[test_case(CheckCode::F823, Path::new("F823.py"); "F823")]
    #[test_case(CheckCode::F831, Path::new("F831.py"); "F831")]
    #[test_case(CheckCode::F841, Path::new("F841.py"); "F841")]
    #[test_case(CheckCode::F901, Path::new("F901.py"); "F901")]
    #[test_case(CheckCode::N801, Path::new("N801.py"); "N801")]
    #[test_case(CheckCode::N802, Path::new("N802.py"); "N802")]
    #[test_case(CheckCode::N803, Path::new("N803.py"); "N803")]
    #[test_case(CheckCode::N804, Path::new("N804.py"); "N804")]
    #[test_case(CheckCode::N805, Path::new("N805.py"); "N805")]
    #[test_case(CheckCode::N806, Path::new("N806.py"); "N806")]
    #[test_case(CheckCode::N807, Path::new("N807.py"); "N807")]
    #[test_case(CheckCode::N811, Path::new("N811.py"); "N811")]
    #[test_case(CheckCode::N812, Path::new("N812.py"); "N812")]
    #[test_case(CheckCode::N813, Path::new("N813.py"); "N813")]
    #[test_case(CheckCode::N814, Path::new("N814.py"); "N814")]
    #[test_case(CheckCode::N815, Path::new("N815.py"); "N815")]
    #[test_case(CheckCode::N816, Path::new("N816.py"); "N816")]
    #[test_case(CheckCode::N817, Path::new("N817.py"); "N817")]
    #[test_case(CheckCode::N818, Path::new("N818.py"); "N818")]
    #[test_case(CheckCode::S101, Path::new("S101.py"); "S101")]
    #[test_case(CheckCode::S102, Path::new("S102.py"); "S102")]
    #[test_case(CheckCode::S104, Path::new("S104.py"); "S104")]
    #[test_case(CheckCode::S105, Path::new("S105.py"); "S105")]
    #[test_case(CheckCode::S106, Path::new("S106.py"); "S106")]
    #[test_case(CheckCode::S107, Path::new("S107.py"); "S107")]
    #[test_case(CheckCode::T201, Path::new("T201.py"); "T201")]
    #[test_case(CheckCode::T203, Path::new("T203.py"); "T203")]
    #[test_case(CheckCode::U001, Path::new("U001.py"); "U001")]
    #[test_case(CheckCode::U003, Path::new("U003.py"); "U003")]
    #[test_case(CheckCode::U004, Path::new("U004.py"); "U004")]
    #[test_case(CheckCode::U005, Path::new("U005.py"); "U005")]
    #[test_case(CheckCode::U006, Path::new("U006.py"); "U006")]
    #[test_case(CheckCode::U007, Path::new("U007.py"); "U007")]
    #[test_case(CheckCode::U008, Path::new("U008.py"); "U008")]
    #[test_case(CheckCode::U009, Path::new("U009_0.py"); "U009_0")]
    #[test_case(CheckCode::U009, Path::new("U009_1.py"); "U009_1")]
    #[test_case(CheckCode::U009, Path::new("U009_2.py"); "U009_2")]
    #[test_case(CheckCode::U009, Path::new("U009_3.py"); "U009_3")]
    #[test_case(CheckCode::U010, Path::new("U010.py"); "U010")]
    #[test_case(CheckCode::U011, Path::new("U011_0.py"); "U011_0")]
    #[test_case(CheckCode::U011, Path::new("U011_1.py"); "U011_1")]
    #[test_case(CheckCode::U012, Path::new("U012.py"); "U012")]
    #[test_case(CheckCode::U013, Path::new("U013.py"); "U013")]
    #[test_case(CheckCode::U014, Path::new("U014.py"); "U014")]
    #[test_case(CheckCode::W292, Path::new("W292_0.py"); "W292_0")]
    #[test_case(CheckCode::W292, Path::new("W292_1.py"); "W292_1")]
    #[test_case(CheckCode::W292, Path::new("W292_2.py"); "W292_2")]
    #[test_case(CheckCode::W605, Path::new("W605_0.py"); "W605_0")]
    #[test_case(CheckCode::W605, Path::new("W605_1.py"); "W605_1")]
    #[test_case(CheckCode::RUF001, Path::new("RUF001.py"); "RUF001")]
    #[test_case(CheckCode::RUF002, Path::new("RUF002.py"); "RUF002")]
    #[test_case(CheckCode::RUF003, Path::new("RUF003.py"); "RUF003")]
    #[test_case(CheckCode::YTT101, Path::new("YTT101.py"); "YTT101")]
    #[test_case(CheckCode::YTT102, Path::new("YTT102.py"); "YTT102")]
    #[test_case(CheckCode::YTT103, Path::new("YTT103.py"); "YTT103")]
    #[test_case(CheckCode::YTT201, Path::new("YTT201.py"); "YTT201")]
    #[test_case(CheckCode::YTT202, Path::new("YTT202.py"); "YTT202")]
    #[test_case(CheckCode::YTT203, Path::new("YTT203.py"); "YTT203")]
    #[test_case(CheckCode::YTT204, Path::new("YTT204.py"); "YTT204")]
    #[test_case(CheckCode::YTT301, Path::new("YTT301.py"); "YTT301")]
    #[test_case(CheckCode::YTT302, Path::new("YTT302.py"); "YTT302")]
    #[test_case(CheckCode::YTT303, Path::new("YTT303.py"); "YTT303")]
    #[test_case(CheckCode::FBT001, Path::new("FBT.py"); "FBT001")]
    #[test_case(CheckCode::FBT002, Path::new("FBT.py"); "FBT002")]
    #[test_case(CheckCode::FBT003, Path::new("FBT.py"); "FBT003")]
    fn checks(check_code: CheckCode, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", check_code.as_ref(), path.to_string_lossy());
        let mut checks = test_path(
            Path::new("./resources/test/fixtures").join(path).as_path(),
            &settings::Settings::for_rule(check_code),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test]
    fn f841_dummy_variable_rgx() -> Result<()> {
        let mut checks = test_path(
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
    fn m001() -> Result<()> {
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/M001.py"),
            &settings::Settings::for_rules(vec![CheckCode::M001, CheckCode::E501, CheckCode::F841]),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }

    #[test]
    fn init() -> Result<()> {
        let mut checks = test_path(
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
        let mut checks = test_path(
            Path::new("./resources/test/fixtures/future_annotations.py"),
            &settings::Settings::for_rules(vec![CheckCode::F401, CheckCode::F821]),
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(checks);
        Ok(())
    }
}
