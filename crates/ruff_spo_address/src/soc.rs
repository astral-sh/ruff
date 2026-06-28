//! `soc` — the "256-cap-is-a-lint" separation-of-concerns check.
//!
//! Promoted from the `medcare_probe` example's §[G] falsifier (`fn main()`)
//! into a **reusable library function** so it can run in `ruff check` / CI
//! instead of by hand (the first step of TD-23 / the `OGAR-SOC` lint).
//!
//! The law: every class whose member set overflows the per-tier `u8` cascade
//! rank (`> 256` siblings) is a DESIGN smell, never a storage limit, and is one
//! (or both) of:
//!
//! 1. **Duplication** — the data members collapse to `<= 256` distinct
//!    `field_type`s; mask them by classid into a `ClassView` (`render_rows <= 64`).
//! 2. **Conflation** — data (`has_field`) and behaviour (`has_function`) are
//!    mixed under one parent; split the concerns.
//!
//! [`law_holds`] is the falsifier: it returns `false` iff some over-cap class is
//! *neither* (a `> 256`, genuinely-distinct, well-separated sibling set).

use ruff_spo_triplet::Triple;
use std::collections::{BTreeMap, BTreeSet};

/// Per-tier sibling cap: a level with more than this many members overflows the
/// 8-bit cascade rank — the smell the law detects.
pub const TIER_CAP: usize = 256;

/// A class's field set must fit one `u64` `FieldMask`.
pub const FIELD_MASK_CAP: usize = 64;

/// The verdict for a class whose member set exceeds [`TIER_CAP`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocVerdict {
    /// Data rows collapse to `<= TIER_CAP` distinct `field_type`s — mask via a `ClassView`.
    Duplication,
    /// `has_field` data + `has_function` behaviour conflated under one parent — split.
    Conflation,
    /// Both duplication and conflation are present.
    DuplicationAndConflation,
    /// Neither — the law's counterexample (`> TIER_CAP` distinct, well-separated siblings).
    Counterexample,
}

/// One over-cap class and its classification.
#[derive(Debug, Clone)]
pub struct SocFinding {
    /// The class IRI (the `has_field` / `has_function` subject).
    pub class: String,
    /// Total members (`has_field` ∪ `has_function`).
    pub members: usize,
    /// Members carrying a `field_type` (data).
    pub data: usize,
    /// Members without a `field_type` (`has_function`).
    pub funcs: usize,
    /// Distinct `field_type`s among the data members.
    pub distinct_field_types: usize,
    /// Data rows reclaimable by a masked `ClassView` (`data - distinct`).
    pub duplicate_rows: usize,
    /// The law's classification of this overflow.
    pub verdict: SocVerdict,
}

/// Classify every class whose member set exceeds [`TIER_CAP`].
///
/// Mirrors the `medcare_probe` §[G] logic exactly: groups `has_field` /
/// `has_function` members by class, and for each over-cap class derives the
/// duplication (type-collapse) and conflation (data⊥behaviour) verdict.
#[must_use]
pub fn soc_findings(triples: &[Triple]) -> Vec<SocFinding> {
    let field_type: BTreeMap<&str, &str> = triples
        .iter()
        .filter(|t| t.p == "field_type")
        .map(|t| (t.s.as_str(), t.o.as_str()))
        .collect();

    let mut members_by_class: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for t in triples {
        if t.p == "has_field" || t.p == "has_function" {
            members_by_class
                .entry(t.s.as_str())
                .or_default()
                .push(t.o.as_str());
        }
    }

    let mut out = Vec::new();
    for (class, members) in &members_by_class {
        if members.len() <= TIER_CAP {
            continue;
        }
        let data: Vec<&str> = members
            .iter()
            .copied()
            .filter(|m| field_type.contains_key(m))
            .collect();
        let funcs = members.len() - data.len();
        let distinct: BTreeSet<&str> =
            data.iter().filter_map(|m| field_type.get(m).copied()).collect();
        let duplicate_rows = data.len().saturating_sub(distinct.len());
        let is_dup = !distinct.is_empty() && distinct.len() <= TIER_CAP;
        let is_conflated = funcs > 0 && !data.is_empty();
        let verdict = match (is_dup, is_conflated) {
            (true, true) => SocVerdict::DuplicationAndConflation,
            (true, false) => SocVerdict::Duplication,
            (false, true) => SocVerdict::Conflation,
            (false, false) => SocVerdict::Counterexample,
        };
        out.push(SocFinding {
            class: (*class).to_string(),
            members: members.len(),
            data: data.len(),
            funcs,
            distinct_field_types: distinct.len(),
            duplicate_rows,
            verdict,
        });
    }
    out
}

/// Does the corpus uphold the 256-cap-is-a-lint law (no counterexample)?
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
        Triple {
            s: s.into(),
            p: p.into(),
            o: o.into(),
            f: 1.0,
            c: 1.0,
        }
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
        assert_eq!(f[0].distinct_field_types, 2);
        assert_eq!(f[0].duplicate_rows, 298);
        assert_eq!(f[0].verdict, SocVerdict::Duplication);
        assert!(law_holds(&tr));
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
            let m = format!("D.fn{i}");
            tr.push(t("D", "has_function", &m));
        }
        let f = soc_findings(&tr);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].members, 300);
        assert_eq!(f[0].funcs, 100);
        assert_eq!(f[0].verdict, SocVerdict::DuplicationAndConflation);
    }

    #[test]
    fn flat_function_root_is_a_counterexample() {
        // 300 pure has_function siblings, no data → neither dup nor conflation.
        let mut tr = Vec::new();
        for i in 0..300 {
            tr.push(t("ogit:Function", "has_function", &format!("fn{i}")));
        }
        let f = soc_findings(&tr);
        assert_eq!(f[0].verdict, SocVerdict::Counterexample);
        assert!(!law_holds(&tr));
    }

    #[test]
    fn under_cap_is_ignored() {
        let tr = vec![t("E", "has_field", "E.a"), t("E.a", "field_type", "i32")];
        assert!(soc_findings(&tr).is_empty());
        assert!(law_holds(&tr));
    }
}
