//! Target emitter: contract → target source (handler + view template).
//!
//! The emitter is **kind-generalized**: each [`HandlerKind`] resolves to a
//! [`KindRecipe`] that describes the shape (signature, query, response) in
//! data, so the other 10 kinds slot in by adding a recipe entry — not new
//! Rust per kind. `list_for_tenant` and `soft_delete` are implemented
//! end-to-end (matching the woa-rs port-drafts oracle, with the corrected
//! model paths).
//!
//! Generated code goes to a draft directory; it is never wired into a build
//! and never emits `unimplemented!()`/`todo!()` into a compiled production
//! path (the PR #102 failure guardrail).

pub mod columns;
pub mod dto;
pub mod jinja;
pub mod pipeline;
pub mod target;

use std::fmt::Write as _;

use crate::codegen::target::{ModelMapping, TargetSpec};
use crate::contract::{HandlerKind, RouteContract};
use crate::extractors::body::OutputKind;

/// The two fully-implemented kinds plus a generic stub for the rest. A recipe
/// is pure data + an enum tag; adding a kind means adding a variant + arm,
/// never a new module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KindRecipe {
    /// Tenant-scoped list page (GET, render, no path param).
    ListForTenant,
    /// Tenant-scoped detail page (GET, render, path param, scoped fetch).
    DetailForTenant,
    /// Static / settings-style template render (GET, render, no model query).
    TemplateGet,
    /// POST delete handler (soft or hard).
    SoftDelete,
    /// Scoped fetch → flip a bool field → redirect (POST).
    ToggleBoolField,
    /// GET that redirects (no render).
    GetRedirectShortcut,
    /// POST → form DTO → engine call → redirect.
    CsrfFormPostEngineCall,
    /// GET render + POST handle, with a form DTO + view.
    FormGetPost,
    /// JSON response (`Json<Dto>`).
    AjaxJson,
    /// Binary blob download (`Response` bytes + Content-Type/Disposition).
    DownloadBlob,
    /// PDF response via the project's PDF crate (call-site shape only).
    PdfRender,
    /// Admin/superadmin-gated render.
    SaAdminView,
    /// Token-verified, guard-aware signed-link action.
    SignedLinkAction,
    /// Documented stub: the kind is recognized but not enabled by this target.
    /// Honest about coverage rather than emitting wrong code.
    Stub,
}

impl KindRecipe {
    fn for_kind(kind: HandlerKind, spec: &TargetSpec) -> Self {
        if !spec.can_emit(kind) {
            return KindRecipe::Stub;
        }
        match kind {
            HandlerKind::ListForTenant => KindRecipe::ListForTenant,
            HandlerKind::DetailForTenant => KindRecipe::DetailForTenant,
            HandlerKind::TemplateGet => KindRecipe::TemplateGet,
            HandlerKind::SoftDelete => KindRecipe::SoftDelete,
            HandlerKind::ToggleBoolField => KindRecipe::ToggleBoolField,
            HandlerKind::GetRedirectShortcut => KindRecipe::GetRedirectShortcut,
            HandlerKind::CsrfFormPostEngineCall => KindRecipe::CsrfFormPostEngineCall,
            HandlerKind::FormGetPost => KindRecipe::FormGetPost,
            HandlerKind::AjaxJson => KindRecipe::AjaxJson,
            HandlerKind::DownloadBlob => KindRecipe::DownloadBlob,
            HandlerKind::PdfRender => KindRecipe::PdfRender,
            HandlerKind::SaAdminView => KindRecipe::SaAdminView,
            HandlerKind::SignedLinkAction => KindRecipe::SignedLinkAction,
            HandlerKind::Other => KindRecipe::Stub,
        }
    }
}

/// The emitted artifacts for one route.
#[derive(Debug, Clone)]
pub struct Emitted {
    /// Target handler source (Rust).
    pub handler_rs: String,
    /// View template (askama), if the kind produces one.
    pub view_html: Option<String>,
    /// Relative file name for the handler (e.g. `geraete__device_delete.rs`).
    pub handler_file: String,
    /// Relative file name for the view template, if any.
    pub view_file: Option<String>,
    /// Models the emitter referenced (for the `unmapped-model` lint).
    pub referenced_models: Vec<String>,
    /// Template context keys the handler provides (for the context lint).
    pub provided_context_keys: Vec<String>,
}

/// Emit target source for a single contract against a target spec.
pub fn emit(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let recipe = KindRecipe::for_kind(contract.handler_kind, spec);
    match recipe {
        KindRecipe::ListForTenant => emit_list_for_tenant(contract, spec),
        KindRecipe::DetailForTenant => emit_detail_for_tenant(contract, spec),
        KindRecipe::TemplateGet => emit_template_get(contract, spec),
        KindRecipe::SoftDelete => emit_soft_delete(contract, spec),
        KindRecipe::ToggleBoolField => emit_toggle_bool_field(contract, spec),
        KindRecipe::GetRedirectShortcut => emit_get_redirect_shortcut(contract, spec),
        KindRecipe::CsrfFormPostEngineCall => emit_csrf_form_post(contract, spec),
        KindRecipe::FormGetPost => emit_form_get_post(contract, spec),
        KindRecipe::AjaxJson => emit_ajax_json(contract, spec),
        KindRecipe::DownloadBlob => emit_download_blob(contract, spec),
        KindRecipe::PdfRender => emit_pdf_render(contract, spec),
        KindRecipe::SaAdminView => emit_sa_admin_view(contract, spec),
        KindRecipe::SignedLinkAction => emit_signed_link_action(contract, spec),
        KindRecipe::Stub => emit_stub(contract, spec),
    }
}

/// `snake_case` → `PascalCase`.
fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Flask `<int:did>` / `<did>` → axum `:did`.
fn axum_path(flask_path: &str) -> String {
    let mut out = String::with_capacity(flask_path.len());
    let mut rest = flask_path;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let inner = &after[..close];
        let name = inner.split_once(':').map_or(inner, |(_, n)| n);
        out.push(':');
        out.push_str(name);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Pick the primary model: the first model in `data.models` with a target
/// mapping. Returns `(python_class, mapping)`.
fn primary_model<'a>(
    contract: &RouteContract,
    spec: &'a TargetSpec,
) -> Option<(&'a str, &'a ModelMapping)> {
    for m in &contract.data.models {
        if let Some((k, v)) = spec.models.get_key_value(m) {
            return Some((k.as_str(), v));
        }
    }
    None
}

