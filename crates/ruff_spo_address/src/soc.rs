//! `soc` — the "256-cap-is-a-lint" separation-of-concerns check.
//!
//! Promoted from the `medcare_probe` example's §[G] falsifier (`fn main()`)
//! into a **reusable library function** so it can run in `ruff check` / CI
//! instead of by hand (the first step of TD-23 / the `OGAR-SOC` lint).
//!
//! The law: every class whose sibling set overflows the per-tier cascade rank
//! is a DESIGN smell, never a storage limit, and is one (or both) of:
//!
//! 1. **Duplication** — the data members collapse to a representable number of
//!    distinct `field_type`s; mask them by classid into a `ClassView`.
//! 2. **Conflation** — data (`has_field`) and behaviour (`has_function`) are
//!    mixed under one parent; split the concerns.
//!
//! [`law_holds`] is the falsifier: `false` iff some over-cap class is *neither*.

use ruff_spo_triplet::Triple;
use std::collections::{BTreeMap, BTreeSet};

/// Per-tier sibling budget. The cascade rank is a 1-based `u8` with `0` reserved
/// ("no tier here"), so ranks `1..=255` are representable — a level with more
/// than `u8::MAX` (255) siblings overflows (the 256th saturates to rank 255 and
/// collides with the 255th, matching `ruff_spo_address::ranks`). The lint
/// therefore fires when `members > MAX_SIBLINGS_PER_TIER`. (Colloquially the
/// "256-cap": 256 is the byte's cardinality; 255 is the representable count.)
pub const MAX_SIBLINGS_PER_TIER: usize = u8::MAX as usize;

/// A class's field set must fit one `u64` `FieldMask`.
pub const FIELD_MASK_CAP: usize = 64;

/// The verdict for a class whose sibling set exceeds [`MAX_SIBLINGS_PER_TIER`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocVerdict {
    /// Data rows collapse to a representable number of distinct `field_type`s —
    /// mask via a `ClassView`.
    Duplication,
    /// `has_field` data + `has_function` behaviour conflated under one parent — split.
    Conflation,
    /// Both duplication and conflation are present.
    DuplicationAndConflation,
    /// Neither — the law's counterexample (an over-cap set that is provably
    /// neither type-collapsible nor data⊥behaviour-mixed).
    Counterexample,
}

/// One over-cap class and its classification.
#[derive(Debug, Clone)]
pub struct SocFinding {
    /// The class IRI (the `has_field` / `has_function` subject).
    pub class: String,
    /// Total members (`has_field` ∪ `has_function`).
    pub members: usize,
    /// `has_field` members (data).
    pub data: usize,
    /// `has_function` members (behaviour).
    pub funcs: usize,
    /// Distinct `field_type`s among the typed data members.
    pub distinct_field_types: usize,
    /// Typed data rows reclaimable by a masked `ClassView` (`typed_data - distinct`).
    pub duplicate_rows: usize,
    /// The law's classification of this overflow.
    pub verdict: SocVerdict,
}

/// Classify every class whose sibling set exceeds [`MAX_SIBLINGS_PER_TIER`].
///
/// Mirrors the `medcare_probe` §[G] logic, with two corrections over the
/// example: `funcs` is derived from the `has_function` predicate (not the
/// untyped-data complement, which would false-positive on `has_field` members
/// whose type lives only in the IR, e.g. `cpp_field`), and the overflow
/// threshold is `> u8::MAX` siblings (the representable rank count).
#[must_use]
pub fn soc_findings(triples: &[Triple]) -> Vec<SocFinding> {
    let field_type: BTreeMap<&str, &str> = triples
        .iter()
        .filter(|t| t.p == "field_type")
        .map(|t| (t.s.as_str(), t.o.as_str()))
        .collect();

    // Bucket each member with its predicate (true == has_function).
    let mut members_by_class: BTreeMap<&str, Vec<(&str, bool)>> = BTreeMap::new();
    for t in triples {
        let is_fn = t.p == "has_function";
        if is_fn || t.p == "has_field" {
            members_by_class
                .entry(t.s.as_str())
                .or_default()
                .push((t.o.as_str(), is_fn));
        }
    }

    let mut out = Vec::new();
    for (class, members) in &members_by_class {
        if members.len() <= MAX_SIBLINGS_PER_TIER {
            continue;
        }
        let funcs = members.iter().filter(|(_, is_fn)| *is_fn).count();
        let data_members: Vec<&str> = members
            .iter()
            .filter(|(_, is_fn)| !*is_fn)
            .map(|(m, _)| *m)
            .collect();
        let data = data_members.len();
        let distinct: BTreeSet<&str> = data_members
            .iter()
            .filter_map(|m| field_type.get(m).copied())
            .collect();
        // Typed data rows reclaimable by a masked ClassView = typed members minus
        // the distinct types they collapse to.
        let typed = data_members.iter().filter_map(|m| field_type.get(m)).count();
        let duplicate_rows = typed.saturating_sub(distinct.len());
        let is_dup = !distinct.is_empty() && distinct.len() <= MAX_SIBLINGS_PER_TIER;
        let is_conflated = funcs > 0 && data > 0;
        let verdict = match (is_dup, is_conflated) {
            (true, true) => SocVerdict::DuplicationAndConflation,
            (true, false) => SocVerdict::Duplication,
            (false, true) => SocVerdict::Conflation,
            (false, false) => SocVerdict::Counterexample,
        };
        out.push(SocFinding {
            class: (*class).to_string(),
            members: members.len(),
            data,
            funcs,
            distinct_field_types: distinct.len(),
            duplicate_rows,
            verdict,
        });
    }
    out
}

