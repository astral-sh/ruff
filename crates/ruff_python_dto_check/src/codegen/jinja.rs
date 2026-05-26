//! jinja → askama translation, ported from
//! `woa-rs/tools/render_routes.py` (`_translate_cell_expr` + the rewriters).
//!
//! This is the proven translation logic; it is NOT reinvented. The functions
//! mirror the Python ones:
//! - [`rewrite_elif_to_else_if`]   ← `_rewrite_elif_to_else_if`
//! - [`rewrite_condition_syntax`]  ← `_rewrite_condition_syntax`
//! - [`translate_cell_expr`]       ← `_translate_cell_expr`
//!
//! `model_fields` (field name → Rust type string) enables Option-aware
//! wrapping: `{{ x.f }}` becomes `{% if let Some(v) = x.f %}{{ v }}{% endif %}`
//! when `x.f` is `Option<T>`.

use std::collections::BTreeMap;

/// One extracted jinja table cell.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    /// Pre-translated truthy conditional (jinja `{{ 'A' if cond else 'B' }}`
    /// already lifted to `{% if .. %}A{% else %}B{% endif %}`).
    pub askama_form: Option<String>,
    /// Raw jinja expression (`x.field`, `x.d.strftime('%Y') if x.d else ''`).
    pub expr: Option<String>,
    /// Static text (no `{{ }}`), or markup containing `{% %}` directives.
    pub static_text: Option<String>,
    /// Column header.
    pub header: String,
    /// `code` wraps the cell in `<code>…</code>`.
    pub wrapper: Option<String>,
}

/// `{% elif X %}` → `{% else if X %}`.
pub fn rewrite_elif_to_else_if(s: &str) -> String {
    replace_tag(s, "elif", |cond| format!("{{% else if {cond} %}}"))
}

/// Rewrite jinja Python operators inside `{% if %}` / `{% else if %}` tags to
/// Rust expression syntax: `'foo'`→`"foo"`, ` and `→` && `, ` or `→` || `,
/// `not `→`!`.
pub fn rewrite_condition_syntax(s: &str) -> String {
    rewrite_if_conditions(s, |cond| {
        let cond = single_to_double_quotes(cond);
        let cond = word_replace(&cond, "and", "&&");
        let cond = word_replace(&cond, "or", "||");
        replace_not_prefix(&cond)
    })
}

/// Translate a single cell to askama HTML. Mirrors `_translate_cell_expr`'s
/// precedence ladder.
pub fn translate_cell_expr(
    cell: &Cell,
    row_var: &str,
    model_fields: Option<&BTreeMap<String, String>>,
) -> String {
    if let Some(af) = &cell.askama_form {
        let mut af = rewrite_elif_to_else_if(af);
        af = rewrite_condition_syntax(&af);
        return af;
    }
    if cell.expr.is_none()
        && let Some(st) = &cell.static_text
    {
        if st.contains("{%") || st.contains("{{") {
            let mut st = rewrite_elif_to_else_if(st);
            st = rewrite_condition_syntax(&st);
            return st;
        }
        return st.clone();
    }
    let expr = cell.expr.as_deref().unwrap_or("").trim().to_string();

    // Direct `<row>.<field>` — Option-aware.
    if let Some(field) = direct_field(&expr, row_var) {
        if let Some(fields) = model_fields
            && is_option_type(fields.get(&field).map(String::as_str).unwrap_or(""))
        {
            return format!("{{% if let Some(v) = {expr} %}}{{{{ v }}}}{{% endif %}}");
        }
        return format!("{{{{ {expr} }}}}");
    }

    // `obj.strftime('fmt') if obj else 'F'` → if-let-Some-date.
    if let Some((obj, fmt, fallback)) = match_strftime_guard(&expr) {
        return format!(
            "{{% if let Some(d) = {obj} %}}{{{{ d.format(\"{fmt}\") }}}}{{% else %}}{fallback}{{% endif %}}"
        );
    }

    // `obj if obj is not none else 'F'` → if-let-Some.
    if let Some((obj, fallback)) = match_is_not_none(&expr) {
        return format!(
            "{{% if let Some(v) = {obj} %}}{{{{ v }}}}{{% else %}}{fallback}{{% endif %}}"
        );
    }

    // `obj or 'F'` → Option-aware string fallback.
    if let Some((obj, fallback)) = match_or_string(&expr) {
        if let Some(field) = direct_field(&obj, row_var)
            && let Some(fields) = model_fields
            && is_option_type(fields.get(&field).map(String::as_str).unwrap_or(""))
        {
            return format!(
                "{{% if let Some(v) = {obj} %}}{{{{ v }}}}{{% else %}}{fallback}{{% endif %}}"
            );
        }
        return format!("{{{{ {obj} }}}}");
    }

    // `obj or N` numeric fallback.
    if let Some((obj, fallback)) = match_or_numeric(&expr) {
        if let Some(field) = direct_field(&obj, row_var)
            && let Some(fields) = model_fields
        {
            let ty = fields.get(&field).map(String::as_str).unwrap_or("");
            if is_option_type(ty) && is_numeric_type(ty) {
                return format!("{{{{ {obj}.unwrap_or({fallback}) }}}}");
            }
            if is_option_type(ty) {
                return format!(
                    "{{% if let Some(v) = {obj} %}}{{{{ v }}}}{{% else %}}{fallback}{{% endif %}}"
                );
            }
        }
        return format!("{{{{ {obj} }}}}");
    }

    // Fallback: a grep-able TODO marker; the page still renders.
    format!("{{# TODO-CODEGEN: translate jinja expr: {expr} #}}")
}

