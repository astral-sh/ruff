//! Rules from [flake8-async](https://pypi.org/project/flake8-async/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    fn rules(path: &Path) -> Result<()> {
        let snapshot = path.to_string_lossy().into_owned();
        let diagnostics = test_path(
            Path::new("flake8_async").join(path).as_path(),
            &settings::Settings::for_rules(vec![Rule::BlockingHttpCallInsideAsyncDef,
                                                Rule::OpenSleepOrSubprocessInsideAsyncDef,
                                                Rule::UnsafeOsMethodInsideAsyncDef]),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
