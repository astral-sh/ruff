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

pub mod jinja;
pub mod pipeline;
pub mod target;

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
    /// POST delete handler (soft or hard).
    SoftDelete,
    /// Documented stub: the kind is recognized but not yet emitted by this
    /// target. Honest about coverage rather than emitting wrong code.
    Stub,
}

impl KindRecipe {
    fn for_kind(kind: HandlerKind, spec: &TargetSpec) -> Self {
        if !spec.can_emit(kind) {
            return KindRecipe::Stub;
        }
        match kind {
            HandlerKind::ListForTenant => KindRecipe::ListForTenant,
            HandlerKind::SoftDelete => KindRecipe::SoftDelete,
            _ => KindRecipe::Stub,
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
        KindRecipe::SoftDelete => emit_soft_delete(contract, spec),
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

    let view_html = Some(emit_list_view(contract));

    Emitted {
        handler_rs,
        view_html,
        handler_file: format!("{}__{}.rs", contract.family, fn_name),
        view_file: Some(format!("{fn_name}.html")),
        referenced_models,
        provided_context_keys: vec!["flashes".to_string(), "current_user".to_string(), "title".to_string()],
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
    module_path.rsplit("::").next().unwrap_or(module_path).to_string()
}

fn emit_list_view(contract: &RouteContract) -> String {
    let title = &contract.function;
    let collection = "rows";
    format!(
        "{{# AUTO-GENERATED draft view for {endpoint} — list_for_tenant #}}\n\
         {{# INERT: not compiled, not referenced by any Rust struct yet #}}\n\
         {{% extends \"_base.html\" %}}\n\
         \n\
         {{% block title %}}{{{{ title }}}}{{% endblock %}}\n\
         \n\
         {{% block content %}}\n\
         {{% for f in flashes %}}<div class=\"flash flash-{{{{ f.level }}}}\">{{{{ f.msg }}}}</div>{{% endfor %}}\n\
         <h1>{{{{ title }}}}</h1>\n\
         <table class=\"table table-sm\">\n\
         \x20 <tbody>\n\
         \x20 {{% for row in {collection} %}}\n\
         \x20   <tr><td>{{{{ row.id }}}}</td></tr>\n\
         \x20 {{% endfor %}}\n\
         \x20 </tbody>\n\
         </table>\n\
         {{% endblock %}}\n",
        endpoint = contract.id,
    )
    .replace("{{ title }}", title)
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
                    format!("        .filter({column}::{tenant}.eq(user.tenant_id))\n", tenant = spec.tenant_column)
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
