//! Golden tests for the codegen pipeline: contract classification, target
//! emission for `list_for_tenant` + `soft_delete`, the model-path-doubling
//! guard (the systematic Sonnet bug), and the calibration lints.

use ruff_python_dto_check::calibrate::Severity;
use ruff_python_dto_check::codegen::pipeline::{RouteOutput, process_source};
use ruff_python_dto_check::codegen::target::TargetSpec;
use ruff_python_dto_check::contract::HandlerKind;
use ruff_python_dto_check::extractors::body::{ExtractionProfile, OutputKind};

const FIXTURE: &str = include_str!("golden/codegen/woa_routes.input.py");
const KINDS_FIXTURE: &str = include_str!("golden/codegen/woa_kinds.input.py");

fn run() -> Vec<RouteOutput> {
    let profile = ExtractionProfile::default();
    let spec = TargetSpec::rust_axum_seaorm();
    process_source("blueprints/geraete.py", FIXTURE, "geraete", &profile, &spec)
}

fn run_kinds() -> Vec<RouteOutput> {
    let profile = ExtractionProfile::default();
    let spec = TargetSpec::rust_axum_seaorm();
    process_source(
        "blueprints/geraete.py",
        KINDS_FIXTURE,
        "geraete",
        &profile,
        &spec,
    )
}

/// Spec with the golden templates root wired, for the jinja-column tests.
fn spec_with_templates() -> TargetSpec {
    let mut spec = TargetSpec::rust_axum_seaorm();
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/codegen/templates");
    spec.templates_root = Some(root.to_string_lossy().into_owned());
    spec
}

fn by_fn<'a>(outputs: &'a [RouteOutput], name: &str) -> &'a RouteOutput {
    outputs
        .iter()
        .find(|o| o.contract.function == name)
        .unwrap_or_else(|| panic!("no route named {name}"))
}

#[test]
fn detects_four_routes() {
    let outputs = run();
    assert_eq!(outputs.len(), 4, "expected four @bp.route handlers");
}

#[test]
fn classifies_list_for_tenant() {
    let outputs = run();
    let dl = by_fn(&outputs, "device_list");
    assert_eq!(dl.contract.handler_kind, HandlerKind::ListForTenant);
    assert!(matches!(dl.contract.output, OutputKind::Template { .. }));
    assert!(
        dl.contract.data.tenant_scoped,
        "device_list should be tenant-scoped"
    );
}

#[test]
fn classifies_soft_delete() {
    let outputs = run();
    let dd = by_fn(&outputs, "device_delete");
    assert_eq!(dd.contract.handler_kind, HandlerKind::SoftDelete);
    assert!(matches!(dd.contract.output, OutputKind::Redirect { .. }));
    assert_eq!(dd.contract.methods, vec!["POST".to_string()]);
    // Hard delete: no `aktiv = False` so soft_delete flag is false.
    assert!(
        !dd.contract.data.soft_delete,
        "device_delete is a hard delete"
    );

    let cd = by_fn(&outputs, "customer_delete");
    assert_eq!(cd.contract.handler_kind, HandlerKind::SoftDelete);
    assert!(
        cd.contract.data.soft_delete,
        "customer_delete sets aktiv=False"
    );
}

