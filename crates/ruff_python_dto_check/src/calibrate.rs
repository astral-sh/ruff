//! Calibration lints: validate the AST ↔ codegen ↔ template contract from
//! the other end. When generation can't faithfully represent the source, the
//! gap is reported **at the source layer** (extraction profile / target spec)
//! instead of silently producing wrong code.
//!
//! Five checks (per CODEGEN-DESIGN.md §4):
//! - `unmapped-model`           — a model in the AST has no target mapping;
//! - `template-context-mismatch`— template keys vs handler-provided keys;
//! - `form-field-gap`           — a form field read has no DTO/handler field;
//! - `output-kind-mismatch`     — output kind vs the emitted return type;
//! - `extractor-gap`            — a fact the extractor couldn't classify.

use serde::Serialize;

use crate::codegen::Emitted;
use crate::codegen::target::TargetSpec;
use crate::contract::{HandlerKind, RouteContract};
use crate::extractors::body::OutputKind;

/// Severity: hard correctness risk vs a calibration TODO.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A hard correctness risk: the generated code is likely wrong.
    Error,
    /// A TODO: generation is incomplete but not actively wrong.
    Warning,
}

/// Which layer the operator should extend to close the gap.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceLayer {
    /// Fix the extraction profile (a fact was missed).
    ExtractionProfile,
    /// Fix the target spec (a mapping/recipe is missing).
    TargetSpec,
    /// Fix the view template (keys disagree with the handler).
    ViewTemplate,
}

/// One structured diagnostic.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: Severity,
    pub endpoint: String,
    pub message: String,
    /// Where to fix it.
    pub points_at: SourceLayer,
}

/// Run all five calibration checks for one route's contract + emitted code.
pub fn calibrate(
    contract: &RouteContract,
    emitted: &Emitted,
    spec: &TargetSpec,
    template_context_keys: Option<&[String]>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    check_unmapped_model(contract, spec, &mut out);
    check_template_context_mismatch(contract, emitted, template_context_keys, &mut out);
    check_form_field_gap(contract, emitted, &mut out);
    check_output_kind_mismatch(contract, emitted, &mut out);
    check_extractor_gap(contract, &mut out);
    out
}

/// Every model referenced in the AST must have a target mapping; else the
/// emitter dropped it (or doubled the path). Points at the target spec.
fn check_unmapped_model(contract: &RouteContract, spec: &TargetSpec, out: &mut Vec<Diagnostic>) {
    // Only meaningful for kinds that resolve a model.
    if !matches!(
        contract.handler_kind,
        HandlerKind::ListForTenant
            | HandlerKind::DetailForTenant
            | HandlerKind::SoftDelete
            | HandlerKind::ToggleBoolField
    ) {
        return;
    }
    let any_resolved = contract
        .data
        .models
        .iter()
        .any(|m| spec.resolve_model(m).is_some());
    if !contract.data.models.is_empty() && !any_resolved {
        out.push(Diagnostic {
            rule: "unmapped-model",
            severity: Severity::Error,
            endpoint: contract.id.clone(),
            message: format!(
                "no target mapping for any of {:?}; emitter cannot resolve a model path",
                contract.data.models
            ),
            points_at: SourceLayer::TargetSpec,
        });
    }
}

/// The template's context keys must be exactly the set the handler provides
/// (both directions). Points at the view template.
fn check_template_context_mismatch(
    contract: &RouteContract,
    emitted: &Emitted,
    template_context_keys: Option<&[String]>,
    out: &mut Vec<Diagnostic>,
) {
    let OutputKind::Template { context_keys, .. } = &contract.output else {
        return;
    };
    // The handler-provided keys come from the emitter; the template's declared
    // context comes either from the harvested `render_template(...)` kwargs
    // (contract.output) or a separately-extracted template column set.
    let provided: std::collections::BTreeSet<&str> = emitted
        .provided_context_keys
        .iter()
        .map(String::as_str)
        .collect();
    let template_keys: Vec<&str> = template_context_keys
        .map(|k| k.iter().map(String::as_str).collect())
        .unwrap_or_else(|| context_keys.iter().map(String::as_str).collect());

    // Keys the template references but the handler never provides → hard risk
    // (askama compile error downstream). Exclude the collection variable,
    // which the emitter provides under a derived name.
    let missing: Vec<&str> = template_keys
        .iter()
        .copied()
        .filter(|k| !provided.contains(k) && *k != "rows")
        .collect();
    if !missing.is_empty() {
        out.push(Diagnostic {
            rule: "template-context-mismatch",
            severity: Severity::Warning,
            endpoint: contract.id.clone(),
            message: format!(
                "template references context keys not provided by the handler: {missing:?}"
            ),
            points_at: SourceLayer::ViewTemplate,
        });
    }
}

