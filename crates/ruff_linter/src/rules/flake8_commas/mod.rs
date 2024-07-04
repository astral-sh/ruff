//! Rules from [flake8-commas](https://pypi.org/project/flake8-commas/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Path::new("COM81.py"))]
    #[test_case(Path::new("COM81_syntax_error.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_commas").join(path).as_path(),
            &settings::LinterSettings::for_rules(vec![
                Rule::MissingTrailingComma,
                Rule::TrailingCommaOnBareTuple,
                Rule::ProhibitedTrailingComma,
            ]),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
