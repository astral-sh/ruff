//! jinja table-column extraction, ported from
//! `woa-rs/tools/template_column_extract.py`.
//!
//! Given a jinja template's text, locate the `<table>…</table>` block that
//! contains a `{% for %}` loop, pull the `<th>` headers and the first data
//! `<tr>`'s `<td>` cells, classify each cell into a [`crate::codegen::jinja::Cell`],
//! and pair headers with cells by position. The askama emission step then runs
//! each cell through [`crate::codegen::jinja::translate_cell_expr`] so the
//! emitted view reproduces the source columns faithfully.
//!
//! This is the proven Python extraction logic re-expressed in Rust; it is NOT
//! reinvented. The regex-driven Python functions become hand-rolled,
//! panic-free string scans (matching the style of `jinja.rs`):
//! - [`find_table_block`]   ← `find_table_block`
//! - [`extract_loop`]       ← `extract_loop`
//! - [`extract_headers`]    ← `extract_headers`
//! - [`extract_body_cells`] ← `extract_body_cells` (+ `_find_outer_else`)
//! - [`classify_cell`]      ← `classify_cell`
//! - [`extract_empty_row`]  ← `extract_empty_row`

use crate::codegen::jinja::Cell;

/// The for-loop header of a table block: `{% for <row_var> in <collection> %}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loop {
    pub row_var: String,
    pub collection: String,
}

/// One paired column: a header plus its classified cell.
#[derive(Debug, Clone)]
pub struct Column {
    pub header: String,
    pub cell: Cell,
}

/// The full table shape extracted from a template.
#[derive(Debug, Clone)]
pub struct TableShape {
    pub loop_: Loop,
    pub columns: Vec<Column>,
    /// Empty-state row text from the for-loop's `{% else %}` branch, if any.
    pub empty_row: Option<String>,
}

/// Extract the table shape from a jinja template's full text, or `None` when
/// there is no `<table>{% for %}` block (card/detail pages keep the skeleton).
pub fn extract_table_shape(text: &str) -> Option<TableShape> {
    let table = find_table_block(text)?;
    let loop_ = extract_loop(table)?;
    let headers = extract_headers(table);
    let body = extract_body_cells(table, &loop_.row_var);
    let columns = body
        .into_iter()
        .enumerate()
        .map(|(i, mut cell)| {
            let header = headers
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("(col {})", i + 1));
            cell.header.clone_from(&header);
            Column { header, cell }
        })
        .collect();
    let empty_row = extract_empty_row(table);
    Some(TableShape {
        loop_,
        columns,
        empty_row,
    })
}

/// Locate the `<table>…</table>` block that contains a `{% for %}` loop.
fn find_table_block(text: &str) -> Option<&str> {
    let mut start = text.find("<table");
    while let Some(s) = start {
        let end_rel = text[s..].find("</table>")?;
        let end = s + end_rel + "</table>".len();
        let block = &text[s..end];
        if block.contains("{% for") {
            return Some(block);
        }
        start = text[end..].find("<table").map(|r| end + r);
    }
    None
}

/// `{% for <row_var> in <collection> %}` → `Loop`.
fn extract_loop(text: &str) -> Option<Loop> {
    let (row_var, collection) = scan_for_header(text)?;
    Some(Loop {
        row_var,
        collection,
    })
}

/// Scan the first `{% for X in Y %}` tag, returning `(X, Y)`.
fn scan_for_header(text: &str) -> Option<(String, String)> {
    let start = text.find("{% for")?;
    let after = &text[start + "{% for".len()..];
    let close = after.find("%}")?;
    let inner = after[..close].trim();
    // inner == "X in Y"  (Y is a dotted identifier).
    let (lhs, rhs) = inner.split_once(" in ")?;
    let row_var = lhs.trim();
    let collection = rhs.trim();
    if row_var.is_empty()
        || !row_var.chars().all(|c| c.is_alphanumeric() || c == '_')
        || collection.is_empty()
        || !collection
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    Some((row_var.to_string(), collection.to_string()))
}

