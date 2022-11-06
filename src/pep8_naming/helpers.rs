use itertools::Itertools;
use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::match_name_or_attr;
use crate::ast::types::{Scope, ScopeKind};
use crate::pep8_naming::settings::Settings;

const CLASS_METHODS: [&str; 3] = ["__new__", "__init_subclass__", "__class_getitem__"];
const METACLASS_BASES: [&str; 2] = ["type", "ABCMeta"];

pub enum FunctionType {
    Function,
    Method,
    ClassMethod,
    StaticMethod,
}

/// Classify a function based on its scope, name, and decorators.
pub fn function_type(
    scope: &Scope,
    name: &str,
    decorator_list: &[Expr],
    settings: &Settings,
) -> FunctionType {
    if let ScopeKind::Class(scope) = &scope.kind {
        // Special-case class method, like `__new__`.
        if CLASS_METHODS.contains(&name)
            // The class itself extends a known metaclass, so all methods are class methods.
            || scope.bases.iter().any(|expr| {
                METACLASS_BASES
                    .iter()
                    .any(|target| match_name_or_attr(expr, target))
            })
            // The method is decorated with a class method decorator (like `@classmethod`).
            || decorator_list.iter().any(|expr| {
            if let ExprKind::Name { id, .. } = &expr.node {
                settings.classmethod_decorators.contains(id)
            } else {
                false
            }
        }) {
            FunctionType::ClassMethod
        } else if decorator_list.iter().any(|expr| {
            if let ExprKind::Name { id, .. } = &expr.node {
                settings.staticmethod_decorators.contains(id)
            } else {
                false
            }
        }) {
            // The method is decorated with a static method decorator (like
            // `@staticmethod`).
            FunctionType::StaticMethod
        } else {
            // It's an instance method.
            FunctionType::Method
        }
    } else {
        FunctionType::Function
    }
}

pub fn is_lower(s: &str) -> bool {
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

pub fn is_upper(s: &str) -> bool {
    let mut cased = false;
    for c in s.chars() {
        if c.is_lowercase() {
            return false;
        } else if !cased && c.is_uppercase() {
            cased = true;
        }
    }
    cased
}

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
            .map_or_else(|| false, |c| c.is_lowercase())
}

pub fn is_acronym(name: &str, asname: &str) -> bool {
    name.chars().filter(|c| c.is_uppercase()).join("") == asname
}

#[cfg(test)]
mod tests {
    use crate::pep8_naming::helpers::{
        is_acronym, is_camelcase, is_lower, is_mixed_case, is_upper,
    };

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
