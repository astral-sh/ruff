//! FastAPI-specific rules.
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::FastApiRedundantResponseModel, Path::new("FAST001.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_0.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_1.py"))]
    #[test_case(Rule::FastApiUnusedPathParameter, Path::new("FAST003.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("fastapi").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }

    // FAST002 autofixes use `typing_extensions` on Python 3.8,
    // since `typing.Annotated` was added in Python 3.9
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_0.py"))]
    #[test_case(Rule::FastApiNonAnnotatedDependency, Path::new("FAST002_1.py"))]
    fn rules_py38(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}_py38", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("fastapi").join(path).as_path(),
            &settings::LinterSettings {
                unresolved_target_version: ruff_python_ast::PythonVersion::PY38,
                ..settings::LinterSettings::for_rule(rule_code)
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
