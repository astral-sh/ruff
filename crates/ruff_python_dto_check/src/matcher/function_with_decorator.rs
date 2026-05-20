//! Matcher for `function_with_decorator` kind.
//!
//! Matches top-level function definitions decorated with a decorator that
//! satisfies the rule's `decorator` selector, then evaluates the `emit`
//! dot-path expressions to produce an [`EmittedBundle`].

use std::collections::BTreeMap;
use std::path::Path;

use ruff_python_ast::{Decorator, Expr, Stmt, StmtFunctionDef};
use ruff_python_parser::parse_module;
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;
use serde_json::Value;

use crate::bundle::EmittedBundle;
use crate::config::{Config, DecoratorSelector, MatchRule};
use crate::emit::{eval_path, FunctionContext};

/// Parse one Python source file and emit one [`EmittedBundle`] per matched
/// function-with-decorator rule combination.
pub fn harvest_module_with_config(
    source_file: &str,
    source: &str,
    config: &Config,
) -> Vec<EmittedBundle> {
    let Ok(parsed) = parse_module(source) else {
        return Vec::new();
    };
    let line_index = LineIndex::from_source_text(source);
    let mut out = Vec::new();

    for stmt in &parsed.syntax().body {
        let Stmt::FunctionDef(func) = stmt else {
            continue;
        };
        for rule in &config.match_rules {
            if let Some(bundle) =
                try_match_function(source_file, source, &line_index, func, rule, config)
            {
                out.push(bundle);
            }
        }
    }
    out
}

/// Try to match a single function against a single rule.
fn try_match_function(
    source_file: &str,
    source: &str,
    line_index: &LineIndex,
    func: &StmtFunctionDef,
    rule: &MatchRule,
    config: &Config,
) -> Option<EmittedBundle> {
    let matched_dec = find_matching_decorator(func, rule)?;

    let ctx = FunctionContext {
        func,
        matched_decorator: matched_dec,
        source,
        line_index,
    };

    let mut fields: BTreeMap<String, Value> = BTreeMap::new();
    for (field_name, path) in &rule.emit {
        let val = eval_path(path, &ctx);
        fields.insert(field_name.clone(), val);
    }

    let function_name = func.name.id.to_string();
    let family = resolve_family(source_file, config);

    let line_start = line_index.line_index(func.range().start()).get() as u32;
    let line_end = line_index.line_index(func.range().end()).get() as u32;
    let body_lines = line_end.saturating_sub(line_start) + 1;

    let all_decorators: Vec<String> = func
        .decorator_list
        .iter()
        .map(|d| {
            let start = d.range().start().to_usize();
            let end = d.range().end().to_usize();
            source[start..end].to_string()
        })
        .collect();

    Some(EmittedBundle {
        match_id: rule.id.clone(),
        file: source_file.to_string(),
        function_name,
        family,
        line_start,
        line_end,
        body_lines,
        all_decorators,
        fields,
        comparison_within_family: None,
    })
}

/// Find the first decorator on `func` that satisfies the rule's selector.
fn find_matching_decorator<'a>(
    func: &'a StmtFunctionDef,
    rule: &MatchRule,
) -> Option<&'a Decorator> {
    let sel = rule.decorator.as_ref();
    for dec in &func.decorator_list {
        if decorator_matches(dec, sel) {
            return Some(dec);
        }
    }
    None
}

/// Check if a decorator satisfies a selector.
fn decorator_matches(dec: &Decorator, sel: Option<&DecoratorSelector>) -> bool {
    let Some(sel) = sel else {
        // No selector → match any decorated function.
        return true;
    };
    if sel.any {
        return true;
    }

    let min_pos = sel.min_positional_args.unwrap_or(0);

    if let Some(attr_name) = &sel.attribute {
        if decorator_has_attribute(dec, attr_name) {
            return call_has_min_positional(dec, min_pos);
        }
    }

    if let Some(bare_name) = &sel.name {
        if decorator_has_bare_name(dec, bare_name) {
            return call_has_min_positional(dec, min_pos);
        }
    }

    false
}

/// `@bp.route(...)` → attribute = "route"
fn decorator_has_attribute(dec: &Decorator, attr_name: &str) -> bool {
    if let Expr::Call(call) = &dec.expression
        && let Expr::Attribute(attr) = &*call.func
    {
        return attr.attr.id.as_str() == attr_name;
    }
    // Non-call decorator with attribute access: `@bp.route` (no parens).
    if let Expr::Attribute(attr) = &dec.expression {
        return attr.attr.id.as_str() == attr_name;
    }
    false
}

/// `@login_required` → name = "login_required"
fn decorator_has_bare_name(dec: &Decorator, name: &str) -> bool {
    if let Expr::Call(call) = &dec.expression
        && let Expr::Name(n) = &*call.func
    {
        return n.id.as_str() == name;
    }
    if let Expr::Name(n) = &dec.expression {
        return n.id.as_str() == name;
    }
    false
}

fn call_has_min_positional(dec: &Decorator, min: u32) -> bool {
    if min == 0 {
        return true;
    }
    if let Expr::Call(call) = &dec.expression {
        return call.arguments.args.len() as u32 >= min;
    }
    false
}

/// Resolve family name from source_file path using the config's group rule.
pub fn resolve_family(source_file: &str, config: &Config) -> String {
    let filename = Path::new(source_file)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if let Some(fff) = &config.group.family_from_filename {
        if let Some(caps) = fff.compiled.captures(filename) {
            if let Some(m) = caps.name("family") {
                return m.as_str().to_string();
            }
        }
    }

    // Fallback: stem without extension.
    Path::new(source_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}
