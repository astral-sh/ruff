//! The triple itself + its closed predicate / entity-kind / provenance
//! vocabularies.
//!
//! These four types are the entire ontological surface. Everything a
//! language frontend produces collapses into a `Vec<Triple>` whose `p`
//! field is one of [`Predicate`], whose `rdf:type` objects are one of
//! [`EntityKind`], and whose `(f, c)` truth comes from a [`Provenance`]
//! tier. Keeping these closed is what lets the Python (Odoo) and Ruby
//! (Rails) frontends emit byte-identical graphs.

use serde::{Deserialize, Serialize};

/// One SPO triple with NARS truth `(frequency, confidence)`.
///
/// `(s, p, o)` is the identity. `(f, c)` carries provenance strength:
/// structural facts are certain, decorator/body-authoritative facts are
/// strong, body-inferred facts are weaker. The downstream store
/// (`lance_graph::graph::spo`) gates queries by NARS expectation, so the
/// truth tier is load-bearing, not decorative.
///
/// This mirrors `lance_graph::graph::spo::odoo_ontology::OntologyTriple`
/// field-for-field so the ndjson this crate writes loads into that store
/// with no transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Triple {
    /// Subject IRI, e.g. `odoo:account_move.amount_total`.
    pub s: String,
    /// Predicate IRI, e.g. `depends_on`.
    pub p: String,
    /// Object IRI, e.g. `odoo:account_move.line_ids.balance`.
    pub o: String,
    /// NARS frequency.
    pub f: f32,
    /// NARS confidence.
    pub c: f32,
}

impl Triple {
    /// Construct a triple from typed parts + a provenance tier.
    #[must_use]
    pub fn new(s: impl Into<String>, p: Predicate, o: impl Into<String>, prov: Provenance) -> Self {
        let (f, c) = prov.truth();
        Self {
            s: s.into(),
            p: p.as_str().to_string(),
            o: o.into(),
            f,
            c,
        }
    }

    /// The identity key — what de-duplication and round-trip equality
    /// compare. Truth values are deliberately excluded.
    #[must_use]
    pub fn key(&self) -> (&str, &str, &str) {
        (&self.s, &self.p, &self.o)
    }
}

/// The closed predicate vocabulary.
///
/// Adding a predicate is a deliberate ontology change: a new variant here,
/// a new arm in [`Predicate::as_str`] / [`Predicate::from_str`], and a
/// decision about which [`Provenance`] tier it carries. Frontends MUST NOT
/// emit raw predicate strings — they go through this enum so the Python and
/// Ruby graphs cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Predicate {
    /// `(subject, rdf:type, EntityKind)` — structural classification.
    RdfType,
    /// `(model, has_function, model.fn)` — a model owns a function.
    HasFunction,
    /// `(model.field, emitted_by, model.fn)` — the function writes the field.
    EmittedBy,
    /// `(model.field, depends_on, model.dep)` — declared compute dependency.
    DependsOn,
    /// `(model.fn, reads_field, model.field)` — body reads the field.
    ReadsField,
    /// `(model.fn, raises, exc:Type)` — body raises the exception.
    Raises,
    /// `(model.fn, traverses_relation, model.rel)` — body walks the relation.
    TraversesRelation,
}

impl Predicate {
    /// The on-the-wire predicate string. Stable; never reformat.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RdfType => "rdf:type",
            Self::HasFunction => "has_function",
            Self::EmittedBy => "emitted_by",
            Self::DependsOn => "depends_on",
            Self::ReadsField => "reads_field",
            Self::Raises => "raises",
            Self::TraversesRelation => "traverses_relation",
        }
    }

    /// Parse a predicate string back to the enum. `None` for unknown
    /// predicates — callers should treat that as a hard schema error
    /// (the vocabulary is closed).
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "rdf:type" => Self::RdfType,
            "has_function" => Self::HasFunction,
            "emitted_by" => Self::EmittedBy,
            "depends_on" => Self::DependsOn,
            "reads_field" => Self::ReadsField,
            "raises" => Self::Raises,
            "traverses_relation" => Self::TraversesRelation,
            _ => return None,
        })
    }

    /// The default provenance tier for this predicate, per the Odoo
    /// extraction calibration:
    ///
    /// - structural (`rdf:type`, `has_function`) → [`Provenance::Structural`]
    /// - declared / body-authoritative (`emitted_by`, `depends_on`,
    ///   `raises`) → [`Provenance::Authoritative`]
    /// - body-inferred (`reads_field`, `traverses_relation`) →
    ///   [`Provenance::Inferred`]
    ///
    /// Frontends may override per-edge (e.g. a Rails frontend that proves a
    /// read statically can promote `reads_field` to Authoritative), but the
    /// default keeps cross-language graphs comparable.
    #[must_use]
    pub const fn default_provenance(self) -> Provenance {
        match self {
            Self::RdfType | Self::HasFunction => Provenance::Structural,
            Self::EmittedBy | Self::DependsOn | Self::Raises => Provenance::Authoritative,
            Self::ReadsField | Self::TraversesRelation => Provenance::Inferred,
        }
    }
}

