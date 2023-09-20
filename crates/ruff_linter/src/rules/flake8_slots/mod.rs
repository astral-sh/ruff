//! Rules from [flake8-slots](https://pypi.org/project/flake8-slots/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::NoSlotsInStrSubclass, Path::new("SLOT000.py"))]
    #[test_case(Rule::NoSlotsInTupleSubclass, Path::new("SLOT001.py"))]
    #[test_case(Rule::NoSlotsInNamedtupleSubclass, Path::new("SLOT002.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_slots").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
