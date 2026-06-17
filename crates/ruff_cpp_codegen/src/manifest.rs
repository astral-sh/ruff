//! The C++ method-resolution manifest — the AST-DLL codegen's stage-2 model.
//!
//! [`crate::project`] extracts the **method plane** of a reassembled
//! [`ModelGraph`] (one [`MethodSig`] per harvested method) into per-class
//! [`ClassManifest`]s. [`crate::render`] then emits those as Rust source.
//!
//! # Why a `MethodSig` and not the harvest `CppMethod`
//!
//! `MethodSig` is the **dispatch-relevant signature** subset: name, ordered
//! parameter types, return type, the `is_const`/`is_static` qualifiers, and the
//! override target. The body-shaping flags `CppMethod` also carries
//! (`is_pure_virtual` / `constexpr_kind` / `is_noexcept` / `operator_kind` /
//! `requires_clause`) are intentionally dropped: they drive *body* generation,
//! not the signature manifest. `MethodSig` is the harvest-IR → Core-registry
//! projection — the Core-side runtime shape the generated text names, distinct
//! from the serde-backed harvest IR (a legitimate boundary, not a parallel
//! model).
//!
//! # The round-trip gate (teeth)
//!
//! The manifest's fidelity to the harvest is proven by [`crate::decompile`]:
//! regenerating the **signature-plane** triples from the manifest must equal
//! `expand`'s signature-plane output for the same graph. A manifest that drops a
//! method, mangles a parameter, loses a return type, or misses an override edge
//! produces a different `(s, p, o)` set and fails the round-trip. This is the
//! `codegen_spine::roundtrip_eq` pattern (project → decompile → compare against
//! the live harvested triples), implemented over `ruff_spo_triplet::Triple` so
//! the codegen crate stays `ruff_spo_triplet`-only (no lance-graph edge).

use ruff_spo_triplet::{CppMethod, ModelGraph, Predicate, Provenance, Triple};

/// One method's dispatch-relevant signature — the shape the generated Rust
/// `MethodSig` literal carries. See the module docs for what is dropped and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSig {
    /// Bare method name (e.g. `unichar_to_id`, `operator==`), already
    /// suffix-stripped by the reassembler.
    pub name: String,
    /// Parameter types in signature order, verbatim (e.g.
    /// `["const char *", "int"]`).
    pub params: Vec<String>,
    /// Return type, verbatim. `None` for void / constructors / destructors.
    pub ret: Option<String>,
    /// `T method() const;` — a const-qualified (read-accessor) member.
    pub is_const: bool,
    /// `static T method();` — a class-level member (no implicit `this`).
    pub is_static: bool,
    /// The fully-qualified overridden base method (cv-aware), if this method
    /// `override`s a virtual base method.
    pub overrides: Option<String>,
}

/// The method manifest for one class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassManifest {
    /// Fully-qualified class name (e.g. `tesseract::UNICHARSET`).
    pub class: String,
    /// Methods in harvest order (the renderer sorts deterministically).
    pub methods: Vec<MethodSig>,
}

/// Project the C++ method plane of a [`ModelGraph`] into per-class manifests.
///
/// Pure extraction. The underlying graph's fidelity to the harvest is the
/// reassembler's `CPP-REASSEMBLE-RT` guarantee; [`crate::decompile`] proves
/// THIS projection re-emits the same signature triples `expand` does.
#[must_use]
pub fn project(graph: &ModelGraph) -> Vec<ClassManifest> {
    graph
        .models
        .iter()
        .map(|model| ClassManifest {
            class: model.name.clone(),
            methods: model.methods.iter().map(method_sig).collect(),
        })
        .collect()
}

fn method_sig(method: &CppMethod) -> MethodSig {
    MethodSig {
        name: method.name.clone(),
        params: method.param_types.clone(),
        ret: method.return_type.clone(),
        is_const: method.is_const,
        is_static: method.is_static,
        overrides: method.overrides.clone(),
    }
}

/// The cv-aware method IRI `expand` builds (`{ns}:{class}.{name}({params}) [const]`).
/// Kept in lockstep with `expand::cpp_method` so the round-trip stays exact; if
/// `expand`'s IRI format changes and this does not, [`decompile`]'s round-trip
/// fails (a drift guard).
fn method_iri(model_iri: &str, method: &MethodSig) -> String {
    format!(
        "{model_iri}.{}({}){}",
        method.name,
        method.params.join(","),
        if method.is_const { " const" } else { "" }
    )
}

