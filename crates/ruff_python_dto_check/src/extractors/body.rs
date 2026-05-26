//! Semantic body extractor: walk a route function body AST and lift the
//! facts the [`crate::contract::RouteContract`] needs — call-sites
//! (`render_template`, `redirect`, `jsonify`, `send_file`), model/query
//! references, `request.form`/`request.args` reads, the response kind, and
//! guard predicates.
//!
//! Everything project-specific is supplied by an [`ExtractionProfile`]
//! (config-driven). The crate never hardcodes WoA/odoo/openproject call
//! names: the profile maps `call-name → fact`, so a different framework
//! supplies its own conventions and the same walker produces its facts.

use std::collections::BTreeSet;

use ruff_python_ast::{Expr, Stmt, StmtFunctionDef};
use serde::Serialize;

/// The response shape — the `output` arm of the contract.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputKind {
    /// `render_template("path.html", k=v, ...)`.
    Template {
        path: String,
        context_keys: Vec<String>,
    },
    /// `redirect(url_for(...))` / `redirect("/path")`.
    Redirect { target: String },
    /// `jsonify(...)` / `return {...}`.
    Json { shape: Vec<String> },
    /// `send_file(...)` / streamed `Response(...)` non-PDF blob.
    Blob { mime: String },
    /// PDF document response.
    Pdf { doc_kind: String },
    /// No response statement classified.
    #[default]
    Unknown,
}

impl OutputKind {
    pub fn tag(&self) -> &'static str {
        match self {
            OutputKind::Template { .. } => "template",
            OutputKind::Redirect { .. } => "redirect",
            OutputKind::Json { .. } => "json",
            OutputKind::Blob { .. } => "blob",
            OutputKind::Pdf { .. } => "pdf",
            OutputKind::Unknown => "unknown",
        }
    }
}

/// Config-driven mapping of call/helper names to extraction facts. Populated
/// from the config's `extraction_profile` block; falls back to the Flask
/// defaults when absent.
#[derive(Debug, Clone)]
pub struct ExtractionProfile {
    /// Call name that renders a template (default `render_template`).
    pub render_call: String,
    /// Call name that redirects (default `redirect`).
    pub redirect_call: String,
    /// Call name that emits JSON (default `jsonify`).
    pub json_call: String,
    /// Call names that stream a binary file (default `send_file`).
    pub blob_calls: Vec<String>,
    /// Attribute reads that count as query reads (default `request.args`).
    pub query_attr: String,
    /// Attribute reads that count as form reads (default `request.form`).
    pub form_attr: String,
    /// Substrings that, in a helper/decorator name, indicate tenant scoping.
    pub tenant_scope_markers: Vec<String>,
    /// Names that indicate a write/commit (default `commit`, `add`, `delete`).
    pub mutation_markers: Vec<String>,
}

impl Default for ExtractionProfile {
    fn default() -> Self {
        Self {
            render_call: "render_template".to_string(),
            redirect_call: "redirect".to_string(),
            json_call: "jsonify".to_string(),
            blob_calls: vec!["send_file".to_string()],
            query_attr: "args".to_string(),
            form_attr: "form".to_string(),
            tenant_scope_markers: vec![
                "tenant_filter".to_string(),
                "get_scoped_or_404".to_string(),
                "ensure_tenant".to_string(),
                "tenant_id".to_string(),
                "require_same_tenant".to_string(),
            ],
            mutation_markers: vec![
                "commit".to_string(),
                "add".to_string(),
                "delete".to_string(),
                "flush".to_string(),
            ],
        }
    }
}

/// Facts lifted from a single route function body.
#[derive(Debug, Clone, Default)]
pub struct BodyFacts {
    pub output: OutputKind,
    pub models: Vec<String>,
    pub query_reads: Vec<String>,
    pub form_fields: Vec<String>,
    pub guards: Vec<String>,
    pub order_by: Option<String>,
    pub order_dir: Option<String>,
    pub tenant_scoped: bool,
    pub mutates: bool,
    pub soft_delete: bool,
}

