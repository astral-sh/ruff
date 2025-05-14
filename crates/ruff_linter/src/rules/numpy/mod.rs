//! NumPy-specific rules.
pub(crate) mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::convert::AsRef;
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::NumpyDeprecatedTypeAlias, Path::new("NPY001.py"))]
    #[test_case(Rule::NumpyLegacyRandom, Path::new("NPY002.py"))]
    #[test_case(Rule::NumpyDeprecatedFunction, Path::new("NPY003.py"))]
    // The NPY201 tests are split into multiple files because they get fixed one by one and too many diagnostic exceed the max-iterations limit.
    #[test_case(Rule::Numpy2Deprecation, Path::new("NPY201.py"))]
    #[test_case(Rule::Numpy2Deprecation, Path::new("NPY201_2.py"))]
    #[test_case(Rule::Numpy2Deprecation, Path::new("NPY201_3.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.as_ref(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("numpy").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
