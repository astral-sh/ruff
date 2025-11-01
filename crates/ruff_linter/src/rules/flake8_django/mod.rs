//! Rules from [django-flake8](https://pypi.org/project/flake8-django/)
mod helpers;
pub(crate) mod rules;
pub mod settings;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};

    #[test_case(Rule::DjangoNullableModelStringField, Path::new("DJ001.py"))]
    #[test_case(Rule::DjangoLocalsInRenderFunction, Path::new("DJ003.py"))]
    #[test_case(Rule::DjangoExcludeWithModelForm, Path::new("DJ006.py"))]
    #[test_case(Rule::DjangoAllWithModelForm, Path::new("DJ007.py"))]
    #[test_case(Rule::DjangoModelWithoutDunderStr, Path::new("DJ008.py"))]
    #[test_case(Rule::DjangoURLPathWithoutTrailingSlash, Path::new("DJ100.py"))]
    #[test_case(Rule::DjangoURLPathWithLeadingSlash, Path::new("DJ101.py"))]
    #[test_case(Rule::DjangoUnorderedBodyContentInModel, Path::new("DJ012.py"))]
    #[test_case(Rule::DjangoNonLeadingReceiverDecorator, Path::new("DJ013.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_django").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }

    #[test]
    fn test_additional_path_functions_dj100() -> Result<()> {
        let mut settings =
            settings::LinterSettings::for_rule(Rule::DjangoURLPathWithoutTrailingSlash);
        settings.flake8_django.additional_path_functions = vec!["mytools.path".to_string()];

        let diagnostics = test_path(Path::new("flake8_django/DJ100_custom_paths.py"), &settings)?;
        assert_diagnostics!("DJ100_custom_paths.py", diagnostics);
        Ok(())
    }

    #[test]
    fn test_additional_path_functions_dj101() -> Result<()> {
        let mut settings = settings::LinterSettings::for_rule(Rule::DjangoURLPathWithLeadingSlash);
        settings.flake8_django.additional_path_functions = vec!["mytools.path".to_string()];

        let diagnostics = test_path(Path::new("flake8_django/DJ101_custom_paths.py"), &settings)?;
        assert_diagnostics!("DJ101_custom_paths.py", diagnostics);
        Ok(())
    }
}