/// Pull `<th>…</th>` text from the `<thead>` (or first `<tr>`).
fn extract_headers(table: &str) -> Vec<String> {
    let scope = section_between(table, "<thead", "</thead>")
        .or_else(|| section_between(table, "<tr", "</tr>"))
        .unwrap_or(table);
    collect_tag_inner(scope, "<th", "</th>")
        .into_iter()
        .map(|h| strip_html(&h))
        .collect()
}

/// Pull `<td>…</td>` cells from the FIRST data `<tr>` inside the for-loop.
fn extract_body_cells(table: &str, row_var: &str) -> Vec<Cell> {
    let Some(body) = for_loop_body(table) else {
        return Vec::new();
    };
    // The outer-level `{% else %}` is the for-loop's empty-state; everything
    // before it is the iteration row.
    let body = match find_outer_else(body) {
        Some(idx) => &body[..idx],
        None => body,
    };
    let Some(first_tr) = section_inner(body, "<tr", "</tr>") else {
        return Vec::new();
    };
    collect_tag_inner(first_tr, "<td", "</td>")
        .into_iter()
        .map(|cell_html| classify_cell(cell_html.trim(), row_var))
        .collect()
}

/// The text between the first `{% for ... %}` and its matching `{% endfor %}`.
/// Uses a greedy match to the LAST `{% endfor %}` (mirroring the Python `.*`)
/// so nested `{% if %}` blocks are walked past.
fn for_loop_body(text: &str) -> Option<&str> {
    let for_start = text.find("{% for")?;
    let after_for = &text[for_start..];
    let tag_close = after_for.find("%}")?;
    let body_start = for_start + tag_close + "%}".len();
    let endfor_rel = text[body_start..].rfind("{% endfor")?;
    Some(&text[body_start..body_start + endfor_rel])
}

/// Index of the OUTER-level `{% else %}` (sibling of `{% endfor %}`), skipping
/// any `{% else %}` nested inside `{% if %}…{% endif %}`. `None` if absent.
fn find_outer_else(text: &str) -> Option<usize> {
    let mut pos = 0;
    let mut depth: i32 = 0;
    while let Some(rel) = text[pos..].find("{%") {
        let open = pos + rel;
        let close_rel = text[open..].find("%}")?;
        let close = open + close_rel + "%}".len();
        let inner = text[open + "{%".len()..open + close_rel].trim();
        let keyword = inner.split_whitespace().next().unwrap_or("");
        match keyword {
            "if" => depth += 1,
            "endif" => depth = (depth - 1).max(0),
            "else" if depth == 0 => return Some(open),
            _ => {}
        }
        pos = close;
    }
    None
}

/// Parse a `<td>` body into a [`Cell`] (`expr` / `wrapper` / `askama_form` /
/// `static_text`). Mirrors `classify_cell`.
fn classify_cell(html: &str, row_var: &str) -> Cell {
    let _ = row_var; // row_var is applied later by translate_cell_expr.
    let mut cell = Cell::default();

    // Wrapper tag (e.g. <code>).
    let inner = if let Some(code_inner) = section_inner(html, "<code", "</code>") {
        cell.wrapper = Some("code".to_string());
        code_inner.trim().to_string()
    } else {
        html.to_string()
    };

    // if/else conditional → emit askama equivalent.
    if let Some((cond, true_branch, false_branch)) = match_if_else(&inner) {
        let true_branch = strip_html(&true_branch);
        let false_branch = strip_html(&false_branch);
        cell.expr = Some(format!("'{true_branch}' if {cond} else '{false_branch}'"));
        cell.askama_form = Some(format!(
            "{{% if {cond} %}}{true_branch}{{% else %}}{false_branch}{{% endif %}}"
        ));
        return cell;
    }

    // Plain `{{ expr }}`.
    if let Some(expr) = match_jinja_expr(&inner) {
        let expr = expr.trim().to_string();
        // Inline ternary `'A' if cond else 'B'` (no `{% %}` blocks).
        if let Some((true_lit, cond, false_lit)) = match_inline_ternary(&expr) {
            cell.askama_form = Some(format!(
                "{{% if {cond} %}}{true_lit}{{% else %}}{false_lit}{{% endif %}}"
            ));
        }
        cell.expr = Some(expr);
        return cell;
    }

    // Static text only.
    cell.expr = None;
    cell.static_text = Some(strip_html(&inner));
    cell
}