/// Collection variable name derived from the function name.
fn collection_name(function: &str, fallback: &str) -> String {
    let stem = function
        .trim_end_matches("_list")
        .trim_end_matches("_overview")
        .trim_end_matches("_index");
    if stem.len() < 3 {
        format!("{fallback}s")
    } else {
        stem.to_string()
    }
}

// ---------------------------------------------------------------------------
// list_for_tenant
// ---------------------------------------------------------------------------

fn emit_list_for_tenant(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let struct_name = format!("{}Template", snake_to_pascal(fn_name));
    let tmpl_path = format!("list_for_tenant/{fn_name}.html");
    let axum = axum_path(&contract.path);
    let template_doc = match &contract.output {
        OutputKind::Template { path, .. } => path.clone(),
        _ => String::new(),
    };

    let (referenced_models, model_block) = match primary_model(contract, spec) {
        Some((py_class, mapping)) => {
            let model_ty = mapping.model_type(&spec.models_root);
            let entity = mapping.entity_path(&spec.models_root);
            let column = mapping.column_path(&spec.models_root);
            let collection = collection_name(fn_name, &leaf(&mapping.module_path));
            let order = order_call(contract, &column);
            let block = format!(
                "// Rust model: {model_ty}\n\
                 \n\
                 use askama::Template;\n\
                 use axum::Router;\n\
                 use axum::extract::State;\n\
                 use axum::routing::get;\n\
                 use sea_orm::{{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder}};\n\
                 use tower_sessions::Session;\n\
                 \n\
                 use crate::error::WoaResult;\n\
                 use crate::flash;\n\
                 use crate::flash::FlashMessage;\n\
                 use crate::middleware::current_user::CurrentUser;\n\
                 \n\
                 #[derive(Template)]\n\
                 #[template(path = \"{tmpl_path}\")]\n\
                 pub struct {struct_name} {{\n\
                 \x20   pub flashes: Vec<FlashMessage>,\n\
                 \x20   pub current_user: Option<CurrentUser>,\n\
                 \x20   pub {collection}: Vec<{model_ty}>,\n\
                 \x20   pub title: &'static str,\n\
                 }}\n\
                 \n\
                 /// List handler for {fn_name}.\n\
                 /// Mirror Python: {endpoint}\n\
                 pub async fn {fn_name}(\n\
                 \x20   State(db): State<DatabaseConnection>,\n\
                 \x20   user: CurrentUser,\n\
                 \x20   session: Session,\n\
                 ) -> WoaResult<{struct_name}> {{\n\
                 \x20   let {collection} = {entity}::find()\n\
                 \x20       .filter({column}::{tenant}.eq(user.tenant_id))\n\
                 {order}\
                 \x20       .all(&db)\n\
                 \x20       .await?;\n\
                 \x20   Ok({struct_name} {{\n\
                 \x20       flashes: flash::take(&session).await,\n\
                 \x20       current_user: Some(user),\n\
                 \x20       {collection},\n\
                 \x20       title: \"{fn_name}\",\n\
                 \x20   }})\n\
                 }}\n\
                 \n\
                 pub fn router() -> Router<DatabaseConnection> {{\n\
                 \x20   Router::new().route(\"{axum}\", get({fn_name}))\n\
                 }}\n",
                endpoint = contract.id,
                tenant = spec.tenant_column,
                order = order,
            );
            (vec![py_class.to_string()], block)
        }
        None => (
            contract.data.models.clone(),
            unresolved_block(contract, spec),
        ),
    };

    let header = kind_header(contract, &template_doc);
    let handler_rs = format!("{header}{model_block}");

    let view_html = Some(emit_list_view(contract, spec));

    Emitted {
        handler_rs,
        view_html,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: Some(format!("{fn_name}.html")),
        referenced_models,
        provided_context_keys: vec![
            "flashes".to_string(),
            "current_user".to_string(),
            "title".to_string(),
        ],
    }
}

/// Build the `.order_by_*(...)` line. `column_path` is the full sea-orm Column
/// path for the primary model (e.g. `crate::models::customer::Column`).
fn order_call(contract: &RouteContract, column_path: &str) -> String {
    let col = contract
        .data
        .order_by
        .as_deref()
        .map(snake_to_pascal)
        .unwrap_or_else(|| "Id".to_string());
    let dir = contract.data.order_dir.as_deref().unwrap_or("asc");
    let call = if dir == "desc" {
        "order_by_desc"
    } else {
        "order_by_asc"
    };
    format!("        .{call}({column_path}::{col})\n")
}

fn leaf(module_path: &str) -> String {
    module_path
        .rsplit("::")
        .next()
        .unwrap_or(module_path)
        .to_string()
}

/// Emit the list view: real columns when the jinja source is resolvable under
/// the spec's `templates_root`, else a faithful skeleton.
fn emit_list_view(contract: &RouteContract, spec: &TargetSpec) -> String {
    match resolve_table_shape(contract, spec) {
        Some(shape) => emit_table_view(contract, "list_for_tenant", &shape),
        None => emit_skeleton_view(contract, "list_for_tenant"),
    }
}

/// Look up the contract's `output` template path under the spec's
/// `templates_root` and extract its table shape. `None` when no templates root
/// is configured, the file is missing, or there is no `<table>{% for %}` block.
fn resolve_table_shape(contract: &RouteContract, spec: &TargetSpec) -> Option<columns::TableShape> {
    let root = spec.templates_root.as_deref()?;
    let OutputKind::Template { path, .. } = &contract.output else {
        return None;
    };
    if path.is_empty() {
        return None;
    }
    let full = std::path::Path::new(root).join(path);
    let text = std::fs::read_to_string(full).ok()?;
    columns::extract_table_shape(&text)
}

