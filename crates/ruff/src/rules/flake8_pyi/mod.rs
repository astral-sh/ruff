//! Rules from [flake8-pyi](https://pypi.org/project/flake8-pyi/).
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;

    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::UnprefixedTypeParam, Path::new("PYI001.py"))]
    #[test_case(Rule::UnprefixedTypeParam, Path::new("PYI001.pyi"))]
    #[test_case(Rule::BadVersionInfoComparison, Path::new("PYI006.py"))]
    #[test_case(Rule::BadVersionInfoComparison, Path::new("PYI006.pyi"))]
    #[test_case(Rule::UnrecognizedPlatformCheck, Path::new("PYI007.py"))]
    #[test_case(Rule::UnrecognizedPlatformCheck, Path::new("PYI007.pyi"))]
    #[test_case(Rule::UnrecognizedPlatformName, Path::new("PYI008.py"))]
    #[test_case(Rule::UnrecognizedPlatformName, Path::new("PYI008.pyi"))]
    #[test_case(Rule::PassStatementStubBody, Path::new("PYI009.py"))]
    #[test_case(Rule::PassStatementStubBody, Path::new("PYI009.pyi"))]
    #[test_case(Rule::NonEmptyStubBody, Path::new("PYI010.py"))]
    #[test_case(Rule::NonEmptyStubBody, Path::new("PYI010.pyi"))]
    #[test_case(Rule::TypedArgumentDefaultInStub, Path::new("PYI011.py"))]
    #[test_case(Rule::TypedArgumentDefaultInStub, Path::new("PYI011.pyi"))]
    #[test_case(Rule::PassInClassBody, Path::new("PYI012.py"))]
    #[test_case(Rule::PassInClassBody, Path::new("PYI012.pyi"))]
    #[test_case(Rule::ArgumentDefaultInStub, Path::new("PYI014.py"))]
    #[test_case(Rule::ArgumentDefaultInStub, Path::new("PYI014.pyi"))]
    #[test_case(Rule::AssignmentDefaultInStub, Path::new("PYI015.py"))]
    #[test_case(Rule::AssignmentDefaultInStub, Path::new("PYI015.pyi"))]
    #[test_case(Rule::DuplicateUnionMember, Path::new("PYI016.py"))]
    #[test_case(Rule::DuplicateUnionMember, Path::new("PYI016.pyi"))]
    #[test_case(Rule::DocstringInStub, Path::new("PYI021.py"))]
    #[test_case(Rule::DocstringInStub, Path::new("PYI021.pyi"))]
    #[test_case(Rule::TypeCommentInStub, Path::new("PYI033.py"))]
    #[test_case(Rule::TypeCommentInStub, Path::new("PYI033.pyi"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("flake8_pyi").join(path).as_path(),
            &settings::Settings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
