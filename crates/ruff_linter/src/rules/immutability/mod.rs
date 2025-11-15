pub mod single_assignment_missing_final;

#[cfg(test)]
mod tests {
    use crate::registry::Rule;
    use crate::test::test_path;
    use crate::{assert_diagnostics, settings};
    use anyhow::Result;
    use std::path::Path;
    use test_case::test_case;

    #[test_case(Rule::SingleAssignmentMissingFinal, Path::new("annotated_final.py"))]
    #[test_case(
        Rule::SingleAssignmentMissingFinal,
        Path::new("augmented_assignment.py")
    )]
    #[test_case(Rule::SingleAssignmentMissingFinal, Path::new("delayed_annotation.py"))]
    #[test_case(Rule::SingleAssignmentMissingFinal, Path::new("for_loop.py"))]
    #[test_case(Rule::SingleAssignmentMissingFinal, Path::new("for_loop_more_complex.py"))]
    #[test_case(Rule::SingleAssignmentMissingFinal, Path::new("reassigned.py"))]
    #[test_case(Rule::SingleAssignmentMissingFinal, Path::new("reassigned_function.py"))]
    #[test_case(
        Rule::SingleAssignmentMissingFinal,
        Path::new("single_assignment_local.py")
    )]
    #[test_case(
        Rule::SingleAssignmentMissingFinal,
        Path::new("single_assignment_module.py")
    )]
    #[test_case(
        Rule::SingleAssignmentMissingFinal,
        Path::new("unpacked_assignment.py")
    )]
    fn rules(rule_code: Rule, path: &Path) -> Result<()> {
        let snapshot = format!("{}_{}", rule_code.noqa_code(), path.to_string_lossy());
        let diagnostics = test_path(
            Path::new("immutability").join(path).as_path(),
            &settings::LinterSettings::for_rule(rule_code),
        )?;
        assert_diagnostics!(snapshot, diagnostics);
        Ok(())
    }
}
