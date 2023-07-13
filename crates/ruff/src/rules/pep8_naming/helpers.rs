use itertools::Itertools;
use rustpython_parser::ast::{self, Expr, Stmt};

use ruff_python_semantic::SemanticModel;
use ruff_python_stdlib::str::{is_cased_lowercase, is_cased_uppercase};

pub(super) fn is_camelcase(name: &str) -> bool {
    !is_cased_lowercase(name) && !is_cased_uppercase(name) && !name.contains('_')
}

pub(super) fn is_mixed_case(name: &str) -> bool {
    !is_cased_lowercase(name)
        && name
            .strip_prefix('_')
            .unwrap_or(name)
            .chars()
            .next()
            .map_or_else(|| false, char::is_lowercase)
}

pub(super) fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

pub(super) fn is_named_tuple_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(
            call_path.as_slice(),
            ["collections", "namedtuple"] | ["typing", "NamedTuple"]
        )
    })
}

pub(super) fn is_typed_dict_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(call_path.as_slice(), ["typing", "TypedDict"])
    })
}

pub(super) fn is_type_var_assignment(stmt: &Stmt, semantic: &SemanticModel) -> bool {
    let Stmt::Assign(ast::StmtAssign { value, .. }) = stmt else {
        return false;
    };
    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return false;
    };
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(call_path.as_slice(), ["typing", "TypeVar" | "NewType"])
    })
}

pub(super) fn is_typed_dict_class(bases: &[Expr], semantic: &SemanticModel) -> bool {
    bases
        .iter()
        .any(|base| semantic.match_typing_expr(base, "TypedDict"))
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