/// Extract the empty-state row text from the for-loop's outer `{% else %}`.
fn extract_empty_row(table: &str) -> Option<String> {
    let body = for_loop_body(table)?;
    let outer = find_outer_else(body)?;
    let after_else = &body[outer..];
    // Skip past the `{% else %}` tag itself.
    let close = after_else.find("%}")?;
    let tail = &after_else[close + "%}".len()..];
    let td = section_inner(tail, "<td", "</td>")?;
    let text = td.trim();
    if text.is_empty() {
        None
    } else {
        Some(strip_html(text))
    }
}

// ---------------------------------------------------------------------------
// String scanners (panic-free; mirror the Python regexes)
// ---------------------------------------------------------------------------

/// `{% if COND %}TRUE{% else %}FALSE{% endif %}` within a single cell.
fn match_if_else(s: &str) -> Option<(String, String, String)> {
    let if_open = s.find("{% if ")?;
    let after_if = &s[if_open + "{% if ".len()..];
    let if_close = after_if.find("%}")?;
    let cond = after_if[..if_close].trim().to_string();
    let rest = &after_if[if_close + "%}".len()..];
    let else_idx = rest.find("{% else %}")?;
    let true_branch = rest[..else_idx].to_string();
    let after_else = &rest[else_idx + "{% else %}".len()..];
    let endif_idx = after_else.find("{% endif %}")?;
    let false_branch = after_else[..endif_idx].to_string();
    Some((cond, true_branch, false_branch))
}

/// Inside `{{ … }}`, return the inner expression.
fn match_jinja_expr(s: &str) -> Option<String> {
    let open = s.find("{{")?;
    let after = &s[open + "{{".len()..];
    let close = after.find("}}")?;
    Some(after[..close].trim().to_string())
}

/// `'A' if cond else 'B'` (quotes either `'` or `"`).
fn match_inline_ternary(expr: &str) -> Option<(String, String, String)> {
    let (true_lit, rest) = quoted_prefix(expr)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("if ")?;
    let (cond, else_part) = rest.split_once(" else ")?;
    let (false_lit, tail) = quoted_prefix(else_part.trim())?;
    if !tail.trim().is_empty() {
        return None;
    }
    Some((true_lit, cond.trim().to_string(), false_lit))
}

/// If `s` starts with a quoted literal, return `(inner, rest_after_quote)`.
fn quoted_prefix(s: &str) -> Option<(String, &str)> {
    let bytes = s.as_bytes();
    let quote = *bytes.first()?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let rest = &s[1..];
    let end = rest.find(quote as char)?;
    Some((rest[..end].to_string(), &rest[end + 1..]))
}

/// Find the inner text of the first `<tag …>…</close>` occurrence (whole
/// element, attributes allowed). Returns the inner content.
fn section_inner<'a>(text: &'a str, open_prefix: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open_prefix)?;
    let after_open = &text[start..];
    let gt = after_open.find('>')?;
    let inner_start = start + gt + 1;
    let close_rel = text[inner_start..].find(close)?;
    Some(&text[inner_start..inner_start + close_rel])
}

/// Like [`section_inner`] but returns the inner text including the open/close
/// markers' boundaries unchanged (used to scope `<thead>`/`<tr>` regions).
fn section_between<'a>(text: &'a str, open_prefix: &str, close: &str) -> Option<&'a str> {
    section_inner(text, open_prefix, close)
}

/// Collect the inner text of every `<tag …>…</close>` element.
fn collect_tag_inner(text: &str, open_prefix: &str, close: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(open_prefix) {
        let after_open = &rest[start..];
        let Some(gt) = after_open.find('>') else {
            break;
        };
        let inner_start = start + gt + 1;
        let Some(close_rel) = rest[inner_start..].find(close) else {
            break;
        };
        out.push(rest[inner_start..inner_start + close_rel].to_string());
        rest = &rest[inner_start + close_rel + close.len()..];
    }
    out
}

