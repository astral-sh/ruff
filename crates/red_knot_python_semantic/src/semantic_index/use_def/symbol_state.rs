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
use super::bitset::{BitSet, BitSetIterator, ReverseBitSetIterator};
use ruff_index::newtype_index;
use smallvec::SmallVec;

/// A newtype-index for a definition in a particular scope.
#[newtype_index]
pub(super) struct ScopedDefinitionId;

/// A newtype-index for a constraint expression in a particular scope.
#[newtype_index]
pub(super) struct ScopedConstraintId;

/// A newtype-index for a [`crate::semantic_index::branching_condition::BranchingCondition`] in a particular scope.
#[newtype_index]
pub(super) struct ScopedBranchingConditionId;

/// Can reference this * 64 total definitions inline; more will fall back to the heap.
const INLINE_BINDING_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing live bindings of a symbol in a scope.
type Bindings = BitSet<INLINE_BINDING_BLOCKS>;
type ReverseBindingsIterator<'a> = ReverseBitSetIterator<'a, INLINE_BINDING_BLOCKS>;

/// Can reference this * 64 total declarations inline; more will fall back to the heap.
const INLINE_DECLARATION_BLOCKS: usize = 3;

/// A [`BitSet`] of [`ScopedDefinitionId`], representing live declarations of a symbol in a scope.
type Declarations = BitSet<INLINE_DECLARATION_BLOCKS>;
type ReverseDeclarationsIterator<'a> = ReverseBitSetIterator<'a, INLINE_DECLARATION_BLOCKS>;

/// Can reference this * 64 total constraints inline; more will fall back to the heap.
const INLINE_CONSTRAINT_BLOCKS: usize = 2;

/// Can keep inline this many live bindings per symbol at a given time; more will go to heap.
const INLINE_BINDINGS_PER_SYMBOL: usize = 4;

/// Which constraints apply to a given binding?
type Constraints = BitSet<INLINE_CONSTRAINT_BLOCKS>;

type InlineConstraintArray = [Constraints; INLINE_BINDINGS_PER_SYMBOL];

/// One [`BitSet`] of applicable [`ScopedConstraintId`]s per live binding.
type ConstraintsPerBinding = SmallVec<InlineConstraintArray>;

/// Iterate over all constraints for a single binding.
type ConstraintsIterator<'a> = std::slice::Iter<'a, Constraints>;
type ConstraintsIntoIterator = smallvec::IntoIter<InlineConstraintArray>;

/// Similar to what we have for constraints, but for active branching conditions.
const INLINE_BRANCHING_BLOCKS: usize = 2;
const INLINE_BRANCHING_CONDITIONS: usize = 4;
pub(super) type BranchingConditions = BitSet<INLINE_BRANCHING_BLOCKS>;
type InlineBranchingConditionsArray = [BranchingConditions; INLINE_BRANCHING_CONDITIONS];
/// One [`BitSet`] of active [`ScopedBranchingConditionId`]s per live binding.
type BranchingConditionsPerBinding = SmallVec<InlineBranchingConditionsArray>;
type BranchingConditionsIterator<'a> = std::slice::Iter<'a, BranchingConditions>;
type BranchingConditionsIntoIterator = smallvec::IntoIter<InlineBranchingConditionsArray>;

/// Live declarations for a single symbol at some point in control flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SymbolDeclarations {
    /// [`BitSet`]: which declarations (as [`ScopedDefinitionId`]) can reach the current location?
    live_declarations: Declarations,

    /// For each live declaration, which [`BranchingConditions`] were active at that declaration?
    branching_conditions: BranchingConditionsPerBinding,

    /// Could the symbol be un-declared at this point?
    may_be_undeclared: bool,
}

impl SymbolDeclarations {
    fn undeclared() -> Self {
        Self {
            live_declarations: Declarations::default(),
            branching_conditions: BranchingConditionsPerBinding::default(),
            may_be_undeclared: true,
        }
    }

    /// Record a newly-encountered declaration for this symbol.
    fn record_declaration(
        &mut self,
        declaration_id: ScopedDefinitionId,
        branching_conditions: &BranchingConditions,
    ) {
        self.live_declarations = Declarations::with(declaration_id.into());
        self.may_be_undeclared = false;

        self.branching_conditions = BranchingConditionsPerBinding::with_capacity(1);
        self.branching_conditions
            .push(BranchingConditions::default());
        for active_constraint_id in branching_conditions.iter() {
            self.branching_conditions[0].insert(active_constraint_id);
        }
    }

