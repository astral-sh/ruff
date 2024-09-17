//! Track live bindings per symbol, applicable constraints per binding, and live declarations.
//!
//! These data structures operate entirely on scope-local newtype-indices for definitions and
//! constraints, referring to their location in the `all_definitions` and `all_constraints`
//! indexvecs in [`super::UseDefMapBuilder`].
//!
//! We need to track arbitrary associations between bindings and constraints, not just a single set
//! of currently dominating constraints (where "dominating" means "control flow must have passed
//! through it to reach this point"), because we can have dominating constraints that apply to some
//! bindings but not others, as in this code:
//!
//! ```python
//! x = 1 if flag else None
//! if x is not None:
//!     if flag2:
//!         x = 2 if flag else None
//!     x
//! ```
//!
//! The `x is not None` constraint dominates the final use of `x`, but it applies only to the first
//! binding of `x`, not the second, so `None` is a possible value for `x`.
//!
//! And we can't just track, for each binding, an index into a list of dominating constraints,
//! either, because we can have bindings which are still visible, but subject to constraints that
//! are no longer dominating, as in this code:
//!
//! ```python
//! x = 0
//! if flag1:
//!     x = 1 if flag2 else None
//!     assert x is not None
//! x
//! ```
//!
//! From the point of view of the final use of `x`, the `x is not None` constraint no longer
//! dominates, but it does dominate the `x = 1 if flag2 else None` binding, so we have to keep
//! track of that.
//!
//! The data structures used here ([`BitSet`] and [`smallvec::SmallVec`]) optimize for keeping all
//! data inline (avoiding lots of scattered allocations) in small-to-medium cases, and falling back
//! to heap allocation to be able to scale to arbitrary numbers of live bindings and constraints
//! when needed.
//!
//! Tracking live declarations is simpler, since constraints are not involved, but otherwise very
//! similar to tracking live bindings.
use super::bitset::{BitSet, BitSetIterator};
use ruff_index::newtype_index;
use smallvec::SmallVec;

/// A newtype-index for a definition in a particular scope.
#[newtype_index]
pub(super) struct ScopedDefinitionId;

/// A newtype-index for a constraint expression in a particular scope.
#[newtype_index]
pub(super) struct ScopedConstraintId;

/// Can reference this * 64 total definitions inline; more will fall back to the heap.
const INLINE_BINDING_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing live bindings of a symbol in a scope.
type Bindings = BitSet<INLINE_BINDING_BLOCKS>;
type BindingsIterator<'a> = BitSetIterator<'a, INLINE_BINDING_BLOCKS>;

/// Can reference this * 64 total declarations inline; more will fall back to the heap.
const INLINE_DECLARATION_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing live declarations of a symbol in a scope.
type Declarations = BitSet<INLINE_DECLARATION_BLOCKS>;
type DeclarationsIterator<'a> = BitSetIterator<'a, INLINE_DECLARATION_BLOCKS>;

/// Can reference this * 64 total constraints inline; more will fall back to the heap.
const INLINE_CONSTRAINT_BLOCKS: usize = 2;

/// Can keep inline this many live bindings per symbol at a given time; more will go to heap.
const INLINE_BINDINGS_PER_SYMBOL: usize = 4;

/// One [`BitSet`] of applicable [`ScopedConstraintId`] per live binding.
type InlineConstraintArray = [BitSet<INLINE_CONSTRAINT_BLOCKS>; INLINE_BINDINGS_PER_SYMBOL];
type Constraints = SmallVec<InlineConstraintArray>;
type ConstraintsIterator<'a> = std::slice::Iter<'a, BitSet<INLINE_CONSTRAINT_BLOCKS>>;
type ConstraintsIntoIterator = smallvec::IntoIter<InlineConstraintArray>;

/// Live declarations for a single symbol at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolDeclarations {
    /// [`BitSet`]: which declarations (as [`ScopedDefinitionId`]) can reach the current location?
    live_declarations: Declarations,

    /// Could the symbol be un-declared at this point?
    may_be_undeclared: bool,
}

impl SymbolDeclarations {
    fn undeclared() -> Self {
        Self {
            live_declarations: Declarations::default(),
            may_be_undeclared: true,
        }
    }

    /// Record a newly-encountered declaration for this symbol.
    fn record_declaration(&mut self, declaration_id: ScopedDefinitionId) {
        self.live_declarations = Declarations::with(declaration_id.into());
        self.may_be_undeclared = false;
    }

    /// Add undeclared as a possibility for this symbol.
    fn set_may_be_undeclared(&mut self) {
        self.may_be_undeclared = true;
    }

