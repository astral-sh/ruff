use num_traits::Zero;
use rustpython_parser::ast::{self, Constant, Expr};

use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::map_callable;
use ruff_python_semantic::binding::{Binding, BindingKind, ExecutionContext};
use ruff_python_semantic::model::SemanticModel;
use ruff_python_semantic::scope::ScopeKind;

/// Return `true` if [`Expr`] is a guard for a type-checking block.
pub(crate) fn is_type_checking_block(model: &SemanticModel, test: &Expr) -> bool {
    // Ex) `if False:`
    if matches!(
        test,
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bool(false),
            ..
        })
    ) {
        return true;
    }

    // Ex) `if 0:`
    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Int(value),
        ..
    }) = &test
    {
        if value.is_zero() {
            return true;
        }
    }

    // Ex) `if typing.TYPE_CHECKING:`
    if model.resolve_call_path(test).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TYPE_CHECKING"]
    }) {
        return true;
    }

    false
}

pub(crate) fn is_valid_runtime_import(binding: &Binding) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Importation(..)
            | BindingKind::FromImportation(..)
            | BindingKind::SubmoduleImportation(..)
    ) {
        !binding.runtime_usage.is_empty() && matches!(binding.context, ExecutionContext::Runtime)
    } else {
        false
    }
}

pub(crate) fn runtime_evaluated(
    model: &SemanticModel,
    base_classes: &[String],
    decorators: &[String],
) -> bool {
    if !base_classes.is_empty() {
        if runtime_evaluated_base_class(model, base_classes) {
            return true;
        }
    }
    if !decorators.is_empty() {
        if runtime_evaluated_decorators(model, decorators) {
            return true;
        }
    }
    false
}

fn runtime_evaluated_base_class(model: &SemanticModel, base_classes: &[String]) -> bool {
    if let ScopeKind::Class(class_def) = &model.scope().kind {
        for base in class_def.bases.iter() {
            if let Some(call_path) = model.resolve_call_path(base) {
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

fn runtime_evaluated_decorators(model: &SemanticModel, decorators: &[String]) -> bool {
    if let ScopeKind::Class(class_def) = &model.scope().kind {
        for decorator in class_def.decorator_list.iter() {
            if let Some(call_path) = model.resolve_call_path(map_callable(decorator)) {
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