    /// Add undeclared as a possibility for this symbol.
    fn set_may_be_undeclared(&mut self) {
        self.may_be_undeclared = true;
    }

    /// Return an iterator over live declarations for this symbol.
    pub(super) fn iter_rev(&self) -> DeclarationIdIterator {
        DeclarationIdIterator {
            inner: self.live_declarations.iter_rev(),
            branching_conditions: self.branching_conditions.iter().rev(),
        }
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
    constraints: ConstraintsPerBinding,

    /// For each live binding, which [`BranchingConditions`] were active at that binding?
    branching_conditions: BranchingConditionsPerBinding,

    /// Could the symbol be unbound at this point?
    may_be_unbound: bool,
}

impl SymbolBindings {
    fn unbound() -> Self {
        Self {
            live_bindings: Bindings::default(),
            constraints: ConstraintsPerBinding::default(),
            branching_conditions: BranchingConditionsPerBinding::default(),
            may_be_unbound: true,
        }
    }

    /// Add Unbound as a possibility for this symbol.
    fn set_may_be_unbound(&mut self) {
        self.may_be_unbound = true;
    }

    /// Record a newly-encountered binding for this symbol.
    pub(super) fn record_binding(
        &mut self,
        binding_id: ScopedDefinitionId,
        branching_conditions: &BranchingConditions,
    ) {
        // The new binding replaces all previous live bindings in this path, and has no
        // constraints.
        self.live_bindings = Bindings::with(binding_id.into());
        self.constraints = ConstraintsPerBinding::with_capacity(1);
        self.constraints.push(Constraints::default());

        self.branching_conditions = BranchingConditionsPerBinding::with_capacity(1);
        self.branching_conditions
            .push(BranchingConditions::default());
        for id in branching_conditions.iter() {
            self.branching_conditions[0].insert(id);
        }
        self.may_be_unbound = false;
    }

    /// Add given constraint to all live bindings.
    pub(super) fn record_constraint(&mut self, constraint_id: ScopedConstraintId) {
        for bitset in &mut self.constraints {
            bitset.insert(constraint_id.into());
        }
    }

