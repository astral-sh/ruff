use itertools::Itertools;
use ruff_python_semantic::context::Context;
use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use ruff_python_stdlib::str::{is_lower, is_upper};

pub(crate) fn is_camelcase(name: &str) -> bool {
    !is_lower(name) && !is_upper(name) && !name.contains('_')
}

pub(crate) fn is_mixed_case(name: &str) -> bool {
    !is_lower(name)
        && name
            .strip_prefix('_')
            .unwrap_or(name)
            .chars()
            .next()
            .map_or_else(|| false, char::is_lowercase)
}

pub(crate) fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

pub(crate) fn is_named_tuple_assignment(context: &Context, stmt: &Stmt) -> bool {
    let StmtKind::Assign { value, .. } = &stmt.node else {
        return false;
    };
    let ExprKind::Call {func, ..} = &value.node else {
        return false;
    };
    context.resolve_call_path(func).map_or(false, |call_path| {
        matches!(
            call_path.as_slice(),
            ["collections", "namedtuple"] | ["typing", "NamedTuple"]
        )
    })
}

pub(crate) fn is_typed_dict_assignment(context: &Context, stmt: &Stmt) -> bool {
    let StmtKind::Assign { value, .. } = &stmt.node else {
        return false;
    };
    let ExprKind::Call {func, ..} = &value.node else {
        return false;
    };
    context.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TypedDict"]
    })
}

pub(crate) fn is_type_var_assignment(context: &Context, stmt: &Stmt) -> bool {
    let StmtKind::Assign { value, .. } = &stmt.node else {
        return false;
    };
    let ExprKind::Call {func, ..} = &value.node else {
        return false;
    };
    context.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TypeVar"]
            || call_path.as_slice() == ["typing", "NewType"]
    })
}

pub(crate) fn is_typed_dict_class(context: &Context, bases: &[Expr]) -> bool {
    bases
        .iter()
        .any(|base| context.match_typing_expr(base, "TypedDict"))
}

#[cfg(test)]
mod tests {
    use super::{is_acronym, is_camelcase, is_mixed_case};

    #[test]
    fn test_is_camelcase() {
        assert!(is_camelcase("Camel"));
        assert!(is_camelcase("CamelCase"));
        assert!(!is_camelcase("camel"));
        assert!(!is_camelcase("camel_case"));
        assert!(!is_camelcase("CAMEL"));
        assert!(!is_camelcase("CAMEL_CASE"));
    }

    #[test]
    fn test_is_mixed_case() {
        assert!(is_mixed_case("mixedCase"));
        assert!(is_mixed_case("mixed_Case"));
        assert!(is_mixed_case("_mixed_Case"));
        assert!(!is_mixed_case("mixed_case"));
        assert!(!is_mixed_case("MIXED_CASE"));
        assert!(!is_mixed_case(""));
        assert!(!is_mixed_case("_"));
    }

    #[test]
    fn test_is_acronym() {
        assert!(is_acronym("AB", "AB"));
        assert!(is_acronym("AbcDef", "AD"));
        assert!(!is_acronym("AbcDef", "Ad"));
        assert!(!is_acronym("AbcDef", "AB"));
    }
}
