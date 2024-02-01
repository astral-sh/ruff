//! Rules from [refurb](https://pypi.org/project/refurb/)/

mod helpers;
pub(crate) mod rules;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use test_case::test_case;

    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_messages, settings};

    #[test_case(Rule::ReadWholeFile, Path::new("FURB101.py"))]
    #[test_case(Rule::RepeatedAppend, Path::new("FURB113.py"))]
    #[test_case(Rule::ReimplementedOperator, Path::new("FURB118.py"))]
    #[test_case(Rule::DeleteFullSlice, Path::new("FURB131.py"))]
    #[test_case(Rule::CheckAndRemoveFromSet, Path::new("FURB132.py"))]
    #[test_case(Rule::IfExprMinMax, Path::new("FURB136.py"))]
    #[test_case(Rule::ReimplementedStarmap, Path::new("FURB140.py"))]
    #[test_case(Rule::SliceCopy, Path::new("FURB145.py"))]
    #[test_case(Rule::UnnecessaryEnumerate, Path::new("FURB148.py"))]
    #[test_case(Rule::MathConstant, Path::new("FURB152.py"))]
    #[test_case(Rule::PrintEmptyString, Path::new("FURB105.py"))]
    #[test_case(Rule::ImplicitCwd, Path::new("FURB177.py"))]
    #[test_case(Rule::SingleItemMembershipTest, Path::new("FURB171.py"))]
    #[test_case(Rule::BitCount, Path::new("FURB161.py"))]
    #[test_case(Rule::RegexFlagAlias, Path::new("FURB167.py"))]
    #[test_case(Rule::IsinstanceTypeNone, Path::new("FURB168.py"))]
    #[test_case(Rule::TypeNoneComparison, Path::new("FURB169.py"))]
    #[test_case(Rule::RedundantLogBase, Path::new("FURB163.py"))]
    #[test_case(Rule::MetaClassABCMeta, Path::new("FURB180.py"))]
    #[test_case(Rule::HashlibDigestHex, Path::new("FURB181.py"))]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("refurb").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_messages!(snapshot, diagnostics);
        Ok(())
    }
}