#[test]
fn list_handler_resolves_flat_model_without_doubling() {
    // The Sonnet drafts emitted `crate::models::customer::customer::Model`.
    // Customer is a FLAT model; the correct path is a single segment.
    let outputs = run();
    let dl = by_fn(&outputs, "device_list");
    let rs = &dl.emitted.handler_rs;
    assert!(
        rs.contains("Vec<crate::models::customer::Model>"),
        "expected single-segment flat model path; got:\n{rs}"
    );
    assert!(
        !rs.contains("customer::customer::Model"),
        "model path must NOT be doubled (the Sonnet bug):\n{rs}"
    );
    // Tenant filter + order + router present.
    assert!(rs.contains(".filter(crate::models::customer::Column::TenantId.eq(user.tenant_id))"));
    assert!(rs.contains(".order_by_asc(crate::models::customer::Column::Id)"));
    assert!(rs.contains(r#".route("/geraete", get(device_list))"#));
}

#[test]
fn list_handler_resolves_nested_erp_model() {
    // ERP models ARE genuinely nested: erp::k6_cash::cash_journal.
    let outputs = run();
    let cj = by_fn(&outputs, "cash_journals_list");
    let rs = &cj.emitted.handler_rs;
    assert!(
        rs.contains("Vec<crate::models::erp::k6_cash::cash_journal::Model>"),
        "expected nested ERP model path; got:\n{rs}"
    );
    assert!(rs.contains("crate::models::erp::k6_cash::cash_journal::Entity::find()"));
}

#[test]
fn soft_delete_emits_active_model_update() {
    let outputs = run();
    let cd = by_fn(&outputs, "customer_delete");
    let rs = &cd.emitted.handler_rs;
    // True soft-delete uses ActiveModel + aktiv=Set(false), not row.delete.
    assert!(
        rs.contains("ActiveModel"),
        "soft delete should use ActiveModel:\n{rs}"
    );
    assert!(
        rs.contains("active.aktiv = sea_orm::Set(false);"),
        "got:\n{rs}"
    );
    assert!(rs.contains(r#"Redirect::to("/kunden")"#));
    assert!(rs.contains("Path(cid): Path<i32>"));
    // No doubled model path here either.
    assert!(!rs.contains("customer::customer"));
}

#[test]
fn hard_delete_emits_row_delete() {
    let outputs = run();
    let dd = by_fn(&outputs, "device_delete");
    let rs = &dd.emitted.handler_rs;
    assert!(rs.contains("row.delete(&db).await?;"), "got:\n{rs}");
    assert!(rs.contains("crate::models::device::Entity::find_by_id(did)"));
    assert!(rs.contains(".filter(crate::models::device::Column::TenantId.eq(user.tenant_id))"));
}

#[test]
fn calibration_is_clean_for_resolved_routes() {
    let outputs = run();
    // The two list routes resolve cleanly: no error-severity diagnostics.
    for name in [
        "device_list",
        "cash_journals_list",
        "device_delete",
        "customer_delete",
    ] {
        let ro = by_fn(&outputs, name);
        let errors: Vec<_> = ro
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{name} produced error diagnostics: {errors:?}"
        );
    }
}

#[test]
fn unmapped_model_lint_fires_on_unknown_model() {
    // A route over a model the target spec doesn't map must produce the
    // `unmapped-model` diagnostic pointing at the target spec.
    let src = r#"
from flask import Blueprint, render_template
bp = Blueprint("widgets", __name__)

@bp.route("/widgets")
@login_required
def widget_list():
    widgets = WidgetThing.query.filter_by(tenant_id=g.tenant_id).all()
    return render_template("widgets/list.html", widgets=widgets)
"#;
    let profile = ExtractionProfile::default();
    let spec = TargetSpec::rust_axum_seaorm();
    let outputs = process_source("blueprints/widgets.py", src, "widgets", &profile, &spec);
    let wl = by_fn(&outputs, "widget_list");
    assert_eq!(wl.contract.handler_kind, HandlerKind::ListForTenant);
    let has_unmapped = wl
        .diagnostics
        .iter()
        .any(|d| d.rule == "unmapped-model" && d.severity == Severity::Error);
    assert!(
        has_unmapped,
        "expected unmapped-model error; got {:?}",
        wl.diagnostics
    );
}

#[test]
fn toml_target_spec_loads_and_resolves_paths() {
    // The example TOML target spec must resolve flat + nested models the same
    // way the built-in spec does (no path doubling).
    let spec = TargetSpec::from_path(std::path::Path::new(
        "examples/rust-axum-seaorm.target.toml",
    ))
    .expect("toml target parses");
    assert_eq!(spec.id, "rust-axum-seaorm");
    assert_eq!(
        spec.resolve_model("Customer")
            .unwrap()
            .model_type("crate::models"),
        "crate::models::customer::Model"
    );
    assert_eq!(
        spec.resolve_model("ErpCashJournal")
            .unwrap()
            .model_type("crate::models"),
        "crate::models::erp::k6_cash::cash_journal::Model"
    );
    assert!(spec.can_emit(HandlerKind::ListForTenant));
    assert!(spec.can_emit(HandlerKind::SoftDelete));
    assert!(spec.can_emit(HandlerKind::DetailForTenant));
    assert!(spec.can_emit(HandlerKind::PdfRender));
    // A kind genuinely absent from the example list is not emitted.
    assert!(!spec.can_emit(HandlerKind::Other));
}

#[test]
fn disabled_kind_emits_documented_stub_not_todo() {
    // A kind NOT listed in a target's `emit_kinds` must produce a documented
    // stub, never `todo!()`/`unimplemented!()` in a compiled path (PR #102
    // guardrail). We use a restricted spec (only list_for_tenant enabled) so
    // download_blob falls through to the stub.
    let src = r#"
from flask import Blueprint, send_file
bp = Blueprint("export", __name__)

@bp.route("/export/customers.csv")
@login_required
def customers_export():
    return send_file(build_csv(), mimetype="text/csv")
"#;
    let profile = ExtractionProfile::default();
    let mut spec = TargetSpec::rust_axum_seaorm();
    spec.emit_kinds = vec!["list_for_tenant".to_string()];
    let outputs = process_source("blueprints/export.py", src, "export", &profile, &spec);
    let ce = by_fn(&outputs, "customers_export");
    assert_eq!(ce.contract.handler_kind, HandlerKind::DownloadBlob);
    let rs = &ce.emitted.handler_rs;
    // No executable handler body at all (the stub is pure comments), so no
    // `todo!()`/`unimplemented!()` can reach a compiled path. The macro names
    // appearing inside the explanatory comment are fine.
    assert!(
        !rs.contains("pub async fn") && !rs.contains("pub fn router"),
        "stub must not emit an executable handler:\n{rs}"
    );
    assert!(rs.contains("not yet emitted by target"), "got:\n{rs}");
}

#[test]
fn emitted_kinds_match_their_output_kind() {
    // The output-kind-mismatch calibration lint must not fire for any emitted
    // kind: each handler's return type must match its contract output (Template
    // → *Template, Redirect → Response, Json → Json, Blob/Pdf → Response).
    let outputs = run_kinds();
    for ro in &outputs {
        let mismatch: Vec<_> = ro
            .diagnostics
            .iter()
            .filter(|d| d.rule == "output-kind-mismatch")
            .collect();
        assert!(
            mismatch.is_empty(),
            "{} ({}) tripped output-kind-mismatch: {mismatch:?}",
            ro.contract.function,
            ro.contract.handler_kind.as_str()
        );
    }
}

#[test]
fn no_emitter_emits_no_todo_macro_in_any_kind() {
    // Across every kind the built-in target enables, no emitted handler may
    // contain a literal `todo!()`/`unimplemented!()` call (the PR #102 +
    // pdf_render guardrail — the pdf draft used todo!() inside its body).
    let outputs = run_kinds();
    for ro in &outputs {
        let rs = &ro.emitted.handler_rs;
        assert!(
            !rs.contains("todo!(") && !rs.contains("unimplemented!("),
            "{} ({}) emitted a panic macro:\n{rs}",
            ro.contract.function,
            ro.contract.handler_kind.as_str()
        );
    }
}

#[test]
fn contract_json_roundtrips() {
    use ruff_python_dto_check::contract::contract_to_json;
    let outputs = run();
    let dl = by_fn(&outputs, "device_list");
    let json = contract_to_json(&dl.contract);
    assert_eq!(json["handler_kind"], "list_for_tenant");
    assert_eq!(json["id"], "geraete.device_list");
    assert_eq!(json["output"]["kind"], "template");
}

// ---------------------------------------------------------------------------
// Task B — golden assertions per newly-added kind
// ---------------------------------------------------------------------------

#[test]
fn detail_for_tenant_emits_scoped_get_or_404() {
    let outputs = run_kinds();
    let dd = by_fn(&outputs, "device_detail");
    assert_eq!(dd.contract.handler_kind, HandlerKind::DetailForTenant);
    let rs = &dd.emitted.handler_rs;
    assert!(
        rs.contains("crate::models::device::Entity::find_by_id(did)"),
        "got:\n{rs}"
    );
    assert!(rs.contains(".ok_or(WoaError::NotFound)?"), "got:\n{rs}");
    assert!(
        rs.contains("ensure_tenant(&user, item.tenant_id)?"),
        "got:\n{rs}"
    );
    assert!(rs.contains("Path(did): Path<i32>"), "got:\n{rs}");
    // No doubled flat model path.
    assert!(!rs.contains("device::device::Model"), "doubled path:\n{rs}");
}

#[test]
fn template_get_emits_static_render_no_query() {
    let outputs = run_kinds();
    let ei = by_fn(&outputs, "erp_index");
    assert_eq!(ei.contract.handler_kind, HandlerKind::TemplateGet);
    let rs = &ei.emitted.handler_rs;
    assert!(rs.contains("WoaResult<ErpIndexTemplate>"), "got:\n{rs}");
    // No model query (static render).
    assert!(
        !rs.contains("Entity::find"),
        "template_get must not query:\n{rs}"
    );
}

#[test]
fn toggle_bool_field_flips_and_redirects() {
    let outputs = run_kinds();
    let dt = by_fn(&outputs, "toggle_device");
    assert_eq!(dt.contract.handler_kind, HandlerKind::ToggleBoolField);
    let rs = &dt.emitted.handler_rs;
    assert!(rs.contains("let new_value = !row.aktiv;"), "got:\n{rs}");
    assert!(
        rs.contains("active.aktiv = sea_orm::Set(new_value);"),
        "got:\n{rs}"
    );
    assert!(rs.contains(r#"Redirect::to("/geraete")"#), "got:\n{rs}");
    assert!(
        rs.contains("crate::models::device::Entity::find_by_id(did)"),
        "got:\n{rs}"
    );
}

#[test]
fn get_redirect_shortcut_emits_redirect() {
    let outputs = run_kinds();
    let si = by_fn(&outputs, "system_index");
    assert_eq!(si.contract.handler_kind, HandlerKind::GetRedirectShortcut);
    let rs = &si.emitted.handler_rs;
    assert!(rs.contains(r#"Redirect::to("/dashboard")"#), "got:\n{rs}");
    assert!(rs.contains("WoaResult<Response>"), "got:\n{rs}");
    assert!(rs.contains("user: Option<CurrentUser>"), "got:\n{rs}");
}

#[test]
fn csrf_form_post_emits_form_dto_and_redirect() {
    let outputs = run_kinds();
    let dc = by_fn(&outputs, "device_create");
    assert_eq!(
        dc.contract.handler_kind,
        HandlerKind::CsrfFormPostEngineCall
    );
    let rs = &dc.emitted.handler_rs;
    assert!(rs.contains("pub struct DeviceCreateForm {"), "got:\n{rs}");
    assert!(rs.contains("pub hostname: Option<String>,"), "got:\n{rs}");
    assert!(rs.contains("pub model: Option<String>,"), "got:\n{rs}");
    assert!(
        rs.contains("Form(form): Form<DeviceCreateForm>"),
        "got:\n{rs}"
    );
    assert!(rs.contains(r#"Redirect::to("/geraete")"#), "got:\n{rs}");
}

#[test]
fn form_get_post_emits_get_and_post_handlers() {
    let outputs = run_kinds();
    let de = by_fn(&outputs, "device_edit");
    assert_eq!(de.contract.handler_kind, HandlerKind::FormGetPost);
    let rs = &de.emitted.handler_rs;
    assert!(rs.contains("pub async fn device_edit_get("), "got:\n{rs}");
    assert!(rs.contains("pub async fn device_edit_post("), "got:\n{rs}");
    assert!(rs.contains("pub struct DeviceEditForm {"), "got:\n{rs}");
    assert!(rs.contains("pub hostname: Option<String>,"), "got:\n{rs}");
    assert!(rs.contains("pub standort: Option<String>,"), "got:\n{rs}");
    assert!(
        de.emitted.view_html.is_some(),
        "form_get_post should emit a view"
    );
}

#[test]
fn ajax_json_emits_json_response() {
    let outputs = run_kinds();
    let dj = by_fn(&outputs, "dashboard_json");
    assert_eq!(dj.contract.handler_kind, HandlerKind::AjaxJson);
    let rs = &dj.emitted.handler_rs;
    assert!(
        rs.contains("WoaResult<Json<DashboardJsonResponse>>"),
        "got:\n{rs}"
    );
    assert!(
        rs.contains("pub struct DashboardJsonResponse {"),
        "got:\n{rs}"
    );
    // jsonify keys become response fields.
    assert!(
        rs.contains("pub open_count:") || rs.contains("pub overdue:"),
        "got:\n{rs}"
    );
}

#[test]
fn download_blob_emits_byte_response() {
    let outputs = run_kinds();
    let dq = by_fn(&outputs, "device_qr");
    assert_eq!(dq.contract.handler_kind, HandlerKind::DownloadBlob);
    let rs = &dq.emitted.handler_rs;
    assert!(rs.contains("WoaResult<Response>"), "got:\n{rs}");
    assert!(rs.contains("header::CONTENT_DISPOSITION"), "got:\n{rs}");
    assert!(rs.contains("Body::from(bytes)"), "got:\n{rs}");
}

#[test]
fn pdf_render_emits_pdf_response_no_todo() {
    let outputs = run_kinds();
    let wp = by_fn(&outputs, "workorder_pdf");
    assert_eq!(wp.contract.handler_kind, HandlerKind::PdfRender);
    let rs = &wp.emitted.handler_rs;
    assert!(rs.contains(r#""application/pdf""#), "got:\n{rs}");
    assert!(rs.contains("Body::from(pdf_bytes)"), "got:\n{rs}");
    // The PR #102 / pdf-draft guardrail: NEVER todo!() in a compiled path.
    assert!(
        !rs.contains("todo!("),
        "pdf_render must not emit todo!():\n{rs}"
    );
}

#[test]
fn sa_admin_view_gates_on_superadmin() {
    let outputs = run_kinds();
    let sh = by_fn(&outputs, "sa_health");
    assert_eq!(sh.contract.handler_kind, HandlerKind::SaAdminView);
    let rs = &sh.emitted.handler_rs;
    // sa_-prefixed → superadmin gate.
    assert!(rs.contains("if !user.is_superadmin"), "got:\n{rs}");
    assert!(rs.contains("WoaResult<SaHealthTemplate>"), "got:\n{rs}");
}

#[test]
fn signed_link_action_uses_token_query_not_current_user() {
    let outputs = run_kinds();
    let pa = by_fn(&outputs, "portal_auto_login");
    assert_eq!(pa.contract.handler_kind, HandlerKind::SignedLinkAction);
    let rs = &pa.emitted.handler_rs;
    assert!(rs.contains("PortalAutoLoginTokenQuery"), "got:\n{rs}");
    assert!(rs.contains("Query(q): Query<"), "got:\n{rs}");
    // Separate auth stack — no CurrentUser extractor in the signature.
    assert!(
        !rs.contains("user: CurrentUser"),
        "signed-link must not take CurrentUser:\n{rs}"
    );
    assert!(rs.contains("SECURITY-SENSITIVE"), "got:\n{rs}");
}

// ---------------------------------------------------------------------------
// Task A — jinja → askama column wiring
// ---------------------------------------------------------------------------

#[test]
fn list_view_gets_real_columns_from_jinja() {
    // device_list renders devices/list.html, which has a <table>{% for %}.
    let profile = ExtractionProfile::default();
    let spec = spec_with_templates();
    let src = r#"
from flask import Blueprint, render_template
bp = Blueprint("geraete", __name__)

@bp.route("/geraete")
@login_required
def device_list():
    items = Device.query.filter_by(tenant_id=g.tenant_id).all()
    return render_template("devices/list.html", items=items)
"#;
    let outputs = process_source("blueprints/geraete.py", src, "geraete", &profile, &spec);
    let dl = by_fn(&outputs, "device_list");
    let view = dl.emitted.view_html.as_deref().expect("view emitted");
    // Real headers from the jinja source.
    assert!(view.contains("<th>Gerät</th>"), "got:\n{view}");
    assert!(view.contains("<th>Kunde</th>"), "got:\n{view}");
    assert!(view.contains("<th>S/N</th>"), "got:\n{view}");
    // The for-loop uses the source row_var + collection.
    assert!(view.contains("{% for d in items %}"), "got:\n{view}");
    // Direct field cell translated.
    assert!(view.contains("{{ d.display_name }}"), "got:\n{view}");
    // <code> wrapper preserved on S/N.
    assert!(
        view.contains("<code>{{ d.seriennummer }}</code>"),
        "got:\n{view}"
    );
    // Inline ternary on Aktiv → askama if/else.
    assert!(
        view.contains("{% if d.aktiv %}Ja{% else %}Nein{% endif %}"),
        "got:\n{view}"
    );
    // Empty-state row text preserved.
    assert!(view.contains("Keine Geräte."), "got:\n{view}");
    // No leftover generic skeleton.
    assert!(
        !view.contains("row.id"),
        "skeleton leaked into wired view:\n{view}"
    );
}

#[test]
fn detail_view_without_table_keeps_skeleton() {
    // device_detail renders devices/detail.html which has NO <table>{% for %},
    // so the view must keep the faithful skeleton (not crash, not invent rows).
    let profile = ExtractionProfile::default();
    let spec = spec_with_templates();
    let src = r#"
from flask import Blueprint, render_template
bp = Blueprint("geraete", __name__)

@bp.route("/geraete/<int:did>")
@login_required
def device_detail(did):
    d = get_scoped_or_404(Device, did)
    return render_template("devices/detail.html", d=d)
"#;
    let outputs = process_source("blueprints/geraete.py", src, "geraete", &profile, &spec);
    let dd = by_fn(&outputs, "device_detail");
    let view = dd.emitted.view_html.as_deref().expect("view emitted");
    assert!(view.contains("SKELETON"), "expected skeleton view:\n{view}");
    // The flashes loop is fine; the skeleton must not invent a data row loop.
    assert!(
        !view.contains("{% for d in"),
        "skeleton must not invent a data loop:\n{view}"
    );
}

#[test]
fn list_view_falls_back_to_skeleton_without_templates_root() {
    // The built-in spec has no templates_root → list views stay skeleton.
    let outputs = run();
    let dl = by_fn(&outputs, "device_list");
    let view = dl.emitted.view_html.as_deref().expect("view emitted");
    assert!(view.contains("list_for_tenant"), "got:\n{view}");
    // Generic skeleton uses the `rows` placeholder loop.
    assert!(
        view.contains("CALIBRATION") || view.contains("row"),
        "got:\n{view}"
    );
}
