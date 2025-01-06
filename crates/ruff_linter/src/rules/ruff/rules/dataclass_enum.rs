use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{dataclass_kind, DataclassKind};
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::StmtClassDef;
use ruff_python_semantic::analyze::class::is_enumeration;
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for enum classes which are also decorated with `@dataclass`.
///
/// ## Why is this bad?
/// Decorating an enum with `@dataclass()` does not cause any errors at runtime,
/// but may cause erroneous results:
///
/// ```python
/// @dataclass
/// class E(Enum):
///     A = 1
///     B = 2
///
/// print(E.A == E.B)  # True
/// ```
///
/// ## Example
///
/// ```python
/// from dataclasses import dataclass
/// from enum import Enum
///
///
/// @dataclass
/// class E(Enum): ...
/// ```
///
/// Use instead:
///
/// ```python
/// from enum import Enum
///
///
/// class E(Enum): ...
/// ```
///
/// ## References
/// - [Python documentation: Enum HOWTO &sect; Dataclass support](https://docs.python.org/3/howto/enum.html#dataclass-support)
#[derive(ViolationMetadata)]
pub(crate) struct DataclassEnum;

impl AlwaysFixableViolation for DataclassEnum {
    #[derive_message_formats]
    fn message(&self) -> String {
        "An enum class should not be decorated with `@dataclass`".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove `@dataclass`".to_string()
    }
}

/// RUF049
pub(crate) fn dataclass_enum(checker: &mut Checker, class_def: &StmtClassDef) {
    let semantic = checker.semantic();

    let Some(DataclassKind::Stdlib { decorator }) = dataclass_kind(class_def, semantic) else {
        return;
    };

    if !is_enumeration(class_def, semantic) {
        return;
    }

    let decorator_last_line_full_end = checker.source().full_line_end(decorator.end());
    let edit = Edit::deletion(decorator.start(), decorator_last_line_full_end);
    let fix = Fix::unsafe_edit(edit);

    let diagnostic = Diagnostic::new(DataclassEnum, decorator.range);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}
