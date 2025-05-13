pub(crate) mod rules;


#[cfg(test)]
mod tests {
    use std::path::Path;
    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::MissingAllureId, Path::new("PTQA001.py"))]
    #[test_case(Rule::MissingTeamMarker, Path::new("PTQA002.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("ptqacore").join(path),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
