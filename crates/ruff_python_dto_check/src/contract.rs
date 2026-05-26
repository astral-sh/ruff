//! `RouteContract` — the shared interface between AST extraction, target
//! codegen, and the calibration lints (the "spine" from CODEGEN-DESIGN.md).
//!
//! The contract is built from [`crate::extractors::body::BodyFacts`] plus the
//! route identity, then classified into a [`HandlerKind`] by a priority
//! classifier ported from
//! `woa-rs/.claude/v0.2/tools/classify_route_handlers.py`.
//!
//! Nothing here is project-specific: the classifier reads neutral facts
//! (HTTP methods, response kind, helper names, path params) so odoo and
//! openproject yield their own kind distribution from the same algebra.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::extractors::body::{BodyFacts, OutputKind};

/// One path parameter lifted from the route URL, e.g. `<int:did>` → `did`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PathParam {
    pub name: String,
    /// Flask converter (`int`, `string`, `path`, `float`, `uuid`) or `None`
    /// for the bare `<name>` form.
    pub converter: Option<String>,
}

/// The inputs side of the contract: everything the handler reads.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Inputs {
    pub path_params: Vec<PathParam>,
    /// `request.args.get("q")` reads, in source order, de-duplicated.
    pub query_reads: Vec<String>,
    /// `request.form.get("x")` / `request.form["x"]` reads.
    pub form_fields: Vec<String>,
}

/// The data side: ORM/model references and the query shape observed.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct Data {
    /// Model/entity class names referenced (e.g. `Customer`, `ErpCashJournal`).
    pub models: Vec<String>,
    /// Order-by column when a single obvious one was seen (`order_by(X.col...)`).
    pub order_by: Option<String>,
    /// `asc` / `desc` for `order_by`.
    pub order_dir: Option<String>,
    /// Whether a tenant-scoping helper / `tenant_id` filter was observed.
    pub tenant_scoped: bool,
    /// Whether the body issues a write/commit (`db.session.commit`, `.delete`,
    /// `.add`, an `aktiv = False` soft-delete assignment, …).
    pub mutates: bool,
    /// Whether the mutation is a soft-delete (`aktiv = False`) vs hard-delete.
    pub soft_delete: bool,
}

/// Provenance: where the contract came from, for diagnostics.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Provenance {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// The full route contract. `output` carries the response shape; `guards`
/// the auth/tenant predicates seen in the body and decorators.
#[derive(Debug, Clone, Serialize)]
pub struct RouteContract {
    /// `<blueprint>.<function>` identity.
    pub id: String,
    pub function: String,
    pub family: String,
    /// Raw HTTP method list, upper-cased.
    pub methods: Vec<String>,
    /// Flask URL pattern, verbatim.
    pub path: String,
    pub inputs: Inputs,
    pub data: Data,
    pub output: OutputKind,
    /// Auth/tenant/permission predicate names (decorators + helper calls).
    pub guards: Vec<String>,
    /// Classified handler kind (priority classifier).
    pub handler_kind: HandlerKind,
    /// Short human reason for the classification (for debugging / report).
    pub classification_reason: String,
    pub provenance: Provenance,
}

/// The emergent handler-kind taxonomy. Ported 1:1 from the Python classifier.
/// `Other` is the catch-all. These are *derived* — a different codebase yields
/// its own distribution from the same `output × inputs` algebra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HandlerKind {
    SignedLinkAction,
    SaAdminView,
    AjaxJson,
    PdfRender,
    DownloadBlob,
    SoftDelete,
    ToggleBoolField,
    CsrfFormPostEngineCall,
    GetRedirectShortcut,
    FormGetPost,
    ListForTenant,
    DetailForTenant,
    TemplateGet,
    Other,
}

impl HandlerKind {
    /// Stable `snake_case` identifier, matching the Python kind strings and the
    /// target-spec keys.
    pub fn as_str(self) -> &'static str {
        match self {
            HandlerKind::SignedLinkAction => "signed_link_action",
            HandlerKind::SaAdminView => "sa_admin_view",
            HandlerKind::AjaxJson => "ajax_json",
            HandlerKind::PdfRender => "pdf_render",
            HandlerKind::DownloadBlob => "download_blob",
            HandlerKind::SoftDelete => "soft_delete",
            HandlerKind::ToggleBoolField => "toggle_bool_field",
            HandlerKind::CsrfFormPostEngineCall => "csrf_form_post_engine_call",
            HandlerKind::GetRedirectShortcut => "get_redirect_shortcut",
            HandlerKind::FormGetPost => "form_get_post",
            HandlerKind::ListForTenant => "list_for_tenant",
            HandlerKind::DetailForTenant => "detail_for_tenant",
            HandlerKind::TemplateGet => "template_get",
            HandlerKind::Other => "other",
        }
    }
}

