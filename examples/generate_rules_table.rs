/// Generate a Markdown-compatible table of supported lint rules.
use ruff::checks::CheckKind;

fn main() {
    let mut check_kinds: Vec<CheckKind> = vec![
        CheckKind::AssertTuple,
        CheckKind::DefaultExceptNotLast,
        CheckKind::DuplicateArgumentName,
        CheckKind::FStringMissingPlaceholders,
        CheckKind::IfTuple,
        CheckKind::IOError("...".to_string()),
        CheckKind::ImportStarUsage,
        CheckKind::LineTooLong,
        CheckKind::DoNotAssignLambda,
        CheckKind::ModuleImportNotAtTopOfFile,
        CheckKind::NoAssertEquals,
        CheckKind::RaiseNotImplemented,
        CheckKind::ReturnOutsideFunction,
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