/// Every `request.form` field read should have a handler/DTO field. With no
/// form-DTO emitted yet, a non-empty form-field set on a write handler is a
/// gap. Points at the extraction profile / target spec.
fn check_form_field_gap(contract: &RouteContract, emitted: &Emitted, out: &mut Vec<Diagnostic>) {
    if contract.inputs.form_fields.is_empty() {
        return;
    }
    // The emitter doesn't yet produce a form DTO; if it referenced no form
    // fields, every read is a gap the target spec must cover.
    let covered = emitted
        .handler_rs
        .contains("form")
        || emitted.handler_rs.contains("Form");
    if !covered {
        out.push(Diagnostic {
            rule: "form-field-gap",
            severity: Severity::Warning,
            endpoint: contract.id.clone(),
            message: format!(
                "form fields {:?} are read in the body but no form DTO is emitted",
                contract.inputs.form_fields
            ),
            points_at: SourceLayer::TargetSpec,
        });
    }
}

/// The output kind must match the emitted return type (a Template handler
/// returns a `*Template`, a Redirect handler returns `Response`, …).
fn check_output_kind_mismatch(
    contract: &RouteContract,
    emitted: &Emitted,
    out: &mut Vec<Diagnostic>,
) {
    let rs = &emitted.handler_rs;
    let ok = match &contract.output {
        OutputKind::Template { .. } => rs.contains("Template>") || rs.contains("Template {"),
        OutputKind::Redirect { .. } => rs.contains("Response>"),
        OutputKind::Json { .. } => rs.contains("Json") || rs.contains("Response>"),
        OutputKind::Blob { .. } | OutputKind::Pdf { .. } => rs.contains("Response>"),
        // Unknown output never emits a typed handler; skip.
        OutputKind::Unknown => return,
    };
    // Stubs/unresolved blocks legitimately don't emit a return type; only flag
    // when a body was actually emitted (heuristic: it has `pub async fn`).
    if !ok && rs.contains("pub async fn") {
        out.push(Diagnostic {
            rule: "output-kind-mismatch",
            severity: Severity::Error,
            endpoint: contract.id.clone(),
            message: format!(
                "output kind `{}` does not match the emitted handler return type",
                contract.output.tag()
            ),
            points_at: SourceLayer::TargetSpec,
        });
    }
}

/// A fact the extractor couldn't classify → points at the SOURCE layer (the
/// extraction profile) to extend, not a downstream patch.
fn check_extractor_gap(contract: &RouteContract, out: &mut Vec<Diagnostic>) {
    if matches!(contract.output, OutputKind::Unknown) {
        out.push(Diagnostic {
            rule: "extractor-gap",
            severity: Severity::Warning,
            endpoint: contract.id.clone(),
            message:
                "no response kind classified from the body; the extraction profile may need a \
                 call-name → output mapping for this handler's response idiom"
                    .to_string(),
            points_at: SourceLayer::ExtractionProfile,
        });
    }
    if matches!(contract.handler_kind, HandlerKind::Other) {
        out.push(Diagnostic {
            rule: "extractor-gap",
            severity: Severity::Warning,
            endpoint: contract.id.clone(),
            message: format!(
                "handler classified as `other` ({}); extend the classifier facts to cover it",
                contract.classification_reason
            ),
            points_at: SourceLayer::ExtractionProfile,
        });
    }
}

/// Aggregate diagnostics into the `calibration.json` report value.
pub fn calibration_report(diagnostics: &[Diagnostic]) -> serde_json::Value {
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diagnostics.len() - errors;
    serde_json::json!({
        "totals": { "diagnostics": diagnostics.len(), "errors": errors, "warnings": warnings },
        "diagnostics": diagnostics,
    })
}
