use crate::{
    Violation, checkers::ast::Checker, codes::Category, rules::ruff::helpers::is_frozen_dataclass,
};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StmtClassDef;
use ruff_python_ast::{Stmt, StmtAnnAssign};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;
use ruff_text_size::Ranged;

/// ## What it does
/// 
/// Detects whether a frozen dataclass has a mutable type annotation
///
/// ## Why is this bad?
/// 
/// Frozen dataclasses are record classes in Python that are meant to be immutable.
/// As a result, declaring an attribute of a dataclass with a mutable type annotation
/// (e.g. list or dict) is senseless and should be avoided.
/// 
/// ## Example
/// 
/// ```py
/// from dataclasses import dataclass
///
/// @dataclass(frozen=True)
/// class SomeDataClass1:
///     list1: list[int]    # snapshot: mutable-type-annotation-in-frozen-dataclass
///
/// @dataclass(frozen=True)
/// class SomeDataClass2:
///     dict1: dict[int, str]   # snapshot: mutable-type-annotation-in-frozen-dataclass
///
/// @dataclass(frozen=True)
/// class SomeDataClass2:
///     set1: set[int]   # snapshot: mutable-type-annotation-in-frozen-dataclass
///
/// @dataclass(frozen=True)
/// class SomeDataClass3:
///     tuple1: tuple[int]  # No error
/// 
/// ```
///
/// ## Use instead
/// 
/// - Use tuples instead of lists 
/// - Use frozensets instead of sets
/// - Use frozendicts instead of dicts (for Python versions >= 3.15)
/// 
/// ```py
/// @dataclass(frozen=True)
/// 
/// class SomeDataClass1:
///     list1: tuple[int]
/// ```
///
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct MutableTypeAnnotationInFrozenDataclass;

impl Violation for MutableTypeAnnotationInFrozenDataclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not use mutable type annotations in a frozen dataclass".to_string()
    }
}

pub(crate) fn mutable_type_annotation_in_frozen_dataclass(
    checker: &Checker,
    classdef: &StmtClassDef,
) {
    let semantic = checker.semantic();

    let mut is_frozen = false;
    for decorator in &classdef.decorator_list {
        if is_frozen_dataclass(decorator, semantic) {
            is_frozen = true;
            break;
        }
    }

    if !is_frozen {
        return;
    }
    
    for statement in &classdef.body {
        let Stmt::AnnAssign(StmtAnnAssign { annotation, .. }) = statement else {
            continue;
        };

        if !is_immutable_annotation(annotation, semantic, &[]) {
            checker.report_diagnostic(MutableTypeAnnotationInFrozenDataclass, annotation.range());
        }
    }
}
