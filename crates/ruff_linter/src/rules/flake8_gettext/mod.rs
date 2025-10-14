//! Rules from [flake8-gettext](https://pypi.org/project/flake8-gettext/).
use crate::checkers::ast::Checker;
use crate::preview::is_extended_i18n_function_matching_enabled;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;

pub(crate) mod rules;
pub mod settings;

/// Returns true if the [`Expr`] is an internationalization function call.
pub(crate) fn is_gettext_func_call(
    checker: &Checker,
    func: &Expr,
    functions_names: &[Name],
) -> bool {
    if func
        .as_name_expr()
        .map(ast::ExprName::id)
        .is_some_and(|id| functions_names.contains(id))
    {
        return true;
    }

    if !is_extended_i18n_function_matching_enabled(checker.settings()) {
        return false;
    }

    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return false;
    };

    if semantic.seen_module(Modules::BUILTINS)
        && matches!(
            qualified_name.segments(),
            ["builtins", id] if functions_names.contains(&Name::new(id)),
        )
    {
        return true;
    }

    matches!(
        qualified_name.segments(),
        ["gettext", "gettext" | "ngettext"]
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::settings::types::PreviewMode;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};

    #[test_case(Rule::FStringInGetTextFuncCall, Path::new("INT001.py"))]
    #[test_case(Rule::FormatInGetTextFuncCall, Path::new("INT002.py"))]
    #[test_case(Rule::PrintfInGetTextFuncCall, Path::new("INT003.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_gettext").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test_case(Rule::FStringInGetTextFuncCall, Path::new("INT001.py"))]
    #[test_case(Rule::FormatInGetTextFuncCall, Path::new("INT002.py"))]
    #[test_case(Rule::PrintfInGetTextFuncCall, Path::new("INT003.py"))]
    fn rules_preview(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("preview__{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_gettext").join(path).as_path(),
            &settings::LinterSettings {
                preview: PreviewMode::Enabled,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
