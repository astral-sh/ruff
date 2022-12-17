use std::iter;

use regex::Regex;
use rustc_hash::FxHashMap;
use rustpython_ast::{Arg, Arguments};

use crate::ast::function_type;
use crate::ast::function_type::FunctionType;
use crate::ast::types::{Binding, BindingKind, FunctionDef, Lambda, Scope, ScopeKind};
use crate::checkers::ast::Checker;
use crate::flake8_unused_arguments::helpers;
use crate::flake8_unused_arguments::types::Argumentable;
use crate::{visibility, Check};

/// Check a plain function for unused arguments.
fn function(
    argumentable: &Argumentable,
    args: &Arguments,
    values: &FxHashMap<&str, usize>,
    bindings: &[Binding],
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];
    for arg in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
        .chain(
            iter::once::<Option<&Arg>>(args.vararg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        )
        .chain(
            iter::once::<Option<&Arg>>(args.kwarg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        )
    {
        if let Some(binding) = values
            .get(&arg.node.arg.as_str())
            .map(|index| &bindings[*index])
        {
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

/// Check a method for unused arguments.
fn method(
    argumentable: &Argumentable,
    args: &Arguments,
    values: &FxHashMap<&str, usize>,
    bindings: &[Binding],
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
) -> Vec<Check> {
    let mut checks: Vec<Check> = vec![];
    for arg in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .skip(1)
        .chain(args.kwonlyargs.iter())
        .chain(
            iter::once::<Option<&Arg>>(args.vararg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        )
        .chain(
            iter::once::<Option<&Arg>>(args.kwarg.as_deref())
                .flatten()
                .skip(usize::from(ignore_variadic_names)),
        )
    {
        if let Some(binding) = values
            .get(&arg.node.arg.as_str())
            .map(|index| &bindings[*index])
        {
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
pub fn unused_arguments(
    checker: &Checker,
    parent: &Scope,
    scope: &Scope,
    bindings: &[Binding],
) -> Vec<Check> {
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
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
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
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
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
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
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
                            bindings,
                            &checker.settings.dummy_variable_rgx,
                            checker
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
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
                    bindings,
                    &checker.settings.dummy_variable_rgx,
                    checker
                        .settings
                        .flake8_unused_arguments
                        .ignore_variadic_names,
                )
            } else {
                vec![]
            }
        }
        _ => unreachable!("Expected ScopeKind::Function | ScopeKind::Lambda"),
    }
}