/// Build a [`RouteContract`] from route identity + extracted [`BodyFacts`],
/// then classify it.
pub fn build_contract(
    blueprint: &str,
    function: &str,
    family: &str,
    methods: &[String],
    path: &str,
    facts: BodyFacts,
    provenance: Provenance,
) -> RouteContract {
    let path_params = parse_path_params(path);
    let inputs = Inputs {
        path_params,
        query_reads: facts.query_reads.clone(),
        form_fields: facts.form_fields.clone(),
    };
    let data = Data {
        models: facts.models.clone(),
        order_by: facts.order_by.clone(),
        order_dir: facts.order_dir.clone(),
        tenant_scoped: facts.tenant_scoped,
        mutates: facts.mutates,
        soft_delete: facts.soft_delete,
    };
    let guards = facts.guards.clone();
    // Endpoint identity. The Flask `endpoint` is `<registered-blueprint>.<fn>`;
    // the registered blueprint name is conventionally the file family. When the
    // detector only sees the local blueprint *variable* (often `bp`), prefer
    // the family so the id matches the Python classifier's `endpoint`.
    let id_prefix = if blueprint == "bp" || blueprint == "app" || blueprint.is_empty() {
        family
    } else {
        blueprint
    };
    let id = format!("{id_prefix}.{function}");

    let methods_up = methods_upper(methods);
    let (handler_kind, classification_reason) =
        classify(&methods_up, &inputs, &data, &facts.output, &guards, function, &id);

    RouteContract {
        id,
        function: function.to_string(),
        family: family.to_string(),
        methods: methods_up,
        path: path.to_string(),
        inputs,
        data,
        output: facts.output,
        guards,
        handler_kind,
        classification_reason,
        provenance,
    }
}

fn methods_upper(methods: &[String]) -> Vec<String> {
    methods.iter().map(|m| m.to_uppercase()).collect()
}

/// Parse Flask `<int:did>` / `<did>` segments out of a URL pattern.
pub fn parse_path_params(path: &str) -> Vec<PathParam> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            break;
        };
        let inner = &after[..close];
        let (converter, name) = match inner.split_once(':') {
            Some((conv, name)) => (Some(conv.to_string()), name.to_string()),
            None => (None, inner.to_string()),
        };
        out.push(PathParam { name, converter });
        rest = &after[close + 1..];
    }
    out
}

