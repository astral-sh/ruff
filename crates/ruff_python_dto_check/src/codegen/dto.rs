//! Form-DTO emission, porting the struct shape from
//! `woa-rs/tools/contracts_to_rust.py`.
//!
//! Form-bearing handler-kinds (`form_get_post`, `csrf_form_post_engine_call`)
//! read `request.form` fields; the emitter turns the contract's
//! `inputs.form_fields` into a `#[derive(Deserialize)]` form struct so the axum
//! handler can extract `Form<…>`. The field *types* are not known from the
//! Python form reads (they are all `str`-ish at the request layer), so every
//! field is emitted as `Option<String>` with a calibration note — the
//! calibration pass resolves the real type against the DTO contract, exactly as
//! `contracts_to_rust.py` resolves against the sea-orm Model.
//!
//! The crate stays generic: the field set comes from the contract, the type
//! mapping is the neutral request-layer default. A project with typed form
//! contracts supplies them through its own DTO source (the `convert_model_to_*`
//! ladder in `contracts_to_rust.py`), not through crate constants.

use std::fmt::Write as _;

/// Emit a `Deserialize` form DTO struct from the contract's form fields.
///
/// Returns `(struct_name, struct_source)`. When there are no form fields the
/// struct is still emitted (empty) so the handler signature stays uniform; the
/// caller decides whether to include it.
pub fn emit_form_dto(struct_name: &str, form_fields: &[String]) -> String {
    let mut fields = String::new();
    if form_fields.is_empty() {
        fields.push_str("    // CALIBRATION: no form fields were read in the Python body;\n");
        fields.push_str("    // add the real fields from the form template / DTO contract.\n");
    } else {
        for f in form_fields {
            let ident = sanitize_ident(f);
            // Request-layer default: every form value arrives as a string and
            // may be absent. The calibration pass narrows the type (i32, bool,
            // NaiveDate, …) against the DTO contract — see contracts_to_rust.py.
            let _ = writeln!(fields, "    /// form field `{f}` (request.form)");
            fields.push_str("    #[serde(default)]\n");
            let _ = writeln!(fields, "    pub {ident}: Option<String>,");
        }
    }
    format!(
        "#[derive(Debug, Default, serde::Deserialize)]\n\
         pub struct {struct_name} {{\n\
         {fields}}}\n"
    )
}

/// Make a valid Rust identifier from a form field name. Non-identifier chars
/// become `_`; a leading digit is prefixed with `f_`. Pure data transform —
/// no project knowledge.
fn sanitize_ident(name: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_optional_string_fields() {
        let src = emit_form_dto(
            "DeviceAddForm",
            &["hostname".to_string(), "model".to_string()],
        );
        assert!(src.contains("pub struct DeviceAddForm {"));
        assert!(src.contains("pub hostname: Option<String>,"));
        assert!(src.contains("pub model: Option<String>,"));
        assert!(src.contains("serde::Deserialize"));
    }

    #[test]
    fn sanitizes_non_ident_fields() {
        assert_eq!(sanitize_ident("kunden-id"), "kunden_id");
        assert_eq!(sanitize_ident("2fa"), "f_2fa");
        assert_eq!(sanitize_ident("ok_name"), "ok_name");
    }

    #[test]
    fn empty_fields_emit_calibration_note() {
        let src = emit_form_dto("EmptyForm", &[]);
        assert!(src.contains("no form fields"));
        assert!(src.contains("pub struct EmptyForm {"));
    }
}
