//! Rules from [mccabe](https://pypi.org/project/mccabe/).
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::assert_messages;
    use crate::registry::Rule;
    use crate::settings::LinterSettings;
    use crate::test::test_path;

    #[test_case(0)]
    #[test_case(3)]
    #[test_case(10)]
    fn max_complexity_zero(max_complexity: usize) -> Result<()> {
        let snapshot = format!("max_complexity_{max_complexity}");
        let diagnostics = test_path(
            Path::new("mccabe/C901.py"),
            &LinterSettings {
                mccabe: super::settings::Settings { max_complexity },
                ..LinterSettings::for_rules(vec![Rule::ComplexStructure])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