/// Priority classifier ported from `classify_route_handlers.py::classify`.
///
/// First match wins. The order is load-bearing — it is the same priority
/// chain the Python classifier uses (signed-link first, then SA-admin, JSON,
/// PDF, blob, delete, toggle, form-post, redirect, form, list, detail,
/// template, catch-all).
fn classify(
    methods: &[String],
    inputs: &Inputs,
    data: &Data,
    output: &OutputKind,
    guards: &[String],
    function: &str,
    endpoint: &str,
) -> (HandlerKind, String) {
    let is_get = methods.iter().any(|m| m == "GET");
    let is_post = methods.iter().any(|m| m == "POST");
    let is_get_only = methods.len() == 1 && is_get;
    let is_post_only = methods.len() == 1 && is_post;
    let is_form = methods.iter().any(|m| m == "GET") && methods.iter().any(|m| m == "POST");

    let renders = matches!(output, OutputKind::Template { .. });
    let redirects = matches!(output, OutputKind::Redirect { .. });
    let jsonifies = matches!(output, OutputKind::Json { .. });
    let sends_file = matches!(output, OutputKind::Blob { .. } | OutputKind::Pdf { .. });
    let has_path_param = !inputs.path_params.is_empty();

    let fn_lc = function.to_ascii_lowercase();
    let guard_set = |needle: &str| guards.iter().any(|g| g.contains(needle));

    // 0. Signed-link action (token-validated public actions).
    if is_signed_link(&fn_lc) {
        return (
            HandlerKind::SignedLinkAction,
            "signed-link function name pattern".to_string(),
        );
    }

    // 0b. SuperAdmin cross-tenant view.
    let sa_prefix = fn_lc.starts_with("sa_") || endpoint.starts_with("sa_admin");
    let require_admin = guards.iter().any(|g| {
        g.contains("require_admin") || g.contains("admin_required") || g.contains("superadmin")
    });
    if sa_prefix && is_get_only && (renders || redirects) {
        return (
            HandlerKind::SaAdminView,
            "SA-prefix + GET + render/redirect".to_string(),
        );
    }
    if require_admin && is_get_only && renders && !data.tenant_scoped {
        return (
            HandlerKind::SaAdminView,
            "require_admin + GET + render without tenant scope".to_string(),
        );
    }

    // 1. Pure JSON endpoints.
    if jsonifies && !renders {
        return (HandlerKind::AjaxJson, "jsonify, no render".to_string());
    }

    // 2. PDF rendering.
    let has_pdf_hint = fn_lc.contains("pdf");
    if has_pdf_hint && (sends_file || fn_lc.ends_with("_pdf") || fn_lc.starts_with("pdf_")) {
        return (
            HandlerKind::PdfRender,
            "pdf hint + send_file or pdf-named".to_string(),
        );
    }
    if sends_file && has_pdf_hint {
        return (HandlerKind::PdfRender, "send_file + pdf".to_string());
    }

    // 3. File/blob download (non-PDF).
    if sends_file
        || fn_lc.contains("download")
        || fn_lc.contains("export")
        || fn_lc.ends_with("_xml")
        || fn_lc.ends_with("_ics")
        || fn_lc.ends_with("_qr")
    {
        return (
            HandlerKind::DownloadBlob,
            "send_file or binary-stream function name".to_string(),
        );
    }

    // 4. Soft / hard delete.
    if is_post_only && is_delete_shaped(&fn_lc) {
        return (
            HandlerKind::SoftDelete,
            "delete-shaped function + POST".to_string(),
        );
    }

    // 5. Toggle bool field.
    if is_post_only && is_toggle_shaped(&fn_lc) && !renders {
        return (
            HandlerKind::ToggleBoolField,
            "toggle function + POST".to_string(),
        );
    }

    // 6. CSRF form POST → engine call → redirect.
    if is_post_only && redirects && !renders {
        return (
            HandlerKind::CsrfFormPostEngineCall,
            "POST + redirect".to_string(),
        );
    }

    // 6b. GET-that-redirects shortcut.
    if is_get_only && redirects && !renders {
        return (
            HandlerKind::GetRedirectShortcut,
            "GET + redirect (no render)".to_string(),
        );
    }

    // 7. Form GET+POST.
    if is_form && renders {
        return (
            HandlerKind::FormGetPost,
            "GET+POST with render".to_string(),
        );
    }

    // 8. List for tenant.
    if is_get_only && renders && !has_path_param {
        if data.tenant_scoped || guard_set("tenant_filter") || guard_set("get_scoped_or_404") {
            return (
                HandlerKind::ListForTenant,
                "GET + render + tenant scope + no path param".to_string(),
            );
        }
        if !data.models.is_empty() && !data.mutates {
            return (
                HandlerKind::ListForTenant,
                "GET + render + models + no commit (admin list)".to_string(),
            );
        }
    }

    // 9. Detail for tenant.
    if is_get_only && renders && has_path_param {
        if data.tenant_scoped
            || guard_set("get_scoped_or_404")
            || guard_set("get_owned_or_404")
            || guard_set("get_portal_or_404")
            || guard_set("require_same_tenant")
        {
            return (
                HandlerKind::DetailForTenant,
                "GET + render + path param + scope helper".to_string(),
            );
        }
        if !data.models.is_empty() {
            return (
                HandlerKind::DetailForTenant,
                "GET + render + path param + models".to_string(),
            );
        }
    }

    // 9b. GET+POST with redirect-fallback.
    if is_form && (renders || redirects) {
        return (
            HandlerKind::FormGetPost,
            "GET+POST with render or redirect".to_string(),
        );
    }

    // 10. Static / settings-style template GET.
    if is_get_only && renders && data.models.is_empty() && !data.mutates {
        return (
            HandlerKind::TemplateGet,
            "GET + render + no models".to_string(),
        );
    }

    // Catch-all.
    let mut bits: Vec<String> = Vec::new();
    if !methods.is_empty() {
        bits.push(format!("methods={}", methods.join("+")));
    }
    bits.push(format!("output={}", output.tag()));
    if has_path_param {
        bits.push("path_param".to_string());
    }
    (HandlerKind::Other, bits.join("; "))
}

fn is_signed_link(fn_lc: &str) -> bool {
    const PATTERNS: &[&str] = &[
        "auto_login",
        "magic_link",
        "verify_email",
        "reset_password",
        "sign_submit",
    ];
    PATTERNS.iter().any(|p| fn_lc.contains(p))
        || fn_lc.ends_with("_accept")
        || fn_lc.ends_with("_cancel")
}

fn is_delete_shaped(fn_lc: &str) -> bool {
    fn_lc.starts_with("delete")
        || fn_lc.starts_with("loeschen")
        || fn_lc.starts_with("sa_delete")
        || fn_lc.starts_with("del_")
        || fn_lc.ends_with("_delete")
        || fn_lc.ends_with("_loeschen")
}

fn is_toggle_shaped(fn_lc: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "toggle",
        "set_aktiv",
        "sa_set_aktiv",
        "activate",
        "deactivate",
        "enable",
        "disable",
    ];
    PREFIXES.iter().any(|p| fn_lc.starts_with(p))
}

/// Serialize a contract to a stable, pretty JSON object.
pub fn contract_to_json(c: &RouteContract) -> serde_json::Value {
    serde_json::to_value(c).unwrap_or(serde_json::Value::Null)
}

/// A bundle of contracts keyed by endpoint id, for whole-tree emission.
pub type ContractMap = BTreeMap<String, RouteContract>;
