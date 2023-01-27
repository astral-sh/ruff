use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, StmtKind};

use crate::ast::helpers::{create_expr, create_stmt, unparse_expr, unparse_stmt};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::source_code::{Locator, Stylist};
use crate::violations;

/// Return `true` if the call path is a reference to `${module}.${any}`.
fn is_module_member(call_path: &[&str], module: &str) -> bool {
    call_path
        .first()
        .map_or(false, |module_name| *module_name == module)
}

fn map_name(name: &str, expr: &Expr, patch: bool) -> Option<Diagnostic> {
    let replacement = match name {
        "text_type" => Some("str"),
        "binary_type" => Some("bytes"),
        "class_types" => Some("(type,)"),
        "string_types" => Some("(str,)"),
        "integer_types" => Some("(int,)"),
        "unichr" => Some("chr"),
        "iterbytes" => Some("iter"),
        "print_" => Some("print"),
        "exec_" => Some("exec"),
        "advance_iterator" => Some("next"),
        "next" => Some("next"),
        "range" => Some("range"),  // TODO: six.moves
        "xrange" => Some("range"), // TODO: six.moves
        "callable" => Some("callable"),
        _ => None,
    };
    if let Some(replacement) = replacement {
        let mut diagnostic =
            Diagnostic::new(violations::RemoveSixCompat, Range::from_located(expr));
        if patch {
            diagnostic.amend(Fix::replacement(
                replacement.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        Some(diagnostic)
    } else {
        None
    }
}

fn replace_by_str_literal(
    arg: &Expr,
    binary: bool,
    expr: &Expr,
    patch: bool,
    locator: &Locator,
) -> Option<Diagnostic> {
    match &arg.node {
        ExprKind::Constant { .. } => {
            let mut diagnostic =
                Diagnostic::new(violations::RemoveSixCompat, Range::from_located(expr));
            if patch {
                let content = format!(
                    "{}{}",
                    if binary { "b" } else { "" },
                    locator.slice_source_code_range(&Range::new(
                        arg.location,
                        arg.end_location.unwrap(),
                    ))
                );
                diagnostic.amend(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            };
            Some(diagnostic)
        }
        _ => None,
    }
}

// `func(arg)` => `arg.attr`
fn replace_call_on_arg_by_arg_attribute(
    attr: &str,
    arg: &Expr,
    expr: &Expr,
    patch: bool,
    stylist: &Stylist,
) -> Diagnostic {
    let attribute = ExprKind::Attribute {
        value: Box::new(arg.clone()),
        attr: attr.to_string(),
        ctx: ExprContext::Load,
    };
    replace_by_expr_kind(attribute, expr, patch, stylist)
}

// `func(arg, **args)` => `arg.method(**args)`
fn replace_call_on_arg_by_arg_method_call(
    method_name: &str,
    args: &[Expr],
    expr: &Expr,
    patch: bool,
    stylist: &Stylist,
) -> Option<Diagnostic> {
    if args.is_empty() {
        None
    } else if let ([arg], other_args) = args.split_at(1) {
        let call = ExprKind::Call {
            func: Box::new(create_expr(ExprKind::Attribute {
                value: Box::new(arg.clone()),
                attr: method_name.to_string(),
                ctx: ExprContext::Load,
            })),
            args: other_args
                .iter()
                .map(|arg| create_expr(arg.node.clone()))
                .collect(),
            keywords: vec![],
        };
        Some(replace_by_expr_kind(call, expr, patch, stylist))
    } else {
        None
    }
}

// `expr` => `Expr(expr_kind)`
fn replace_by_expr_kind(node: ExprKind, expr: &Expr, patch: bool, stylist: &Stylist) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(violations::RemoveSixCompat, Range::from_located(expr));
    if patch {
        diagnostic.amend(Fix::replacement(
            unparse_expr(&create_expr(node), stylist),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    diagnostic
}

fn replace_by_stmt_kind(node: StmtKind, expr: &Expr, patch: bool, stylist: &Stylist) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(violations::RemoveSixCompat, Range::from_located(expr));
    if patch {
        diagnostic.amend(Fix::replacement(
            unparse_stmt(&create_stmt(node), stylist),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    diagnostic
}

// => `raise exc from cause`
fn replace_by_raise_from(
    exc: Option<ExprKind>,
    cause: Option<ExprKind>,
    expr: &Expr,
    patch: bool,
    stylist: &Stylist,
) -> Diagnostic {
    let stmt_kind = StmtKind::Raise {
        exc: exc.map(|exc| Box::new(create_expr(exc))),
        cause: cause.map(|cause| Box::new(create_expr(cause))),
    };
    replace_by_stmt_kind(stmt_kind, expr, patch, stylist)
}

fn replace_by_index_on_arg(
    arg: &Expr,
    index: &ExprKind,
    expr: &Expr,
    patch: bool,
    stylist: &Stylist,
) -> Diagnostic {
    let index = ExprKind::Subscript {
        value: Box::new(create_expr(arg.node.clone())),
        slice: Box::new(create_expr(index.clone())),
        ctx: ExprContext::Load,
    };
    replace_by_expr_kind(index, expr, patch, stylist)
}

fn handle_reraise(
    args: &[Expr],
    expr: &Expr,
    patch: bool,
    stylist: &Stylist,
) -> Option<Diagnostic> {
    if let [_, exc, tb] = args {
        Some(replace_by_raise_from(
            Some(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Attribute {
                    value: Box::new(create_expr(exc.node.clone())),
                    attr: "with_traceback".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![create_expr(tb.node.clone())],
                keywords: vec![],
            }),
            None,
            expr,
            patch,
            stylist,
        ))
    } else if let [arg] = args {
        if let ExprKind::Starred { value, .. } = &arg.node {
            if let ExprKind::Call { func, .. } = &value.node {
                if let ExprKind::Attribute { value, attr, .. } = &func.node {
                    if let ExprKind::Name { id, .. } = &value.node {
                        if id == "sys" && attr == "exc_info" {
                            return Some(replace_by_raise_from(None, None, expr, patch, stylist));
                        };
                    };
                };
            };
        };
        None
    } else {
        None
    }
}

fn handle_func(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    expr: &Expr,
    patch: bool,
    stylist: &Stylist,
    locator: &Locator,
) -> Option<Diagnostic> {
    let func_name = match &func.node {
        ExprKind::Attribute { attr, .. } => attr,
        ExprKind::Name { id, .. } => id,
        _ => return None,
    };
    match (func_name.as_str(), args, keywords) {
        ("b", [arg], []) => replace_by_str_literal(arg, true, expr, patch, locator),
        ("ensure_binary", [arg], []) => replace_by_str_literal(arg, true, expr, patch, locator),
        ("u", [arg], []) => replace_by_str_literal(arg, false, expr, patch, locator),
        ("ensure_str", [arg], []) => replace_by_str_literal(arg, false, expr, patch, locator),
        ("ensure_text", [arg], []) => replace_by_str_literal(arg, false, expr, patch, locator),
        ("iteritems", args, []) => {
            replace_call_on_arg_by_arg_method_call("items", args, expr, patch, stylist)
        }
        ("viewitems", args, []) => {
            replace_call_on_arg_by_arg_method_call("items", args, expr, patch, stylist)
        }
        ("iterkeys", args, []) => {
            replace_call_on_arg_by_arg_method_call("keys", args, expr, patch, stylist)
        }
        ("viewkeys", args, []) => {
            replace_call_on_arg_by_arg_method_call("keys", args, expr, patch, stylist)
        }
        ("itervalues", args, []) => {
            replace_call_on_arg_by_arg_method_call("values", args, expr, patch, stylist)
        }
        ("viewvalues", args, []) => {
            replace_call_on_arg_by_arg_method_call("values", args, expr, patch, stylist)
        }
        ("get_method_function", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__func__", arg, expr, patch, stylist,
        )),
        ("get_method_self", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__self__", arg, expr, patch, stylist,
        )),
        ("get_function_closure", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__closure__",
            arg,
            expr,
            patch,
            stylist,
        )),
        ("get_function_code", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__code__", arg, expr, patch, stylist,
        )),
        ("get_function_defaults", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__defaults__",
            arg,
            expr,
            patch,
            stylist,
        )),
        ("get_function_globals", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__globals__",
            arg,
            expr,
            patch,
            stylist,
        )),
        ("create_unbound_method", [arg, _], _) => {
            Some(replace_by_expr_kind(arg.node.clone(), expr, patch, stylist))
        }
        ("get_unbound_function", [arg], []) => {
            Some(replace_by_expr_kind(arg.node.clone(), expr, patch, stylist))
        }
        ("assertCountEqual", args, []) => {
            replace_call_on_arg_by_arg_method_call("assertCountEqual", args, expr, patch, stylist)
        }
        ("assertRaisesRegex", args, []) => {
            replace_call_on_arg_by_arg_method_call("assertRaisesRegex", args, expr, patch, stylist)
        }
        ("assertRegex", args, []) => {
            replace_call_on_arg_by_arg_method_call("assertRegex", args, expr, patch, stylist)
        }
        ("raise_from", [exc, cause], []) => Some(replace_by_raise_from(
            Some(exc.node.clone()),
            Some(cause.node.clone()),
            expr,
            patch,
            stylist,
        )),
        ("reraise", args, []) => handle_reraise(args, expr, patch, stylist),
        ("byte2int", [arg], []) => Some(replace_by_index_on_arg(
            arg,
            &ExprKind::Constant {
                value: Constant::Int(0.into()),
                kind: None,
            },
            expr,
            patch,
            stylist,
        )),
        ("indexbytes", [arg, index], []) => Some(replace_by_index_on_arg(
            arg,
            &index.node,
            expr,
            patch,
            stylist,
        )),
        ("int2byte", [arg], []) => Some(replace_by_expr_kind(
            ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Name {
                    id: "bytes".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![create_expr(ExprKind::Tuple {
                    elts: vec![create_expr(arg.node.clone())],
                    ctx: ExprContext::Load,
                })],
                keywords: vec![],
            },
            expr,
            patch,
            stylist,
        )),
        _ => None,
    }
}

fn handle_next_on_six_dict(expr: &Expr, patch: bool, checker: &Checker) -> Option<Diagnostic> {
    let ExprKind::Call { func, args, .. } = &expr.node else {
        return None;
    };
    let ExprKind::Name { id, .. } = &func.node else {
        return None;
    };
    if id != "next" {
        return None;
    }
    let [arg] = &args[..] else { return None; };
    if !checker
        .resolve_call_path(arg)
        .map_or(false, |call_path| is_module_member(&call_path, "six"))
    {
        return None;
    }
    let ExprKind::Call { func, args, .. } = &arg.node else {return None;};
    let ExprKind::Attribute { attr, .. } = &func.node else {return None;};
    let [dict_arg] = &args[..] else {return None;};
    let method_name = match attr.as_str() {
        "iteritems" => "items",
        "iterkeys" => "keys",
        "itervalues" => "values",
        _ => return None,
    };
    Some(replace_by_expr_kind(
        ExprKind::Call {
            func: Box::new(create_expr(ExprKind::Name {
                id: "iter".to_string(),
                ctx: ExprContext::Load,
            })),
            args: vec![create_expr(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Attribute {
                    value: Box::new(dict_arg.clone()),
                    attr: method_name.to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![],
                keywords: vec![],
            })],
            keywords: vec![],
        },
        arg,
        patch,
        checker.stylist,
    ))
}

/// UP016
pub fn remove_six_compat(checker: &mut Checker, expr: &Expr) {
    if let Some(diagnostic) =
        handle_next_on_six_dict(expr, checker.patch(&Rule::RemoveSixCompat), checker)
    {
        checker.diagnostics.push(diagnostic);
        return;
    }

    if checker
        .resolve_call_path(expr)
        .map_or(false, |call_path| is_module_member(&call_path, "six"))
    {
        let patch = checker.patch(&Rule::RemoveSixCompat);
        let diagnostic = match &expr.node {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => handle_func(
                func,
                args,
                keywords,
                expr,
                patch,
                checker.stylist,
                checker.locator,
            ),
            ExprKind::Attribute { attr, .. } => map_name(attr.as_str(), expr, patch),
            ExprKind::Name { id, .. } => map_name(id.as_str(), expr, patch),
            _ => return,
        };
        if let Some(diagnostic) = diagnostic {
            checker.diagnostics.push(diagnostic);
        }
    }
}