// ---------------------------------------------------------------------------
// Type helpers (ported from `_is_option_type` / `_is_numeric_type`)
// ---------------------------------------------------------------------------

pub fn is_option_type(ty: &str) -> bool {
    let s = ty.trim();
    s.starts_with("Option<") && s.ends_with('>')
}

fn option_inner(ty: &str) -> &str {
    let s = ty.trim();
    if s.starts_with("Option<") && s.ends_with('>') {
        s["Option<".len()..s.len() - 1].trim()
    } else {
        s
    }
}

fn is_numeric_type(ty: &str) -> bool {
    let inner = option_inner(ty);
    matches!(
        inner,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "f32"
            | "f64"
            | "usize"
            | "isize"
    ) || inner.ends_with("Decimal")
        || inner == "Decimal"
}

// ---------------------------------------------------------------------------
// Expression matchers (port of the RE_CG_* regexes, hand-rolled, no regex dep
// inside the hot path to keep this self-contained and panic-free)
// ---------------------------------------------------------------------------

/// `<row>.<field>` → `Some("field")` (single attribute access only).
fn direct_field(expr: &str, row_var: &str) -> Option<String> {
    let prefix = format!("{row_var}.");
    let field = expr.strip_prefix(&prefix)?;
    if !field.is_empty() && field.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(field.to_string())
    } else {
        None
    }
}

/// `obj.strftime('fmt') if obj else 'fallback'`.
fn match_strftime_guard(expr: &str) -> Option<(String, String, String)> {
    let (lhs, rest) = expr.split_once(" if ")?;
    let lhs = lhs.trim();
    let strftime_idx = lhs.find(".strftime(")?;
    let obj = lhs[..strftime_idx].trim().to_string();
    let fmt_part = &lhs[strftime_idx + ".strftime(".len()..];
    let fmt = quoted_inner(fmt_part.trim_end_matches(')'))?;
    let (cond, else_part) = rest.split_once(" else ")?;
    if cond.trim() != obj {
        return None;
    }
    let fallback = quoted_inner(else_part.trim())?;
    Some((obj, fmt, fallback))
}

/// `obj if obj is not none else 'fallback'`.
fn match_is_not_none(expr: &str) -> Option<(String, String)> {
    let (obj, rest) = expr.split_once(" if ")?;
    let obj = obj.trim().to_string();
    let marker = format!("{obj} is not none");
    let rest = rest.trim();
    let after = rest.strip_prefix(&marker)?;
    let fallback = quoted_inner(after.trim().strip_prefix("else ")?.trim())?;
    Some((obj, fallback))
}

/// `obj or 'fallback'` (string).
fn match_or_string(expr: &str) -> Option<(String, String)> {
    let (obj, rest) = expr.split_once(" or ")?;
    let fallback = quoted_inner(rest.trim())?;
    Some((obj.trim().to_string(), fallback))
}

/// `obj or N` (numeric, no quotes).
fn match_or_numeric(expr: &str) -> Option<(String, String)> {
    let (obj, rest) = expr.split_once(" or ")?;
    let rest = rest.trim();
    if !rest.is_empty()
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.')
        && rest.chars().any(|c| c.is_ascii_digit())
    {
        Some((obj.trim().to_string(), rest.to_string()))
    } else {
        None
    }
}

/// Extract the inside of a single- or double-quoted literal.
fn quoted_inner(s: &str) -> Option<String> {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'\'' || bytes[0] == b'"')
        && bytes[bytes.len() - 1] == bytes[0]
    {
        Some(s[1..s.len() - 1].to_string())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tag rewriters (string-level; mirror the regex rewriters)
// ---------------------------------------------------------------------------

/// Find `{% <keyword> COND %}` tags and replace via `f(cond)`.
fn replace_tag(s: &str, keyword: &str, f: impl Fn(&str) -> String) -> String {
    let open = "{%";
    let close = "%}";
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + open.len()..];
        let Some(end_rel) = after_open.find(close) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = after_open[..end_rel].trim();
        let kw_prefix = format!("{keyword} ");
        if let Some(cond) = inner.strip_prefix(&kw_prefix) {
            out.push_str(&f(cond.trim()));
        } else {
            out.push_str(&rest[start..start + open.len() + end_rel + close.len()]);
        }
        rest = &after_open[end_rel + close.len()..];
    }
    out.push_str(rest);
    out
}

