use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{Definition, Member, MemberKind, SemanticModel};

/// Return the name of the function, if it's overloaded.
pub(crate) fn overloaded_name(definition: &Definition, semantic: &SemanticModel) -> Option<String> {
    let Definition::Member(Member {
        kind:
            MemberKind::Function(function)
            | MemberKind::NestedFunction(function)
            | MemberKind::Method(function),
        ..
    }) = definition
    else {
        return None;
    };

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
    let Definition::Member(Member {
        kind:
            MemberKind::Function(function)
            | MemberKind::NestedFunction(function)
            | MemberKind::Method(function),
        ..
    }) = definition
    else {
        return false;
    };

    if visibility::is_overload(&function.decorator_list, semantic) {
        false
    } else {
        function.name.as_str() == overloaded_name
    }
}