    /// Iterate over currently live bindings for this symbol, in reverse order.
    pub(super) fn iter_rev(&self) -> BindingIdWithConstraintsIterator {
        BindingIdWithConstraintsIterator {
            definitions: self.live_bindings.iter_rev(),
            constraints: self.constraints.iter().rev(),
            branching_conditions: self.branching_conditions.iter().rev(),
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
    pub(super) fn record_binding(
        &mut self,
        binding_id: ScopedDefinitionId,
        branching_conditions: &BranchingConditions,
    ) {
        self.bindings
            .record_binding(binding_id, branching_conditions);
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
    pub(super) fn record_declaration(
        &mut self,
        declaration_id: ScopedDefinitionId,
        branching_conditions: &BranchingConditions,
    ) {
        self.declarations
            .record_declaration(declaration_id, branching_conditions);
    }

    /// Merge another [`SymbolState`] into this one.
    pub(super) fn merge(&mut self, b: SymbolState) {
        let mut a = Self {
            bindings: SymbolBindings {
                live_bindings: Bindings::default(),
                constraints: ConstraintsPerBinding::default(),
                branching_conditions: BranchingConditionsPerBinding::default(),
                may_be_unbound: self.bindings.may_be_unbound || b.bindings.may_be_unbound,
            },
            declarations: SymbolDeclarations {
                live_declarations: self.declarations.live_declarations.clone(),
                branching_conditions: BranchingConditionsPerBinding::default(),
                may_be_undeclared: self.declarations.may_be_undeclared
                    || b.declarations.may_be_undeclared,
            },
        };

        std::mem::swap(&mut a, self);

        let mut a_defs_iter = a.bindings.live_bindings.iter();
        let mut b_defs_iter = b.bindings.live_bindings.iter();
        let mut a_constraints_iter = a.bindings.constraints.into_iter();
        let mut b_constraints_iter = b.bindings.constraints.into_iter();
        let mut a_conditions_iter = a.bindings.branching_conditions.into_iter();
        let mut b_conditions_iter = b.bindings.branching_conditions.into_iter();

        let mut opt_a_def: Option<u32> = a_defs_iter.next();
        let mut opt_b_def: Option<u32> = b_defs_iter.next();

        // Iterate through the definitions from `a` and `b`, always processing the lower definition
        // ID first, and pushing each definition onto the merged `SymbolState` with its
        // constraints. If a definition is found in both `a` and `b`, push it with the intersection
        // of the constraints from the two paths; a constraint that applies from only one possible
        // path is irrelevant.

        // Helper to push `def`, with constraints in `constraints_iter`, onto `self`.
        let push = |def,
                    constraints_iter: &mut ConstraintsIntoIterator,
                    branching_conditions_iter: &mut BranchingConditionsIntoIterator,
                    merged: &mut Self| {
            merged.bindings.live_bindings.insert(def);
            // SAFETY: we only ever create SymbolState with either no definitions and no constraint
            // bitsets (`::unbound`) or one definition and one constraint bitset (`::with`), and
            // `::merge` always pushes one definition and one constraint bitset together (just
            // below), so the number of definitions and the number of constraint bitsets can never
            // get out of sync.
            let constraints = constraints_iter
                .next()
                .expect("definitions and constraints length mismatch");
            let branching_conditions = branching_conditions_iter
                .next()
                .expect("definitions and branching_conditions length mismatch");
            merged.bindings.constraints.push(constraints);
            merged
                .bindings
                .branching_conditions
                .push(branching_conditions);
        };

        loop {
            match (opt_a_def, opt_b_def) {
                (Some(a_def), Some(b_def)) => match a_def.cmp(&b_def) {
                    std::cmp::Ordering::Less => {
                        // Next definition ID is only in `a`, push it to `self` and advance `a`.
                        push(a_def, &mut a_constraints_iter, &mut a_conditions_iter, self);
                        opt_a_def = a_defs_iter.next();
                    }
                    std::cmp::Ordering::Greater => {
                        // Next definition ID is only in `b`, push it to `self` and advance `b`.
                        push(b_def, &mut b_constraints_iter, &mut b_conditions_iter, self);
                        opt_b_def = b_defs_iter.next();
                    }
                    std::cmp::Ordering::Equal => {
                        // Next definition is in both; push to `self` and intersect constraints.
                        push(a_def, &mut b_constraints_iter, &mut b_conditions_iter, self);
                        // SAFETY: we only ever create SymbolState with either no definitions and
                        // no constraint bitsets (`::unbound`) or one definition and one constraint
                        // bitset (`::with`), and `::merge` always pushes one definition and one
                        // constraint bitset together (just below), so the number of definitions
                        // and the number of constraint bitsets can never get out of sync.
                        let a_constraints = a_constraints_iter
                            .next()
                            .expect("definitions and constraints length mismatch");
                        // SAFETY: The same is true for branching_conditions.
                        a_conditions_iter
                            .next()
                            .expect("branching_conditions length mismatch");
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
                    push(a_def, &mut a_constraints_iter, &mut a_conditions_iter, self);
                    opt_a_def = a_defs_iter.next();
                }
                (None, Some(b_def)) => {
                    // We've exhausted `a`, just push the def from `b` and move on to the next.
                    push(b_def, &mut b_constraints_iter, &mut b_conditions_iter, self);
                    opt_b_def = b_defs_iter.next();
                }
                (None, None) => break,
            }
        }

        // Same as above, but for declarations.
        let mut a_decls_iter = a.declarations.live_declarations.iter();
        let mut b_decls_iter = b.declarations.live_declarations.iter();
        let mut a_conditions_iter = a.declarations.branching_conditions.into_iter();
        let mut b_conditions_iter = b.declarations.branching_conditions.into_iter();

        let mut opt_a_decl: Option<u32> = a_decls_iter.next();
        let mut opt_b_decl: Option<u32> = b_decls_iter.next();

        let push =
            |decl, conditions_iter: &mut BranchingConditionsIntoIterator, merged: &mut Self| {
                merged.declarations.live_declarations.insert(decl);
                let conditions = conditions_iter
                    .next()
                    .expect("declarations and branching_conditions length mismatch");
                merged.declarations.branching_conditions.push(conditions);
            };

        loop {
            match (opt_a_decl, opt_b_decl) {
                (Some(a_decl), Some(b_decl)) => {
                    match a_decl.cmp(&b_decl) {
                        std::cmp::Ordering::Less => {
                            push(a_decl, &mut a_conditions_iter, self);
                            opt_a_decl = a_decls_iter.next();
                        }
                        std::cmp::Ordering::Greater => {
                            push(b_decl, &mut b_conditions_iter, self);
                            opt_b_decl = b_decls_iter.next();
                        }
                        std::cmp::Ordering::Equal => {
                            push(a_decl, &mut b_conditions_iter, self);
                            self.declarations
                                .branching_conditions
                                .last_mut()
                                .expect("declarations and branching_conditions length mismatch")
                                .intersect(&a_conditions_iter.next().expect(
                                    "declarations and branching_conditions length mismatch",
                                ));

                            opt_a_decl = a_decls_iter.next();
                            opt_b_decl = b_decls_iter.next();
                        }
                    }
                }
                (Some(a_decl), None) => {
                    push(a_decl, &mut a_conditions_iter, self);
                    opt_a_decl = a_decls_iter.next();
                }
                (None, Some(b_decl)) => {
                    push(b_decl, &mut b_conditions_iter, self);
                    opt_b_decl = b_decls_iter.next();
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
    pub(super) branching_conditions_ids: BranchingConditionIdIterator<'a>,
}

#[derive(Debug)]
pub(super) struct BindingIdWithConstraintsIterator<'a> {
    definitions: ReverseBindingsIterator<'a>,
    constraints: std::iter::Rev<ConstraintsIterator<'a>>,
    branching_conditions: std::iter::Rev<BranchingConditionsIterator<'a>>,
}

impl<'a> Iterator for BindingIdWithConstraintsIterator<'a> {
    type Item = BindingIdWithConstraints<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match (
            self.definitions.next(),
            self.constraints.next(),
            self.branching_conditions.next(),
        ) {
            (None, None, None) => None,
            (Some(def), Some(constraints), Some(branching_conditions)) => {
                Some(BindingIdWithConstraints {
                    definition: ScopedDefinitionId::from_u32(def),
                    constraint_ids: ConstraintIdIterator {
                        wrapped: constraints.iter(),
                    },
                    branching_conditions_ids: BranchingConditionIdIterator {
                        wrapped: branching_conditions.iter(),
                    },
                })
            }
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

#[derive(Debug, Clone)]
pub(super) struct BranchingConditionIdIterator<'a> {
    wrapped: BitSetIterator<'a, INLINE_BRANCHING_BLOCKS>,
}

impl Iterator for BranchingConditionIdIterator<'_> {
    type Item = ScopedBranchingConditionId;

    fn next(&mut self) -> Option<Self::Item> {
        self.wrapped
            .next()
            .map(ScopedBranchingConditionId::from_u32)
    }
}

impl std::iter::FusedIterator for BranchingConditionIdIterator<'_> {}

#[derive(Clone)]
pub(super) struct DeclarationIdIterator<'a> {
    inner: ReverseDeclarationsIterator<'a>,
    branching_conditions: std::iter::Rev<BranchingConditionsIterator<'a>>,
}

impl<'a> Iterator for DeclarationIdIterator<'a> {
    type Item = (ScopedDefinitionId, BranchingConditionIdIterator<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        match (self.inner.next(), self.branching_conditions.next()) {
            (None, None) => None,
            (Some(declaration), Some(branching_conditions)) => Some((
                ScopedDefinitionId::from_u32(declaration),
                BranchingConditionIdIterator {
                    wrapped: branching_conditions.iter(),
                },
            )),
            // SAFETY: see above.
            _ => unreachable!("declarations and branching_conditions length mismatch"),
        }
    }
}

impl std::iter::FusedIterator for DeclarationIdIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn assert_bindings(symbol: &SymbolState, may_be_unbound: bool, expected: &[&str]) {
        assert_eq!(symbol.bindings.may_be_unbound, may_be_unbound);
        let mut actual = symbol
            .bindings()
            .iter_rev()
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
        actual.reverse();
        assert_eq!(actual, expected);
    }

    #[track_caller]
    pub(crate) fn assert_declarations(
        symbol: &SymbolState,
        may_be_undeclared: bool,
        expected: &[u32],
    ) {
        assert_eq!(symbol.declarations.may_be_undeclared(), may_be_undeclared);
        let mut actual = symbol
            .declarations()
            .iter_rev()
            .map(|(d, _)| d.as_u32())
            .collect::<Vec<_>>();
        actual.reverse();
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
        sym.record_binding(
            ScopedDefinitionId::from_u32(0),
            &BranchingConditions::default(),
        );

        assert_bindings(&sym, false, &["0<>"]);
    }

    #[test]
    fn set_may_be_unbound() {
        let mut sym = SymbolState::undefined();
        sym.record_binding(
            ScopedDefinitionId::from_u32(0),
            &BranchingConditions::default(),
        );
        sym.set_may_be_unbound();

        assert_bindings(&sym, true, &["0<>"]);
    }

    #[test]
    fn record_constraint() {
        let mut sym = SymbolState::undefined();
        sym.record_binding(
            ScopedDefinitionId::from_u32(0),
            &BranchingConditions::default(),
        );
        sym.record_constraint(ScopedConstraintId::from_u32(0));

        assert_bindings(&sym, false, &["0<0>"]);
    }

    #[test]
    fn merge() {
        // merging the same definition with the same constraint keeps the constraint
        let mut sym0a = SymbolState::undefined();
        sym0a.record_binding(
            ScopedDefinitionId::from_u32(0),
            &BranchingConditions::default(),
        );
        sym0a.record_constraint(ScopedConstraintId::from_u32(0));

        let mut sym0b = SymbolState::undefined();
        sym0b.record_binding(
            ScopedDefinitionId::from_u32(0),
            &BranchingConditions::default(),
        );
        sym0b.record_constraint(ScopedConstraintId::from_u32(0));

        sym0a.merge(sym0b);
        let mut sym0 = sym0a;
        assert_bindings(&sym0, false, &["0<0>"]);

        // merging the same definition with differing constraints drops all constraints
        let mut sym1a = SymbolState::undefined();
        sym1a.record_binding(
            ScopedDefinitionId::from_u32(1),
            &BranchingConditions::default(),
        );
        sym1a.record_constraint(ScopedConstraintId::from_u32(1));

        let mut sym1b = SymbolState::undefined();
        sym1b.record_binding(
            ScopedDefinitionId::from_u32(1),
            &BranchingConditions::default(),
        );
        sym1b.record_constraint(ScopedConstraintId::from_u32(2));

        sym1a.merge(sym1b);
        let sym1 = sym1a;
        assert_bindings(&sym1, false, &["1<>"]);

        // merging a constrained definition with unbound keeps both
        let mut sym2a = SymbolState::undefined();
        sym2a.record_binding(
            ScopedDefinitionId::from_u32(2),
            &BranchingConditions::default(),
        );
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
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            &BranchingConditions::default(),
        );

        assert_declarations(&sym, false, &[1]);
    }

    #[test]
    fn record_declaration_override() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            &BranchingConditions::default(),
        );
        sym.record_declaration(
            ScopedDefinitionId::from_u32(2),
            &BranchingConditions::default(),
        );

        assert_declarations(&sym, false, &[2]);
    }

    #[test]
    fn record_declaration_merge() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            &BranchingConditions::default(),
        );

        let mut sym2 = SymbolState::undefined();
        sym2.record_declaration(
            ScopedDefinitionId::from_u32(2),
            &BranchingConditions::default(),
        );

        sym.merge(sym2);

        assert_declarations(&sym, false, &[1, 2]);
    }

    #[test]
    fn record_declaration_merge_partial_undeclared() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(
            ScopedDefinitionId::from_u32(1),
            &BranchingConditions::default(),
        );

        let sym2 = SymbolState::undefined();

        sym.merge(sym2);

        assert_declarations(&sym, true, &[1]);
    }

    #[test]
    fn set_may_be_undeclared() {
        let mut sym = SymbolState::undefined();
        sym.record_declaration(
            ScopedDefinitionId::from_u32(0),
            &BranchingConditions::default(),
        );
        sym.set_may_be_undeclared();

        assert_declarations(&sym, true, &[0]);
    }
}
