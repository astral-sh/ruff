use rustpython_parser::ast;

use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::{Binding, BindingKind, ScopeKind, SemanticModel};

pub(crate) fn is_valid_runtime_import(semantic_model: &SemanticModel, binding: &Binding) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Importation(..)
            | BindingKind::FromImportation(..)
            | BindingKind::SubmoduleImportation(..)
    ) {
        binding.context.is_runtime()
            && binding.references().any(|reference_id| {
                semantic_model
                    .reference(reference_id)
                    .context()
                    .is_runtime()
            })
    } else {
        false
    }
}

pub(crate) fn runtime_evaluated(
    semantic_model: &SemanticModel,
    base_classes: &[String],
    decorators: &[String],
) -> bool {
    if !base_classes.is_empty() {
        if runtime_evaluated_base_class(semantic_model, base_classes) {
            return true;
        }
    }
    if !decorators.is_empty() {
        if runtime_evaluated_decorators(semantic_model, decorators) {
            return true;
        }
    }
    false
}

fn runtime_evaluated_base_class(semantic_model: &SemanticModel, base_classes: &[String]) -> bool {
    if let ScopeKind::Class(ast::StmtClassDef { bases, .. }) = &semantic_model.scope().kind {
        for base in bases.iter() {
            if let Some(call_path) = semantic_model.resolve_call_path(base) {
                if base_classes
                    .iter()
                    .any(|base_class| from_qualified_name(base_class) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn runtime_evaluated_decorators(semantic_model: &SemanticModel, decorators: &[String]) -> bool {
    if let ScopeKind::Class(ast::StmtClassDef { decorator_list, .. }) = &semantic_model.scope().kind
    {
        for decorator in decorator_list.iter() {
            if let Some(call_path) =
                semantic_model.resolve_call_path(map_callable(&decorator.expression))
            {
                if decorators
                    .iter()
                    .any(|decorator| from_qualified_name(decorator) == call_path)
                {
                    return true;
                }
            }
        }
    }
    false
}