/// Walk a route function's body and decorators to produce [`BodyFacts`].
pub fn extract_body(func: &StmtFunctionDef, profile: &ExtractionProfile) -> BodyFacts {
    let mut w = Walker {
        profile,
        models: BTreeSet::new(),
        query_reads: Vec::new(),
        form_fields: Vec::new(),
        guards: Vec::new(),
        output: OutputKind::Unknown,
        order_by: None,
        order_dir: None,
        tenant_scoped: false,
        mutates: false,
        soft_delete: false,
    };

    // Decorators feed guard predicates (auth, modul, tenant gates).
    for dec in &func.decorator_list {
        if let Some(name) = guard_name(&dec.expression) {
            w.guards.push(name);
        }
    }

    for stmt in &func.body {
        w.walk_stmt(stmt);
    }

    // A tenant-scoping guard implies tenant_scoped even without a body filter.
    if !w.tenant_scoped
        && w.guards
            .iter()
            .any(|g| profile.tenant_scope_markers.iter().any(|m| g.contains(m)))
    {
        w.tenant_scoped = true;
    }

    BodyFacts {
        output: w.output,
        models: w.models.into_iter().collect(),
        query_reads: dedup(w.query_reads),
        form_fields: dedup(w.form_fields),
        guards: w.guards,
        order_by: w.order_by,
        order_dir: w.order_dir,
        tenant_scoped: w.tenant_scoped,
        mutates: w.mutates,
        soft_delete: w.soft_delete,
    }
}

fn dedup(v: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    v.into_iter().filter(|x| seen.insert(x.clone())).collect()
}

struct Walker<'a> {
    profile: &'a ExtractionProfile,
    models: BTreeSet<String>,
    query_reads: Vec<String>,
    form_fields: Vec<String>,
    guards: Vec<String>,
    output: OutputKind,
    order_by: Option<String>,
    order_dir: Option<String>,
    tenant_scoped: bool,
    mutates: bool,
    soft_delete: bool,
}

