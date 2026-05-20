//! Flask route detector — finds `@bp.route(...)`, `@app.route(...)`, or
//! `@<blueprint>.route(...)` on top-level functions and extracts the
//! `(path, methods)` arguments verbatim.

use ruff_python_ast::{Expr, ExprCall, Keyword, StmtFunctionDef};

#[derive(Debug)]
pub struct RouteInfo {
    pub path: String,
    pub methods: Vec<String>,
    pub blueprint: String,
    /// Same as `methods`, exposed separately so the caller can run
    /// [`infer_action`] without cloning.
    pub methods_for_action: Vec<String>,
}

/// Try to detect a Flask route on the function. Returns the first
/// `*.route(...)` decorator's args.
pub fn detect_route(func: &StmtFunctionDef) -> Option<RouteInfo> {
    for d in &func.decorator_list {
        let Some(call) = as_call(&d.expression) else {
            continue;
        };
        let Some((blueprint, method_name)) = attr_chain(&call.func) else {
            continue;
        };
        if method_name != "route" {
            continue;
        }
        let path = call.arguments.args.first().and_then(string_literal)?;
        let methods = methods_kw(&call.arguments.keywords);
        let methods = if methods.is_empty() {
            vec!["GET".to_string()]
        } else {
            methods
        };
        return Some(RouteInfo {
            path,
            methods: methods.clone(),
            blueprint,
            methods_for_action: methods,
        });
    }
    None
}

/// Map an HTTP method set to one of `read | mutation | form`. Mirrors
/// the heuristic in `WoA/.claude/v0.2/tools/harvest_routes_v2.py`.
pub fn infer_action(methods: &[String]) -> String {
    let upper: Vec<String> = methods.iter().map(|m| m.to_uppercase()).collect();
    let has_post = upper.iter().any(|m| m == "POST");
    let has_get = upper.iter().any(|m| m == "GET");
    if has_post && has_get {
        "form".to_string()
    } else if has_post
        || upper
            .iter()
            .any(|m| m == "PUT" || m == "DELETE" || m == "PATCH")
    {
        "mutation".to_string()
    } else {
        "read".to_string()
    }
}

fn as_call(expr: &Expr) -> Option<&ExprCall> {
    if let Expr::Call(c) = expr {
        Some(c)
    } else {
        None
    }
}

/// `bp.route` -> Some(("bp", "route"))
fn attr_chain(expr: &Expr) -> Option<(String, String)> {
    let Expr::Attribute(attr) = expr else {
        return None;
    };
    let Expr::Name(base) = &*attr.value else {
        return None;
    };
    Some((base.id.to_string(), attr.attr.id.to_string()))
}

fn string_literal(expr: &Expr) -> Option<String> {
    if let Expr::StringLiteral(s) = expr {
        Some(s.value.to_str().to_string())
    } else {
        None
    }
}

/// Extract the `methods=[...]` kwarg if present.
fn methods_kw(kws: &[Keyword]) -> Vec<String> {
    for kw in kws {
        let Some(name) = kw.arg.as_ref() else {
            continue;
        };
        if name.id.as_str() != "methods" {
            continue;
        }
        let Expr::List(list) = &kw.value else {
            return Vec::new();
        };
        return list.elts.iter().filter_map(string_literal).collect();
    }
    Vec::new()
}
