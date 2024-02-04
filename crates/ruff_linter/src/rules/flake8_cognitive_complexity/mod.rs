//! Rules from [flake8-cognitive-complexity](https://pypi.org/project/flake8-cognitive-complexity/).
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
    fn max_cognitive_complexity_zero(max_cognitive_complexity: usize) -> Result<()> {
        let snapshot = format!("max_cognitive_complexity_{max_cognitive_complexity}");
        let diagnostics = test_path(
            Path::new("flake8_cognitive_complexity/CCR001.py"),
            &LinterSettings {
                flake8_cognitive_complexity: super::settings::Settings {
                    max_cognitive_complexity,
                },
                ..LinterSettings::for_rules(vec![Rule::CognitiveComplexStructure])
            },
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
