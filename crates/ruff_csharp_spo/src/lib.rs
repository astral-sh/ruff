//! C# (Roslyn) machine-plane frontend for [`ruff_spo_triplet`].
//!
//! The actual parse runs in `harvester/` — a .NET console tool built on
//! Roslyn (`Microsoft.CodeAnalysis.CSharp`) that walks a C# corpus
//! (`MedCare` first) and writes one SPO [`Triple`] per line of ndjson, in the
//! exact shape the Python/Odoo, Ruby/Rails, and C++/Tesseract frontends
//! emit. Roslyn is .NET-only, so — unlike `ruff_cpp_spo`, which drives
//! libclang from Rust — the parse step is an out-of-process tool. The seam
//! between the two halves is the ndjson contract; this crate loads it and
//! validates every predicate against the closed [`Predicate`] vocabulary so
//! a harvester bug surfaces as a hard schema error instead of silent drift.
//!
//! ```text
//! MedCare (C#) --Roslyn harvester--> triples.ndjson --load()-->
//!     Vec<Triple> --(ruff_spo_triplet::reassemble / SPO store)--> ClassView
//! ```
//!
//! Why an out-of-process tool rather than a Rust `walk_tu` like
//! `ruff_cpp_spo`: there is no Rust-callable Roslyn. Roslyn *is* the C#
//! compiler, so it resolves base types, overrides, and member types
//! authoritatively — far better than reparsing C# with a hand-rolled
//! grammar. The cost is a process boundary; the ndjson contract keeps it
//! honest, and this crate's [`unknown_predicates`] check is the gate.

pub use ruff_spo_triplet::{ParseError, Predicate, Triple, from_ndjson};

/// The IRI namespace prefix every C# subject/object carries, e.g.
/// `medcare:Patient` / `medcare:Patient.kdnr`. Mirrors `ruff_cpp_spo`'s
/// `cpp:` and the Odoo/Rails `odoo:` / `openproject:` prefixes.
pub const NAMESPACE: &str = "medcare";

/// Load harvester ndjson into triples.
///
/// A thin wrapper over [`from_ndjson`] kept so callers depend on this
/// frontend's surface rather than reaching through to `ruff_spo_triplet`.
/// Validate the result with [`unknown_predicates`] before handing it to the
/// store — a line can parse as a well-formed [`Triple`] yet still carry an
/// out-of-vocab predicate, which is a harvester bug, not a wire-format one.
///
/// # Errors
///
/// Returns [`ParseError`] if any non-empty line is not a valid [`Triple`].
pub fn load(ndjson: &str) -> Result<Vec<Triple>, ParseError> {
    from_ndjson(ndjson)
}

/// The distinct predicate strings in `triples` that are **not** in the
/// closed [`Predicate`] vocabulary. An empty result means every triple is
/// schema-valid; a non-empty one names exactly the harvester predicates that
/// need either a fix in the .NET tool or a deliberate addition to
/// [`Predicate`].
#[must_use]
pub fn unknown_predicates(triples: &[Triple]) -> Vec<String> {
    let mut unknown: Vec<String> = triples
        .iter()
        .filter(|t| Predicate::from_str(t.p.as_str()).is_none())
        .map(|t| t.p.clone())
        .collect();
    unknown.sort_unstable();
    unknown.dedup();
    unknown
}

#[cfg(test)]
mod tests {
    use super::{load, unknown_predicates};

    /// The shape the Roslyn harvester emits for one MedCare model — every
    /// predicate here is in the closed vocabulary, so the round-trip loads
    /// clean and validates clean.
    #[test]
    fn loads_and_validates_harvester_ndjson() {
        let ndjson = concat!(
            r#"{"s":"medcare:Patient","p":"rdf:type","o":"ogit:ObjectType","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient","p":"inherits_from","o":"medcare:DbBase","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient","p":"has_field","o":"medcare:Patient.kdnr","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient.kdnr","p":"field_type","o":"string","f":1.0,"c":0.9}"#,
            "\n",
            r#"{"s":"medcare:Patient","p":"has_function","o":"medcare:Patient.Save","f":1.0,"c":0.9}"#,
            "\n",
        );
        let triples = load(ndjson).expect("valid ndjson");
        assert_eq!(triples.len(), 5);
        assert_eq!(triples[0].s, "medcare:Patient");
        assert!(
            unknown_predicates(&triples).is_empty(),
            "every harvester predicate must be in the closed vocab"
        );
    }

    /// A predicate the .NET tool must never emit — the validator names it so
    /// the schema break is loud, not silent.
    #[test]
    fn flags_out_of_vocab_predicate() {
        let ndjson = r#"{"s":"medcare:X","p":"totally_made_up","o":"medcare:Y","f":1.0,"c":0.9}"#;
        let triples = load(ndjson).expect("parses as a well-formed triple");
        assert_eq!(unknown_predicates(&triples), vec!["totally_made_up".to_owned()]);
    }
}