    /// Return an iterator over live declarations for this symbol.
    pub(super) fn iter(&self) -> DeclarationIdIterator {
        DeclarationIdIterator {
            inner: self.live_declarations.iter(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.live_declarations.is_empty()
    }

    pub(super) fn may_be_undeclared(&self) -> bool {
        self.may_be_undeclared
    }
}

/// Live bindings and narrowing constraints for a single symbol at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolBindings {
    /// [`BitSet`]: which bindings (as [`ScopedDefinitionId`]) can reach the current location?
    live_bindings: Bindings,

    /// For each live binding, which [`ScopedConstraintId`] apply?
    ///
    /// This is a [`smallvec::SmallVec`] which should always have one [`BitSet`] of constraints per
    /// binding in `live_bindings`.
    constraints: Constraints,

    /// Could the symbol be unbound at this point?
    may_be_unbound: bool,
}

impl SymbolBindings {
    fn unbound() -> Self {
        Self {
            live_bindings: Bindings::default(),
            constraints: Constraints::default(),
            may_be_unbound: true,
        }
    }

    /// Add Unbound as a possibility for this symbol.
    fn set_may_be_unbound(&mut self) {
        self.may_be_unbound = true;
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(&mut self, binding_id: ScopedDefinitionId) {
        // The new binding replaces all previous live bindings in this path, and has no
        // constraints.
        self.live_bindings = Bindings::with(binding_id.into());
        self.constraints = Constraints::with_capacity(1);
        self.constraints.push(BitSet::default());
        self.may_be_unbound = false;
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(&mut self, constraint_id: ScopedConstraintId) {
        for bitset in &mut self.constraints {
            bitset.insert(constraint_id.into());
        }
    }

    /// Iterate over currently live bindings for this symbol.
    pub(super) fn iter(&self) -> BindingIdWithConstraintsIterator {
        BindingIdWithConstraintsIterator {
            definitions: self.live_bindings.iter(),
            constraints: self.constraints.iter(),
        }
    }

    pub(super) fn may_be_unbound(&self) -> bool {
        self.may_be_unbound
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolState {
    declarations: SymbolDeclarations,
    bindings: SymbolBindings,
}

impl SymbolState {
    /// Return a new [`SymbolState`] representing an unbound, undeclared symbol.
    pub(super) fn undefined() -> Self {
        Self {
            declarations: SymbolDeclarations::undeclared(),
            bindings: SymbolBindings::unbound(),
        }
    }

    /// Add Unbound as a possibility for this symbol.
    pub(super) fn set_may_be_unbound(&mut self) {
        self.bindings.set_may_be_unbound();
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(&mut self, binding_id: ScopedDefinitionId) {
        self.bindings.record_binding(binding_id);
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(&mut self, constraint_id: ScopedConstraintId) {
        self.bindings.record_constraint(constraint_id);
    }

    /// Add undeclared as a possibility for this symbol.
    pub(super) fn set_may_be_undeclared(&mut self) {
        self.declarations.set_may_be_undeclared();
    }

    /// Record a newly-encountered declaration of this symbol.
    pub(super) fn record_declaration(&mut self, declaration_id: ScopedDefinitionId) {
        self.declarations.record_declaration(declaration_id);
    }

    /// Merge another [`SymbolState`] into this one.
    pub(super) fn merge(&mut self, b: SymbolState) {
        let mut a = Self {
            bindings: SymbolBindings {
                live_bindings: Bindings::default(),
                constraints: Constraints::default(),
                may_be_unbound: self.bindings.may_be_unbound || b.bindings.may_be_unbound,
            },
            declarations: SymbolDeclarations {
                live_declarations: self.declarations.live_declarations.clone(),
                may_be_undeclared: self.declarations.may_be_undeclared
                    || b.declarations.may_be_undeclared,
            },
        };

        std::mem::swap(&mut a, self);
        self.declarations
            .live_declarations
            .union(&b.declarations.live_declarations);

        let mut a_defs_iter = a.bindings.live_bindings.iter();
        let mut b_defs_iter = b.bindings.live_bindings.iter();
        let mut a_constraints_iter = a.bindings.constraints.into_iter();
        let mut b_constraints_iter = b.bindings.constraints.into_iter();

        let mut opt_a_def: Option<u32> = a_defs_iter.next();
        let mut opt_b_def: Option<u32> = b_defs_iter.next();

        // Iterate through the definitions from `a` and `b`, always processing the lower definition
        // ID first, and pushing each definition onto the merged `SymbolState` with its
        // constraints. If a definition is found in both `a` and `b`, push it with the intersection
        // of the constraints from the two paths; a constraint that applies from only one possible
        // path is irrelevant.

        // Helper to push `def`, with constraints in `constraints_iter`, onto `self`.
        let push = |def, constraints_iter: &mut ConstraintsIntoIterator, merged: &mut Self| {
            merged.bindings.live_bindings.insert(def);
            // SAFETY: we only ever create SymbolState with either no definitions and no constraint
            // bitsets (`::unbound`) or one definition and one constraint bitset (`::with`), and
            // `::merge` always pushes one definition and one constraint bitset together (just
            // below), so the number of definitions and the number of constraint bitsets can never
            // get out of sync.
            let constraints = constraints_iter
                .next()
                .expect("definitions and constraints length mismatch");
            merged.bindings.constraints.push(constraints);
        };

        loop {
            match (opt_a_def, opt_b_def) {
                (Some(a_def), Some(b_def)) => match a_def.cmp(&b_def) {
                    std::cmp::Ordering::Less => {
                        // Next definition ID is only in `a`, push it to `self` and advance `a`.
                        push(a_def, &mut a_constraints_iter, self);
                        opt_a_def = a_defs_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        // Next definition ID is only in `b`, push it to `self` and advance `b`.
                        push(b_def, &mut b_constraints_iter, self);
                        opt_b_def = b_defs_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        // Next definition is in both; push to `self` and intersect constraints.
                        push(a_def, &mut b_constraints_iter, self);
                        // SAFETY: we only ever create SymbolState with either no definitions and
                        // no constraint bitsets (`::unbound`) or one definition and one constraint
                        // bitset (`::with`), and `::merge` always pushes one definition and one
                        // constraint bitset together (just below), so the number of definitions
                        // and the number of constraint bitsets can never get out of sync.
                        let a_constraints = a_constraints_iter
                            .next()
                            .expect("definitions and constraints length mismatch");
                        // If the same definition is visible through both paths, any constraint
                        // that applies on only one path is irrelevant to the resulting type from
                        // unioning the two paths, so we intersect the constraints.
                        self.bindings
                            .constraints
                            .last_mut()
                            .unwrap()
                            .intersect(&a_constraints);
                        opt_a_def = a_defs_iter.next();
                        opt_b_def = b_defs_iter.next();
                    }
                },
                (Some(a_def), None) => {
                    // We've exhausted `b`, just push the def from `a` and move on to the next.
                    push(a_def, &mut a_constraints_iter, self);
                    opt_a_def = a_defs_iter.next();
                }
                (None, Some(b_def)) => {
                    // We've exhausted `a`, just push the def from `b` and move on to the next.
                    push(b_def, &mut b_constraints_iter, self);
                    opt_b_def = b_defs_iter.next();
                }
                (None, None) => break,
            }
        }
    }

    pub(super) fn bindings(&self) -> &SymbolBindings {
        &self.bindings
    }

    pub(super) fn declarations(&self) -> &SymbolDeclarations {
        &self.declarations
    }

    /// Could the symbol be unbound?
    pub(super) fn may_be_unbound(&self) -> bool {
        self.bindings.may_be_unbound()
    }
}

/// The default state of a symbol, if we've seen no definitions of it, is undefined (that is,
/// both unbound and undeclared).
impl Default for SymbolState {
    fn default() -> Self {
        SymbolState::undefined()
    }
}

/// A single binding (as [`ScopedDefinitionId`]) with an iterator of its applicable
/// [`ScopedConstraintId`].
#[derive(Debug)]
pub(super) struct BindingIdWithConstraints<'a> {
    pub(super) definition: ScopedDefinitionId,
    pub(super) constraint_ids: ConstraintIdIterator<'a>,
}

#[derive(Debug)]
pub(super) struct BindingIdWithConstraintsIterator<'a> {
    definitions: BindingsIterator<'a>,
    constraints: ConstraintsIterator<'a>,
}

impl<'a> Iterator for BindingIdWithConstraintsIterator<'a> {
    type Item = BindingIdWithConstraints<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match (self.definitions.next(), self.constraints.next()) {
            (None, None) => None,
            (Some(def), Some(constraints)) => Some(BindingIdWithConstraints {
                definition: ScopedDefinitionId::from_u32(def),
                constraint_ids: ConstraintIdIterator {
                    wrapped: constraints.iter(),
                },
            }),
            // SAFETY: see above.
            _ => unreachable!("definitions and constraints length mismatch"),
        }
    }
}

impl std::iter::FusedIterator for BindingIdWithConstraintsIterator<'_> {}

#[derive(Debug)]
pub(super) struct ConstraintIdIterator<'a> {
    wrapped: BitSetIterator<'a, INLINE_CONSTRAINT_BLOCKS>,
}

impl Iterator for ConstraintIdIterator<'_> {
    type Item = ScopedConstraintId;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped.next().map(ScopedConstraintId::from_u32)
    }
}

impl std::iter::FusedIterator for ConstraintIdIterator<'_> {}

#[derive(Debug)]
pub(super) struct DeclarationIdIterator<'a> {
    inner: DeclarationsIterator<'a>,
}

