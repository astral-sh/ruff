pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_diagnostics;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(Rule::ComplexArumentAnnotation, Path::new("TAE002.py"))]
    #[test_case(Rule::ComplexArumentAnnotation, Path::new("TAE002_quoted.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.name(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_annotation_complexity")
                .join(path)
                .as_path(),
            &LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
