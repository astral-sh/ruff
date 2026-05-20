//! Single-pass collector for all preflight signals.
//!
//! One pass over the tree gathers everything needed for the proposed config
//! and the structured report. No advisory English strings in any output field.

use std::collections::BTreeMap;
use std::path::Path;

use ruff_python_ast::{Expr, Stmt};
use ruff_python_parser::parse_module;

/// All signals gathered from a single-pass tree scan.
#[derive(Debug, Default)]
pub struct PreflightScanner {
    pub py_files_scanned: usize,
    pub py_files_parseable: usize,
    pub py_files_failed_parse: usize,
    pub total_function_defs: usize,
    pub total_class_defs: usize,
    /// Import histogram: module name → count of files importing it.
    pub imports_seen: BTreeMap<String, usize>,
    /// Decorator attribute name → count.
    pub decorator_by_attribute: BTreeMap<String, usize>,
    /// Full decorator pattern (object.method) → count.
    pub decorator_by_full_pattern: BTreeMap<String, usize>,
    /// Functions with decorators that didn't match any proposed rule.
    pub candidate_misses: Vec<CandidateMiss>,
    /// Filename stem suffix histogram.
    pub stem_suffix_histogram: BTreeMap<String, usize>,
    /// Files with at least one matched decorator.
    pub files_with_matched_routes: usize,
    /// URL template segments histogram.
    pub url_template_segments: BTreeMap<String, usize>,
    /// Body string scan hits.
    pub body_string_hits: BTreeMap<String, usize>,
    /// `add_url_rule` call findings.
    pub add_url_rule_findings: Vec<AddUrlRuleFinding>,
    /// `register_blueprint` graph entries.
    pub register_blueprint_graph: Vec<RegisterBlueprintEdge>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CandidateMiss {
    pub file: String,
    pub function: String,
    pub decorators_raw: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AddUrlRuleFinding {
    pub file: String,
    pub line: u32,
    pub expr: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RegisterBlueprintEdge {
    pub parent: String,
    pub child: String,
    pub url_prefix: Option<String>,
}

/// Known body-scan patterns (§3.2 body_string_scan_hits).
const BODY_SCAN_PATTERNS: &[&str] = &[
    "g.tenant",
    "current_app.tenant",
    "request.tenant",
    "Tenant.query",
];

/// Known frameworks for the fingerprint.
const FRAMEWORK_NAMES: &[&str] = &["flask", "fastapi", "django.urls", "starlette", "pyramid"];

/// Stem suffixes recognized in the filename convention histogram.
const STEM_SUFFIXES: &[&str] = &["_ops", "_bp", "_routes"];

impl PreflightScanner {
    /// Scan an entire tree rooted at `root`.
    pub fn scan(root: &Path) -> anyhow::Result<Self> {
        let mut scanner = Self::default();
        // Initialize import map with known frameworks.
        for &fw in FRAMEWORK_NAMES {
            scanner.imports_seen.insert(fw.to_string(), 0);
        }

        for entry in walkdir::WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let path = entry.path();
            if !path.is_file() || path.extension().is_none_or(|e| e != "py") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            if rel.contains("__pycache__/") || rel.starts_with(".venv/") || rel.starts_with("venv/") {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(path) else {
                continue;
            };
            scanner.py_files_scanned += 1;
            scanner.scan_file(&rel, &source);
        }
        Ok(scanner)
    }

    /// Scan one file's source text.
    pub fn scan_file(&mut self, rel: &str, source: &str) {
        let Ok(parsed) = parse_module(source) else {
            self.py_files_failed_parse += 1;
            return;
        };
        self.py_files_parseable += 1;

        let filename = Path::new(rel).file_name().and_then(|s| s.to_str()).unwrap_or("");
        self.record_stem_suffix(filename);

        let mut file_has_matched = false;

        for stmt in &parsed.syntax().body {
            match stmt {
                Stmt::Import(i) => {
                    for alias in &i.names {
                        let name = alias.name.to_string();
                        self.count_import(&name);
                    }
                }
                Stmt::ImportFrom(i) => {
                    let module = i.module.as_ref().map(|m| m.to_string()).unwrap_or_default();
                    self.count_import(&module);
                }
                Stmt::FunctionDef(func) => {
                    self.total_function_defs += 1;
                    let mut matched = false;
                    for dec in &func.decorator_list {
                        if let Some((obj, attr)) = decorator_obj_attr(dec) {
                            let attr_str = attr.clone();
                            *self.decorator_by_attribute.entry(attr_str.clone()).or_default() += 1;
                            let pattern = format!("{obj}.{attr}");
                            *self.decorator_by_full_pattern.entry(pattern).or_default() += 1;
                            if attr_str == "route" {
                                matched = true;
                                file_has_matched = true;
                                // Collect URL template segments from first arg.
                                if let Expr::Call(call) = &dec.expression {
                                    if let Some(first) = call.arguments.args.first() {
                                        if let Expr::StringLiteral(s) = first {
                                            let url = s.value.to_str().to_string();
                                            for seg in extract_url_segments(&url) {
                                                *self.url_template_segments.entry(seg).or_default() += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        } else if let Some(name) = decorator_bare_name(dec) {
                            *self.decorator_by_attribute.entry(name.clone()).or_default() += 1;
                            *self.decorator_by_full_pattern.entry(name).or_default() += 1;
                        }
                    }

                    // Body scan for known patterns.
                    let body_start = func.body.first().map(|s| s.range().start().to_usize()).unwrap_or(0);
                    let body_end = func.range().end().to_usize();
                    if body_start < body_end && body_end <= source.len() {
                        let body_text = &source[body_start..body_end];
                        for &pattern in BODY_SCAN_PATTERNS {
                            if body_text.contains(pattern) {
                                *self.body_string_hits.entry(pattern.to_string()).or_default() += 1;
                            }
                        }
                    }

                    if !matched && !func.decorator_list.is_empty() {
                        let decs: Vec<String> = func
                            .decorator_list
                            .iter()
                            .map(|d| {
                                let start = d.range().start().to_usize();
                                let end = d.range().end().to_usize();
                                source[start..end].to_string()
                            })
                            .collect();
                        self.candidate_misses.push(CandidateMiss {
                            file: rel.to_string(),
                            function: func.name.id.to_string(),
                            decorators_raw: decs,
                        });
                    }

                    // Scan for `add_url_rule` calls inside the function body.
                    for body_stmt in &func.body {
                        self.scan_for_add_url_rule(rel, body_stmt, source);
                    }
                }
                Stmt::ClassDef(_) => {
                    self.total_class_defs += 1;
                }
                _ => {}
            }
            // Top-level `add_url_rule` and `register_blueprint` calls.
            if let Stmt::Expr(e) = stmt {
                if let Expr::Call(call) = &*e.value {
                    self.scan_call_for_blueprint_ops(rel, call, source);
                }
            }
        }

        if file_has_matched {
            self.files_with_matched_routes += 1;
        }
    }

    fn count_import(&mut self, module: &str) {
        for &fw in FRAMEWORK_NAMES {
            if module == fw || module.starts_with(&format!("{fw}.")) {
                *self.imports_seen.entry(fw.to_string()).or_default() += 1;
            }
        }
    }

    fn record_stem_suffix(&mut self, filename: &str) {
        let stem = filename.strip_suffix(".py").unwrap_or(filename);
        let mut found = false;
        for &suffix in STEM_SUFFIXES {
            if stem.ends_with(suffix) {
                *self.stem_suffix_histogram.entry(suffix.to_string()).or_default() += 1;
                found = true;
                break;
            }
        }
        if !found {
            *self.stem_suffix_histogram.entry("(none)".to_string()).or_default() += 1;
        }
    }

    fn scan_for_add_url_rule(&mut self, rel: &str, stmt: &Stmt, source: &str) {
        if let Stmt::Expr(e) = stmt
            && let Expr::Call(call) = &*e.value
        {
            self.scan_call_for_blueprint_ops(rel, call, source);
        }
    }

    fn scan_call_for_blueprint_ops(
        &mut self,
        rel: &str,
        call: &ruff_python_ast::ExprCall,
        source: &str,
    ) {
        use ruff_text_size::Ranged;

        if let Expr::Attribute(attr) = &*call.func {
            if attr.attr.id.as_str() == "add_url_rule" {
                let start = call.range().start().to_usize();
                let end = call.range().end().to_usize();
                let expr_text = if start <= end && end <= source.len() {
                    source[start..end].to_string()
                } else {
                    String::new()
                };
                let line = {
                    // Cheap line estimation from source offset.
                    source[..start].chars().filter(|&c| c == '\n').count() as u32 + 1
                };
                self.add_url_rule_findings.push(AddUrlRuleFinding {
                    file: rel.to_string(),
                    line,
                    expr: expr_text,
                });
            } else if attr.attr.id.as_str() == "register_blueprint" {
                // `app.register_blueprint(billing_bp, url_prefix='/billing')`
                let parent = if let Expr::Name(n) = &*attr.value {
                    n.id.to_string()
                } else {
                    "?".to_string()
                };
                let child = call
                    .arguments
                    .args
                    .first()
                    .map(|e| {
                        let start = e.range().start().to_usize();
                        let end = e.range().end().to_usize();
                        if start <= end && end <= source.len() {
                            source[start..end].to_string()
                        } else {
                            "?".to_string()
                        }
                    })
                    .unwrap_or_else(|| "?".to_string());
                let url_prefix = call.arguments.keywords.iter().find_map(|kw| {
                    if kw.arg.as_ref().map(|a| a.id.as_str() == "url_prefix").unwrap_or(false) {
                        if let Expr::StringLiteral(s) = &kw.value {
                            return Some(s.value.to_str().to_string());
                        }
                    }
                    None
                });
                self.register_blueprint_graph.push(RegisterBlueprintEdge {
                    parent,
                    child,
                    url_prefix,
                });
            }
        }
    }
}

fn decorator_obj_attr(dec: &ruff_python_ast::Decorator) -> Option<(String, String)> {
    if let Expr::Call(call) = &dec.expression
        && let Expr::Attribute(attr) = &*call.func
    {
        let obj = if let Expr::Name(n) = &*attr.value {
            n.id.to_string()
        } else {
            "?".to_string()
        };
        return Some((obj, attr.attr.id.to_string()));
    }
    if let Expr::Attribute(attr) = &dec.expression {
        let obj = if let Expr::Name(n) = &*attr.value {
            n.id.to_string()
        } else {
            "?".to_string()
        };
        return Some((obj, attr.attr.id.to_string()));
    }
    None
}

fn decorator_bare_name(dec: &ruff_python_ast::Decorator) -> Option<String> {
    if let Expr::Name(n) = &dec.expression {
        return Some(n.id.to_string());
    }
    if let Expr::Call(call) = &dec.expression
        && let Expr::Name(n) = &*call.func
    {
        return Some(n.id.to_string());
    }
    None
}

fn extract_url_segments(url: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut in_seg = false;
    let mut current = String::new();
    for ch in url.chars() {
        if ch == '<' {
            in_seg = true;
            current.clear();
        } else if ch == '>' && in_seg {
            in_seg = false;
            segs.push(format!("<{current}>"));
        } else if in_seg {
            current.push(ch);
        }
    }
    segs
}

// Bring in Ranged for scan_call_for_blueprint_ops.
use ruff_text_size::Ranged;
