use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::{Binding, BindingKind, ScopeKind, SemanticModel};

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
    let ScopeKind::Class(class_def) = &semantic.current_scope().kind else {
        return false;
    };

    class_def.bases().iter().any(|base| {
        semantic.resolve_call_path(base).is_some_and(|call_path| {
            base_classes
                .iter()
                .any(|base_class| from_qualified_name(base_class) == call_path)
        })
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
