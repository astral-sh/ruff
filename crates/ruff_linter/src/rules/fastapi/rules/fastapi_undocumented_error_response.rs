use std::collections::BTreeSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::visitor::source_order;
use ruff_python_ast::{AnyNodeRef, Expr, ExprCall, Number, Stmt};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::fastapi::rules::is_fastapi_route_decorator;

/// ## What it does
/// Checks for FastAPI routes that raise an `HTTPException` (or return a response
/// with a `status_code=`) for an HTTP error status code that is not documented
/// in the route's `responses=` parameter, the parent router's `responses=`, or
/// the decorator's `openapi_extra` value.
///
/// ## Why is this bad?
/// FastAPI does not auto-document an error response when a route's body calls
/// `raise HTTPException(status_code=404, ...)` or returns
/// `JSONResponse(..., status_code=404)`. Clients generated from the resulting
/// `OpenAPI` schema have no type information for the error body. The fix is to
/// list the missing code in `responses=`.
///
/// ## Example
///
/// ```python
/// from fastapi import FastAPI, HTTPException
///
/// app = FastAPI()
///
///
/// @app.get("/items/{item_id}")
/// async def read_item(item_id: int):
///     if item_id < 0:
///         raise HTTPException(status_code=404, detail="Not found")
///     return {"item_id": item_id}
/// ```
///
/// Use instead:
///
/// ```python
/// from fastapi import FastAPI, HTTPException
///
/// app = FastAPI()
///
///
/// @app.get("/items/{item_id}", responses={404: {"description": "Not found"}})
/// async def read_item(item_id: int):
///     if item_id < 0:
///         raise HTTPException(status_code=404, detail="Not found")
///     return {"item_id": item_id}
/// ```
///
/// ## Known problems
/// This rule only inspects the route function's own body and its decorator. It does
/// not follow `include_router(..., responses=...)` composition, custom router
/// subclasses, or `HTTPException` subclasses. Codes that flow through helper
/// functions, attribute assignment (`response.status_code = 500`), or other
/// indirection are also missed.
///
/// ## References
/// - [FastAPI Additional Responses](https://fastapi.tiangolo.com/advanced/additional-responses/)
/// - [FastAPI Response Status Code](https://fastapi.tiangolo.com/tutorial/response-status-code/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct FastApiUndocumentedErrorResponse {
    codes: Vec<u16>,
}

impl Violation for FastApiUndocumentedErrorResponse {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { codes } = self;
        let codes_str = codes
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        if codes.len() == 1 {
            format!(
                "FastAPI route raises HTTP {codes_str} but does not document it in `responses=`"
            )
        } else {
            format!(
                "FastAPI route raises HTTP {codes_str} but does not document them in `responses=`"
            )
        }
    }
}

pub(crate) fn fastapi_undocumented_error_response(
    checker: &Checker,
    function_def: &ast::StmtFunctionDef,
) {
    if !checker.semantic().seen_module(Modules::FASTAPI) {
        return;
    }

    let route_decorators: Vec<(&ast::Decorator, &ExprCall)> = function_def
        .decorator_list
        .iter()
        .filter_map(|decorator| {
            let call = is_fastapi_route_decorator(decorator, checker.semantic())?;
            if has_include_in_schema_false(call)
                || route_suppressed_by_router(call, checker.semantic())
            {
                return None;
            }
            Some((decorator, call))
        })
        .collect();

    if route_decorators.is_empty() {
        return;
    }

    let raised = {
        let mut visitor = RaisedCodeVisitor::new(checker.semantic());
        source_order::walk_body(&mut visitor, &function_def.body);
        visitor.into_codes()
    };

    if raised.is_empty() {
        return;
    }

    for (decorator, call) in route_decorators {
        let mut documented = DocumentedCodes::default();
        collect_decorator_documented(call, checker.semantic(), &mut documented);
        collect_router_documented(call, checker.semantic(), &mut documented);

        let missing: Vec<u16> = raised
            .iter()
            .copied()
            .filter(|code| !documented.covers(*code))
            .collect();

        if !missing.is_empty() {
            checker.report_diagnostic(
                FastApiUndocumentedErrorResponse { codes: missing },
                decorator.range(),
            );
        }
    }
}

/// Returns `true` if the decorator literal `include_in_schema=False` argument is present.
fn has_include_in_schema_false(call: &ExprCall) -> bool {
    let Some(keyword) = call.arguments.find_keyword("include_in_schema") else {
        return false;
    };
    matches!(
        &keyword.value,
        Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: false, .. })
    )
}

