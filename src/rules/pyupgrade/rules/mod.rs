pub(crate) use convert_named_tuple_functional_to_class::convert_named_tuple_functional_to_class;
pub(crate) use convert_typed_dict_functional_to_class::convert_typed_dict_functional_to_class;
pub(crate) use datetime_utc_alias::datetime_utc_alias;
pub(crate) use deprecated_unittest_alias::deprecated_unittest_alias;
pub(crate) use extraneous_parentheses::extraneous_parentheses;
pub(crate) use f_strings::f_strings;
pub(crate) use format_literals::format_literals;
pub(crate) use functools_cache::functools_cache;
pub(crate) use lru_cache_without_parameters::lru_cache_without_parameters;
pub(crate) use native_literals::native_literals;
use once_cell::sync::Lazy;
pub(crate) use open_alias::open_alias;
pub(crate) use os_error_alias::os_error_alias;
pub(crate) use printf_string_formatting::printf_string_formatting;
pub(crate) use redundant_open_modes::redundant_open_modes;
use regex::Regex;
pub(crate) use remove_six_compat::remove_six_compat;
pub(crate) use replace_stdout_stderr::replace_stdout_stderr;
pub(crate) use replace_universal_newlines::replace_universal_newlines;
pub(crate) use rewrite_c_element_tree::replace_c_element_tree;
pub(crate) use rewrite_mock_import::{rewrite_mock_attribute, rewrite_mock_import};
pub(crate) use rewrite_unicode_literal::rewrite_unicode_literal;
pub(crate) use rewrite_yield_from::rewrite_yield_from;
use rustpython_ast::Location;
use rustpython_parser::ast::{ArgData, Expr, ExprKind, Stmt, StmtKind};
pub(crate) use super_call_with_parameters::super_call_with_parameters;
pub(crate) use type_of_primitive::type_of_primitive;
pub(crate) use typing_text_str_alias::typing_text_str_alias;
pub(crate) use unnecessary_builtin_import::unnecessary_builtin_import;
pub(crate) use unnecessary_encode_utf8::unnecessary_encode_utf8;
pub(crate) use unnecessary_future_import::unnecessary_future_import;
pub(crate) use unpack_list_comprehension::unpack_list_comprehension;
pub(crate) use use_pep585_annotation::use_pep585_annotation;
pub(crate) use use_pep604_annotation::use_pep604_annotation;
pub(crate) use useless_metaclass_type::useless_metaclass_type;
pub(crate) use useless_object_inheritance::useless_object_inheritance;

use crate::ast::helpers;
use crate::ast::types::{Range, Scope, ScopeKind};
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violations;

mod convert_named_tuple_functional_to_class;
mod convert_typed_dict_functional_to_class;
mod datetime_utc_alias;
mod deprecated_unittest_alias;
mod extraneous_parentheses;
mod f_strings;
mod format_literals;
mod functools_cache;
mod lru_cache_without_parameters;
mod native_literals;
mod open_alias;
mod os_error_alias;
mod printf_string_formatting;
mod redundant_open_modes;
mod remove_six_compat;
mod replace_stdout_stderr;
mod replace_universal_newlines;
mod rewrite_c_element_tree;
mod rewrite_mock_import;
mod rewrite_unicode_literal;
mod rewrite_yield_from;
mod super_call_with_parameters;
mod type_of_primitive;
mod typing_text_str_alias;
mod unnecessary_builtin_import;
mod unnecessary_encode_utf8;
mod unnecessary_future_import;
mod unpack_list_comprehension;
mod use_pep585_annotation;
mod use_pep604_annotation;
mod useless_metaclass_type;
mod useless_object_inheritance;

/// UP008
pub fn super_args(
    scope: &Scope,
    parents: &[&Stmt],
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) -> Option<Diagnostic> {
    if !helpers::is_super_call_with_arguments(func, args) {
        return None;
    }

    // Check: are we in a Function scope?
    if !matches!(scope.kind, ScopeKind::Function { .. }) {
        return None;
    }

    let mut parents = parents.iter().rev();

    // For a `super` invocation to be unnecessary, the first argument needs to match
    // the enclosing class, and the second argument needs to match the first
    // argument to the enclosing function.
    let [first_arg, second_arg] = args else {
        return None;
    };

    // Find the enclosing function definition (if any).
    let Some(StmtKind::FunctionDef {
        args: parent_args, ..
    }) = parents
        .find(|stmt| matches!(stmt.node, StmtKind::FunctionDef { .. }))
        .map(|stmt| &stmt.node) else {
        return None;
    };

    // Extract the name of the first argument to the enclosing function.
    let Some(ArgData {
        arg: parent_arg, ..
    }) = parent_args.args.first().map(|expr| &expr.node) else {
        return None;
    };

    // Find the enclosing class definition (if any).
    let Some(StmtKind::ClassDef {
        name: parent_name, ..
    }) = parents
        .find(|stmt| matches!(stmt.node, StmtKind::ClassDef { .. }))
        .map(|stmt| &stmt.node) else {
        return None;
    };

    let (
        ExprKind::Name {
            id: first_arg_id, ..
        },
        ExprKind::Name {
            id: second_arg_id, ..
        },
    ) = (&first_arg.node, &second_arg.node) else {
        return None;
    };

    if first_arg_id == parent_name && second_arg_id == parent_arg {
        return Some(Diagnostic::new(
            violations::SuperCallWithParameters,
            Range::from_located(expr),
        ));
    }

    None
}

// Regex from PEP263.
static CODING_COMMENT_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[ \t\f]*#.*?coding[:=][ \t]*utf-?8").unwrap());

/// UP009
pub fn unnecessary_coding_comment(lineno: usize, line: &str, autofix: bool) -> Option<Diagnostic> {
    // PEP3120 makes utf-8 the default encoding.
    if CODING_COMMENT_REGEX.is_match(line) {
        let mut diagnostic = Diagnostic::new(
            violations::PEP3120UnnecessaryCodingComment,
            Range::new(Location::new(lineno + 1, 0), Location::new(lineno + 2, 0)),
        );
        if autofix {
            diagnostic.amend(Fix::deletion(
                Location::new(lineno + 1, 0),
                Location::new(lineno + 2, 0),
            ));
        }
        Some(diagnostic)
    } else {
        None
    }
}
