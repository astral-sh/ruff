use std::borrow::Cow;
use std::path::Path;

use pyproject_toml::PyProjectToml;

use ruff_db::diagnostic::Diagnostic;
use ruff_python_ast::TomlSourceType;

use crate::Locator;
use crate::checkers::ast::LintContext;
use crate::fix::{FixResult, fix_file};
use crate::linter::{FixTable, MAX_ITERATIONS, report_failed_to_converge_error};
use crate::registry::Rule;
use crate::rules::ruff::rules::{invalid_pyproject_toml, rule_codes_in_selectors};
use crate::settings::LinterSettings;
use crate::settings::types::UnsafeFixes;

pub struct TomlFixerResult<'a> {
    pub diagnostics: Vec<Diagnostic>,
    pub transformed: Cow<'a, str>,
    pub fixed: FixTable,
}

pub fn lint_toml(
    path: &Path,
    contents: &str,
    settings: &LinterSettings,
    source_type: TomlSourceType,
) -> Vec<Diagnostic> {
    let context = LintContext::new(path, contents, settings);

    if let Err(err) = toml::from_str::<PyProjectToml>(contents) {
        if source_type.is_pyproject() && context.is_rule_enabled(Rule::InvalidPyprojectToml) {
            invalid_pyproject_toml(&context, &err);
        }
    }

    if context.is_rule_enabled(Rule::RuleCodesInSelectors) {
        rule_codes_in_selectors(&context, source_type);
    }

    context.into_diagnostics()
}

/// Generate [`Diagnostic`]s for a TOML configuration file, iteratively fixing until stable.
pub fn lint_fix_toml<'a>(
    path: &Path,
    source: &'a str,
    settings: &LinterSettings,
    source_type: TomlSourceType,
    unsafe_fixes: UnsafeFixes,
) -> TomlFixerResult<'a> {
    let mut diagnostics = lint_toml(path, source, settings, source_type);
    let mut transformed = Cow::Borrowed(source);
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
            report_failed_to_converge_error(path, transformed.as_ref(), &diagnostics);
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

        diagnostics = lint_toml(path, transformed.as_ref(), settings, source_type);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use insta::assert_snapshot;

    use ruff_db::diagnostic::{DisplayDiagnosticConfig, DisplayDiagnostics, DummyFileResolver};
    use ruff_python_ast::TomlSourceType;

    use crate::codes::Rule;
    use crate::settings::LinterSettings;
    use crate::settings::types::UnsafeFixes;

    use super::lint_fix_toml;

    #[test]
    fn fixes_toml() {
        let source = r#"lint.select = ["F401"]"#;
        let settings = LinterSettings::for_rule(Rule::RuleCodesInSelectors).with_preview_mode();

        let result = lint_fix_toml(
            Path::new("ruff.toml"),
            source,
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