/// AST visitor that walks the body of a route function and collects 4xx/5xx status codes
/// from `raise HTTPException(...)` and `return SomeResponse(..., status_code=...)`.
///
/// Nested function, class, lambda, and comprehension scopes are skipped, since their
/// status codes belong to a different callable. `RUF029` uses the same approach.
struct RaisedCodeVisitor<'a> {
    codes: BTreeSet<u16>,
    semantic: &'a SemanticModel<'a>,
}

impl<'a> RaisedCodeVisitor<'a> {
    fn new(semantic: &'a SemanticModel<'a>) -> Self {
        Self {
            codes: BTreeSet::new(),
            semantic,
        }
    }

    fn into_codes(self) -> BTreeSet<u16> {
        self.codes
    }

    fn record_call_status(&mut self, call: &ExprCall) {
        if !is_http_exception(&call.func, self.semantic) {
            return;
        }
        if let Some(code) = call
            .arguments
            .find_argument_value("status_code", 0)
            .and_then(|expr| resolve_status_code(expr, self.semantic))
            && is_error_status(code)
        {
            self.codes.insert(code);
        }
    }

    fn record_return_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call(call) => {
                if is_response_class(&call.func, self.semantic)
                    && let Some(status_code_expr) =
                        call.arguments.find_argument_value("status_code", 1)
                    && let Some(code) = resolve_status_code(status_code_expr, self.semantic)
                    && is_error_status(code)
                {
                    self.codes.insert(code);
                }
            }
            Expr::If(ast::ExprIf { body, orelse, .. }) => {
                self.record_return_expr(body);
                self.record_return_expr(orelse);
            }
            Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
                for value in values {
                    self.record_return_expr(value);
                }
            }
            _ => {}
        }
    }
}

impl<'a> source_order::SourceOrderVisitor<'a> for RaisedCodeVisitor<'a> {
    fn enter_node(&mut self, _node: AnyNodeRef<'a>) -> source_order::TraversalSignal {
        source_order::TraversalSignal::Traverse
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(function_def) => {
                function_def_visit_preorder_except_body(function_def, self);
            }
            Stmt::ClassDef(class_def) => {
                class_def_visit_preorder_except_body(class_def, self);
            }
            Stmt::Raise(ast::StmtRaise { exc: Some(exc), .. }) => {
                if let Expr::Call(call) = exc.as_ref() {
                    self.record_call_status(call);
                }
            }
            Stmt::Return(ast::StmtReturn {
                value: Some(value), ..
            }) => {
                self.record_return_expr(value);
            }
            _ => source_order::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(_) => {}
            _ => source_order::walk_expr(self, expr),
        }
    }

    fn visit_comprehension(&mut self, _comprehension: &'a ast::Comprehension) {}
}

/// Visit a `StmtFunctionDef`'s decorators, type params, parameters, and return annotation,
/// without descending into the body.
fn function_def_visit_preorder_except_body<'a, V>(
    function_def: &'a ast::StmtFunctionDef,
    visitor: &mut V,
) where
    V: source_order::SourceOrderVisitor<'a>,
{
    let ast::StmtFunctionDef {
        parameters,
        decorator_list,
        returns,
        type_params,
        ..
    } = function_def;

    for decorator in decorator_list {
        visitor.visit_decorator(decorator);
    }

    if let Some(type_params) = type_params {
        visitor.visit_type_params(type_params);
    }

    visitor.visit_parameters(parameters);

    if let Some(expr) = returns {
        visitor.visit_annotation(expr);
    }
}

/// Same as above for `StmtClassDef`.
fn class_def_visit_preorder_except_body<'a, V>(class_def: &'a ast::StmtClassDef, visitor: &mut V)
where
    V: source_order::SourceOrderVisitor<'a>,
{
    let ast::StmtClassDef {
        arguments,
        decorator_list,
        type_params,
        ..
    } = class_def;

    for decorator in decorator_list {
        visitor.visit_decorator(decorator);
    }

    if let Some(type_params) = type_params {
        visitor.visit_type_params(type_params);
    }

    if let Some(arguments) = arguments {
        visitor.visit_arguments(arguments);
    }
}

/// Cheap pre-filter that skips qualified-name resolution for things that obviously
/// cannot be a class call. Literals, subscripts, computed values, etc. never match
/// a fastapi/starlette qualified name, so resolving them is wasted work.
fn is_possibly_imported_class(expr: &Expr) -> bool {
    matches!(expr, Expr::Name(_) | Expr::Attribute(_))
}