impl Walker<'_> {
    fn walk_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Return(r) => {
                if let Some(value) = &r.value {
                    self.classify_return(value);
                    self.walk_expr(value);
                }
            }
            Stmt::Assign(a) => {
                // Soft-delete pattern: `obj.aktiv = False` / `obj.active = False`.
                self.detect_soft_delete(&a.targets, &a.value);
                self.walk_expr(&a.value);
            }
            Stmt::Expr(e) => self.walk_expr(&e.value),
            Stmt::AnnAssign(a) => {
                if let Some(v) = &a.value {
                    self.walk_expr(v);
                }
            }
            Stmt::AugAssign(a) => self.walk_expr(&a.value),
            Stmt::If(i) => {
                self.walk_expr(&i.test);
                for s in &i.body {
                    self.walk_stmt(s);
                }
                for clause in &i.elif_else_clauses {
                    if let Some(t) = &clause.test {
                        self.walk_expr(t);
                    }
                    for s in &clause.body {
                        self.walk_stmt(s);
                    }
                }
            }
            Stmt::For(f) => {
                self.walk_expr(&f.iter);
                for s in &f.body {
                    self.walk_stmt(s);
                }
            }
            Stmt::While(wstmt) => {
                self.walk_expr(&wstmt.test);
                for s in &wstmt.body {
                    self.walk_stmt(s);
                }
            }
            Stmt::With(wstmt) => {
                for s in &wstmt.body {
                    self.walk_stmt(s);
                }
            }
            Stmt::Try(t) => {
                for s in &t.body {
                    self.walk_stmt(s);
                }
                for h in &t.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(eh) = h;
                    for s in &eh.body {
                        self.walk_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    /// Classify the response kind from a `return <value>` expression.
    fn classify_return(&mut self, value: &Expr) {
        // Only the first classified return wins (handlers typically have one
        // primary response shape; the body classifier prefers render/redirect).
        if !matches!(self.output, OutputKind::Unknown) {
            return;
        }
        if let Expr::Call(call) = value {
            if let Some(name) = simple_call_name(&call.func) {
                if name == self.profile.render_call {
                    self.output = template_output(call);
                    return;
                }
                if name == self.profile.redirect_call {
                    let target = call
                        .arguments
                        .args
                        .first()
                        .map(expr_repr)
                        .unwrap_or_default();
                    self.output = OutputKind::Redirect { target };
                    return;
                }
                if name == self.profile.json_call {
                    self.output = OutputKind::Json {
                        shape: jsonify_keys(call),
                    };
                    return;
                }
                if self.profile.blob_calls.iter().any(|b| b == &name) {
                    self.output = OutputKind::Blob {
                        mime: String::new(),
                    };
                    return;
                }
            }
        }
        // `return {...}` dict literal → JSON.
        if let Expr::Dict(d) = value {
            let shape: Vec<String> = d
                .items
                .iter()
                .filter_map(|item| item.key.as_ref().map(string_value))
                .collect();
            self.output = OutputKind::Json { shape };
        }
    }

    fn walk_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call(call) => {
                self.inspect_call(call);
                self.walk_expr(&call.func);
                for a in &call.arguments.args {
                    self.walk_expr(a);
                }
                for kw in &call.arguments.keywords {
                    self.walk_expr(&kw.value);
                }
            }
            Expr::Attribute(a) => {
                // Model reference heuristic: `Customer.query`, `Order.query`
                // — a Capitalized Name followed by `.query`.
                if a.attr.id.as_str() == "query"
                    && let Expr::Name(n) = &*a.value
                    && is_model_name(n.id.as_str())
                {
                    self.models.insert(n.id.to_string());
                }
                // `<X>.tenant_id` attribute access marks tenant scoping.
                if a.attr.id.as_str() == "tenant_id" {
                    self.tenant_scoped = true;
                }
                self.walk_expr(&a.value);
            }
            Expr::Subscript(s) => {
                self.inspect_subscript(s);
                self.walk_expr(&s.value);
                self.walk_expr(&s.slice);
            }
            Expr::BinOp(b) => {
                self.walk_expr(&b.left);
                self.walk_expr(&b.right);
            }
            Expr::BoolOp(b) => {
                for v in &b.values {
                    self.walk_expr(v);
                }
            }
            Expr::Compare(c) => {
                self.walk_expr(&c.left);
                for comp in &c.comparators {
                    self.walk_expr(comp);
                }
            }
            Expr::Name(n) => {
                if is_model_name(n.id.as_str()) {
                    self.models.insert(n.id.to_string());
                }
            }
            _ => {}
        }
    }

    fn inspect_call(&mut self, call: &ruff_python_ast::ExprCall) {
        // `request.args.get("q")` / `request.form.get("x")`.
        if let Expr::Attribute(method) = &*call.func
            && method.attr.id.as_str() == "get"
            && let Expr::Attribute(inner) = &*method.value
        {
            let attr = inner.attr.id.as_str();
            if attr == self.profile.query_attr
                && let Some(key) = call.arguments.args.first().map(string_value_or_repr)
            {
                self.query_reads.push(key);
            } else if attr == self.profile.form_attr
                && let Some(key) = call.arguments.args.first().map(string_value_or_repr)
            {
                self.form_fields.push(key);
            }
        }

        // ORM order_by: `.order_by(Model.col.desc())` / `.order_by(Model.col)`.
        if let Expr::Attribute(method) = &*call.func
            && method.attr.id.as_str() == "order_by"
            && let Some(first) = call.arguments.args.first()
        {
            self.detect_order_by(first);
        }

        // Mutation markers: `db.session.commit()`, `.delete()`, `.add(...)`.
        if let Expr::Attribute(method) = &*call.func {
            let m = method.attr.id.as_str();
            if self.profile.mutation_markers.iter().any(|x| x == m) {
                self.mutates = true;
            }
        }

        // Helper-call guards: a bare call to a tenant-scoping helper name.
        if let Some(name) = simple_call_name(&call.func)
            && self
                .profile
                .tenant_scope_markers
                .iter()
                .any(|m| name.contains(m))
        {
            self.tenant_scoped = true;
            self.guards.push(name);
        }

        // `filter_by(tenant_id=...)` keyword-arg tenant scoping.
        for kw in &call.arguments.keywords {
            if kw
                .arg
                .as_ref()
                .is_some_and(|a| a.id.as_str() == "tenant_id")
            {
                self.tenant_scoped = true;
            }
        }
    }

    fn inspect_subscript(&mut self, s: &ruff_python_ast::ExprSubscript) {
        // `request.form["x"]` / `request.args["q"]`.
        if let Expr::Attribute(inner) = &*s.value {
            let attr = inner.attr.id.as_str();
            let key = string_value_or_repr(&s.slice);
            if attr == self.profile.query_attr {
                self.query_reads.push(key);
            } else if attr == self.profile.form_attr {
                self.form_fields.push(key);
            }
        }
    }

    fn detect_order_by(&mut self, expr: &Expr) {
        // `Model.col.desc()` → order_by=col, dir=desc.
        if let Expr::Call(call) = expr
            && let Expr::Attribute(dir_attr) = &*call.func
        {
            let dir = dir_attr.attr.id.as_str();
            if dir == "desc" || dir == "asc" {
                if let Expr::Attribute(col_attr) = &*dir_attr.value {
                    self.order_by = Some(col_attr.attr.id.to_string());
                    self.order_dir = Some(dir.to_string());
                }
                return;
            }
        }
        // `Model.col` (no direction) → default asc.
        if let Expr::Attribute(col_attr) = expr {
            self.order_by = Some(col_attr.attr.id.to_string());
            self.order_dir = Some("asc".to_string());
        }
    }

    fn detect_soft_delete(&mut self, targets: &[Expr], value: &Expr) {
        let is_false = matches!(value, Expr::BooleanLiteral(b) if !b.value);
        if !is_false {
            return;
        }
        for t in targets {
            if let Expr::Attribute(a) = t {
                let attr = a.attr.id.as_str();
                if attr == "aktiv" || attr == "active" || attr == "is_active" {
                    self.soft_delete = true;
                    self.mutates = true;
                }
            }
        }
    }
}