/// Render an askama list/detail table from an extracted [`columns::TableShape`].
/// Each cell expression is translated through the jinja→askama translator so
/// the columns are byte-faithful to the source (modulo Option-awareness).
fn emit_table_view(
    contract: &RouteContract,
    kind_dir: &str,
    shape: &columns::TableShape,
) -> String {
    let title = &contract.function;
    let row_var = &shape.loop_.row_var;
    let collection = &shape.loop_.collection;

    let mut headers = String::new();
    for col in &shape.columns {
        let _ = writeln!(headers, "    <th>{}</th>", col.header);
    }

    let mut cells = String::new();
    for col in &shape.columns {
        // The model_fields map is not resolved in this slice; pass None so the
        // translator falls back to the non-Option form (the calibration pass
        // resolves Option-wrapping against the real sea-orm Model).
        let translated = jinja::translate_cell_expr(&col.cell, row_var, None);
        let inner = match &col.cell.wrapper {
            Some(w) if w == "code" => format!("<code>{translated}</code>"),
            _ => translated,
        };
        let _ = writeln!(cells, "      <td>{inner}</td>");
    }

    let n = shape.columns.len().max(1);
    let empty = shape.empty_row.as_deref().unwrap_or("Keine Einträge.");

    format!(
        "{{# AUTO-GENERATED draft view for {endpoint} — {kind_dir} #}}\n\
         {{# INERT: not compiled, not referenced by any Rust struct yet #}}\n\
         {{# Columns extracted from the canonical jinja source (Iron Rule 4) #}}\n\
         {{% extends \"_base.html\" %}}\n\
         \n\
         {{% block title %}}{title}{{% endblock %}}\n\
         \n\
         {{% block content %}}\n\
         {{% for f in flashes %}}<div class=\"flash flash-{{{{ f.level }}}}\">{{{{ f.msg }}}}</div>{{% endfor %}}\n\
         <h1>{title}</h1>\n\
         <table class=\"table table-sm\">\n\
         \x20 <thead><tr>\n\
         {headers}\
         \x20 </tr></thead>\n\
         \x20 <tbody>\n\
         \x20 {{% for {row_var} in {collection} %}}\n\
         \x20   <tr>\n\
         {cells}\
         \x20   </tr>\n\
         \x20 {{% else %}}\n\
         \x20   <tr><td colspan=\"{n}\" class=\"text-muted\">{empty}</td></tr>\n\
         \x20 {{% endfor %}}\n\
         \x20 </tbody>\n\
         </table>\n\
         {{% endblock %}}\n",
        endpoint = contract.id,
    )
}

/// The faithful skeleton view for kinds/pages with no table block (cards,
/// detail pages without a list, or when the jinja source is unavailable).
fn emit_skeleton_view(contract: &RouteContract, kind_dir: &str) -> String {
    let title = &contract.function;
    format!(
        "{{# AUTO-GENERATED draft view for {endpoint} — {kind_dir} #}}\n\
         {{# INERT: not compiled, not referenced by any Rust struct yet #}}\n\
         {{# SKELETON: no <table>{{% for %}}> block in the jinja source (or no #}}\n\
         {{# templates_root configured) — calibrate the body against the source. #}}\n\
         {{% extends \"_base.html\" %}}\n\
         \n\
         {{% block title %}}{title}{{% endblock %}}\n\
         \n\
         {{% block content %}}\n\
         {{% for f in flashes %}}<div class=\"flash flash-{{{{ f.level }}}}\">{{{{ f.msg }}}}</div>{{% endfor %}}\n\
         <h1>{title}</h1>\n\
         {{# CALIBRATION: fill page body from the jinja source #}}\n\
         {{% endblock %}}\n",
        endpoint = contract.id,
    )
}

fn unresolved_block(contract: &RouteContract, spec: &TargetSpec) -> String {
    format!(
        "// Rust model: UNRESOLVED — no mapping in target {target} for {models:?}\n\
         //\n\
         // The extractor saw these models but the target spec has no entry.\n\
         // Add a [models.<Class>] mapping to the target spec, or extend the\n\
         // extraction profile if the model reference was missed entirely.\n\
         // (Calibration lint: unmapped-model)\n",
        target = spec.id,
        models = contract.data.models,
    )
}

// ---------------------------------------------------------------------------
// soft_delete
// ---------------------------------------------------------------------------

fn emit_soft_delete(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params: Vec<&str> = contract
        .inputs
        .path_params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    let redirect_target = match &contract.output {
        OutputKind::Redirect { target } => normalize_redirect(target),
        _ => "/".to_string(),
    };

    let (referenced_models, body) = match primary_model(contract, spec) {
        Some((py_class, mapping)) => {
            let module = leaf(&mapping.module_path);
            let entity = mapping.entity_path(&spec.models_root);
            let column = mapping.column_path(&spec.models_root);
            let model_use = format!("crate::models::{}", mapping.module_path);
            let primary_param = params.first().copied().unwrap_or("id");
            let param_sig = param_signature(&params);

            let scope_filter = if contract.data.tenant_scoped
                || contract.guards.iter().any(|g| g.contains("get_owned"))
            {
                if contract.guards.iter().any(|g| g.contains("get_owned")) {
                    format!("        .filter({module}::Column::UserId.eq(user.id))\n")
                } else {
                    format!(
                        "        .filter({column}::{tenant}.eq(user.tenant_id))\n",
                        tenant = spec.tenant_column
                    )
                }
            } else {
                String::new()
            };

            let delete_block = if contract.data.soft_delete {
                format!(
                    "    // SOFT-DELETE: Python sets .aktiv = false, does NOT delete the row.\n\
                     \x20   let mut active: {module}::ActiveModel = row.into();\n\
                     \x20   active.aktiv = sea_orm::Set(false);\n\
                     \x20   active.update(&db).await?;\n"
                )
            } else {
                "    row.delete(&db).await?;\n".to_string()
            };

            let block = format!(
                "//! rust model:   {model_use}\n\
                 //!\n\
                 //! Iron Rule 7: mirror Python behaviour verbatim (no bug fixes without RFC).\n\
                 \n\
                 use axum::extract::{{Path, State}};\n\
                 use axum::response::{{IntoResponse, Redirect, Response}};\n\
                 use sea_orm::{{ColumnTrait, DatabaseConnection, EntityTrait, ModelTrait, QueryFilter}};\n\
                 use tower_sessions::Session;\n\
                 \n\
                 use crate::error::{{WoaError, WoaResult}};\n\
                 use crate::flash;\n\
                 use crate::middleware::current_user::CurrentUser;\n\
                 use {model_use};\n\
                 \n\
                 pub async fn post_delete(\n\
                 \x20   State(db): State<DatabaseConnection>,\n\
                 \x20   user: CurrentUser,\n\
                 \x20   session: Session,\n\
                 {param_sig}\
                 ) -> WoaResult<Response> {{\n\
                 \x20   let row = {entity}::find_by_id({primary_param})\n\
                 {scope_filter}\
                 \x20       .one(&db)\n\
                 \x20       .await?\n\
                 \x20       .ok_or(WoaError::NotFound)?;\n\
                 {delete_block}\
                 {flash_line}\
                 \x20   Ok(Redirect::to(\"{redirect_target}\").into_response())\n\
                 }}\n",
                flash_line = flash_line_for(contract),
            );
            (vec![py_class.to_string()], block)
        }
        None => (
            contract.data.models.clone(),
            unresolved_block(contract, spec),
        ),
    };

    let header = format!(
        "//! endpoint:     {endpoint}\n\
         //! flask path:   {flask}\n\
         //! axum path:    {axum}\n\
         //! method:       {methods}\n\
         //! source:       {file}:{line}\n\
         //! handler_kind: soft_delete\n\
         //! scoping:      {guards}\n",
        endpoint = contract.id,
        flask = contract.path,
        methods = contract.methods.join(","),
        file = contract.provenance.file,
        line = contract.provenance.line_start,
        guards = if contract.guards.is_empty() {
            "none".to_string()
        } else {
            contract.guards.join(", ")
        },
    );

    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models,
        provided_context_keys: Vec::new(),
    }
}

