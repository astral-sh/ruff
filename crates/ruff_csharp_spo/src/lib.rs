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
//! honest, and [`load`] is the gate — [`from_ndjson`] rejects any predicate
//! outside the closed [`Predicate`] vocabulary at parse time, so a harvester
//! bug surfaces as a hard [`ParseError`] (line + offending predicate) rather
//! than silent drift into the store.

pub use ruff_spo_triplet::{ParseError, Predicate, Triple, from_ndjson};

/// The IRI namespace prefix every C# subject/object carries, e.g.
/// `medcare:Patient` / `medcare:Patient.kdnr`. Mirrors `ruff_cpp_spo`'s
/// `cpp:` and the Odoo/Rails `odoo:` / `openproject:` prefixes.
pub const NAMESPACE: &str = "medcare";

/// Load harvester ndjson into triples, validating every predicate against
/// the closed [`Predicate`] vocabulary.
///
/// A thin wrapper over [`from_ndjson`] kept so callers depend on this
/// frontend's surface rather than reaching through to `ruff_spo_triplet`.
/// The validation *is* the load: `from_ndjson` rejects any non-empty line
/// that is not a well-formed [`Triple`] **and** any line whose predicate is
/// outside the closed vocabulary. An out-of-vocab predicate is a harvester
/// bug (the .NET tool emitted a string no frontend agreed on), and it
/// surfaces here as a hard [`ParseError`] naming the line and predicate —
/// never as a silently-stored triple. So a clean `Ok(_)` is itself the
/// schema guarantee; there is no separate post-load check to run.
///
/// # Errors
///
/// Returns [`ParseError`] if any non-empty line is not a valid [`Triple`],
/// or carries a predicate outside the closed [`Predicate`] vocabulary.
pub fn load(ndjson: &str) -> Result<Vec<Triple>, ParseError> {
    from_ndjson(ndjson)
}

#[cfg(test)]
mod tests {
    use super::load;

    /// The shape the Roslyn harvester emits for one MedCare model. This
    /// fixture exercises *every* predicate `harvester/Program.cs` can emit —
    /// `rdf:type`, `inherits_from`, `has_field`, `field_type`, `has_function`,
    /// and `is_static` — so a clean load is the standing proof that the full
    /// emitted set stays inside the closed vocabulary. If the harvester grows
    /// a new predicate, it must be added to [`super::Predicate`] first, or
    /// this load fails.
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
            r#"{"s":"medcare:Patient.Save","p":"is_static","o":"true","f":1.0,"c":0.9}"#,
            "\n",
        );
        let triples = load(ndjson).expect("every harvester predicate is in the closed vocab");
        assert_eq!(triples.len(), 6);
        assert_eq!(triples[0].s, "medcare:Patient");
    }

    /// A predicate the .NET tool must never emit. `load` (via `from_ndjson`)
    /// rejects it at parse time, naming the offending predicate, so the
    /// schema break is loud — a hard error, never a silently-stored triple.
    #[test]
    fn rejects_out_of_vocab_predicate() {
        let ndjson = r#"{"s":"medcare:X","p":"totally_made_up","o":"medcare:Y","f":1.0,"c":0.9}"#;
        let err = load(ndjson).expect_err("out-of-vocab predicate must fail the load");
        assert_eq!(err.line, 1);
        assert!(
            err.message.contains("totally_made_up"),
            "the error must name the offending predicate, got: {}",
            err.message
        );
    }
}