fn is_http_exception(expr: &Expr, semantic: &SemanticModel) -> bool {
    if !is_possibly_imported_class(expr) {
        return false;
    }
    semantic
        .resolve_qualified_name(expr)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["fastapi", "HTTPException"]
                    | ["fastapi" | "starlette", "exceptions", "HTTPException"]
            )
        })
}

fn is_response_class(expr: &Expr, semantic: &SemanticModel) -> bool {
    if !is_possibly_imported_class(expr) {
        return false;
    }
    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return false;
    };
    matches!(
        qualified_name.segments(),
        ["fastapi", "Response"]
            | [
                "fastapi" | "starlette",
                "responses",
                "Response"
                    | "JSONResponse"
                    | "PlainTextResponse"
                    | "HTMLResponse"
                    | "RedirectResponse"
                    | "StreamingResponse"
                    | "FileResponse"
            ]
            | ["fastapi", "responses", "ORJSONResponse" | "UJSONResponse"]
    )
}

fn is_error_status(code: u16) -> bool {
    (400..=599).contains(&code)
}

/// Try to statically resolve an `Expr` to an HTTP status code integer.
///
/// Handles integer literals, `fastapi.status.HTTP_*`, `starlette.status.HTTP_*`, and
/// `http.HTTPStatus.*`. Returns `None` for anything we can't resolve.
fn resolve_status_code(expr: &Expr, semantic: &SemanticModel) -> Option<u16> {
    if let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: Number::Int(int_value),
        ..
    }) = expr
    {
        return int_value.as_u64().and_then(|n| u16::try_from(n).ok());
    }

    let qualified_name = semantic.resolve_qualified_name(expr)?;
    match qualified_name.segments() {
        ["fastapi" | "starlette", "status", name] => parse_status_constant(name),
        ["http", "HTTPStatus", name] | ["http", "HTTPStatus", name, "value"] => {
            http_status_name_to_code(name)
        }
        _ => None,
    }
}

/// Parse a name like `HTTP_404_NOT_FOUND` and return `404`.
fn parse_status_constant(name: &str) -> Option<u16> {
    let after_http = name.strip_prefix("HTTP_")?;
    let digit_end = after_http
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_http.len());
    after_http[..digit_end].parse().ok()
}

/// Map a Python `http.HTTPStatus` enum member name to its numeric code. Only includes
/// 4xx and 5xx values, since other codes are out of scope for this rule.
fn http_status_name_to_code(name: &str) -> Option<u16> {
    Some(match name {
        "BAD_REQUEST" => 400,
        "UNAUTHORIZED" => 401,
        "PAYMENT_REQUIRED" => 402,
        "FORBIDDEN" => 403,
        "NOT_FOUND" => 404,
        "METHOD_NOT_ALLOWED" => 405,
        "NOT_ACCEPTABLE" => 406,
        "PROXY_AUTHENTICATION_REQUIRED" => 407,
        "REQUEST_TIMEOUT" => 408,
        "CONFLICT" => 409,
        "GONE" => 410,
        "LENGTH_REQUIRED" => 411,
        "PRECONDITION_FAILED" => 412,
        "REQUEST_ENTITY_TOO_LARGE" | "CONTENT_TOO_LARGE" => 413,
        "REQUEST_URI_TOO_LONG" | "URI_TOO_LONG" => 414,
        "UNSUPPORTED_MEDIA_TYPE" => 415,
        "REQUESTED_RANGE_NOT_SATISFIABLE" | "RANGE_NOT_SATISFIABLE" => 416,
        "EXPECTATION_FAILED" => 417,
        "IM_A_TEAPOT" => 418,
        "MISDIRECTED_REQUEST" => 421,
        "UNPROCESSABLE_ENTITY" | "UNPROCESSABLE_CONTENT" => 422,
        "LOCKED" => 423,
        "FAILED_DEPENDENCY" => 424,
        "TOO_EARLY" => 425,
        "UPGRADE_REQUIRED" => 426,
        "PRECONDITION_REQUIRED" => 428,
        "TOO_MANY_REQUESTS" => 429,
        "REQUEST_HEADER_FIELDS_TOO_LARGE" => 431,
        "UNAVAILABLE_FOR_LEGAL_REASONS" => 451,
        "INTERNAL_SERVER_ERROR" => 500,
        "NOT_IMPLEMENTED" => 501,
        "BAD_GATEWAY" => 502,
        "SERVICE_UNAVAILABLE" => 503,
        "GATEWAY_TIMEOUT" => 504,
        "HTTP_VERSION_NOT_SUPPORTED" => 505,
        "VARIANT_ALSO_NEGOTIATES" => 506,
        "INSUFFICIENT_STORAGE" => 507,
        "LOOP_DETECTED" => 508,
        "NOT_EXTENDED" => 510,
        "NETWORK_AUTHENTICATION_REQUIRED" => 511,
        _ => return None,
    })
}