fn param_signature(params: &[&str]) -> String {
    match params.len() {
        0 => String::new(),
        1 => format!("    Path({}): Path<i32>,\n", params[0]),
        _ => {
            let names = params.join(", ");
            let tys = vec!["i32"; params.len()].join(", ");
            format!("    Path(({names})): Path<({tys})>,\n")
        }
    }
}

fn flash_line_for(contract: &RouteContract) -> String {
    // The flash message is not extracted from the body in this slice; the
    // emitter leaves a faithful placeholder for the reviewer (the Python
    // source line is in the header). Kept minimal to avoid inventing copy.
    let _ = contract;
    String::new()
}

fn normalize_redirect(target: &str) -> String {
    // `url_for('x')` or a raw path. Keep raw paths; leave url_for refs as-is
    // (the reviewer resolves cross-blueprint reverses).
    if target.starts_with('/') {
        target.to_string()
    } else if target.starts_with("url_for") {
        // Best-effort: extract the first string literal if present.
        target.to_string()
    } else {
        format!("/{target}")
    }
}

// ---------------------------------------------------------------------------
// detail_for_tenant (near-copy of the list emitter: scoped get_or_404 + view)
// ---------------------------------------------------------------------------

fn emit_detail_for_tenant(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let struct_name = format!("{}Template", snake_to_pascal(fn_name));
    let tmpl_path = format!("detail_for_tenant/{fn_name}.html");
    let axum = axum_path(&contract.path);
    let template_doc = template_path_of(contract);
    let params: Vec<&str> = path_param_names(contract);
    let param_sig = param_signature(&params);
    let primary_param = params.first().copied().unwrap_or("id");

    let (referenced_models, model_block) = match primary_model(contract, spec) {
        Some((py_class, mapping)) => {
            let model_ty = mapping.model_type(&spec.models_root);
            let entity = mapping.entity_path(&spec.models_root);
            let block = format!(
                "// Rust model: {model_ty}\n\
                 \n\
                 use askama::Template;\n\
                 use axum::extract::{{Path, State}};\n\
                 use sea_orm::{{DatabaseConnection, EntityTrait}};\n\
                 use tower_sessions::Session;\n\
                 \n\
                 use crate::error::{{WoaError, WoaResult}};\n\
                 use crate::flash;\n\
                 use crate::flash::FlashMessage;\n\
                 use crate::middleware::current_user::CurrentUser;\n\
                 use crate::scoping::ensure_tenant;\n\
                 \n\
                 #[derive(Template)]\n\
                 #[template(path = \"{tmpl_path}\")]\n\
                 pub struct {struct_name} {{\n\
                 \x20   pub flashes: Vec<FlashMessage>,\n\
                 \x20   pub current_user: Option<CurrentUser>,\n\
                 \x20   pub item: {model_ty},\n\
                 }}\n\
                 \n\
                 /// Detail handler for {fn_name}. Mirror Python: {endpoint}\n\
                 pub async fn {fn_name}(\n\
                 \x20   State(db): State<DatabaseConnection>,\n\
                 \x20   user: CurrentUser,\n\
                 \x20   session: Session,\n\
                 {param_sig}\
                 ) -> WoaResult<{struct_name}> {{\n\
                 \x20   let item = {entity}::find_by_id({primary_param})\n\
                 \x20       .one(&db)\n\
                 \x20       .await?\n\
                 \x20       .ok_or(WoaError::NotFound)?;\n\
                 \x20   ensure_tenant(&user, item.tenant_id)?;\n\
                 \x20   Ok({struct_name} {{\n\
                 \x20       flashes: flash::take(&session).await,\n\
                 \x20       current_user: Some(user),\n\
                 \x20       item,\n\
                 \x20   }})\n\
                 }}\n\
                 \n\
                 // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
                endpoint = contract.id,
            );
            (vec![py_class.to_string()], block)
        }
        None => (
            contract.data.models.clone(),
            unresolved_block(contract, spec),
        ),
    };

    let header = kind_header(contract, &template_doc);
    let handler_rs = format!("{header}{model_block}");
    // Detail pages usually have no table; emit columns only when the source has
    // a `<table>{% for %}` block (e.g. a sub-list on the detail page).
    let view_html = Some(match resolve_table_shape(contract, spec) {
        Some(shape) => emit_table_view(contract, "detail_for_tenant", &shape),
        None => emit_skeleton_view(contract, "detail_for_tenant"),
    });

    Emitted {
        handler_rs,
        view_html,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: Some(format!("{fn_name}.html")),
        referenced_models,
        provided_context_keys: vec![
            "flashes".to_string(),
            "current_user".to_string(),
            "item".to_string(),
        ],
    }
}

