use itertools::Itertools;
use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::{FunctionScope, Range, Scope, ScopeKind};
use crate::checks::{Check, CheckKind};
use crate::pep8_naming::settings::Settings;

/// N801
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

/// N802
pub fn invalid_function_name(func_def: &Stmt, name: &str, settings: &Settings) -> Option<Check> {
    if !is_lower(name)
        && !settings
            .ignore_names
            .iter()
            .any(|ignore_name| ignore_name == name)
    {
        return Some(Check::new(
            CheckKind::InvalidFunctionName(name.to_string()),
            Range::from_located(func_def),
        ));
    }
    None
}

/// N803
pub fn invalid_argument_name(location: Range, name: &str) -> Option<Check> {
    if !is_lower(name) {
        return Some(Check::new(
            CheckKind::InvalidArgumentName(name.to_string()),
            location,
        ));
    }
    None
}

/// N804
pub fn invalid_first_argument_name_for_class_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
    settings: &Settings,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            settings.classmethod_decorators.contains(id)
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

/// N805
pub fn invalid_first_argument_name_for_method(
    scope: &Scope,
    decorator_list: &[Expr],
    args: &Arguments,
    settings: &Settings,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }

    if decorator_list.iter().any(|decorator| {
        if let ExprKind::Name { id, .. } = &decorator.node {
            settings.classmethod_decorators.contains(id)
                || settings.staticmethod_decorators.contains(id)
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

/// N806
pub fn non_lowercase_variable_in_function(scope: &Scope, expr: &Expr, name: &str) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Function(FunctionScope { .. })) {
        return None;
    }
    if !is_lower(name) {
        return Some(Check::new(
            CheckKind::NonLowercaseVariableInFunction(name.to_string()),
            Range::from_located(expr),
        ));
    }
    None
}

/// N807
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

/// N811
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

/// N812
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

/// N813
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

/// N814
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

/// N815
pub fn mixed_case_variable_in_class_scope(scope: &Scope, expr: &Expr, name: &str) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Class) {
        return None;
    }
    if is_mixed_case(name) {
        return Some(Check::new(
            CheckKind::MixedCaseVariableInClassScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
    None
}

/// N816
pub fn mixed_case_variable_in_global_scope(
    scope: &Scope,
    expr: &Expr,
    name: &str,
) -> Option<Check> {
    if !matches!(scope.kind, ScopeKind::Module) {
        return None;
    }
    if is_mixed_case(name) {
        return Some(Check::new(
            CheckKind::MixedCaseVariableInGlobalScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
    None
}

/// N817
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

/// N818
pub fn error_suffix_on_exception_name(
    class_def: &Stmt,
    bases: &[Expr],
    name: &str,
) -> Option<Check> {
    if bases.iter().any(|base| {
        if let ExprKind::Name { id, .. } = &base.node {
            id == "Exception"
        } else {
            false
        }
    }) {
        if !name.ends_with("Error") {
            return Some(Check::new(
                CheckKind::ErrorSuffixOnExceptionName(name.to_string()),
                Range::from_located(class_def),
            ));
        }
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

fn is_camelcase(name: &str) -> bool {
    !is_lower(name) && !is_upper(name) && !name.contains('_')
}

fn is_mixed_case(name: &str) -> bool {
    !is_lower(name)
        && name
            .strip_prefix('_')
            .unwrap_or(name)
            .chars()
            .next()
            .map_or_else(|| false, |c| c.is_lowercase())
}

fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

#[cfg(test)]
mod tests {
    use super::{is_acronym, is_camelcase, is_lower, is_mixed_case, is_upper};

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
    fn test_is_mixed_case() -> () {
        assert!(is_mixed_case("mixedCase"));
        assert!(is_mixed_case("mixed_Case"));
        assert!(is_mixed_case("_mixed_Case"));
        assert!(!is_mixed_case("mixed_case"));
        assert!(!is_mixed_case("MIXED_CASE"));
        assert!(!is_mixed_case(""));
        assert!(!is_mixed_case("_"));
    }

    #[test]
    fn test_is_acronym() -> () {
        assert!(is_acronym("AB", "AB"));
        assert!(is_acronym("AbcDef", "AD"));
        assert!(!is_acronym("AbcDef", "Ad"));
        assert!(!is_acronym("AbcDef", "AB"));
    }
}
