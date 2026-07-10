use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;

use colored::Colorize;
use log::warn;
use pyproject_toml::PyProjectToml;
use ruff_python_ast::TomlSourceType;
use ruff_text_size::{TextRange, TextSize};

use ruff_db::diagnostic::Diagnostic;
use ruff_source_file::{SourceFile, SourceFileBuilder};

use crate::Locator;
use crate::fix::{FixResult, fix_file};
use crate::linter::{FixTable, MAX_ITERATIONS, report_failed_to_converge_error};
use crate::registry::Rule;
use crate::rules::ruff::rules::{InvalidPyprojectToml, rule_codes_in_selectors};
use crate::settings::LinterSettings;
use crate::settings::types::UnsafeFixes;
use crate::{IOError, Violation};

pub struct TomlFixerResult<'a> {
    pub diagnostics: Vec<Diagnostic>,
    pub transformed: Cow<'a, str>,
    pub fixed: FixTable,
}

/// RUF200
pub fn lint_toml(
    source_file: &SourceFile,
    settings: &LinterSettings,
    source_type: TomlSourceType,
) -> Vec<Diagnostic> {
    let mut messages = rule_codes_in_selectors(source_file, settings, source_type);

    if settings.rules.enabled(Rule::InvalidPyprojectToml) && source_type.is_pyproject() {
        let Some(err) = toml::from_str::<PyProjectToml>(source_file.source_text()).err() else {
            return messages;
        };

        let range = match err.span() {
            // This is bad but sometimes toml and/or serde just don't give us spans
            // TODO(konstin,micha): https://github.com/astral-sh/ruff/issues/4571
            None => TextRange::default(),
            Some(range) => {
                let Some(range) = text_range_from_std(range, source_file, settings, &mut messages)
                else {
                    return messages;
                };
                range
            }
        };

        let toml_err = err.message().to_string();
        let diagnostic =
            InvalidPyprojectToml { message: toml_err }.into_diagnostic(range, source_file);
        messages.push(diagnostic);
    }

    messages
}

/// Generate diagnostics for a TOML configuration file, iteratively applying fixes until the source
/// stabilizes.
pub fn lint_fix_toml<'a>(
    source_file: &'a SourceFile,
    settings: &LinterSettings,
    source_type: TomlSourceType,
    unsafe_fixes: UnsafeFixes,
) -> TomlFixerResult<'a> {
    let mut diagnostics = lint_toml(source_file, settings, source_type);
    let mut transformed = Cow::Borrowed(source_file.source_text());
    let mut fixed = FixTable::default();
    let mut iterations = 0;

    loop {
        let locator = Locator::new(transformed.as_ref());
        let Some(FixResult { code, fixes, .. }) = fix_file(&diagnostics, &locator, unsafe_fixes)
        else {
            return TomlFixerResult {
                diagnostics,
                transformed,
                fixed,
            };
        };

        if iterations >= MAX_ITERATIONS {
            report_failed_to_converge_error(
                Path::new(source_file.name()),
                transformed.as_ref(),
                &diagnostics,
            );
            return TomlFixerResult {
                diagnostics,
                transformed,
                fixed,
            };
        }

        for (rule, name, count) in fixes.iter() {
            *fixed.entry(rule).or_default(name) += count;
        }

        transformed = Cow::Owned(code);
        iterations += 1;

        let transformed_source =
            SourceFileBuilder::new(source_file.name(), transformed.as_ref()).finish();
        diagnostics = lint_toml(&transformed_source, settings, source_type);
    }
}

/// Try to convert a `range` into a `TextRange`, emitting an `IOError` diagnostic if the file is too
/// large or a warning if the `IOError` lint rule is disabled.
pub(crate) fn text_range_from_std(
    range: Range<usize>,
    source_file: &SourceFile,
    settings: &LinterSettings,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TextRange> {
    let Ok(end) = TextSize::try_from(range.end) else {
        let message = format!(
            "{} is larger than 4GB, but ruff assumes all files to be smaller",
            source_file.name(),
        );
        if settings.rules.enabled(Rule::IOError) {
            let diagnostic = IOError { message }.into_diagnostic(TextRange::default(), source_file);
            diagnostics.push(diagnostic);
        } else {
            warn!(
                "{}{}{} {message}",
                "Failed to lint ".bold(),
                source_file.name().bold(),
                ":".bold()
            );
        }
        return None;
    };
    Some(TextRange::new(
        // start <= end, so if end < 4GB follows start < 4GB
        TextSize::try_from(range.start).unwrap(),
        end,
    ))
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;

    use ruff_db::diagnostic::{DisplayDiagnosticConfig, DisplayDiagnostics, DummyFileResolver};
    use ruff_python_ast::TomlSourceType;
    use ruff_source_file::SourceFileBuilder;

    use crate::codes::Rule;
    use crate::settings::LinterSettings;
    use crate::settings::types::UnsafeFixes;

    use super::lint_fix_toml;

    #[test]
    fn fixes_toml() {
        let source_file = SourceFileBuilder::new("ruff.toml", r#"lint.select = ["F401"]"#).finish();
        let settings = LinterSettings::for_rule(Rule::RuleCodesInSelectors).with_preview_mode();

        let result = lint_fix_toml(
            &source_file,
            &settings,
            TomlSourceType::Ruff,
            UnsafeFixes::Disabled,
        );

        let result = std::fmt::from_fn(|f| {
            let count = result.fixed.counts().sum::<usize>();
            writeln!(
                f,
                "Applied {count} fix{es}",
                es = if count != 1 { "es" } else { "" }
            )?;
            writeln!(
                f,
                "\n## Transformed output\n\n```toml\n{}\n```",
                result.transformed
            )?;
            if !result.diagnostics.is_empty() {
                writeln!(
                    f,
                    "\n## Remaining diagnostics\n\n{}",
                    DisplayDiagnostics::new(
                        &DummyFileResolver,
                        &DisplayDiagnosticConfig::new("ruff"),
                        &result.diagnostics
                    )
                )?;
            }
            Ok(())
        });

        assert_snapshot!(
            result,
            @r#"
        Applied 1 fix

        ## Transformed output

        ```toml
        lint.select = ["unused-import"]
        ```
        "#,
        );
    }
}