/// Apply `f` to the condition inside every `{% if .. %}` / `{% else if .. %}`.
fn rewrite_if_conditions(s: &str, f: impl Fn(&str) -> String) -> String {
    let open = "{%";
    let close = "%}";
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after_open = &rest[start + open.len()..];
        let Some(end_rel) = after_open.find(close) else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = after_open[..end_rel].trim();
        let rewritten = if let Some(cond) = inner.strip_prefix("if ") {
            format!("{{% if {} %}}", f(cond.trim()))
        } else if let Some(cond) = inner.strip_prefix("else if ") {
            format!("{{% else if {} %}}", f(cond.trim()))
        } else {
            rest[start..start + open.len() + end_rel + close.len()].to_string()
        };
        out.push_str(&rewritten);
        rest = &after_open[end_rel + close.len()..];
    }
    out.push_str(rest);
    out
}

/// `'foo'` → `"foo"` (single-quoted Python strings to Rust strings).
fn single_to_double_quotes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_single = false;
    for c in s.chars() {
        match c {
            '\'' => {
                in_single = !in_single;
                out.push('"');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Word-boundary replace of `word` with `repl`.
fn word_replace(s: &str, word: &str, repl: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let wb = word.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(wb)
            && boundary(bytes, i.wrapping_sub(1), i == 0)
            && boundary(bytes, i + wb.len(), i + wb.len() >= bytes.len())
        {
            out.push_str(repl);
            i += wb.len();
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn boundary(bytes: &[u8], idx: usize, at_edge: bool) -> bool {
    if at_edge {
        return true;
    }
    match bytes.get(idx) {
        Some(&b) => !(b.is_ascii_alphanumeric() || b == b'_'),
        None => true,
    }
}

/// `not X` → `!X` (prefix form).
fn replace_not_prefix(s: &str) -> String {
    let needle = "not ";
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(needle.as_bytes())
            && (i == 0 || boundary(bytes, i - 1, false))
            && bytes
                .get(i + needle.len())
                .is_some_and(|&b| b.is_ascii_alphanumeric() || b == b'_' || b == b'(')
        {
            out.push('!');
            i += needle.len();
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn direct_field_non_option() {
        let cell = Cell {
            expr: Some("c.name".to_string()),
            ..Default::default()
        };
        assert_eq!(translate_cell_expr(&cell, "c", None), "{{ c.name }}");
    }

    #[test]
    fn direct_field_option_wraps() {
        let mut fields = BTreeMap::new();
        fields.insert("note".to_string(), "Option<String>".to_string());
        let cell = Cell {
            expr: Some("c.note".to_string()),
            ..Default::default()
        };
        assert_eq!(
            translate_cell_expr(&cell, "c", Some(&fields)),
            "{% if let Some(v) = c.note %}{{ v }}{% endif %}"
        );
    }

    #[test]
    fn strftime_guard_becomes_if_let_date() {
        let cell = Cell {
            expr: Some("c.datum.strftime('%d.%m.%Y') if c.datum else ''".to_string()),
            ..Default::default()
        };
        assert_eq!(
            translate_cell_expr(&cell, "c", None),
            "{% if let Some(d) = c.datum %}{{ d.format(\"%d.%m.%Y\") }}{% else %}{% endif %}"
        );
    }

    #[test]
    fn or_numeric_unwrap() {
        let mut fields = BTreeMap::new();
        fields.insert("tage".to_string(), "Option<i32>".to_string());
        let cell = Cell {
            expr: Some("c.tage or 0".to_string()),
            ..Default::default()
        };
        assert_eq!(
            translate_cell_expr(&cell, "c", Some(&fields)),
            "{{ c.tage.unwrap_or(0) }}"
        );
    }

    #[test]
    fn elif_and_condition_syntax() {
        let af = "{% if x == 'a' %}A{% elif x == 'b' %}B{% endif %}";
        let cell = Cell {
            askama_form: Some(af.to_string()),
            ..Default::default()
        };
        let out = translate_cell_expr(&cell, "x", None);
        assert!(out.contains("{% else if x == \"b\" %}"), "got {out}");
        assert!(out.contains("{% if x == \"a\" %}"), "got {out}");
    }

    #[test]
    fn condition_and_or_not() {
        let s = "{% if a and not b or c %}x{% endif %}";
        let got = rewrite_condition_syntax(s);
        assert_eq!(got, "{% if a && !b || c %}x{% endif %}");
    }
}
