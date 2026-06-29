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

/// One `u64` `FieldMask` **bucket** holds 64 field positions — the unit the
/// Redmine-ERB ClassView bitmask iterates
/// (`OGAR/docs/CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK.md`).
pub const FIELD_MASK_BUCKET_BITS: usize = 64;

/// A clean `ClassView` chains up to 4 buckets — the **quadruplet** `[u64; 4]` —
/// so its distinct-field set fits 256 positions (operator 2026-06-29: expand the
/// single-`u64` cap 64 → 256; an Odoo model with ~109 fields is then clean in
/// one quadruplet ClassView, no split needed).
pub const FIELD_MASK_MAX_BUCKETS: usize = 4;

/// A class's distinct-field set must fit a chained-bucket `FieldMask`:
/// `FIELD_MASK_BUCKET_BITS * FIELD_MASK_MAX_BUCKETS` = **256**. At/below this it
/// is maskable by one ClassView; *beyond* it the class is a **god object** — the
/// SoC signal to **split** (chain a second ClassView), never to widen a single
/// mask past the quadruplet.
pub const FIELD_MASK_CAP: usize = FIELD_MASK_BUCKET_BITS * FIELD_MASK_MAX_BUCKETS;

/// How many `u64` buckets a field set of `n` distinct positions needs — the
/// **clean separation overflow automation**: each run of 64 positions chains the
/// next bucket. `<= FIELD_MASK_MAX_BUCKETS` (4) fits one ClassView (the
/// quadruplet); more buckets is the god-object SoC signal to split into
/// sub-ClassViews, each its own ≤4-bucket clean concern.
#[must_use]
pub const fn field_mask_buckets(n: usize) -> usize {
    n.div_ceil(FIELD_MASK_BUCKET_BITS)
}