/// Regenerate the **signature-plane** triples a manifest encodes — the inverse
/// of [`project`] composed with `expand`'s signature arm.
///
/// The round-trip `decompile(project(g))` must equal `expand(g)` restricted to
/// the signature plane (`rdf:type→ogit:Function`, `has_function`, `returns_type`,
/// `has_param_type`, `is_const`, `is_static`, `virtually_overrides`). The
/// body-shaping predicates (`is_pure_virtual` / `is_constexpr` / `is_noexcept` /
/// `defines_operator` / `requires_concept`) are deliberately out of the
/// manifest's scope and out of this comparison.
#[must_use]
pub fn decompile(manifests: &[ClassManifest], namespace: &str) -> Vec<Triple> {
    let mut out = Vec::new();
    for manifest in manifests {
        let model_iri = format!("{namespace}:{}", manifest.class);
        for method in &manifest.methods {
            let iri = method_iri(&model_iri, method);
            out.push(Triple::new(
                iri.clone(),
                Predicate::RdfType,
                "ogit:Function",
                Provenance::Structural,
            ));
            out.push(Triple::new(
                model_iri.clone(),
                Predicate::HasFunction,
                iri.clone(),
                Provenance::Structural,
            ));
            if let Some(ret) = &method.ret {
                out.push(Triple::new(
                    iri.clone(),
                    Predicate::ReturnsType,
                    ret.clone(),
                    Provenance::CppExtracted,
                ));
            }
            for (i, param) in method.params.iter().enumerate() {
                out.push(Triple::new(
                    iri.clone(),
                    Predicate::HasParamType,
                    format!("{i}:{param}"),
                    Provenance::CppExtracted,
                ));
            }
            if method.is_const {
                out.push(Triple::new(
                    iri.clone(),
                    Predicate::IsConst,
                    "true",
                    Provenance::CppExtracted,
                ));
            }
            if method.is_static {
                out.push(Triple::new(
                    iri.clone(),
                    Predicate::IsStatic,
                    "true",
                    Provenance::CppExtracted,
                ));
            }
            if let Some(over) = &method.overrides {
                out.push(Triple::new(
                    iri.clone(),
                    Predicate::VirtuallyOverrides,
                    format!("{namespace}:{over}"),
                    Provenance::CppExtracted,
                ));
            }
        }
    }
    out
}

/// Is `t` a signature-plane triple — the subset the manifest round-trips?
#[must_use]
pub fn is_signature_plane(t: &Triple) -> bool {
    matches!(
        t.p.as_str(),
        "has_function"
            | "returns_type"
            | "has_param_type"
            | "is_const"
            | "is_static"
            | "virtually_overrides"
    ) || (t.p == "rdf:type" && t.o == "ogit:Function")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_spo_triplet::{Model, expand};
    use std::collections::BTreeSet;

    /// A C++-plane graph covering ret/params/const/static/override + a
    /// const/non-const overload pair (the cv-aware identity).
    fn fixture() -> ModelGraph {
        let mut rec = Model::new("tesseract::UNICHARSET");
        rec.methods.push(CppMethod {
            name: "unichar_to_id".to_string(),
            return_type: Some("UNICHAR_ID".to_string()),
            param_types: vec!["const char *".to_string()],
            is_const: true,
            ..Default::default()
        });
        rec.methods.push(CppMethod {
            name: "unichar_to_id".to_string(),
            return_type: Some("UNICHAR_ID".to_string()),
            param_types: vec!["const char *".to_string(), "int".to_string()],
            is_const: true,
            ..Default::default()
        });
        rec.methods.push(CppMethod {
            name: "at".to_string(),
            return_type: Some("T &".to_string()),
            param_types: vec!["int".to_string()],
            is_const: false,
            ..Default::default()
        });
        rec.methods.push(CppMethod {
            name: "at".to_string(),
            return_type: Some("const T &".to_string()),
            param_types: vec!["int".to_string()],
            is_const: true,
            ..Default::default()
        });
        rec.methods.push(CppMethod {
            name: "kMax".to_string(),
            return_type: Some("int".to_string()),
            param_types: Vec::new(),
            is_static: true,
            ..Default::default()
        });
        rec.methods.push(CppMethod {
            name: "Recognize".to_string(),
            param_types: vec!["int".to_string()],
            overrides: Some("tesseract::Classify.Recognize(int)".to_string()),
            ..Default::default()
        });
        ModelGraph {
            namespace: "cpp".to_string(),
            models: vec![rec],
        }
    }

    fn spo(triples: &[Triple]) -> BTreeSet<(String, String, String)> {
        triples
            .iter()
            .map(|t| (t.s.clone(), t.p.clone(), t.o.clone()))
            .collect()
    }

    /// The teeth: regenerating the signature plane from the projected manifest
    /// equals `expand`'s signature-plane output for the same graph. This is the
    /// `codegen_spine::roundtrip_eq` pattern over the live harvested triples.
    #[test]
    fn decompile_roundtrips_signature_plane_against_expand() {
        let g = fixture();
        let manifest = project(&g);
        let from_manifest = spo(&decompile(&manifest, &g.namespace));
        let from_expand: BTreeSet<_> = expand(&g)
            .into_iter()
            .filter(is_signature_plane)
            .map(|t| (t.s, t.p, t.o))
            .collect();
        assert_eq!(
            from_manifest, from_expand,
            "manifest signature plane must round-trip against expand"
        );
    }

    /// A dropped method makes the round-trip fail — proof the gate has teeth
    /// (it is not a tautology).
    #[test]
    fn round_trip_detects_a_dropped_method() {
        let g = fixture();
        let mut manifest = project(&g);
        manifest[0].methods.pop(); // drop the override method
        let from_manifest = spo(&decompile(&manifest, &g.namespace));
        let from_expand: BTreeSet<_> = expand(&g)
            .into_iter()
            .filter(is_signature_plane)
            .map(|t| (t.s, t.p, t.o))
            .collect();
        assert_ne!(
            from_manifest, from_expand,
            "dropping a method must break the round-trip"
        );
    }

    #[test]
    fn project_keeps_const_and_nonconst_overloads_distinct() {
        let g = fixture();
        let m = project(&g);
        let ats: Vec<&MethodSig> = m[0].methods.iter().filter(|s| s.name == "at").collect();
        assert_eq!(ats.len(), 2);
        assert!(ats.iter().any(|s| s.is_const));
        assert!(ats.iter().any(|s| !s.is_const));
    }
}
