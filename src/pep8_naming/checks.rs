use itertools::Itertools;
use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::{Range, Scope, ScopeKind};
use crate::checks::{Check, CheckKind};

pub fn invalid_class_name(class_def: &Stmt, name: &str) -> Option<Check> {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    if !stripped
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        || stripped.contains('_')
    {
        return Some(Check::new(
            CheckKind::InvalidClassName(name.to_string()),
            Range::from_located(class_def),
        ));
    }
    None
}

pub fn invalid_function_name(func_def: &Stmt, name: &str) -> Option<Check> {
    if name.chars().any(|c| c.is_uppercase()) {
        return Some(Check::new(
            CheckKind::InvalidFunctionName(name.to_string()),
            Range::from_located(func_def),
        ));
    }
    None
}

pub fn invalid_argument_name(location: Range, name: &str) -> Option<Check> {
    if name.chars().any(|c| c.is_uppercase()) {
        return Some(Check::new(
            CheckKind::InvalidArgumentName(name.to_string()),
            location,
        ));
    }
    None
}

pub fn invalid_first_argument_name_for_class_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            id == "classmethod"
        } else {
            false
        }
    }) {
        if let Some(arg) = args.args.first() {
            if arg.node.arg != "cls" {
                return Some(Check::new(
                    CheckKind::InvalidFirstArgumentNameForClassMethod,
                    Range::from_located(arg),
                ));
            }
        }
    }
    None
}

pub fn invalid_first_argument_name_for_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            id == "classmethod" || id == "staticmethod"
        } else {
            false
        }
    }) {
        return None;
    }

    if let Some(arg) = args.args.first() {
        if arg.node.arg != "self" {
            return Some(Check::new(
                CheckKind::InvalidFirstArgumentNameForMethod,
                Range::from_located(arg),
            ));
        }
    }
    None
}

pub fn dunder_function_name(func_def: &Stmt, scope: &Scope, name: &str) -> Option<Check> {
    if matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if name.starts_with("__") && name.ends_with("__") {
        return Some(Check::new(
            CheckKind::DunderFunctionName,
            Range::from_located(func_def),
        ));
    }

    None
}

fn is_lower(s: &str) -> bool {
    let mut cased = false;
    for c in s.chars() {
        if c.is_uppercase() {
            return false;
        } else if !cased && c.is_lowercase() {
            cased = true;
        }
    }
    cased
}

fn is_upper(s: &str) -> bool {
    let mut cased = false;
    for c in s.chars() {
        if c.is_lowercase() {
            return false;
        } else if (!cased) && c.is_uppercase() {
            cased = true;
        }
    }
    cased
}

pub fn constant_imported_as_non_constant(
    import_from: &Stmt,
    name: &str,
    asname: &str,
) -> Option<Check> {
    if is_upper(name) && !is_upper(asname) {
        return Some(Check::new(
            CheckKind::ConstantImportedAsNonConstant(name.to_string(), asname.to_string()),
            Range::from_located(import_from),
        ));
    }
    None
}

pub fn lowercase_imported_as_non_lowercase(
    import_from: &Stmt,
    name: &str,
    asname: &str,
) -> Option<Check> {
    if is_lower(name) && asname.to_lowercase() != asname {
        return Some(Check::new(
            CheckKind::LowercaseImportedAsNonLowercase(name.to_string(), asname.to_string()),
            Range::from_located(import_from),
        ));
    }
    None
}

fn is_camelcase(name: &str) -> bool {
    !is_lower(name) && !is_upper(name) && !name.contains('_')
}
fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

pub fn camelcase_imported_as_lowercase(
    import_from: &Stmt,
    name: &str,
    asname: &str,
) -> Option<Check> {
    if is_camelcase(name) && is_lower(asname) {
        return Some(Check::new(
            CheckKind::CamelcaseImportedAsLowercase(name.to_string(), asname.to_string()),
            Range::from_located(import_from),
        ));
    }
    None
}

pub fn camelcase_imported_as_constant(
    import_from: &Stmt,
    name: &str,
    asname: &str,
) -> Option<Check> {
    if is_camelcase(name) && is_upper(asname) && !is_acronym(name, asname) {
        return Some(Check::new(
            CheckKind::CamelcaseImportedAsConstant(name.to_string(), asname.to_string()),
            Range::from_located(import_from),
        ));
    }
    None
}

pub fn camelcase_imported_as_acronym(
    import_from: &Stmt,
    name: &str,
    asname: &str,
) -> Option<Check> {
    if is_camelcase(name) && is_upper(asname) && is_acronym(name, asname) {
        return Some(Check::new(
            CheckKind::CamelcaseImportedAsAcronym(name.to_string(), asname.to_string()),
            Range::from_located(import_from),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{is_acronym, is_camelcase, is_lower, is_upper};

    #[test]
    fn test_is_lower() -> () {
        assert!(is_lower("abc"));
        assert!(is_lower("a_b_c"));
        assert!(is_lower("a2c"));
        assert!(!is_lower("aBc"));
        assert!(!is_lower("ABC"));
        assert!(!is_lower(""));
        assert!(!is_lower("_"));
    }

    #[test]
    fn test_is_upper() -> () {
        assert!(is_upper("ABC"));
        assert!(is_upper("A_B_C"));
        assert!(is_upper("A2C"));
        assert!(!is_upper("aBc"));
        assert!(!is_upper("abc"));
        assert!(!is_upper(""));
        assert!(!is_upper("_"));
    }

    #[test]
    fn test_is_camelcase() -> () {
        assert!(is_camelcase("Camel"));
        assert!(is_camelcase("CamelCase"));
        assert!(!is_camelcase("camel"));
        assert!(!is_camelcase("camel_case"));
        assert!(!is_camelcase("CAMEL"));
        assert!(!is_camelcase("CAMEL_CASE"));
    }

    #[test]
    fn test_is_acronym() -> () {
        assert!(is_acronym("AB", "AB"));
        assert!(is_acronym("AbcDef", "AD"));
        assert!(!is_acronym("AbcDef", "Ad"));
        assert!(!is_acronym("AbcDef", "AB"));
    }
}
