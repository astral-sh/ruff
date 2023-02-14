use itertools::Itertools;
use ruff_python::string::{is_lower, is_upper};
use rustpython_parser::ast::{ExprKind, Stmt, StmtKind};

use crate::checkers::ast::Checker;

pub fn is_camelcase(name: &str) -> bool {
    !is_lower(name) && !is_upper(name) && !name.contains('_')
}

pub fn is_mixed_case(name: &str) -> bool {
    !is_lower(name)
        && name
            .strip_prefix('_')
            .unwrap_or(name)
            .chars()
            .next()
            .map_or_else(|| false, char::is_lowercase)
}

pub fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

pub fn is_namedtuple_assignment(checker: &Checker, stmt: &Stmt) -> bool {
    let StmtKind::Assign { value, .. } = &stmt.node else {
        return false;
    };
    let ExprKind::Call {func, ..} = &value.node else {
        return false;
    };
    checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["collections", "namedtuple"]
            || call_path.as_slice() == ["typing", "NamedTuple"]
    })
}

pub fn is_typeddict_assignment(checker: &Checker, stmt: &Stmt) -> bool {
    let StmtKind::Assign { value, .. } = &stmt.node else {
        return false;
    };
    let ExprKind::Call {func, ..} = &value.node else {
        return false;
    };
    checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TypedDict"]
    })
}

pub fn is_type_var_assignment(checker: &Checker, stmt: &Stmt) -> bool {
    let StmtKind::Assign { value, .. } = &stmt.node else {
        return false;
    };
    let ExprKind::Call {func, ..} = &value.node else {
        return false;
    };
    checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["typing", "TypeVar"]
            || call_path.as_slice() == ["typing", "NewType"]
    })
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
