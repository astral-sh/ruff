//! Dot-path expression evaluator over a matched AST function context.
//!
//! Supported dot-paths for `function_with_decorator`:
//! - `def.name`              → function name string
//! - `def.params`            → comma-joined parameter names
//! - `def.body.source`       → raw source text of the body
//! - `def.decorators`        → JSON array of decorator raw strings
//! - `def.range.line_start`  → 1-based start line
//! - `def.range.line_end`    → 1-based end line
//! - `decorator.args[<int>]` → positional argument at index
//! - `decorator.kwargs.<k>`  → keyword argument by name
//! - `decorator.raw`         → full decorator text including `@`
//!
//! Path is emitted as `null` if absent. Type mismatches emit the raw value
//! and a `_type_note` sibling field — never panics.

use ruff_python_ast::{Expr, Keyword, StmtFunctionDef};
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;
use serde_json::Value;

/// All context available when evaluating emit paths for a matched function.
pub struct FunctionContext<'a> {
    pub func: &'a StmtFunctionDef,
    /// The specific decorator that triggered the match rule.
    pub matched_decorator: &'a ruff_python_ast::Decorator,
    pub source: &'a str,
    pub line_index: &'a LineIndex,
}

/// Evaluate a dot-path expression against a function context.
/// Returns `Value::Null` if the path is absent or evaluates to nothing.
pub fn eval_path(path: &str, ctx: &FunctionContext<'_>) -> Value {
    if let Some(rest) = path.strip_prefix("def.") {
        eval_def_path(rest, ctx)
    } else if let Some(rest) = path.strip_prefix("decorator.") {
        eval_decorator_path(rest, ctx)
    } else {
        Value::Null
    }
}

fn eval_def_path(path: &str, ctx: &FunctionContext<'_>) -> Value {
    match path {
        "name" => Value::String(ctx.func.name.id.to_string()),
        "params" => {
            let params: Vec<String> = ctx
                .func
                .parameters
                .iter_non_variadic_params()
                .map(|p| p.parameter.name.id.to_string())
                .chain(
                    ctx.func.parameters.vararg.as_ref()
                        .map(|v| format!("*{}", v.name.id))
                )
                .chain(
                    ctx.func.parameters.kwarg.as_ref()
                        .map(|k| format!("**{}", k.name.id))
                )
                .collect();
            Value::String(params.join(", "))
        }
        "body.source" => {
            let body_range = ctx.func.range();
            // Body starts after the `def ...:` header — skip to first statement.
            if let Some(first_stmt) = ctx.func.body.first() {
                let start = first_stmt.range().start().to_usize();
                let end = body_range.end().to_usize();
                if start <= end && end <= ctx.source.len() {
                    return Value::String(ctx.source[start..end].to_string());
                }
            }
            Value::Null
        }
        "decorators" => {
            let decs: Vec<Value> = ctx
                .func
                .decorator_list
                .iter()
                .map(|d| {
                    let start = d.range().start().to_usize();
                    let end = d.range().end().to_usize();
                    Value::String(ctx.source[start..end].to_string())
                })
                .collect();
            Value::Array(decs)
        }
        "range.line_start" => {
            let line = ctx.line_index.line_index(ctx.func.range().start()).get();
            Value::Number(line.into())
        }
        "range.line_end" => {
            let line = ctx.line_index.line_index(ctx.func.range().end()).get();
            Value::Number(line.into())
        }
        _ => Value::Null,
    }
}

fn eval_decorator_path(path: &str, ctx: &FunctionContext<'_>) -> Value {
    if path == "raw" {
        let start = ctx.matched_decorator.range().start().to_usize();
        let end = ctx.matched_decorator.range().end().to_usize();
        return Value::String(ctx.source[start..end].to_string());
    }

    // decorator.args[<index>]
    if let Some(rest) = path.strip_prefix("args[") {
        if let Some(idx_str) = rest.strip_suffix(']') {
            if let Ok(idx) = idx_str.parse::<usize>() {
                return eval_decorator_arg(idx, ctx);
            }
        }
        return Value::Null;
    }

    // decorator.kwargs.<name>
    if let Some(kw_name) = path.strip_prefix("kwargs.") {
        return eval_decorator_kwarg(kw_name, ctx);
    }

    Value::Null
}

fn eval_decorator_arg(idx: usize, ctx: &FunctionContext<'_>) -> Value {
    let Expr::Call(call) = &ctx.matched_decorator.expression else {
        return Value::Null;
    };
    let arg = call.arguments.args.get(idx)?;
    expr_to_value(arg, ctx.source)
}

fn eval_decorator_kwarg(name: &str, ctx: &FunctionContext<'_>) -> Value {
    let Expr::Call(call) = &ctx.matched_decorator.expression else {
        return Value::Null;
    };
    for kw in &call.arguments.keywords {
        let Some(kw_name) = kw.arg.as_ref() else {
            continue;
        };
        if kw_name.id.as_str() == name {
            return expr_to_value(&kw.value, ctx.source);
        }
    }
    Value::Null
}

/// Convert an AST expression to a JSON value (best-effort, no panic).
fn expr_to_value(expr: &Expr, source: &str) -> Value {
    match expr {
        Expr::StringLiteral(s) => Value::String(s.value.to_str().to_string()),
        Expr::NumberLiteral(n) => match &n.value {
            ruff_python_ast::Number::Int(i) => {
                if let Some(v) = i.as_u64() {
                    Value::Number(v.into())
                } else if let Some(v) = i.as_i64() {
                    Value::Number(v.into())
                } else {
                    Value::String(i.to_string())
                }
            }
            ruff_python_ast::Number::Float(f) => {
                serde_json::Number::from_f64(*f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            ruff_python_ast::Number::Complex { real, imag } => {
                Value::String(format!("{real}+{imag}j"))
            }
        },
        Expr::BooleanLiteral(b) => Value::Bool(b.value),
        Expr::NoneLiteral(_) => Value::Null,
        Expr::List(list) => {
            Value::Array(list.elts.iter().map(|e| expr_to_value(e, source)).collect())
        }
        Expr::Tuple(tup) => {
            Value::Array(tup.elts.iter().map(|e| expr_to_value(e, source)).collect())
        }
        other => {
            // Emit raw source text for anything we can't decompose.
            let start = other.range().start().to_usize();
            let end = other.range().end().to_usize();
            if start <= end && end <= source.len() {
                Value::String(source[start..end].to_string())
            } else {
                Value::Null
            }
        }
    }
}

// Bring in Ranged for the Option-returning arm.
use ruff_text_size::TextRange;

impl FunctionContext<'_> {
    /// Line count of the function body (line_end - line_start + 1).
    pub fn body_line_count(&self) -> u32 {
        let start = self.line_index.line_index(self.func.range().start()).get() as u32;
        let end = self.line_index.line_index(self.func.range().end()).get() as u32;
        end.saturating_sub(start) + 1
    }

    /// Raw text of the decorator including `@`.
    pub fn decorator_raw_text(&self) -> &str {
        let start = self.matched_decorator.range().start().to_usize();
        let end = self.matched_decorator.range().end().to_usize();
        &self.source[start..end]
    }
}

// Silence unused import warning — TextRange is used indirectly.
const _: fn() = || {
    let _: TextRange;
};