/// Does the corpus uphold the law (no counterexample)?
#[must_use]
pub fn law_holds(triples: &[Triple]) -> bool {
    soc_findings(triples)
        .iter()
        .all(|f| f.verdict != SocVerdict::Counterexample)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str, p: &str, o: &str) -> Triple {
        Triple { s: s.into(), p: p.into(), o: o.into(), f: 1.0, c: 1.0 }
    }

    #[test]
    fn over_cap_pure_data_is_duplication() {
        let mut tr = Vec::new();
        for i in 0..300 {
            let m = format!("C.f{i}");
            tr.push(t("C", "has_field", &m));
            tr.push(t(&m, "field_type", if i % 2 == 0 { "i32" } else { "str" }));
        }
        let f = soc_findings(&tr);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].members, 300);
        assert_eq!(f[0].funcs, 0);
        assert_eq!(f[0].distinct_field_types, 2);
        assert_eq!(f[0].duplicate_rows, 298);
        assert_eq!(f[0].verdict, SocVerdict::Duplication);
        assert!(law_holds(&tr));
    }

    #[test]
    fn untyped_fields_are_not_counted_as_functions() {
        // 300 has_field members, NONE with a field_type triple (type in the IR).
        let mut tr = Vec::new();
        for i in 0..300 {
            tr.push(t("U", "has_field", &format!("U.f{i}")));
        }
        let f = soc_findings(&tr);
        assert_eq!(f[0].funcs, 0, "has_field with no field_type must not be a function");
        assert_eq!(f[0].data, 300);
        // No types resolved and no functions -> not provably dup/conflation.
        assert_eq!(f[0].verdict, SocVerdict::Counterexample);
    }

    #[test]
    fn data_plus_functions_is_duplication_and_conflation() {
        let mut tr = Vec::new();
        for i in 0..200 {
            let m = format!("D.f{i}");
            tr.push(t("D", "has_field", &m));
            tr.push(t(&m, "field_type", "str"));
        }
        for i in 0..100 {
            tr.push(t("D", "has_function", &format!("D.fn{i}")));
        }
        let f = soc_findings(&tr);
        assert_eq!(f[0].members, 300);
        assert_eq!(f[0].funcs, 100);
        assert_eq!(f[0].data, 200);
        assert_eq!(f[0].verdict, SocVerdict::DuplicationAndConflation);
    }

    #[test]
    fn boundary_255_ignored_256_caught() {
        let mk = |n: usize| {
            let mut tr = Vec::new();
            for i in 0..n {
                let m = format!("B.f{i}");
                tr.push(t("B", "has_field", &m));
                tr.push(t(&m, "field_type", "str"));
            }
            tr
        };
        assert!(soc_findings(&mk(255)).is_empty(), "255 siblings are representable");
        assert_eq!(soc_findings(&mk(256)).len(), 1, "256 overflows the u8 rank");
    }

    #[test]
    fn under_cap_is_ignored() {
        let tr = vec![t("E", "has_field", "E.a"), t("E.a", "field_type", "i32")];
        assert!(soc_findings(&tr).is_empty());
        assert!(law_holds(&tr));
    }
}