/// Strip simple HTML tags + collapse whitespace (mirrors `_strip_html`).
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // Collapse runs of whitespace to a single space, trim ends.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_ws = false;
    for c in out.trim().chars() {
        if c.is_whitespace() {
            if !prev_ws {
                collapsed.push(' ');
            }
            prev_ws = true;
        } else {
            collapsed.push(c);
            prev_ws = false;
        }
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES_LIST: &str = r#"
<div class="card"><div class="table-responsive"><table class="table table-sm mb-0">
<thead><tr><th style="width:30px"></th><th>Gerät</th><th>Kunde</th><th>S/N</th><th>Standort</th><th>Guarantee</th></tr></thead>
<tbody>{% for d in items %}
<tr class="clickable-row">
  <td><i class="bi"></i></td>
  <td><strong>{{ d.display_name }}</strong></td>
  <td>{{ d.customer.display_name if d.customer else '–' }}</td>
  <td class="small">{{ d.seriennummer or '–' }}</td>
  <td class="small">{{ d.standort or '–' }}</td>
  <td>{{ d.guarantee_bis.strftime('%d.%m.%Y') if d.guarantee_bis else '' }}</td>
</tr>{% else %}<tr><td colspan="6" class="text-center text-muted py-4">Keine Geräte.</td></tr>{% endfor %}
</tbody></table></div></div>
"#;

    const CASH_JOURNALS: &str = r#"
<table class="table table-striped table-sm">
  <thead><tr><th>Bezeichnung</th><th>Konto</th><th>Aktiv</th></tr></thead>
  <tbody>
    {% for j in journals %}
    <tr>
      <td>{{ j.bezeichnung }}</td>
      <td><code>{{ j.konto_id }}</code></td>
      <td>{{ 'Ja' if j.aktiv else 'Nein' }}</td>
    </tr>
    {% else %}
    <tr><td colspan="3" class="text-muted">Noch keine Kassen angelegt.</td></tr>
    {% endfor %}
  </tbody>
</table>
"#;

    #[test]
    fn extracts_loop_and_collection() {
        let shape = extract_table_shape(DEVICES_LIST).expect("table found");
        assert_eq!(shape.loop_.row_var, "d");
        assert_eq!(shape.loop_.collection, "items");
    }

    #[test]
    fn pairs_headers_with_cells() {
        let shape = extract_table_shape(DEVICES_LIST).expect("table found");
        // 6 <th> and 6 <td>.
        assert_eq!(shape.columns.len(), 6);
        assert_eq!(shape.columns[1].header, "Gerät");
        assert_eq!(shape.columns[2].header, "Kunde");
        assert_eq!(shape.columns[3].header, "S/N");
    }

    #[test]
    fn extracts_empty_row() {
        let shape = extract_table_shape(DEVICES_LIST).expect("table found");
        assert_eq!(shape.empty_row.as_deref(), Some("Keine Geräte."));
    }

    #[test]
    fn classifies_wrapper_and_ternary() {
        let shape = extract_table_shape(CASH_JOURNALS).expect("table found");
        assert_eq!(shape.columns.len(), 3);
        // <code> wrapper on Konto.
        assert_eq!(shape.columns[1].cell.wrapper.as_deref(), Some("code"));
        assert_eq!(shape.columns[1].cell.expr.as_deref(), Some("j.konto_id"));
        // Inline ternary on Aktiv → askama_form.
        let aktiv = &shape.columns[2].cell;
        assert_eq!(
            aktiv.askama_form.as_deref(),
            Some("{% if j.aktiv %}Ja{% else %}Nein{% endif %}")
        );
    }

    #[test]
    fn no_table_block_returns_none() {
        let card =
            "{% extends \"_base.html\" %}{% block content %}<div>no table</div>{% endblock %}";
        assert!(extract_table_shape(card).is_none());
    }
}
