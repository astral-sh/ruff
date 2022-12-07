use std::iter;

use regex::Regex;
use rustc_hash::FxHashMap;
use rustpython_ast::{Arg, Arguments};

use crate::ast::function_type;
use crate::ast::function_type::FunctionType;
use crate::ast::helpers::collect_arg_names;
use crate::ast::types::{Binding, BindingKind, FunctionDef, Lambda, Scope, ScopeKind};
use crate::check_ast::Checker;
use crate::flake8_unused_arguments::helpers;
use crate::flake8_unused_arguments::types::Argumentable;
use crate::{visibility, Check};

/// Check a plain function for unused arguments.
fn function(
    argumentable: &Argumentable,
    args: &Arguments,
    bindings: &FxHashMap<&str, Binding>,
    dummy_variable_rgx: &Regex,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];
    for arg_name in collect_arg_names(args) {
        if let Some(binding) = bindings.get(arg_name) {
            if binding.used.is_none()
                && matches!(binding.kind, BindingKind::Argument)
                && !dummy_variable_rgx.is_match(arg_name)
            {
                checks.push(Check::new(
                    argumentable.check_for(arg_name.to_string()),
                    binding.range,
                ));
            }
        }
    }
    checks
}

/// Check a method for unused arguments.
fn method(
    argumentable: &Argumentable,
    args: &Arguments,
    bindings: &FxHashMap<&str, Binding>,
    dummy_variable_rgx: &Regex,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];
    for arg in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .skip(1)
        .chain(args.kwonlyargs.iter())
        .chain(iter::once::<Option<&Arg>>(args.vararg.as_deref()).flatten())
        .chain(iter::once::<Option<&Arg>>(args.kwarg.as_deref()).flatten())
    {
        if let Some(binding) = bindings.get(&arg.node.arg.as_str()) {
            if binding.used.is_none()
                && matches!(binding.kind, BindingKind::Argument)
                && !dummy_variable_rgx.is_match(arg.node.arg.as_str())
            {
                checks.push(Check::new(
                    argumentable.check_for(arg.node.arg.to_string()),
                    binding.range,
                ));
            }
        }
    }
    checks
}

/// ARG001, ARG002, ARG003, ARG004, ARG005
pub fn unused_arguments(checker: &Checker, parent: &Scope, scope: &Scope) -> Vec<Check> {
    match &scope.kind {
        ScopeKind::Function(FunctionDef {
            name,
            args,
            body,
            decorator_list,
            ..
        }) => {
            match function_type::classify(
                parent,
                name,
                decorator_list,
                &checker.from_imports,
                &checker.import_aliases,
                &checker.settings.pep8_naming.classmethod_decorators,
                &checker.settings.pep8_naming.staticmethod_decorators,
            ) {
                FunctionType::Function => {
                    if checker
                        .settings
                        .enabled
                        .contains(Argumentable::Function.check_code())
                    {
                        function(
                            &Argumentable::Function,
                            args,
                            &scope.values,
                            &checker.settings.dummy_variable_rgx,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::Method => {
                    if checker
                        .settings
                        .enabled
                        .contains(Argumentable::Method.check_code())
                        && !helpers::is_empty(body)
                        && !visibility::is_abstract(checker, decorator_list)
                        && !visibility::is_override(checker, decorator_list)
                    {
                        method(
                            &Argumentable::Method,
                            args,
                            &scope.values,
                            &checker.settings.dummy_variable_rgx,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::ClassMethod => {
                    if checker
                        .settings
                        .enabled
                        .contains(Argumentable::ClassMethod.check_code())
                        && !helpers::is_empty(body)
                        && !visibility::is_abstract(checker, decorator_list)
                        && !visibility::is_override(checker, decorator_list)
                    {
                        method(
                            &Argumentable::ClassMethod,
                            args,
                            &scope.values,
                            &checker.settings.dummy_variable_rgx,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::StaticMethod => {
                    if checker
                        .settings
                        .enabled
                        .contains(Argumentable::StaticMethod.check_code())
                        && !helpers::is_empty(body)
                        && !visibility::is_abstract(checker, decorator_list)
                        && !visibility::is_override(checker, decorator_list)
                    {
                        function(
                            &Argumentable::StaticMethod,
                            args,
                            &scope.values,
                            &checker.settings.dummy_variable_rgx,
                        )
                    } else {
                        vec![]
                    }
                }
            }
        }
        ScopeKind::Lambda(Lambda { args, .. }) => {
            if checker
                .settings
                .enabled
                .contains(Argumentable::Lambda.check_code())
            {
                function(
                    &Argumentable::Lambda,
                    args,
                    &scope.values,
                    &checker.settings.dummy_variable_rgx,
                )
            } else {
                vec![]
            }
        }
        _ => unreachable!("Expected ScopeKind::Function | ScopeKind::Lambda"),
    }
}
