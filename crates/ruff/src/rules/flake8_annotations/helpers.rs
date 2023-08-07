use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Definition, SemanticModel};

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name(definition: &Definition, semantic: &SemanticModel) -> Option<String> {
    let function = definition.as_function_def()?;
    if visibility::is_overload(&function.decorator_list, semantic) {
        Some(function.name.to_string())
    } else {
        None
    }
}

/// Return `true` if the definition is the implementation for an overloaded
/// function.
pub(crate) fn is_overload_impl(
    definition: &Definition,
    overloaded_name: &str,
    semantic: &SemanticModel,
) -> bool {
    let Some(function) = definition.as_function_def() else {
        return false;
    };
    if visibility::is_overload(&function.decorator_list, semantic) {
        false
    } else {
        function.name.as_str() == overloaded_name
    }
}