#[derive(Default)]
struct DocumentedCodes {
    explicit: BTreeSet<u16>,
    has_4xx_wildcard: bool,
    has_5xx_wildcard: bool,
    has_default: bool,
}

impl DocumentedCodes {
    fn covers(&self, code: u16) -> bool {
        if self.has_default {
            return true;
        }
        if self.has_4xx_wildcard && (400..500).contains(&code) {
            return true;
        }
        if self.has_5xx_wildcard && (500..600).contains(&code) {
            return true;
        }
        self.explicit.contains(&code)
    }
}

/// Look at the route decorator for `responses=` and `openapi_extra={"responses": ...}`.
fn collect_decorator_documented(
    call: &ExprCall,
    semantic: &SemanticModel,
    documented: &mut DocumentedCodes,
) {
    if let Some(keyword) = call.arguments.find_keyword("responses") {
        collect_codes_from_dict(&keyword.value, semantic, documented);
    }
    if let Some(keyword) = call.arguments.find_keyword("openapi_extra")
        && let Expr::Dict(ast::ExprDict { items, .. }) = &keyword.value
    {
        for item in items {
            let Some(key) = item.key.as_ref() else {
                continue;
            };
            if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = key
                && value.to_str() == "responses"
            {
                collect_codes_from_dict(&item.value, semantic, documented);
            }
        }
    }
}

/// Look up the router instance in the same module and read its `responses=` kwarg.
fn collect_router_documented(
    call: &ExprCall,
    semantic: &SemanticModel,
    documented: &mut DocumentedCodes,
) {
    let Some(router_call) = resolve_router_call(call, semantic) else {
        return;
    };
    if let Some(keyword) = router_call.arguments.find_keyword("responses") {
        collect_codes_from_dict(&keyword.value, semantic, documented);
    }
}

/// Returns `true` if the route's router was constructed with `include_in_schema=False`,
/// which suppresses all of its routes from the `OpenAPI` schema (so there is nothing to
/// document).
fn route_suppressed_by_router(call: &ExprCall, semantic: &SemanticModel) -> bool {
    let Some(router_call) = resolve_router_call(call, semantic) else {
        return false;
    };
    has_include_in_schema_false(router_call)
}

/// Resolve `@router.get(...)` to the `APIRouter(...)` (or `FastAPI(...)`) call site that
/// bound `router` in the same module. Handles both plain `router = APIRouter(...)` and
/// annotated `router: APIRouter = APIRouter(...)` bindings.
fn resolve_router_call<'a>(
    call: &'a ExprCall,
    semantic: &'a SemanticModel,
) -> Option<&'a ExprCall> {
    let Expr::Attribute(ast::ExprAttribute { value, .. }) = call.func.as_ref() else {
        return None;
    };
    let name = value.as_name_expr()?;
    let binding_id = semantic.resolve_name(name)?;
    let binding = semantic.binding(binding_id);
    let source = binding.source?;

    let value = match semantic.statement(source) {
        Stmt::Assign(ast::StmtAssign { value, .. }) => value.as_ref(),
        Stmt::AnnAssign(ast::StmtAnnAssign {
            value: Some(value), ..
        }) => value.as_ref(),
        _ => return None,
    };
    if let Expr::Call(router_call) = value {
        Some(router_call)
    } else {
        None
    }
}

/// Parse a `{<status>: ..., ...}` dict literal and record what's covered.
fn collect_codes_from_dict(
    expr: &Expr,
    semantic: &SemanticModel,
    documented: &mut DocumentedCodes,
) {
    let Expr::Dict(ast::ExprDict { items, .. }) = expr else {
        return;
    };
    for item in items {
        let Some(key) = item.key.as_ref() else {
            continue;
        };
        match key {
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: Number::Int(int_value),
                ..
            }) => {
                if let Some(code) = int_value.as_u64().and_then(|n| u16::try_from(n).ok()) {
                    documented.explicit.insert(code);
                }
            }
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                let s = value.to_str();
                match s {
                    "4XX" | "4xx" => documented.has_4xx_wildcard = true,
                    "5XX" | "5xx" => documented.has_5xx_wildcard = true,
                    "default" => documented.has_default = true,
                    _ => {
                        if let Ok(code) = s.parse::<u16>() {
                            documented.explicit.insert(code);
                        }
                    }
                }
            }
            _ => {
                if let Some(code) = resolve_status_code(key, semantic) {
                    documented.explicit.insert(code);
                }
            }
        }
    }
}
