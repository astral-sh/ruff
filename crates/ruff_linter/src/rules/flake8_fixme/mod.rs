pub mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};

    #[test_case(Rule::LineContainsFixme; "T001")]
    #[test_case(Rule::LineContainsHack; "T002")]
    #[test_case(Rule::LineContainsTodo; "T003")]
    #[test_case(Rule::LineContainsXxx; "T004")]
    fn rules(rule_code: Rule) -> Result<()> {
        let snapshot = format!("{}_T00.py", rule_code.name());
        let diagnostics = test_path(
            Path::new("flake8_fixme/T00.py"),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
