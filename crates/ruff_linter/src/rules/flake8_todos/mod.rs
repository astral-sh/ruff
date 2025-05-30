pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};

    #[test_case(Rule::InvalidTodoTag, Path::new("TD001.py"))]
    #[test_case(Rule::MissingTodoAuthor, Path::new("TD002.py"))]
    #[test_case(Rule::MissingTodoLink, Path::new("TD003.py"))]
    #[test_case(Rule::MissingTodoColon, Path::new("TD004.py"))]
    #[test_case(Rule::MissingTodoDescription, Path::new("TD005.py"))]
    #[test_case(Rule::InvalidTodoCapitalization, Path::new("TD006.py"))]
    #[test_case(Rule::MissingSpaceAfterTodoColon, Path::new("TD007.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_todos").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
