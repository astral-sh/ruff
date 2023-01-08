use std::iter;

use regex::Regex;
use rustc_hash::FxHashMap;
use rustpython_ast::{Arg, Arguments};

use crate::ast::function_type;
use crate::ast::function_type::FunctionType;
use crate::ast::types::{Binding, BindingKind, FunctionDef, Lambda, Scope, ScopeKind};
use crate::flake8_unused_arguments::helpers;
use crate::flake8_unused_arguments::types::Argumentable;
use crate::xxxxxxxxs::ast::xxxxxxxx;
use crate::{visibility, Diagnostic};

/// Check a plain function for unused arguments.
fn function(
    argumentable: &Argumentable,
    args: &Arguments,
    values: &FxHashMap<&str, usize>,
    bindings: &[Binding],
    dummy_variable_rgx: &Regex,
    ignore_variadic_names: bool,
) -> Vec<Diagnostic> {
    let mut checks: Vec<Diagnostic> = vec![];
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
                checks.push(Diagnostic::new(
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
) -> Vec<Diagnostic> {
    let mut checks: Vec<Diagnostic> = vec![];
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
                checks.push(Diagnostic::new(
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
    xxxxxxxx: &xxxxxxxx,
    parent: &Scope,
    scope: &Scope,
    bindings: &[Binding],
) -> Vec<Diagnostic> {
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
                &xxxxxxxx.from_imports,
                &xxxxxxxx.import_aliases,
                &xxxxxxxx.settings.pep8_naming.classmethod_decorators,
                &xxxxxxxx.settings.pep8_naming.staticmethod_decorators,
            ) {
                FunctionType::Function => {
                    if xxxxxxxx
                        .settings
                        .enabled
                        .contains(Argumentable::Function.rule_code())
                        && !visibility::is_overload(xxxxxxxx, decorator_list)
                    {
                        function(
                            &Argumentable::Function,
                            args,
                            &scope.values,
                            bindings,
                            &xxxxxxxx.settings.dummy_variable_rgx,
                            xxxxxxxx
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::Method => {
                    if xxxxxxxx
                        .settings
                        .enabled
                        .contains(Argumentable::Method.rule_code())
                        && !helpers::is_empty(body)
                        && !visibility::is_abstract(xxxxxxxx, decorator_list)
                        && !visibility::is_override(xxxxxxxx, decorator_list)
                        && !visibility::is_overload(xxxxxxxx, decorator_list)
                    {
                        method(
                            &Argumentable::Method,
                            args,
                            &scope.values,
                            bindings,
                            &xxxxxxxx.settings.dummy_variable_rgx,
                            xxxxxxxx
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::ClassMethod => {
                    if xxxxxxxx
                        .settings
                        .enabled
                        .contains(Argumentable::ClassMethod.rule_code())
                        && !helpers::is_empty(body)
                        && !visibility::is_abstract(xxxxxxxx, decorator_list)
                        && !visibility::is_override(xxxxxxxx, decorator_list)
                        && !visibility::is_overload(xxxxxxxx, decorator_list)
                    {
                        method(
                            &Argumentable::ClassMethod,
                            args,
                            &scope.values,
                            bindings,
                            &xxxxxxxx.settings.dummy_variable_rgx,
                            xxxxxxxx
                                .settings
                                .flake8_unused_arguments
                                .ignore_variadic_names,
                        )
                    } else {
                        vec![]
                    }
                }
                FunctionType::StaticMethod => {
                    if xxxxxxxx
                        .settings
                        .enabled
                        .contains(Argumentable::StaticMethod.rule_code())
                        && !helpers::is_empty(body)
                        && !visibility::is_abstract(xxxxxxxx, decorator_list)
                        && !visibility::is_override(xxxxxxxx, decorator_list)
                        && !visibility::is_overload(xxxxxxxx, decorator_list)
                    {
                        function(
                            &Argumentable::StaticMethod,
                            args,
                            &scope.values,
                            bindings,
                            &xxxxxxxx.settings.dummy_variable_rgx,
                            xxxxxxxx
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
            if xxxxxxxx
                .settings
                .enabled
                .contains(Argumentable::Lambda.rule_code())
            {
                function(
                    &Argumentable::Lambda,
                    args,
                    &scope.values,
                    bindings,
                    &xxxxxxxx.settings.dummy_variable_rgx,
                    xxxxxxxx
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