fn template_output(call: &ruff_python_ast::ExprCall) -> OutputKind {
    let path = call
        .arguments
        .args
        .first()
        .and_then(|e| {
            if let Expr::StringLiteral(s) = e {
                Some(s.value.to_str().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();
    let mut context_keys: Vec<String> = call
        .arguments
        .keywords
        .iter()
        .filter_map(|kw| kw.arg.as_ref().map(|a| a.id.to_string()))
        .collect();
    context_keys.sort();
    OutputKind::Template { path, context_keys }
}

/// Heuristic: a Python model class name is `CamelCase` (starts uppercase,
/// contains a lowercase letter, not `SCREAMING_CASE`).
fn is_model_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    name.chars().any(|c| c.is_ascii_lowercase()) && !name.contains('_')
}

/// `redirect`, `render_template` etc — a bare `Name` call target.
fn simple_call_name(func: &Expr) -> Option<String> {
    if let Expr::Name(n) = func {
        return Some(n.id.to_string());
    }
    None
}

/// Decorator → guard predicate name (`login_required`, `bp.route`, `require_admin`).
fn guard_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(n) => Some(n.id.to_string()),
        Expr::Attribute(a) => Some(a.attr.id.to_string()),
        Expr::Call(c) => guard_name(&c.func),
        _ => None,
    }
}

fn jsonify_keys(call: &ruff_python_ast::ExprCall) -> Vec<String> {
    let mut keys: Vec<String> = call
        .arguments
        .keywords
        .iter()
        .filter_map(|kw| kw.arg.as_ref().map(|a| a.id.to_string()))
        .collect();
    // `jsonify({"k": v})` positional dict.
    if let Some(Expr::Dict(d)) = call.arguments.args.first() {
        for item in &d.items {
            if let Some(k) = &item.key {
                keys.push(string_value(k));
            }
        }
    }
    keys.sort();
    keys
}

fn string_value(expr: &Expr) -> String {
    if let Expr::StringLiteral(s) = expr {
        s.value.to_str().to_string()
    } else {
        expr_repr(expr)
    }
}

fn string_value_or_repr(expr: &Expr) -> String {
    string_value(expr)
}

/// Best-effort textual repr of an expression (Name → id, Attribute → chain).
fn expr_repr(expr: &Expr) -> String {
    match expr {
        Expr::Name(n) => n.id.to_string(),
        Expr::StringLiteral(s) => s.value.to_str().to_string(),
        Expr::Attribute(a) => format!("{}.{}", expr_repr(&a.value), a.attr.id),
        Expr::Call(c) => {
            let inner = c
                .arguments
                .args
                .iter()
                .map(expr_repr)
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({inner})", expr_repr(&c.func))
        }
        _ => String::new(),
    }
}