// ---------------------------------------------------------------------------
// template_get (static render, no model query)
// ---------------------------------------------------------------------------

fn emit_template_get(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let struct_name = format!("{}Template", snake_to_pascal(fn_name));
    let tmpl_path = format!("template_get/{fn_name}.html");
    let axum = axum_path(&contract.path);
    let template_doc = template_path_of(contract);
    let gate = admin_gate_block(contract);

    let block = format!(
        "use askama::Template;\n\
         use axum::extract::State;\n\
         use sea_orm::DatabaseConnection;\n\
         use tower_sessions::Session;\n\
         \n\
         use crate::error::{{WoaError, WoaResult}};\n\
         use crate::flash;\n\
         use crate::flash::FlashMessage;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         #[derive(Template)]\n\
         #[template(path = \"{tmpl_path}\")]\n\
         pub struct {struct_name} {{\n\
         \x20   pub flashes: Vec<FlashMessage>,\n\
         \x20   pub current_user: Option<CurrentUser>,\n\
         }}\n\
         \n\
         /// Static render handler for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         \x20   session: Session,\n\
         ) -> WoaResult<{struct_name}> {{\n\
         \x20   let _ = &db;\n\
         {gate}\
         \x20   Ok({struct_name} {{\n\
         \x20       flashes: flash::take(&session).await,\n\
         \x20       current_user: Some(user),\n\
         \x20   }})\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
    );

    let header = kind_header(contract, &template_doc);
    let view_html = Some(emit_skeleton_view(contract, "template_get"));
    Emitted {
        handler_rs: format!("{header}{block}"),
        view_html,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: Some(format!("{fn_name}.html")),
        referenced_models: Vec::new(),
        provided_context_keys: vec!["flashes".to_string(), "current_user".to_string()],
    }
}

// ---------------------------------------------------------------------------
// toggle_bool_field (scoped fetch → flip bool → redirect; soft_delete shape)
// ---------------------------------------------------------------------------

