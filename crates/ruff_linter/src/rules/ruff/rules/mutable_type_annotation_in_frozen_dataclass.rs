use crate::{
    Violation, checkers::ast::Checker, codes::Category, rules::ruff::helpers::is_frozen_dataclass,
};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StmtClassDef;
use ruff_python_ast::{Stmt, StmtAnnAssign};
use ruff_python_semantic::analyze::typing::is_immutable_annotation;
use ruff_text_size::Ranged;

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
