/// Generate a Markdown-compatible table of supported lint rules.
use ruff::checks::{CheckKind, RejectedCmpop};

fn main() {
    let mut check_kinds: Vec<CheckKind> = vec![
        CheckKind::AmbiguousClassName("...".to_string()),
        CheckKind::AmbiguousFunctionName("...".to_string()),
        CheckKind::AmbiguousVariableName("...".to_string()),
        CheckKind::AssertTuple,
        CheckKind::BreakOutsideLoop,
        CheckKind::ContinueOutsideLoop,
        CheckKind::DefaultExceptNotLast,
        CheckKind::DoNotAssignLambda,
        CheckKind::DoNotUseBareExcept,
        CheckKind::DuplicateArgumentName,
        CheckKind::FStringMissingPlaceholders,
        CheckKind::ForwardAnnotationSyntaxError("...".to_string()),
        CheckKind::FutureFeatureNotDefined("...".to_string()),
        CheckKind::IOError("...".to_string()),
        CheckKind::IfTuple,
        CheckKind::ImportStarNotPermitted("...".to_string()),
        CheckKind::ImportStarUsage("...".to_string()),
        CheckKind::InvalidPrintSyntax,
        CheckKind::IsLiteral,
        CheckKind::LateFutureImport,
        CheckKind::LineTooLong(89, 88),
        CheckKind::ModuleImportNotAtTopOfFile,
        CheckKind::MultiValueRepeatedKeyLiteral,
        CheckKind::MultiValueRepeatedKeyVariable("...".to_string()),
        CheckKind::NoAssertEquals,
        CheckKind::NoneComparison(RejectedCmpop::Eq),
        CheckKind::NotInTest,
        CheckKind::NotIsTest,
        CheckKind::RaiseNotImplemented,
        CheckKind::ReturnOutsideFunction,
        CheckKind::SyntaxError("...".to_string()),
        CheckKind::TooManyExpressionsInStarredAssignment,
        CheckKind::TrueFalseComparison(true, RejectedCmpop::Eq),
        CheckKind::TwoStarredExpressions,
        CheckKind::TypeComparison,
        CheckKind::UndefinedExport("...".to_string()),
        CheckKind::UndefinedLocal("...".to_string()),
        CheckKind::UndefinedName("...".to_string()),
        CheckKind::UnusedImport("...".to_string()),
        CheckKind::UnusedVariable("...".to_string()),
        CheckKind::UselessObjectInheritance("...".to_string()),
        CheckKind::YieldOutsideFunction,
    ];
    check_kinds.sort_by_key(|check_kind| check_kind.code());

    println!("| Code | Name | Message |");
    println!("| ---- | ----- | ------- |");
    for check_kind in check_kinds {
        println!(
            "| {} | {} | {} |",
            check_kind.code().as_str(),
            check_kind.name(),
            check_kind.body()
        );
    }
}