impl<'a> Iterator for DeclarationIdIterator<'a> {
    type Item = ScopedDefinitionId;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(ScopedDefinitionId::from_u32)
    }
}

impl std::iter::FusedIterator for DeclarationIdIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::{ScopedConstraintId, ScopedDefinitionId, SymbolState};

    fn assert_bindings(symbol: &SymbolState, may_be_unbound: bool, expected: &[&str]) {
        assert_eq!(symbol.may_be_unbound(), may_be_unbound);
        let actual = symbol
            .bindings()
            .iter()
            .map(|def_id_with_constraints| {
                format!(
                    "{}<{}>",
                    def_id_with_constraints.definition.as_u32(),
                    def_id_with_constraints
                        .constraint_ids
                        .map(ScopedConstraintId::as_u32)
                        .map(|idx| idx.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    pub(crate) fn assert_declarations(
        symbol: &SymbolState,
        may_be_undeclared: bool,
        expected: &[u32],
    ) {
        assert_eq!(symbol.declarations.may_be_undeclared(), may_be_undeclared);
        let actual = symbol
            .declarations()
            .iter()
            .map(ScopedDefinitionId::as_u32)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn unbound() {
        let sym = SymbolState::undefined();

        assert_bindings(&sym, true, &[]);
    }

    #[test]
    fn with() {
        let mut sym = SymbolState::undefined();
        sym.record_binding(ScopedDefinitionId::from_u32(0));

        assert_bindings(&sym, false, &["0<>"]);
    }

    #[test]
    fn set_may_be_unbound() {
        let mut sym = SymbolState::undefined();
        sym.record_binding(ScopedDefinitionId::from_u32(0));
        sym.set_may_be_unbound();

        assert_bindings(&sym, true, &["0<>"]);
    }

    #[test]
    fn record_constraint() {
        let mut sym = SymbolState::undefined();
        sym.record_binding(ScopedDefinitionId::from_u32(0));
        sym.record_constraint(ScopedConstraintId::from_u32(0));

        assert_bindings(&sym, false, &["0<0>"]);
    }

    #[test]
    fn merge() {
        // merging the same definition with the same constraint keeps the constraint
        let mut sym0a = SymbolState::undefined();
        sym0a.record_binding(ScopedDefinitionId::from_u32(0));
        sym0a.record_constraint(ScopedConstraintId::from_u32(0));

        let mut sym0b = SymbolState::undefined();
        sym0b.record_binding(ScopedDefinitionId::from_u32(0));
        sym0b.record_constraint(ScopedConstraintId::from_u32(0));

        sym0a.merge(sym0b);
        let mut sym0 = sym0a;
        assert_bindings(&sym0, false, &["0<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut sym1a = SymbolState::undefined();
        sym1a.record_binding(ScopedDefinitionId::from_u32(1));
        sym1a.record_constraint(ScopedConstraintId::from_u32(1));

        let mut sym1b = SymbolState::undefined();
        sym1b.record_binding(ScopedDefinitionId::from_u32(1));
        sym1b.record_constraint(ScopedConstraintId::from_u32(2));

        sym1a.merge(sym1b);
        let sym1 = sym1a;
        assert_bindings(&sym1, false, &["1<>"]);

        // merging a constrained definition with unbound keeps both
        let mut sym2a = SymbolState::undefined();
        sym2a.record_binding(ScopedDefinitionId::from_u32(2));
        sym2a.record_constraint(ScopedConstraintId::from_u32(3));

        let sym2b = SymbolState::undefined();

        sym2a.merge(sym2b);
        let sym2 = sym2a;
        assert_bindings(&sym2, true, &["2<3>"]);

        // merging different definitions keeps them each with their existing constraints
        sym0.merge(sym2);
        let sym = sym0;
        assert_bindings(&sym, true, &["0<0>", "2<3>"]);
    }

    #[test]
    fn no_declaration() {
        let sym = SymbolState::undefined();

        assert_declarations(&sym, true, &[]);
    }

    #[test]
    fn record_declaration() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        assert_declarations(&sym, false, &[1]);
    }

    #[test]
    fn record_declaration_override() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(ScopedDefinitionId::from_u32(1));
        sym.record_declaration(ScopedDefinitionId::from_u32(2));

        assert_declarations(&sym, false, &[2]);
    }

    #[test]
    fn record_declaration_merge() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        let mut sym2 = SymbolState::undefined();
        sym2.record_declaration(ScopedDefinitionId::from_u32(2));

        sym.merge(sym2);

        assert_declarations(&sym, false, &[1, 2]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(ScopedDefinitionId::from_u32(1));

        let sym2 = SymbolState::undefined();

        sym.merge(sym2);

        assert_declarations(&sym, true, &[1]);
    }

    #[test]
    fn set_may_be_undeclared() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(ScopedDefinitionId::from_u32(0));
        sym.set_may_be_undeclared();

        assert_declarations(&sym, true, &[0]);
    }
}