fn emit_toggle_bool_field(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    let primary_param = params.first().copied().unwrap_or("id");
    let redirect_target = redirect_target_of(contract);

    let (referenced_models, body) = match primary_model(contract, spec) {
        Some((py_class, mapping)) => {
            let module = leaf(&mapping.module_path);
            let entity = mapping.entity_path(&spec.models_root);
            let column = mapping.column_path(&spec.models_root);
            let model_use = format!("crate::models::{}", mapping.module_path);
            let scope_filter = tenant_scope_filter(contract, &column, spec);
            let block = format!(
                "//! rust model:   {model_use}\n\
                 //! Iron Rule 7: mirror Python behaviour verbatim.\n\
                 \n\
                 use axum::extract::{{Path, State}};\n\
                 use axum::response::{{IntoResponse, Redirect, Response}};\n\
                 use sea_orm::{{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter}};\n\
                 use tower_sessions::Session;\n\
                 \n\
                 use crate::error::{{WoaError, WoaResult}};\n\
                 use crate::flash;\n\
                 use crate::middleware::current_user::CurrentUser;\n\
                 use {model_use};\n\
                 \n\
                 pub async fn {fn_name}(\n\
                 \x20   State(db): State<DatabaseConnection>,\n\
                 \x20   user: CurrentUser,\n\
                 \x20   session: Session,\n\
                 {param_sig}\
                 ) -> WoaResult<Response> {{\n\
                 \x20   let _ = &session;\n\
                 \x20   let row = {entity}::find_by_id({primary_param})\n\
                 {scope_filter}\
                 \x20       .one(&db)\n\
                 \x20       .await?\n\
                 \x20       .ok_or(WoaError::NotFound)?;\n\
                 \x20   // CALIBRATION: confirm the toggled column against the Python source.\n\
                 \x20   let new_value = !row.aktiv;\n\
                 \x20   let mut active: {module}::ActiveModel = row.into();\n\
                 \x20   active.aktiv = sea_orm::Set(new_value);\n\
                 \x20   active.update(&db).await?;\n\
                 \x20   Ok(Redirect::to(\"{redirect_target}\").into_response())\n\
                 }}\n\
                 \n\
                 // axum route: .route(\"{axum}\", axum::routing::post({fn_name}))\n",
            );
            (vec![py_class.to_string()], block)
        }
        None => (
            contract.data.models.clone(),
            unresolved_block(contract, spec),
        ),
    };

    let header = redirect_header(contract, "toggle_bool_field");
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models,
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// get_redirect_shortcut (GET → Redirect)
// ---------------------------------------------------------------------------

fn emit_get_redirect_shortcut(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let redirect_target = redirect_target_of(contract);
    let header = redirect_header(contract, "get_redirect_shortcut");
    let body = format!(
        "use axum::response::{{IntoResponse, Redirect, Response}};\n\
         use tower_sessions::Session;\n\
         \n\
         use crate::error::WoaResult;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         /// GET-redirect shortcut for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   user: Option<CurrentUser>,\n\
         \x20   session: Session,\n\
         ) -> WoaResult<Response> {{\n\
         \x20   let _ = (&user, &session);\n\
         \x20   // CALIBRATION: mirror any conditional redirect (e.g. logged-in vs not).\n\
         \x20   Ok(Redirect::to(\"{redirect_target}\").into_response())\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// csrf_form_post_engine_call (POST → form DTO → redirect)
// ---------------------------------------------------------------------------

fn emit_csrf_form_post(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    let redirect_target = redirect_target_of(contract);
    let form_struct = format!("{}Form", snake_to_pascal(fn_name));
    let form_dto = dto::emit_form_dto(&form_struct, &contract.inputs.form_fields);

    let header = redirect_header(contract, "csrf_form_post_engine_call");
    let body = format!(
        "use axum::extract::{{Path, State}};\n\
         use axum::response::{{IntoResponse, Redirect, Response}};\n\
         use axum::Form;\n\
         use sea_orm::DatabaseConnection;\n\
         use tower_sessions::Session;\n\
         \n\
         use crate::error::WoaResult;\n\
         use crate::flash;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         {form_dto}\
         \n\
         /// POST form handler for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         \x20   session: Session,\n\
         {param_sig}\
         \x20   Form(form): Form<{form_struct}>,\n\
         ) -> WoaResult<Response> {{\n\
         \x20   let _ = (&db, &user, &session, &form);\n\
         \x20   // CALIBRATION: invoke the engine/service call with the form data,\n\
         \x20   // then flash + redirect exactly as the Python source does.\n\
         \x20   Ok(Redirect::to(\"{redirect_target}\").into_response())\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::post({fn_name}))\n",
        endpoint = contract.id,
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// form_get_post (GET render + POST handle; form DTO + view)
// ---------------------------------------------------------------------------

fn emit_form_get_post(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let struct_name = format!("{}Template", snake_to_pascal(fn_name));
    let tmpl_path = format!("form_get_post/{fn_name}.html");
    let axum = axum_path(&contract.path);
    let template_doc = template_path_of(contract);
    let params = path_param_names(contract);
    let get_param_sig = param_signature(&params);
    let post_param_sig = param_signature(&params);
    let form_struct = format!("{}Form", snake_to_pascal(fn_name));
    let form_dto = dto::emit_form_dto(&form_struct, &contract.inputs.form_fields);
    let redirect_target = redirect_target_of(contract);

    let block = format!(
        "use askama::Template;\n\
         use axum::extract::{{Path, State}};\n\
         use axum::response::{{IntoResponse, Redirect, Response}};\n\
         use axum::Form;\n\
         use sea_orm::DatabaseConnection;\n\
         use tower_sessions::Session;\n\
         \n\
         use crate::error::WoaResult;\n\
         use crate::flash;\n\
         use crate::flash::FlashMessage;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         {form_dto}\
         \n\
         #[derive(Template)]\n\
         #[template(path = \"{tmpl_path}\")]\n\
         pub struct {struct_name} {{\n\
         \x20   pub flashes: Vec<FlashMessage>,\n\
         \x20   pub current_user: Option<CurrentUser>,\n\
         }}\n\
         \n\
         /// GET form render for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}_get(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         \x20   session: Session,\n\
         {get_param_sig}\
         ) -> WoaResult<{struct_name}> {{\n\
         \x20   let _ = &db;\n\
         \x20   Ok({struct_name} {{\n\
         \x20       flashes: flash::take(&session).await,\n\
         \x20       current_user: Some(user),\n\
         \x20   }})\n\
         }}\n\
         \n\
         /// POST form handle for {fn_name}.\n\
         pub async fn {fn_name}_post(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         \x20   session: Session,\n\
         {post_param_sig}\
         \x20   Form(form): Form<{form_struct}>,\n\
         ) -> WoaResult<Response> {{\n\
         \x20   let _ = (&db, &user, &session, &form);\n\
         \x20   // CALIBRATION: validate + persist the form, then flash + redirect.\n\
         \x20   Ok(Redirect::to(\"{redirect_target}\").into_response())\n\
         }}\n\
         \n\
         // axum routes:\n\
         //   .route(\"{axum}\", axum::routing::get({fn_name}_get).post({fn_name}_post))\n",
        endpoint = contract.id,
    );

    let header = kind_header(contract, &template_doc);
    let view_html = Some(emit_skeleton_view(contract, "form_get_post"));
    Emitted {
        handler_rs: format!("{header}{block}"),
        view_html,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: Some(format!("{fn_name}.html")),
        referenced_models: Vec::new(),
        provided_context_keys: vec!["flashes".to_string(), "current_user".to_string()],
    }
}

// ---------------------------------------------------------------------------
// ajax_json (Json<Dto> response)
// ---------------------------------------------------------------------------

fn emit_ajax_json(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    let resp_struct = format!("{}Response", snake_to_pascal(fn_name));
    let shape = match &contract.output {
        OutputKind::Json { shape } => shape.clone(),
        _ => Vec::new(),
    };
    let mut fields = String::new();
    if shape.is_empty() {
        fields
            .push_str("    // CALIBRATION: fill the JSON response fields from the Python body.\n");
    } else {
        for k in &shape {
            let ident = sanitize_field(k);
            let _ = writeln!(fields, "    pub {ident}: serde_json::Value,");
        }
    }

    let header = kind_header(contract, "");
    let body = format!(
        "use axum::extract::{{Path, State}};\n\
         use axum::Json;\n\
         use sea_orm::DatabaseConnection;\n\
         \n\
         use crate::error::WoaResult;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         #[derive(Debug, Default, serde::Serialize)]\n\
         pub struct {resp_struct} {{\n\
         {fields}}}\n\
         \n\
         /// JSON handler for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         {param_sig}\
         ) -> WoaResult<Json<{resp_struct}>> {{\n\
         \x20   let _ = (&db, &user);\n\
         \x20   // CALIBRATION: build the response body from the Python source.\n\
         \x20   Ok(Json({resp_struct}::default()))\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// download_blob (Response bytes + Content-Type/Disposition)
// ---------------------------------------------------------------------------

fn emit_download_blob(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    let mime = match &contract.output {
        OutputKind::Blob { mime } if !mime.is_empty() => mime.clone(),
        _ => "application/octet-stream".to_string(),
    };

    let header = kind_header(contract, "");
    let body = format!(
        "use axum::body::Body;\n\
         use axum::extract::{{Path, State}};\n\
         use axum::http::header;\n\
         use axum::response::{{IntoResponse, Response}};\n\
         use sea_orm::DatabaseConnection;\n\
         \n\
         use crate::error::{{WoaError, WoaResult}};\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         /// Binary download for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         {param_sig}\
         ) -> WoaResult<Response> {{\n\
         \x20   let _ = (&db, &user);\n\
         \x20   // CALIBRATION: produce the bytes (filesystem, DB blob, or generated)\n\
         \x20   // and the exact filename, mirroring the Python source.\n\
         \x20   let bytes: Vec<u8> = Vec::new();\n\
         \x20   let disposition = \"attachment; filename=\\\"{fn_name}.bin\\\"\";\n\
         \x20   let response = Response::builder()\n\
         \x20       .header(header::CONTENT_TYPE, \"{mime}\")\n\
         \x20       .header(header::CONTENT_DISPOSITION, disposition)\n\
         \x20       .body(Body::from(bytes))\n\
         \x20       .map_err(|e| WoaError::Internal(format!(\"response build: {{e}}\")))?;\n\
         \x20   Ok(response.into_response())\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// pdf_render (Response application/pdf via the project's PDF crate)
// ---------------------------------------------------------------------------

fn emit_pdf_render(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    let doc_kind = match &contract.output {
        OutputKind::Pdf { doc_kind } if !doc_kind.is_empty() => doc_kind.clone(),
        _ => "document".to_string(),
    };

    let header = kind_header(contract, "");
    // The doc-kind has no resolved woa_pdf API here, so we emit the call-site
    // SHAPE with a documented stub for the byte source — NEVER `todo!()` in a
    // compiled path (PR #102 guardrail). The `bytes` start empty and the
    // calibration note points at the project PDF crate to fill in.
    let body = format!(
        "use axum::body::Body;\n\
         use axum::extract::{{Path, State}};\n\
         use axum::http::header;\n\
         use axum::response::{{IntoResponse, Response}};\n\
         use sea_orm::DatabaseConnection;\n\
         \n\
         use crate::error::{{WoaError, WoaResult}};\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         /// PDF render for {fn_name} (doc kind: {doc_kind}). Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         {param_sig}\
         ) -> WoaResult<Response> {{\n\
         \x20   let _ = (&db, &user);\n\
         \x20   // CALIBRATION: build the DTO and call the project PDF crate, e.g.\n\
         \x20   //   let pdf_bytes = woa_pdf::render_{doc_kind}(dto, &settings)?;\n\
         \x20   // The PDF API for this doc kind is not resolved by the target spec,\n\
         \x20   // so the byte source is left as a documented stub (no panic macro).\n\
         \x20   let pdf_bytes: Vec<u8> = Vec::new();\n\
         \x20   let response = Response::builder()\n\
         \x20       .header(header::CONTENT_TYPE, \"application/pdf\")\n\
         \x20       .header(header::CONTENT_DISPOSITION, \"inline; filename=\\\"{fn_name}.pdf\\\"\")\n\
         \x20       .body(Body::from(pdf_bytes))\n\
         \x20       .map_err(|e| WoaError::Internal(format!(\"response build: {{e}}\")))?;\n\
         \x20   Ok(response.into_response())\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// sa_admin_view (admin/superadmin gate + render)
// ---------------------------------------------------------------------------

fn emit_sa_admin_view(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let struct_name = format!("{}Template", snake_to_pascal(fn_name));
    let tmpl_path = format!("sa_admin_view/{fn_name}.html");
    let axum = axum_path(&contract.path);
    let template_doc = template_path_of(contract);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    // SA-prefixed functions gate on superadmin; admin-required ones on either.
    let gate = if fn_name.starts_with("sa_") {
        "    if !user.is_superadmin {\n        return Err(WoaError::Forbidden);\n    }\n"
            .to_string()
    } else {
        admin_gate_block(contract)
    };

    let header = kind_header(contract, &template_doc);
    let block = format!(
        "use askama::Template;\n\
         use axum::extract::{{Path, State}};\n\
         use sea_orm::DatabaseConnection;\n\
         use tower_sessions::Session;\n\
         \n\
         use crate::error::{{WoaError, WoaResult}};\n\
         use crate::flash;\n\
         use crate::flash::FlashMessage;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         #[derive(Template)]\n\
         #[template(path = \"{tmpl_path}\")]\n\
         pub struct {struct_name} {{\n\
         \x20   pub flashes: Vec<FlashMessage>,\n\
         \x20   pub current_user: Option<CurrentUser>,\n\
         }}\n\
         \n\
         /// Admin-gated view for {fn_name}. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   user: CurrentUser,\n\
         \x20   session: Session,\n\
         {param_sig}\
         ) -> WoaResult<{struct_name}> {{\n\
         \x20   let _ = &db;\n\
         {gate}\
         \x20   // CALIBRATION: query the listed models ({models}) and fill context.\n\
         \x20   Ok({struct_name} {{\n\
         \x20       flashes: flash::take(&session).await,\n\
         \x20       current_user: Some(user),\n\
         \x20   }})\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
        models = if contract.data.models.is_empty() {
            "none".to_string()
        } else {
            contract.data.models.join(", ")
        },
    );
    let view_html = Some(match resolve_table_shape(contract, spec) {
        Some(shape) => emit_table_view(contract, "sa_admin_view", &shape),
        None => emit_skeleton_view(contract, "sa_admin_view"),
    });
    Emitted {
        handler_rs: format!("{header}{block}"),
        view_html,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: Some(format!("{fn_name}.html")),
        referenced_models: Vec::new(),
        provided_context_keys: vec!["flashes".to_string(), "current_user".to_string()],
    }
}

// ---------------------------------------------------------------------------
// signed_link_action (token-verified action; guard-aware signature)
// ---------------------------------------------------------------------------

fn emit_signed_link_action(contract: &RouteContract, _spec: &TargetSpec) -> Emitted {
    let fn_name = &contract.function;
    let axum = axum_path(&contract.path);
    let params = path_param_names(contract);
    let param_sig = param_signature(&params);
    // Signed-link actions use a separate token auth stack, NOT CurrentUser; the
    // token arrives via a query param (`?t=`) or a path param.
    let header = kind_header(contract, &template_path_of(contract));
    let body = format!(
        "use axum::extract::{{Path, Query, State}};\n\
         use axum::response::{{IntoResponse, Redirect, Response}};\n\
         use sea_orm::DatabaseConnection;\n\
         use serde::Deserialize;\n\
         use tower_sessions::Session;\n\
         \n\
         use crate::error::{{WoaError, WoaResult}};\n\
         use crate::flash;\n\
         use crate::middleware::current_user::CurrentUser;\n\
         \n\
         #[derive(Debug, Default, Deserialize)]\n\
         pub struct {struct}TokenQuery {{\n\
         \x20   /// token from the signed link (Python: request.args.get(\"t\"))\n\
         \x20   #[serde(default)]\n\
         \x20   pub t: String,\n\
         }}\n\
         \n\
         /// Signed-link action for {fn_name}. SECURITY-SENSITIVE — token auth is\n\
         /// a SEPARATE stack from CurrentUser. Mirror Python: {endpoint}\n\
         pub async fn {fn_name}(\n\
         \x20   State(db): State<DatabaseConnection>,\n\
         \x20   session: Session,\n\
         {param_sig}\
         \x20   Query(q): Query<{struct}TokenQuery>,\n\
         ) -> WoaResult<Response> {{\n\
         \x20   let _ = (&db, &session);\n\
         \x20   let token = q.t.trim().to_string();\n\
         \x20   if token.is_empty() {{\n\
         \x20       return Ok(Redirect::to(\"/\").into_response());\n\
         \x20   }}\n\
         \x20   // CALIBRATION: verify the token (revocation, expiry, owner checks)\n\
         \x20   // against the Python token-validation helper, then act + redirect.\n\
         \x20   Ok(Redirect::to(\"/\").into_response())\n\
         }}\n\
         \n\
         // axum route: .route(\"{axum}\", axum::routing::get({fn_name}))\n",
        endpoint = contract.id,
        struct = snake_to_pascal(fn_name),
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// shared helpers for the kinds above
// ---------------------------------------------------------------------------

fn path_param_names(contract: &RouteContract) -> Vec<&str> {
    contract
        .inputs
        .path_params
        .iter()
        .map(|p| p.name.as_str())
        .collect()
}

fn template_path_of(contract: &RouteContract) -> String {
    match &contract.output {
        OutputKind::Template { path, .. } => path.clone(),
        _ => String::new(),
    }
}

fn redirect_target_of(contract: &RouteContract) -> String {
    match &contract.output {
        OutputKind::Redirect { target } => normalize_redirect(target),
        _ => "/".to_string(),
    }
}

/// Tenant/ownership scope filter line for a fetch, reused by delete + toggle.
fn tenant_scope_filter(contract: &RouteContract, column: &str, spec: &TargetSpec) -> String {
    if contract.guards.iter().any(|g| g.contains("get_owned")) {
        // Ownership-scoped: filter on UserId via the column path's module.
        let module = column.trim_end_matches("::Column");
        format!("        .filter({module}::Column::UserId.eq(user.id))\n")
    } else if contract.data.tenant_scoped {
        format!(
            "        .filter({column}::{tenant}.eq(user.tenant_id))\n",
            tenant = spec.tenant_column
        )
    } else {
        String::new()
    }
}

/// `if !(user.is_admin || user.is_superadmin) { Forbidden }` when the contract
/// shows an admin guard; empty otherwise.
fn admin_gate_block(contract: &RouteContract) -> String {
    let needs_admin = contract.guards.iter().any(|g| {
        g.contains("require_admin")
            || g.contains("admin_required")
            || g.contains("superadmin")
            || g.contains("is_admin")
    });
    if needs_admin {
        "    if !(user.is_admin || user.is_superadmin) {\n        return Err(WoaError::Forbidden);\n    }\n"
            .to_string()
    } else {
        String::new()
    }
}

/// Header for redirect-producing kinds (delete/toggle/redirect/csrf).
fn redirect_header(contract: &RouteContract, kind: &str) -> String {
    format!(
        "//! endpoint:     {endpoint}\n\
         //! flask path:   {flask}\n\
         //! axum path:    {axum}\n\
         //! method:       {methods}\n\
         //! source:       {file}:{line}\n\
         //! handler_kind: {kind}\n\
         //! scoping:      {guards}\n\
         //!\n\
         //! AUTO-GENERATED draft (inert). Calibrate against the Python source.\n",
        endpoint = contract.id,
        flask = contract.path,
        axum = axum_path(&contract.path),
        methods = contract.methods.join(","),
        file = contract.provenance.file,
        line = contract.provenance.line_start,
        guards = if contract.guards.is_empty() {
            "none".to_string()
        } else {
            contract.guards.join(", ")
        },
    )
}

/// Sanitize a JSON key into a Rust field identifier (reused by `ajax_json`).
fn sanitize_field(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            if i == 0 && c.is_ascii_digit() {
                out.push_str("f_");
            }
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("field");
    }
    out
}

// ---------------------------------------------------------------------------
// stub (kinds not yet implemented by this target)
// ---------------------------------------------------------------------------

fn emit_stub(contract: &RouteContract, spec: &TargetSpec) -> Emitted {
    let header = kind_header(contract, "");
    let body = format!(
        "// Rust model: {models:?}\n\
         //\n\
         // handler_kind `{kind}` is recognized but not yet emitted by target\n\
         // `{target}`. Add it to the target spec's `emit_kinds` and an emitter\n\
         // recipe. No code is generated to avoid shipping a wrong handler\n\
         // (the engine reports coverage honestly rather than emitting a\n\
         // placeholder into a compiled path — PR #102 guardrail).\n",
        models = contract.data.models,
        kind = contract.handler_kind.as_str(),
        target = spec.id,
    );
    Emitted {
        handler_rs: format!("{header}{body}"),
        view_html: None,
        handler_file: format!("{}__{}.rs", contract.family, contract.function),
        view_file: None,
        referenced_models: Vec::new(),
        provided_context_keys: Vec::new(),
    }
}

fn kind_header(contract: &RouteContract, template_doc: &str) -> String {
    format!(
        "// ============================================================\n\
         // AUTO-GENERATED by ruff_python_dto_check codegen\n\
         // handler_kind: {kind}\n\
         // Python endpoint: {endpoint}\n\
         // Family: {family}  Function: {function}\n\
         // Python path: {path}  methods: {methods}\n\
         // Models used: {models}\n\
         // Template: {template}\n\
         // Classification: {reason}\n\
         // ============================================================\n\
         //\n",
        kind = contract.handler_kind.as_str(),
        endpoint = contract.id,
        family = contract.family,
        function = contract.function,
        path = axum_path(&contract.path),
        methods = contract.methods.join(","),
        models = contract.data.models.join(", "),
        template = template_doc,
        reason = contract.classification_reason,
    )
}
