//! Golden tests for the codegen pipeline: contract classification, target
//! emission for `list_for_tenant` + `soft_delete`, the model-path-doubling
//! guard (the systematic Sonnet bug), and the calibration lints.

use ruff_python_dto_check::calibrate::Severity;
use ruff_python_dto_check::codegen::pipeline::{RouteOutput, process_source};
use ruff_python_dto_check::codegen::target::TargetSpec;
use ruff_python_dto_check::contract::HandlerKind;
use ruff_python_dto_check::extractors::body::{ExtractionProfile, OutputKind};

const FIXTURE: &str = include_str!("golden/codegen/woa_routes.input.py");

fn run() -> Vec<RouteOutput> {
    let profile = ExtractionProfile::default();
    let spec = TargetSpec::rust_axum_seaorm();
    process_source(
        "blueprints/geraete.py",
        FIXTURE,
        "geraete",
        &profile,
        &spec,
    )
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
    assert!(dl.contract.data.tenant_scoped, "device_list should be tenant-scoped");
}

#[test]
fn classifies_soft_delete() {
    let outputs = run();
    let dd = by_fn(&outputs, "device_delete");
    assert_eq!(dd.contract.handler_kind, HandlerKind::SoftDelete);
    assert!(matches!(dd.contract.output, OutputKind::Redirect { .. }));
    assert_eq!(dd.contract.methods, vec!["POST".to_string()]);
    // Hard delete: no `aktiv = False` so soft_delete flag is false.
    assert!(!dd.contract.data.soft_delete, "device_delete is a hard delete");

    let cd = by_fn(&outputs, "customer_delete");
    assert_eq!(cd.contract.handler_kind, HandlerKind::SoftDelete);
    assert!(cd.contract.data.soft_delete, "customer_delete sets aktiv=False");
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
    assert!(rs.contains("ActiveModel"), "soft delete should use ActiveModel:\n{rs}");
    assert!(rs.contains("active.aktiv = sea_orm::Set(false);"), "got:\n{rs}");
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
    for name in ["device_list", "cash_journals_list", "device_delete", "customer_delete"] {
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
        spec.resolve_model("Customer").unwrap().model_type("crate::models"),
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
    assert!(!spec.can_emit(HandlerKind::PdfRender));
}

#[test]
fn unsupported_kind_emits_documented_stub_not_todo() {
    // A kind the target doesn't emit must produce a documented stub, never
    // `todo!()`/`unimplemented!()` in a compiled path (PR #102 guardrail).
    let src = r#"
from flask import Blueprint, send_file
bp = Blueprint("export", __name__)

@bp.route("/export/customers.csv")
@login_required
def customers_export():
    return send_file(build_csv(), mimetype="text/csv")
"#;
    let profile = ExtractionProfile::default();
    let spec = TargetSpec::rust_axum_seaorm();
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
fn contract_json_roundtrips() {
    use ruff_python_dto_check::contract::contract_to_json;
    let outputs = run();
    let dl = by_fn(&outputs, "device_list");
    let json = contract_to_json(&dl.contract);
    assert_eq!(json["handler_kind"], "list_for_tenant");
    assert_eq!(json["id"], "geraete.device_list");
    assert_eq!(json["output"]["kind"], "template");
}
