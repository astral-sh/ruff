//! Rules from [django-flake8](https://pypi.org/project/flake8-django/)
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::DjangoNullableModelStringField, Path::new("DJ001.py"); "DJ001")]
    #[test_case(Rule::DjangoLocalsInRenderFunction, Path::new("DJ003.py"); "DJ003")]
    #[test_case(Rule::DjangoExcludeWithModelForm, Path::new("DJ006.py"); "DJ006")]
    #[test_case(Rule::DjangoAllWithModelForm, Path::new("DJ007.py"); "DJ007")]
    #[test_case(Rule::DjangoModelWithoutDunderStr, Path::new("DJ008.py"); "DJ008")]
    #[test_case(Rule::DjangoUnorderedBodyContentInModel, Path::new("DJ012.py"); "DJ012")]
    #[test_case(Rule::DjangoNonLeadingReceiverDecorator, Path::new("DJ013.py"); "DJ013")]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_django").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
