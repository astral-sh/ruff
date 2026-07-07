//! The recipe centroid classifier — correlates a method's fact-set to the
//! nearest declarative recipe, per
//! `.claude/knowledge/fuzzy-recipe-codebook.md` §3.
//!
//! The centroids are defined ONLY on the four [`crate::Function`] fact sets
//! (`writes` / `reads` / `raises` / `calls`) plus the J1 guard fact
//! (`guarded_writes`) — no language tokens — so the identical ladder
//! classifies Ruby hooks, Odoo `_compute_*` methods, C# handlers, and C++
//! methods alike. A frontend "adds the arm" purely by populating those
//! `Vec`s from its own AST; this module runs unchanged on the result.

use crate::ir::Function;

/// The nearest declarative recipe a method body correlates to, or one of
/// the two irreducible "essential" kinds that stay hand-ported.
///
/// See [`classify`] for the first-match-wins ladder that assigns this, and
/// [`is_recoverable`] for which centroids collapse to an order-free recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecipeCentroid {
    /// `calls ∧ raises` — a manual transaction (rollback/raise mid-dispatch).
    /// No recipe expresses this: essential, order-dependent.
    Compensate,
    /// `calls ∧ ¬raises` — a relation/mutator dispatch with no abort.
    /// Recipe: a `dependent:` cascade / association callback.
    Cascade,
    /// `raises ∧ ¬writes ∧ ¬calls` — abort-only. Recipe: a validation.
    Guard,
    /// `writes ∧ raises` (and not already caught by [`Self::Compensate`],
    /// i.e. no `calls`) — a partial write followed by an escape. Essential:
    /// the write already happened before the abort, so the order matters.
    WriteRaise,
    /// `writes` and every written field is also in `guarded_writes` (the J1
    /// fact: the write is guarded by a blank/nil test on that same field).
    /// Recipe: a schema default / `attribute default:`.
    Default,
    /// `writes` and at least one written field is never read elsewhere in
    /// the body — a fresh write. Recipe: an `emitted_by` compute edge.
    Compute,
    /// `writes` and every written field is also read (unguarded) — an
    /// idempotent self-transform. Recipe: `normalizes`.
    Normalize,
    /// Reads only — no writes, raises, or calls. Excluded from the DO arm
    /// (query-shape, not command-shape).
    Observe,
    /// No facts at all. Unresolved — a scope boundary, not a recipe.
    Empty,
}

/// Classify a function's fact-set into its nearest recipe centroid.
///
/// First-match-wins, in exactly the order of
/// `.claude/knowledge/fuzzy-recipe-codebook.md` §3. The J1 guard-write fact
/// (`guarded_writes`) splits [`RecipeCentroid::Default`] out **before**
/// [`RecipeCentroid::Compute`] / [`RecipeCentroid::Normalize`] are checked,
/// per the J1 finding recorded there.
#[must_use]
pub fn classify(f: &Function) -> RecipeCentroid {
    let writes = !f.writes.is_empty();
    let reads = !f.reads.is_empty();
    let raises = !f.raises.is_empty();
    let calls = !f.calls.is_empty();

    if calls && raises {
        return RecipeCentroid::Compensate;
    }
    if calls && !raises {
        return RecipeCentroid::Cascade;
    }
    if raises && !writes && !calls {
        return RecipeCentroid::Guard;
    }
    if writes && raises {
        return RecipeCentroid::WriteRaise;
    }
    if writes && is_subset(&f.writes, &f.guarded_writes) {
        return RecipeCentroid::Default;
    }
    if writes && !is_subset(&f.writes, &f.reads) {
        return RecipeCentroid::Compute;
    }
    if writes && is_subset(&f.writes, &f.reads) {
        return RecipeCentroid::Normalize;
    }
    if !writes && !raises && !calls && reads {
        return RecipeCentroid::Observe;
    }
    RecipeCentroid::Empty
}

/// Whether a centroid collapses to an order-free declarative recipe.
///
/// `true` for the five recoverable centroids (`Compute` / `Default` /
/// `Normalize` / `Cascade` / `Guard`). `false` for the two essential,
/// order-dependent centroids (`Compensate` / `WriteRaise`) AND for the two
/// excluded centroids (`Observe` / `Empty`) — callers that need the
/// essential/excluded distinction should match on [`RecipeCentroid`]
/// directly rather than relying on the negation of this predicate.
#[must_use]
pub fn is_recoverable(c: RecipeCentroid) -> bool {
    matches!(
        c,
        RecipeCentroid::Compute
            | RecipeCentroid::Default
            | RecipeCentroid::Normalize
            | RecipeCentroid::Cascade
            | RecipeCentroid::Guard
    )
}

