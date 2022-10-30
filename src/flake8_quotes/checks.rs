use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};
use crate::flake8_quotes::settings::{Quote, Settings};
use crate::source_code_locator::SourceCodeLocator;

fn good_single(quote: &Quote) -> char {
    match quote {
        Quote::Single => '\'',
        Quote::Double => '"',
    }
}

fn bad_single(quote: &Quote) -> char {
    match quote {
        Quote::Double => '\'',
        Quote::Single => '"',
    }
}

fn good_multiline(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'''",
        Quote::Double => "\"\"\"",
    }
}

fn good_multiline_ending(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'\"\"\"",
        Quote::Double => "\"'''",
    }
}

fn good_docstring(quote: &Quote) -> &str {
    match quote {
        Quote::Single => "'''",
        Quote::Double => "\"\"\"",
    }
}

pub fn quotes(
    locator: &SourceCodeLocator,
    start: &Location,
    end: &Location,
    is_docstring: bool,
    settings: &Settings,
) -> Option<Check> {
    let text = locator.slice_source_code_range(&Range {
        location: *start,
        end_location: *end,
    });

    // Remove any prefixes (e.g., remove `u` from `u"foo"`).
    let last_quote_char = text.chars().last().unwrap();
    let first_quote_char = text.find(last_quote_char).unwrap();
    let prefix = &text[..first_quote_char].to_lowercase();
    let raw_text = &text[first_quote_char..];

    // Determine if the string is multiline-based.
    let is_multiline = if raw_text.len() >= 3 {
        let mut chars = raw_text.chars();
        let first = chars.next().unwrap();
        let second = chars.next().unwrap();
        let third = chars.next().unwrap();
        first == second && second == third
    } else {
        false
    };

    if is_docstring {
        if raw_text.contains(good_docstring(&settings.docstring_quotes)) {
            return None;
        }

        return Some(Check::new(
            CheckKind::BadQuotesDocstring(settings.docstring_quotes.clone()),
            Range {
                location: *start,
                end_location: *end,
            },
        ));
    } else if is_multiline {
        // If our string is or contains a known good string, ignore it.
        if raw_text.contains(good_multiline(&settings.multiline_quotes)) {
            return None;
        }

        // If our string ends with a known good ending, then ignore it.
        if raw_text.ends_with(good_multiline_ending(&settings.multiline_quotes)) {
            return None;
        }

        return Some(Check::new(
            CheckKind::BadQuotesMultilineString(settings.multiline_quotes.clone()),
            Range {
                location: *start,
                end_location: *end,
            },
        ));
    } else {
        let string_contents = &raw_text[1..raw_text.len() - 1];

        // If we're using the preferred quotation type, check for escapes.
        if last_quote_char == good_single(&settings.inline_quotes) {
            if !settings.avoid_escape || prefix.contains('r') {
                return None;
            }
            if string_contents.contains(good_single(&settings.inline_quotes))
                && !string_contents.contains(bad_single(&settings.inline_quotes))
            {
                return Some(Check::new(
                    CheckKind::AvoidQuoteEscape,
                    Range {
                        location: *start,
                        end_location: *end,
                    },
                ));
            }
            return None;
        }

        // If we're not using the preferred type, only allow use to avoid escapes.
        if !string_contents.contains(good_single(&settings.inline_quotes)) {
            return Some(Check::new(
                CheckKind::BadQuotesInlineString(settings.inline_quotes.clone()),
                Range {
                    location: *start,
                    end_location: *end,
                },
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use rustpython_parser::lexer::LexResult;
    use test_case::test_case;

    use crate::autofix::fixer;
    use crate::checks::{Check, CheckCode};
    use crate::flake8_quotes::settings::Quote;
    use crate::linter::tokenize;
    use crate::{flake8_quotes, linter, Settings};
    use crate::{fs, noqa};

    fn check_path(path: &Path, settings: &Settings, autofix: &fixer::Mode) -> Result<Vec<Check>> {
        let contents = fs::read_file(path)?;
        let tokens: Vec<LexResult> = tokenize(&contents);
        let noqa_line_for = noqa::extract_noqa_line_for(&tokens);
        linter::check_path(path, &contents, tokens, &noqa_line_for, settings, autofix)
    }

    #[test_case(Path::new("doubles.py"))]
    #[test_case(Path::new("doubles_escaped.py"))]
    #[test_case(Path::new("doubles_multiline_string.py"))]
    #[test_case(Path::new("doubles_noqa.py"))]
    #[test_case(Path::new("doubles_wrapped.py"))]
    fn doubles(path: &Path) -> Result<()> {
        let snapshot = format!("doubles_{}", path.to_string_lossy());
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("singles.py"))]
    #[test_case(Path::new("singles_escaped.py"))]
    #[test_case(Path::new("singles_multiline_string.py"))]
    #[test_case(Path::new("singles_noqa.py"))]
    #[test_case(Path::new("singles_wrapped.py"))]
    fn singles(path: &Path) -> Result<()> {
        let snapshot = format!("singles_{}", path.to_string_lossy());
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Double,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("docstring_doubles.py"))]
    #[test_case(Path::new("docstring_doubles_module_multiline.py"))]
    #[test_case(Path::new("docstring_doubles_module_singleline.py"))]
    #[test_case(Path::new("docstring_doubles_class.py"))]
    #[test_case(Path::new("docstring_doubles_function.py"))]
    #[test_case(Path::new("docstring_singles.py"))]
    #[test_case(Path::new("docstring_singles_module_multiline.py"))]
    #[test_case(Path::new("docstring_singles_module_singleline.py"))]
    #[test_case(Path::new("docstring_singles_class.py"))]
    #[test_case(Path::new("docstring_singles_function.py"))]
    fn double_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("double_docstring_{}", path.to_string_lossy());
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Single,
                    docstring_quotes: Quote::Double,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }

    #[test_case(Path::new("docstring_doubles.py"))]
    #[test_case(Path::new("docstring_doubles_module_multiline.py"))]
    #[test_case(Path::new("docstring_doubles_module_singleline.py"))]
    #[test_case(Path::new("docstring_doubles_class.py"))]
    #[test_case(Path::new("docstring_doubles_function.py"))]
    #[test_case(Path::new("docstring_singles.py"))]
    #[test_case(Path::new("docstring_singles_module_multiline.py"))]
    #[test_case(Path::new("docstring_singles_module_singleline.py"))]
    #[test_case(Path::new("docstring_singles_class.py"))]
    #[test_case(Path::new("docstring_singles_function.py"))]
    fn single_docstring(path: &Path) -> Result<()> {
        let snapshot = format!("single_docstring_{}", path.to_string_lossy());
        let mut checks = check_path(
            Path::new("./resources/test/fixtures/flake8_quotes")
                .join(path)
                .as_path(),
            &Settings {
                flake8_quotes: flake8_quotes::settings::Settings {
                    inline_quotes: Quote::Single,
                    multiline_quotes: Quote::Double,
                    docstring_quotes: Quote::Single,
                    avoid_escape: true,
                },
                ..Settings::for_rules(vec![
                    CheckCode::Q000,
                    CheckCode::Q001,
                    CheckCode::Q002,
                    CheckCode::Q003,
                ])
            },
            &fixer::Mode::Generate,
        )?;
        checks.sort_by_key(|check| check.location);
        insta::assert_yaml_snapshot!(snapshot, checks);
        Ok(())
    }
}
