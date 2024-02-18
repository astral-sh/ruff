//! Rules from [flake8-class-newline](https://pypi.org/project/flake8-class-newline/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Path::new("CNL100.py"))]
    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_class_newline").join(path).as_path(),
            &settings::LinterSettings::for_rule(Rule::MissingClassNewLine),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
