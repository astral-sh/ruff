use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_ast::{Expr, Stmt, StmtKind};

use crate::ast::helpers::{
    collect_call_paths, dealias_call_path, match_call_path, to_module_and_member,
};
use crate::ast::types::{Scope, ScopeKind};
use crate::pep8_naming::settings::Settings;
use crate::python::string::{is_lower, is_upper};

const CLASS_METHODS: [&str; 3] = ["__new__", "__init_subclass__", "__class_getitem__"];
const METACLASS_BASES: [(&str, &str); 2] = [("", "type"), ("abc", "ABCMeta")];

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
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
    import_aliases: &FxHashMap<&str, &str>,
    settings: &Settings,
) -> FunctionType {
    if let ScopeKind::Class(scope) = &scope.kind {
        // Special-case class method, like `__new__`.
        if CLASS_METHODS.contains(&name)
            || scope.bases.iter().any(|expr| {
                // The class itself extends a known metaclass, so all methods are class methods.
                let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
                METACLASS_BASES.iter().any(|(module, member)| {
                    match_call_path(&call_path, module, member, from_imports)
                })
            })
            || decorator_list.iter().any(|expr| {
                // The method is decorated with a class method decorator (like `@classmethod`).
                let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
                settings.classmethod_decorators.iter().any(|decorator| {
                    let (module, member) = to_module_and_member(decorator);
                    match_call_path(&call_path, module, member, from_imports)
                })
            })
        {
            FunctionType::ClassMethod
        } else if decorator_list.iter().any(|expr| {
            // The method is decorated with a static method decorator (like
            // `@staticmethod`).
            let call_path = dealias_call_path(collect_call_paths(expr), import_aliases);
            settings.staticmethod_decorators.iter().any(|decorator| {
                let (module, member) = to_module_and_member(decorator);
                match_call_path(&call_path, module, member, from_imports)
            })
        }) {
            FunctionType::StaticMethod
        } else {
            // It's an instance method.
            FunctionType::Method
        }
    } else {
        FunctionType::Function
    }
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

pub fn is_namedtuple_assignment(
    stmt: &Stmt,
    from_imports: &FxHashMap<&str, FxHashSet<&str>>,
) -> bool {
    if let StmtKind::Assign { value, .. } = &stmt.node {
        match_call_path(
            &collect_call_paths(value),
            "collections",
            "namedtuple",
            from_imports,
        )
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::pep8_naming::helpers::{is_acronym, is_camelcase, is_mixed_case};

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
