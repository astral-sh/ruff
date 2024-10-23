//! Rules from [flake8-gettext](https://pypi.org/project/flake8-gettext/).
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr};

pub(crate) mod rules;
pub mod settings;

/// Returns true if the [`Expr`] is an internationalization function call.
pub(crate) fn is_gettext_func_call(func: &Expr, functions_names: &[Name]) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = func {
        functions_names.contains(id)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::FStringInGetTextFuncCall, Path::new("INT001.py"))]
    #[test_case(Rule::FormatInGetTextFuncCall, Path::new("INT002.py"))]
    #[test_case(Rule::PrintfInGetTextFuncCall, Path::new("INT003.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_gettext").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
