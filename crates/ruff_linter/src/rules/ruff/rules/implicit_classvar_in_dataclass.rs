use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::{is_dunder, map_subscript};
use ruff_python_ast::{Expr, ExprName, ExprSubscript, Stmt, StmtAssign, StmtClassDef};
use ruff_python_semantic::analyze::class::{
    any_base_class, any_member_declaration, ClassMemberKind,
};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::ruff::rules::helpers::{dataclass_kind, DataclassKind};

/// ## What it does
/// Checks for implicit class variables in dataclasses.
///
/// Variables matching the [`lint.dummy-variable-rgx`] are excluded
/// from this rule.
///
/// ## Why is this bad?
/// Class variables are shared between all instances of that class.
/// In dataclasses, fields with no annotations at all
/// are implicitly considered class variables, and a `TypeError` is
/// raised if a user attempts to initialize an instance of the class
/// with this field.
///
///
/// ```python
/// @dataclass
/// class C:
///     a = 1
///     b: str = ""
///
/// C(a = 42)  # TypeError: C.__init__() got an unexpected keyword argument 'a'
/// ```
///
/// ## Example
///
/// ```python
/// @dataclass
/// class C:
///     a = 1
/// ```
///
/// Use instead:
///
/// ```python
/// from typing import ClassVar
///
///
/// @dataclass
/// class C:
///     a: ClassVar[int] = 1
/// ```
///
/// ## Options
/// - [`lint.dummy-variable-rgx`]
#[derive(ViolationMetadata)]
pub(crate) struct ImplicitClassVarInDataclass;

impl Violation for ImplicitClassVarInDataclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Assignment without annotation found in dataclass body".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `ClassVar[...]`".to_string())
    }
}

/// RUF045
pub(crate) fn implicit_class_var_in_dataclass(checker: &mut Checker, class_def: &StmtClassDef) {
    let semantic = checker.semantic();
    let dataclass_kind = dataclass_kind(class_def, semantic);

    if !matches!(dataclass_kind, Some((DataclassKind::Stdlib, _))) {
        return;
    };

    for statement in &class_def.body {
        let Stmt::Assign(StmtAssign { targets, .. }) = statement else {
            continue;
        };

        if targets.len() > 1 {
            continue;
        }

        let target = targets.first().unwrap();
        let Expr::Name(ExprName { id, .. }) = target else {
            continue;
        };

        if checker.settings.dummy_variable_rgx.is_match(id.as_str()) {
            continue;
        }

        if is_dunder(id.as_str()) {
            continue;
        }

        if might_have_class_var_annotation_in_superclass(id, class_def, semantic) {
            continue;
        }

        let diagnostic = Diagnostic::new(ImplicitClassVarInDataclass, target.range());

        checker.report_diagnostic(diagnostic);
    }
}

/// Inspect each base class:
///
/// * If a base class is not inspectable, return true.
/// * If there is a member with the same `id` whose annotation has `ClassVar`, return true.
///
/// Otherwise, return false.
fn might_have_class_var_annotation_in_superclass(
    id: &str,
    class_def: &StmtClassDef,
    semantic: &SemanticModel,
) -> bool {
    if class_def.bases().is_empty() {
        return false;
    }

    any_base_class(class_def, semantic, &mut |base| {
        let Expr::Name(name) = map_subscript(base) else {
            return false;
        };

        let Some(binding) = semantic.only_binding(name).map(|id| semantic.binding(id)) else {
            return true;
        };
        let Some(Stmt::ClassDef(base_class_def)) = binding.statement(semantic) else {
            return true;
        };

        any_member_declaration(base_class_def, &mut |declaration| {
            let ClassMemberKind::AnnAssign(ann_assign) = declaration.kind() else {
                return false;
            };

            let Expr::Name(name) = &*ann_assign.target else {
                return false;
            };

            if name.id != id {
                return false;
            }

            annotation_contains_class_var(&ann_assign.annotation, semantic)
        })
    })
}

fn annotation_contains_class_var(annotation: &Expr, semantic: &SemanticModel) -> bool {
    if !semantic.seen_typing() {
        return false;
    }

    let Expr::Subscript(ExprSubscript { value, slice, .. }) = annotation else {
        return false;
    };

    let Some(qualified_name) = semantic.resolve_qualified_name(value) else {
        return false;
    };

    match qualified_name.segments() {
        ["typing" | "_typeshed" | "typing_extensions", "ClassVar"] => true,

        ["typing" | "_typeshed" | "typing_extensions", "Final"] => {
            if matches!(&**slice, Expr::Tuple(_)) {
                return false;
            }

            annotation_contains_class_var(slice, semantic)
        }

        ["typing" | "_typeshed" | "typing_extensions", "Annotated"] => {
            let Expr::Tuple(tuple) = &**slice else {
                return false;
            };
            let Some(wrapped) = tuple.elts.first() else {
                return false;
            };

            annotation_contains_class_var(wrapped, semantic)
        }

        _ => false,
    }
}