/// The `rdf:type` object vocabulary — the Foundry-shape entity classes.
///
/// Object Type = an entity/model (Odoo model, Rails ActiveRecord class).
/// Property = a field/attribute. Function = a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    /// A model / entity (e.g. `account.move`, `WorkPackage`).
    ObjectType,
    /// A field / attribute (e.g. `amount_total`, `subject`).
    Property,
    /// A method / function (e.g. `_compute_amount`, `compute_total_hours`).
    Function,
}

impl EntityKind {
    /// The OGIT-namespaced IRI used as the `rdf:type` object.
    ///
    /// Uses the canonical OGIT vocabulary prefix `ogit:` — NOT a
    /// project-local namespace. The OGIT base is
    /// `http://www.purl.org/ogit/`; consumers resolve `ogit:` against it.
    #[must_use]
    pub const fn iri(self) -> &'static str {
        match self {
            Self::ObjectType => "ogit:ObjectType",
            Self::Property => "ogit:Property",
            Self::Function => "ogit:Function",
        }
    }
}

/// Provenance tier → NARS `(frequency, confidence)`.
///
/// The three tiers are the calibration from the Odoo harvest:
/// structural facts are certain; decorator/declared/body-authoritative
/// facts are strong; purely body-inferred facts are weaker so the
/// `TruthGate` can filter them out under a strict expectation threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// Structural facts that are true by construction — `(1.0, 1.0)`.
    /// e.g. "this name is a model", "this model owns this method".
    Structural,
    /// Declared or directly-observed-in-body facts — `(0.95, 0.90)`.
    /// e.g. an `@api.depends(...)` argument, a `raise` statement, the
    /// field a compute method assigns to.
    Authoritative,
    /// Heuristically inferred from body shape — `(0.85, 0.75)`.
    /// e.g. a field name that appears as an attribute read, a relation
    /// walked in a `for r in self.<rel>` loop.
    Inferred,
}

impl Provenance {
    /// The NARS `(frequency, confidence)` pair for this tier.
    #[must_use]
    pub const fn truth(self) -> (f32, f32) {
        match self {
            Self::Structural => (1.0, 1.0),
            Self::Authoritative => (0.95, 0.90),
            Self::Inferred => (0.85, 0.75),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_string_round_trips() {
        for p in [
            Predicate::RdfType,
            Predicate::HasFunction,
            Predicate::EmittedBy,
            Predicate::DependsOn,
            Predicate::ReadsField,
            Predicate::Raises,
            Predicate::TraversesRelation,
        ] {
            assert_eq!(Predicate::from_str(p.as_str()), Some(p));
        }
        assert_eq!(Predicate::from_str("not_a_predicate"), None);
    }

    #[test]
    fn provenance_truth_tiers_match_odoo_calibration() {
        assert_eq!(Provenance::Structural.truth(), (1.0, 1.0));
        assert_eq!(Provenance::Authoritative.truth(), (0.95, 0.90));
        assert_eq!(Provenance::Inferred.truth(), (0.85, 0.75));
    }

    #[test]
    fn default_provenance_matches_predicate_class() {
        assert_eq!(
            Predicate::RdfType.default_provenance(),
            Provenance::Structural
        );
        assert_eq!(
            Predicate::DependsOn.default_provenance(),
            Provenance::Authoritative
        );
        assert_eq!(
            Predicate::ReadsField.default_provenance(),
            Provenance::Inferred
        );
    }

    #[test]
    fn triple_new_carries_provenance_truth() {
        let t = Triple::new(
            "odoo:m.f",
            Predicate::EmittedBy,
            "odoo:m._fn",
            Provenance::Authoritative,
        );
        assert_eq!(t.p, "emitted_by");
        assert_eq!((t.f, t.c), (0.95, 0.90));
        assert_eq!(t.key(), ("odoo:m.f", "emitted_by", "odoo:m._fn"));
    }

    #[test]
    fn entity_kind_uses_canonical_ogit_prefix() {
        assert_eq!(EntityKind::ObjectType.iri(), "ogit:ObjectType");
        assert_eq!(EntityKind::Property.iri(), "ogit:Property");
        assert_eq!(EntityKind::Function.iri(), "ogit:Function");
    }
}
