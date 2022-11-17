use fnv::{FnvHashMap, FnvHashSet};
use rustpython_ast::{Arguments, Expr, ExprKind, Stmt};

use crate::ast::types::{Range, Scope, ScopeKind};
use crate::checks::{Check, CheckKind};
use crate::pep8_naming::helpers;
use crate::pep8_naming::helpers::FunctionType;
use crate::pep8_naming::settings::Settings;
use crate::python::string::{self};

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
    if name.to_lowercase() != name
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
pub fn invalid_argument_name(name: &str, location: Range) -> Option<Check> {
    if name.to_lowercase() != name {
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
    name: &str,
    decorator_list: &[Expr],
    args: &Arguments,
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
    import_aliases: &FnvHashMap<&str, &str>,
    settings: &Settings,
) -> Option<Check> {
    if matches!(
        helpers::function_type(
            scope,
            name,
            decorator_list,
            from_imports,
            import_aliases,
            settings,
        ),
        FunctionType::ClassMethod
    ) {
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
    name: &str,
    decorator_list: &[Expr],
    args: &Arguments,
    from_imports: &FnvHashMap<&str, FnvHashSet<&str>>,
    import_aliases: &FnvHashMap<&str, &str>,
    settings: &Settings,
) -> Option<Check> {
    if matches!(
        helpers::function_type(
            scope,
            name,
            decorator_list,
            from_imports,
            import_aliases,
            settings,
        ),
        FunctionType::Method
    ) {
        if let Some(arg) = args.args.first() {
            if arg.node.arg != "self" {
                return Some(Check::new(
                    CheckKind::InvalidFirstArgumentNameForMethod,
                    Range::from_located(arg),
                ));
            }
        }
    }
    None
}

/// N807
pub fn dunder_function_name(scope: &Scope, stmt: &Stmt, name: &str) -> Option<Check> {
    if matches!(scope.kind, ScopeKind::Class(_)) {
        return None;
    }
    if name.starts_with("__") && name.ends_with("__") {
        return Some(Check::new(
            CheckKind::DunderFunctionName,
            Range::from_located(stmt),
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
    if string::is_upper(name) && !string::is_upper(asname) {
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
    if !string::is_upper(name) && string::is_lower(name) && asname.to_lowercase() != asname {
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
    if helpers::is_camelcase(name) && string::is_lower(asname) {
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
    if helpers::is_camelcase(name)
        && !string::is_lower(asname)
        && string::is_upper(asname)
        && !helpers::is_acronym(name, asname)
    {
        return Some(Check::new(
            CheckKind::CamelcaseImportedAsConstant(name.to_string(), asname.to_string()),
            Range::from_located(import_from),
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
    if helpers::is_camelcase(name)
        && !string::is_lower(asname)
        && string::is_upper(asname)
        && helpers::is_acronym(name, asname)
    {
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
            id == "Exception" || id.ends_with("Error")
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
