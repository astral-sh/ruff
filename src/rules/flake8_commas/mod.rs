//! Rules from [flake8-commas](https://pypi.org/project/flake8-commas/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_yaml_snapshot, settings};

    #[test_case(Path::new("COM81.py"); "COM81")]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_commas").join(path).as_path(),
            &settings::Settings::for_rules(vec![
                Rule::TrailingCommaMissing,
                Rule::TrailingCommaOnBareTupleProhibited,
                Rule::TrailingCommaProhibited,
            ]),
        )?;
        assert_yaml_snapshot!(snapshot, diagnostics);
        Ok(())
    }
}
