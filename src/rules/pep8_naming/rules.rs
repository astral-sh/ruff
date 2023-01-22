use rustpython_ast::{Arg, Arguments, Expr, ExprKind, Stmt};

use super::helpers;
use crate::ast::function_type;
use crate::ast::helpers::identifier_range;
use crate::ast::types::{Range, Scope, ScopeKind};
use crate::checkers::ast::Checker;
use crate::python::string::{self};
use crate::registry::Diagnostic;
use crate::source_code::Locator;
use crate::violations;

/// N801
pub fn invalid_class_name(class_def: &Stmt, name: &str, locator: &Locator) -> Option<Diagnostic> {
    let stripped = name.strip_prefix('_').unwrap_or(name);
    if !stripped.chars().next().map_or(false, char::is_uppercase) || stripped.contains('_') {
        return Some(Diagnostic::new(
            violations::InvalidClassName(name.to_string()),
            identifier_range(class_def, locator),
        ));
    }
    None
}

/// N802
pub fn invalid_function_name(
    func_def: &Stmt,
    name: &str,
    ignore_names: &[String],
    locator: &Locator,
) -> Option<Diagnostic> {
    if name.to_lowercase() != name && !ignore_names.iter().any(|ignore_name| ignore_name == name) {
        return Some(Diagnostic::new(
            violations::InvalidFunctionName(name.to_string()),
            identifier_range(func_def, locator),
        ));
    }
    None
}

/// N803
pub fn invalid_argument_name(name: &str, arg: &Arg) -> Option<Diagnostic> {
    if name.to_lowercase() != name {
        return Some(Diagnostic::new(
            violations::InvalidArgumentName(name.to_string()),
            Range::from_located(arg),
        ));
    }
    None
}

/// N804
pub fn invalid_first_argument_name_for_class_method(
    checker: &Checker,
    scope: &Scope,
    name: &str,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Diagnostic> {
    if !matches!(
        function_type::classify(
            checker,
            scope,
            name,
            decorator_list,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::ClassMethod
    ) {
        return None;
    }
    if let Some(arg) = args.posonlyargs.first() {
        if arg.node.arg != "cls" {
            return Some(Diagnostic::new(
                violations::InvalidFirstArgumentNameForClassMethod,
                Range::from_located(arg),
            ));
        }
    } else if let Some(arg) = args.args.first() {
        if arg.node.arg != "cls" {
            return Some(Diagnostic::new(
                violations::InvalidFirstArgumentNameForClassMethod,
                Range::from_located(arg),
            ));
        }
    }
    None
}

/// N805
pub fn invalid_first_argument_name_for_method(
    checker: &Checker,
    scope: &Scope,
    name: &str,
    decorator_list: &[Expr],
    args: &Arguments,
) -> Option<Diagnostic> {
    if !matches!(
        function_type::classify(
            checker,
            scope,
            name,
            decorator_list,
            &checker.settings.pep8_naming.classmethod_decorators,
            &checker.settings.pep8_naming.staticmethod_decorators,
        ),
        function_type::FunctionType::Method
    ) {
        return None;
    }
    let arg = args.args.first()?;
    if arg.node.arg == "self" {
        return None;
    }
    Some(Diagnostic::new(
        violations::InvalidFirstArgumentNameForMethod,
        Range::from_located(arg),
    ))
}

/// N806
pub fn non_lowercase_variable_in_function(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if name.to_lowercase() != name
        && !helpers::is_namedtuple_assignment(checker, stmt)
        && !helpers::is_type_var_assignment(checker, stmt)
    {
        checker.diagnostics.push(Diagnostic::new(
            violations::NonLowercaseVariableInFunction(name.to_string()),
            Range::from_located(expr),
        ));
    }
}

/// N807
pub fn dunder_function_name(
    scope: &Scope,
    stmt: &Stmt,
    name: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if matches!(scope.kind, ScopeKind::Class(_)) {
        return None;
    }
    if !(name.starts_with("__") && name.ends_with("__")) {
        return None;
    }
    // Allowed under PEP 562 (https://peps.python.org/pep-0562/).
    if matches!(scope.kind, ScopeKind::Module) && (name == "__getattr__" || name == "__dir__") {
        return None;
    }

    Some(Diagnostic::new(
        violations::DunderFunctionName,
        identifier_range(stmt, locator),
    ))
}

/// N811
pub fn constant_imported_as_non_constant(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if string::is_upper(name) && !string::is_upper(asname) {
        return Some(Diagnostic::new(
            violations::ConstantImportedAsNonConstant(name.to_string(), asname.to_string()),
            identifier_range(import_from, locator),
        ));
    }
    None
}

/// N812
pub fn lowercase_imported_as_non_lowercase(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if !string::is_upper(name) && string::is_lower(name) && asname.to_lowercase() != asname {
        return Some(Diagnostic::new(
            violations::LowercaseImportedAsNonLowercase(name.to_string(), asname.to_string()),
            identifier_range(import_from, locator),
        ));
    }
    None
}

/// N813
pub fn camelcase_imported_as_lowercase(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name) && string::is_lower(asname) {
        return Some(Diagnostic::new(
            violations::CamelcaseImportedAsLowercase(name.to_string(), asname.to_string()),
            identifier_range(import_from, locator),
        ));
    }
    None
}

/// N814
pub fn camelcase_imported_as_constant(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name)
        && !string::is_lower(asname)
        && string::is_upper(asname)
        && !helpers::is_acronym(name, asname)
    {
        return Some(Diagnostic::new(
            violations::CamelcaseImportedAsConstant(name.to_string(), asname.to_string()),
            identifier_range(import_from, locator),
        ));
    }
    None
}

/// N815
pub fn mixed_case_variable_in_class_scope(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if helpers::is_mixed_case(name) && !helpers::is_namedtuple_assignment(checker, stmt) {
        checker.diagnostics.push(Diagnostic::new(
            violations::MixedCaseVariableInClassScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
}

/// N816
pub fn mixed_case_variable_in_global_scope(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if helpers::is_mixed_case(name) && !helpers::is_namedtuple_assignment(checker, stmt) {
        checker.diagnostics.push(Diagnostic::new(
            violations::MixedCaseVariableInGlobalScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
}

/// N817
pub fn camelcase_imported_as_acronym(
    import_from: &Stmt,
    name: &str,
    asname: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if helpers::is_camelcase(name)
        && !string::is_lower(asname)
        && string::is_upper(asname)
        && helpers::is_acronym(name, asname)
    {
        return Some(Diagnostic::new(
            violations::CamelcaseImportedAsAcronym(name.to_string(), asname.to_string()),
            identifier_range(import_from, locator),
        ));
    }
    None
}

/// N818
pub fn error_suffix_on_exception_name(
    class_def: &Stmt,
    bases: &[Expr],
    name: &str,
    locator: &Locator,
) -> Option<Diagnostic> {
    if !bases.iter().any(|base| {
        if let ExprKind::Name { id, .. } = &base.node {
            id == "Exception" || id.ends_with("Error")
        } else {
            false
        }
    }) {
        return None;
    }

    if name.ends_with("Error") {
        return None;
    }
    Some(Diagnostic::new(
        violations::ErrorSuffixOnExceptionName(name.to_string()),
        identifier_range(class_def, locator),
    ))
}