/// `true` when every element of `a` is also present in `b`.
fn is_subset(a: &[String], b: &[String]) -> bool {
    a.iter().all(|item| b.contains(item))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a bare, named [`Function`] — every fact-set field defaults
    /// empty via `..Function::default()`, forward-compatible with any new
    /// additive field.
    fn function(name: &str) -> Function {
        Function {
            name: name.to_string(),
            ..Function::default()
        }
    }

    /// `SetDefaults` shape from `recipe_shapes.cs`: `this.Name ??= "unknown"`.
    /// `writes == guarded_writes == {Name}`, everything else empty.
    #[test]
    fn set_defaults_via_null_coalesce_is_default() {
        let mut f = function("Widget.SetDefaults");
        f.writes = vec!["Name".to_string()];
        f.guarded_writes = vec!["Name".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Default);
        assert!(is_recoverable(classify(&f)));
    }

    /// `Backfill` shape: `if (this.Name == null) { this.Name = "backfilled"; }`.
    /// Same fact-set shape as the `??=` spelling — same centroid.
    #[test]
    fn backfill_via_null_check_is_default() {
        let mut f = function("Widget.Backfill");
        f.writes = vec!["Name".to_string()];
        f.guarded_writes = vec!["Name".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Default);
    }

    /// `Tidy` shape: `this.Name = this.Name.Trim();` — unconditional
    /// self-transform, `writes == reads == {Name}`, no guard.
    #[test]
    fn tidy_is_normalize() {
        let mut f = function("Widget.Tidy");
        f.writes = vec!["Name".to_string()];
        f.reads = vec!["Name".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Normalize);
        assert!(is_recoverable(classify(&f)));
    }

    /// `ComputeDisplay` shape: `this.Display = this.Name + " (" + this._count + ")"`.
    /// Writes a field it never reads (`Display ∉ {Name, _count}`).
    #[test]
    fn compute_display_is_compute() {
        let mut f = function("Widget.ComputeDisplay");
        f.writes = vec!["Display".to_string()];
        f.reads = vec!["Name".to_string(), "_count".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Compute);
        assert!(is_recoverable(classify(&f)));
    }

    /// `Cascade` / `AddViaMain` shape: `this.ctx.SaveChanges();` — a
    /// mutator dispatch only, no abort.
    #[test]
    fn persist_is_cascade() {
        let mut f = function("Widget.Cascade");
        f.calls = vec!["this.ctx.SaveChanges".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Cascade);
        assert!(is_recoverable(classify(&f)));
    }

    /// `Guard` shape: `if (this.Name == null) throw new ArgumentException(...);`
    /// — abort-only, no write, no call.
    #[test]
    fn validate_is_guard() {
        let mut f = function("Widget.Guard");
        f.raises = vec!["ArgumentException".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Guard);
        assert!(is_recoverable(classify(&f)));
    }

    /// `Compensate` shape: write + call + raise — checked before
    /// `Cascade`/`WriteRaise` so it wins even though it also writes.
    #[test]
    fn risky_is_compensate() {
        let mut f = function("Widget.Compensate");
        f.writes = vec!["_count".to_string()];
        f.calls = vec!["this.ctx.SaveChanges".to_string()];
        f.raises = vec!["InvalidOperationException".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Compensate);
        assert!(!is_recoverable(classify(&f)));
    }

    /// `WriteRaise` shape: write + raise, no call.
    #[test]
    fn write_raise_is_write_raise() {
        let mut f = function("Widget.WriteRaise");
        f.writes = vec!["_count".to_string()];
        f.raises = vec!["InvalidOperationException".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::WriteRaise);
        assert!(!is_recoverable(classify(&f)));
    }

    /// Read-only body: excluded from the arm, not recoverable and not
    /// essential.
    #[test]
    fn read_only_is_observe() {
        let mut f = function("Widget.ReadOnly");
        f.reads = vec!["Name".to_string()];
        assert_eq!(classify(&f), RecipeCentroid::Observe);
        assert!(!is_recoverable(classify(&f)));
    }

    /// No facts at all: unresolved, a scope boundary.
    #[test]
    fn no_facts_is_empty() {
        let f = function("Widget.Sum");
        assert_eq!(classify(&f), RecipeCentroid::Empty);
        assert!(!is_recoverable(classify(&f)));
    }

    /// J1 invariant sanity check: a write-if-blank field with no read at
    /// all (`x ??= v` with no prior read) is `Default`, not `Empty` /
    /// `Normalize` — `guarded_writes` alone is enough, `reads` need not be
    /// populated (mirrors the deferred `self.x ||= v` op-assign note in
    /// the codebook doc).
    #[test]
    fn guarded_write_without_read_is_still_default() {
        let mut f = function("Widget.NoReadDefault");
        f.writes = vec!["Flag".to_string()];
        f.guarded_writes = vec!["Flag".to_string()];
        assert!(f.reads.is_empty());
        assert_eq!(classify(&f), RecipeCentroid::Default);
    }
}
