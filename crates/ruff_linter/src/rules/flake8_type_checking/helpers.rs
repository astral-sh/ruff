use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::{self as ast};
use ruff_python_semantic::{Binding, BindingId, BindingKind, ScopeKind, SemanticModel};
use rustc_hash::FxHashSet;

pub(crate) fn is_valid_runtime_import(binding: &Binding, semantic: &SemanticModel) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Import(..) | BindingKind::FromImport(..) | BindingKind::SubmoduleImport(..)
    ) {
        binding.context.is_runtime()
            && binding
                .references()
                .any(|reference_id| semantic.reference(reference_id).context().is_runtime())
    } else {
        false
    }
}

pub(crate) fn runtime_evaluated(
    base_classes: &[String],
    decorators: &[String],
    semantic: &SemanticModel,
) -> bool {
    if !base_classes.is_empty() {
        if runtime_evaluated_base_class(base_classes, semantic) {
            return true;
        }
    }
    if !decorators.is_empty() {
        if runtime_evaluated_decorators(decorators, semantic) {
            return true;
        }
    }
    false
}

fn runtime_evaluated_base_class(base_classes: &[String], semantic: &SemanticModel) -> bool {
    fn inner(
        class_def: &ast::StmtClassDef,
        base_classes: &[String],
        semantic: &SemanticModel,
        seen: &mut FxHashSet<BindingId>,
    ) -> bool {
        class_def.bases().iter().any(|expr| {
            // If the base class is itself runtime-evaluated, then this is too.
            // Ex) `class Foo(BaseModel): ...`
            if semantic
                .resolve_call_path(map_subscript(expr))
                .is_some_and(|call_path| {
                    base_classes
                        .iter()
                        .any(|base_class| from_qualified_name(base_class) == call_path)
                })
            {
                return true;
            }

            // If the base class extends a runtime-evaluated class, then this does too.
            // Ex) `class Bar(BaseModel): ...; class Foo(Bar): ...`
            if let Some(id) = semantic.lookup_attribute(map_subscript(expr)) {
                if seen.insert(id) {
                    let binding = semantic.binding(id);
                    if let Some(base_class) = binding
                        .kind
                        .as_class_definition()
                        .map(|id| &semantic.scopes[*id])
                        .and_then(|scope| scope.kind.as_class())
                    {
                        if inner(base_class, base_classes, semantic, seen) {
                            return true;
                        }
                    }
                }
            }
            false
        })
    }

    semantic
        .current_scope()
        .kind
        .as_class()
        .is_some_and(|class_def| {
            inner(class_def, base_classes, semantic, &mut FxHashSet::default())
        })
}

fn runtime_evaluated_decorators(decorators: &[String], semantic: &SemanticModel) -> bool {
    let ScopeKind::Class(class_def) = &semantic.current_scope().kind else {
        return false;
    };

    class_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| {
                decorators
                    .iter()
                    .any(|base_class| from_qualified_name(base_class) == call_path)
            })
    })
}
