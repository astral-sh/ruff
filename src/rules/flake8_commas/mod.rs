pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::linter::test_path;
    use crate::registry::RuleCode;
    use crate::settings;

    #[test_case(Path::new("COM81.py"); "COM81")]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("./resources/test/fixtures/flake8_commas")
                .join(path)
                .as_path(),
            &settings::Settings::for_rules(vec![
                RuleCode::COM812,
                RuleCode::COM818,
                RuleCode::COM819,
            ]),
        )?;
        insta::assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