/// The verdict for a class whose sibling set exceeds [`MAX_SIBLINGS_PER_TIER`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SocVerdict {
    /// All data members are typed and collapse to `<= FIELD_MASK_CAP` (256)
    /// distinct `field_type`s — maskable by one `ClassView` (a chained-bucket
    /// `FieldMask`, up to the `[u64; 4]` quadruplet).
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
        let typed = data_members
            .iter()
            .filter_map(|m| field_type.get(m))
            .count();
        let duplicate_rows = typed.saturating_sub(distinct.len());
        // Duplication ⇒ the data collapses to a ClassView-maskable view: every
        // data member is typed (untyped siblings are not proven collapsible) AND
        // the distinct types fit a chained-bucket FieldMask (≤ 256, the [u64; 4]
        // quadruplet — not the 255 tier rank — is the real collapse target).
        let is_dup = data > 0 && typed == data && distinct.len() <= FIELD_MASK_CAP;
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
        assert_eq!(
            f[0].funcs, 0,
            "has_field with no field_type must not be a function"
        );
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
        assert!(
            soc_findings(&mk(255)).is_empty(),
            "255 siblings are representable"
        );
        assert_eq!(soc_findings(&mk(256)).len(), 1, "256 overflows the u8 rank");
    }

    #[test]
    fn wide_distinct_types_exceed_field_mask_is_counterexample() {
        // 300 typed fields, every one a distinct type → cannot collapse even into
        // the [u64; 4] quadruplet FieldMask (300 > 256 distinct), so NOT maskable
        // duplication — a genuine god object.
        let mut tr = Vec::new();
        for i in 0..300 {
            let m = format!("W.f{i}");
            tr.push(t("W", "has_field", &m));
            tr.push(t(&m, "field_type", &format!("T{i}")));
        }
        let f = soc_findings(&tr);
        assert_eq!(f[0].distinct_field_types, 300);
        assert!(f[0].distinct_field_types > FIELD_MASK_CAP);
        assert_eq!(f[0].verdict, SocVerdict::Counterexample);
        assert!(!law_holds(&tr));
    }

    #[test]
    fn field_mask_buckets_chain_at_64() {
        // Clean separation overflow automation: each run of 64 chains the next bucket.
        assert_eq!(field_mask_buckets(0), 0);
        assert_eq!(field_mask_buckets(1), 1);
        assert_eq!(field_mask_buckets(64), 1);
        assert_eq!(field_mask_buckets(65), 2);
        assert_eq!(field_mask_buckets(109), 2); // the Odoo case: 2 buckets, still one ClassView
        assert_eq!(field_mask_buckets(192), 3);
        assert_eq!(field_mask_buckets(193), 4);
        assert_eq!(field_mask_buckets(256), 4); // the full quadruplet
        assert_eq!(field_mask_buckets(257), 5); // god object — beyond the quadruplet
        // FIELD_MASK_CAP is exactly the 4-bucket quadruplet (the 64 → 256 expansion).
        assert_eq!(FIELD_MASK_BUCKET_BITS, 64);
        assert_eq!(FIELD_MASK_MAX_BUCKETS, 4);
        assert_eq!(FIELD_MASK_CAP, 256);
        assert_eq!(field_mask_buckets(FIELD_MASK_CAP), FIELD_MASK_MAX_BUCKETS);
    }

    #[test]
    fn odoo_109_distinct_fields_fit_a_quadruplet_classview() {
        // An Odoo-shaped over-cap class: >255 members so the lint fires, with 109
        // DISTINCT field types — too wide for the old single-u64 cap (64) but clean
        // in a 2-bucket quadruplet ClassView (109 <= 256). The epiphany, tested:
        // expanding the cap 64 → 256 turns this from a Counterexample into
        // Duplication — "if odoo has 109 in classview and it's clean we're fine".
        let mut tr = Vec::new();
        for i in 0..300 {
            let m = format!("account_move.f{i}");
            tr.push(t("account_move", "has_field", &m));
            tr.push(t(&m, "field_type", &format!("T{}", i % 109))); // 109 distinct types
        }
        let f = soc_findings(&tr);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].distinct_field_types, 109);
        assert!(f[0].distinct_field_types <= FIELD_MASK_CAP);
        assert_eq!(
            field_mask_buckets(f[0].distinct_field_types),
            2,
            "109 fields = 2 buckets"
        );
        assert_eq!(
            f[0].verdict,
            SocVerdict::Duplication,
            "clean in a quadruplet ClassView, not a god object"
        );
        assert!(law_holds(&tr));
    }

    #[test]
    fn quadruplet_boundary_256_clean_257_is_god_object() {
        let mk = |distinct: usize| {
            let mut tr = Vec::new();
            for i in 0..300 {
                let m = format!("B.f{i}");
                tr.push(t("B", "has_field", &m));
                tr.push(t(&m, "field_type", &format!("T{}", i % distinct)));
            }
            tr
        };
        // 256 distinct = exactly the quadruplet → Duplication (clean, 4 buckets).
        let f256 = soc_findings(&mk(256));
        assert_eq!(f256[0].distinct_field_types, 256);
        assert_eq!(field_mask_buckets(256), FIELD_MASK_MAX_BUCKETS);
        assert_eq!(f256[0].verdict, SocVerdict::Duplication);
        // 257 distinct → overflows the quadruplet → god-object Counterexample
        // (the SoC signal: split into chained sub-ClassViews, don't widen).
        let f257 = soc_findings(&mk(257));
        assert_eq!(f257[0].distinct_field_types, 257);
        assert!(f257[0].distinct_field_types > FIELD_MASK_CAP);
        assert_eq!(f257[0].verdict, SocVerdict::Counterexample);
    }

    #[test]
    fn untyped_data_blocks_duplication_verdict() {
        // 256 has_field: 1 typed + 255 untyped → cannot approve duplication on
        // the strength of a single resolved type.
        let mut tr = vec![
            t("M", "has_field", "M.typed"),
            t("M.typed", "field_type", "i32"),
        ];
        for i in 0..255 {
            tr.push(t("M", "has_field", &format!("M.u{i}")));
        }
        let f = soc_findings(&tr);
        assert_eq!(f[0].data, 256);
        assert_ne!(f[0].verdict, SocVerdict::Duplication);
        assert_eq!(f[0].verdict, SocVerdict::Counterexample);
    }

    #[test]
    fn under_cap_is_ignored() {
        let tr = vec![t("E", "has_field", "E.a"), t("E.a", "field_type", "i32")];
        assert!(soc_findings(&tr).is_empty());
        assert!(law_holds(&tr));
    }
}
