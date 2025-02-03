//! # Visibility constraints
//!
//! During semantic index building, we collect visibility constraints for each binding and
//! declaration. These constraints are then used during type-checking to determine the static
//! visibility of a certain definition. This allows us to re-analyze control flow during type
//! checking, potentially "hiding" some branches that we can statically determine to never be
//! taken. Consider the following example first. We added implicit "unbound" definitions at the
//! start of the scope. Note how visibility constraints can apply to bindings outside of the
//! if-statement:
//! ```py
//! x = <unbound>  # not a live binding for the use of x below, shadowed by `x = 1`
//! y = <unbound>  # visibility constraint: ~test
//!
//! x = 1  # visibility constraint: ~test
//! if test:
//!     x = 2  # visibility constraint: test
//!
//!     y = 2  # visibility constraint: test
//!
//! use(x)
//! use(y)
//! ```
//! The static truthiness of the `test` condition can either be always-false, ambiguous, or
//! always-true. Similarly, we have the same three options when evaluating a visibility constraint.
//! This outcome determines the visibility of a definition: always-true means that the definition
//! is definitely visible for a given use, always-false means that the definition is definitely
//! not visible, and ambiguous means that we might see this definition or not. In the latter case,
//! we need to consider both options during type inference and boundness analysis. For the example
//! above, these are the possible type inference / boundness results for the uses of `x` and `y`:
//!
//! ```text
//!       | `test` truthiness | `~test` truthiness | type of `x`     | boundness of `y` |
//!       |-------------------|--------------------|-----------------|------------------|
//!       | always false      | always true        | `Literal[1]`    | unbound          |
//!       | ambiguous         | ambiguous          | `Literal[1, 2]` | possibly unbound |
//!       | always true       | always false       | `Literal[2]`    | bound            |
//! ```
//!
//! ### Sequential constraints (ternary AND)
//!
//! As we have seen above, visibility constraints can apply outside of a control flow element.
//! So we need to consider the possibility that multiple constraints apply to the same binding.
//! Here, we consider what happens if multiple `if`-statements lead to a sequence of constraints.
//! Consider the following example:
//! ```py
//! x = 0
//!
//! if test1:
//!     x = 1
//!
//! if test2:
//!     x = 2
//! ```
//! The binding `x = 2` is easy to analyze. Its visibility corresponds to the truthiness of `test2`.
//! For the `x = 1` binding, things are a bit more interesting. It is always visible if `test1` is
//! always-true *and* `test2` is always-false. It is never visible if `test1` is always-false *or*
//! `test2` is always-true. And it is ambiguous otherwise. This corresponds to a ternary *test1 AND
//! ~test2* operation in three-valued Kleene logic [Kleene]:
//!
//! ```text
//!       | AND          | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | always-false | always-false |
//!       | ambiguous    | always-false | ambiguous    | ambiguous    |
//!       | always true  | always-false | ambiguous    | always-true  |
//! ```
//!
//! The `x = 0` binding can be handled similarly, with the difference that both `test1` and `test2`
//! are negated:
//! ```py
//! x = 0  # ~test1 AND ~test2
//!
//! if test1:
//!     x = 1  # test1 AND ~test2
//!
//! if test2:
//!     x = 2  # test2
//! ```
//!
//! ### Merged constraints (ternary OR)
//!
//! Finally, we consider what happens in "parallel" control flow. Consider the following example
//! where we have omitted the test condition for the outer `if` for clarity:
//! ```py
//! x = 0
//!
//! if <…>:
//!     if test1:
//!         x = 1
//! else:
//!     if test2:
//!         x = 2
//!
//! use(x)
//! ```
//! At the usage of `x`, i.e. after control flow has been merged again, the visibility of the `x =
//! 0` binding behaves as follows: the binding is always visible if `test1` is always-false *or*
//! `test2` is always-false; and it is never visible if `test1` is always-true *and* `test2` is
//! always-true. This corresponds to a ternary *OR* operation in Kleene logic:
//!
//! ```text
//!       | OR           | always-false | ambiguous    | always-true  |
//!       |--------------|--------------|--------------|--------------|
//!       | always false | always-false | ambiguous    | always-true  |
//!       | ambiguous    | ambiguous    | ambiguous    | always-true  |
//!       | always true  | always-true  | always-true  | always-true  |
//! ```
//!
//! Using this, we can annotate the visibility constraints for the example above:
//! ```py
//! x = 0  # ~test1 OR ~test2
//!
//! if <…>:
//!     if test1:
//!         x = 1  # test1
//! else:
//!     if test2:
//!         x = 2  # test2
//!
//! use(x)
//! ```
//!
//! ### Explicit ambiguity
//!
//! In some cases, we explicitly add an “ambiguous” constraint to all bindings
//! in a certain control flow path. We do this when branching on something that we can not (or
//! intentionally do not want to) analyze statically. `for` loops are one example:
//! ```py
//! x = <unbound>
//!
//! for _ in range(2):
//!    x = 1
//! ```
//! Here, we report an ambiguous visibility constraint before branching off. If we don't do this,
//! the `x = <unbound>` binding would be considered unconditionally visible in the no-loop case.
//! And since the other branch does not have the live `x = <unbound>` binding, we would incorrectly
//! create a state where the `x = <unbound>` binding is always visible.
//!
//!
//! ### Properties
//!
//! The ternary `AND` and `OR` operations have the property that `~a OR ~b = ~(a AND b)`. This
//! means we could, in principle, get rid of either of these two to simplify the representation.
//!
//! However, we already apply negative constraints `~test1` and `~test2` to the "branches not
//! taken" in the example above. This means that the tree-representation `~test1 OR ~test2` is much
//! cheaper/shallower than basically creating `~(~(~test1) AND ~(~test2))`. Similarly, if we wanted
//! to get rid of `AND`, we would also have to create additional nodes. So for performance reasons,
//! there is a small "duplication" in the code between those two constraint types.
//!
//! [Kleene]: <https://en.wikipedia.org/wiki/Three-valued_logic#Kleene_and_Priest_logics>

use std::cmp::Ordering;

use rustc_hash::FxHashMap;

use crate::semantic_index::{
    ast_ids::HasScopedExpressionId,
    constraint::{Constraint, ConstraintNode, PatternConstraintKind},
};
use crate::types::{infer_expression_types, Truthiness};
use crate::Db;

/// A ternary formula that defines under what conditions a binding is visible. (A ternary formula
/// is just like a boolean formula, but with `Ambiguous` as a third potential result. See the
/// module documentation for more details.)
///
/// The primitive atoms of the formula are [`Constraint`]s, which express some property of the
/// runtime state of the code that we are analyzing.
///
/// We assume that each atom has a stable value each time that the formula is evaluated. An atom
/// that resolves to `Ambiguous` might be true or false, and we can't tell which — but within that
/// evaluation, we assume that the atom has the _same_ unknown value each time it appears. That
/// allows us to perform simplifications like `A ∨ !A → true` and `A ∧ !A → false`.
///
/// That means that when you are constructing a formula, you might need to create distinct atoms
/// for a particular [`Constraint`], if your formula needs to consider how a particular runtime
/// property might be different at different points in the execution of the program.
///
/// Visibility constraints are normalized, so equivalent constraints are guaranteed to have equal
/// IDs.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) struct ScopedVisibilityConstraintId(u32);

impl std::fmt::Debug for ScopedVisibilityConstraintId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_tuple("ScopedVisibilityConstraintId");
        match *self {
            ALWAYS_TRUE => f.field(&format_args!("AlwaysTrue")).finish(),
            AMBIGUOUS => f.field(&format_args!("Ambiguous")).finish(),
            ALWAYS_FALSE => f.field(&format_args!("AlwaysFalse")).finish(),
            _ => f.field(&self.0).finish(),
        }
    }
}

// Internal details:
//
// There are 3 terminals, with hard-coded constraint IDs: true, ambiguous, and false.
//
// _Atoms_ are the underlying Constraints, which are the variables that are evaluated by the
// ternary function.
//
// _Interior nodes_ provide the TDD structure for the formula. Interior nodes are stored in an
// arena Vec, with the constraint ID providing an index into the arena.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct InteriorNode {
    atom: Atom,
    if_true: ScopedVisibilityConstraintId,
    if_ambiguous: ScopedVisibilityConstraintId,
    if_false: ScopedVisibilityConstraintId,
}

/// A "variable" that is evaluated as part of a TDD ternary function. For visibility constraints,
/// this is (one of the copies of) a `Constraint` that represents some runtime property of the
/// Python code that we are evaluating. We intern these constraints in an arena
/// ([`VisibilityConstraints::constraints`]). An atom consists of an index into this arena, and a
/// copy number.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Atom(u32);

impl Atom {
    /// Deconstruct an atom into a constraint index and a copy number.
    #[inline]
    fn into_index_and_copy(self) -> (u32, u8) {
        let copy = self.0 >> 24;
        let index = self.0 & 0x00ff_ffff;
        (index, copy as u8)
    }

    /// Construct an atom from a constraint index and a copy number.
    #[inline]
    fn from_index_and_copy(index: u32, copy: u8) -> Self {
        debug_assert!(index <= 0x00ff_ffff);
        Self((u32::from(copy) << 24) | index)
    }
}

// A custom Debug implementation that prints out the constraint index and copy number as distinct
// fields.
impl std::fmt::Debug for Atom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (index, copy) = self.into_index_and_copy();
        f.debug_tuple("Atom").field(&index).field(&copy).finish()
    }
}

impl ScopedVisibilityConstraintId {
    /// A special ID that is used for an "always true" / "always visible" constraint.
    pub(crate) const ALWAYS_TRUE: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId(0xffff_ffff);

    /// A special ID that is used for an ambiguous constraint.
    pub(crate) const AMBIGUOUS: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId(0xffff_fffe);

    /// A special ID that is used for an "always false" / "never visible" constraint.
    pub(crate) const ALWAYS_FALSE: ScopedVisibilityConstraintId =
        ScopedVisibilityConstraintId(0xffff_fffd);

    fn is_terminal(self) -> bool {
        self.0 >= SMALLEST_TERMINAL
    }
}

// Rebind some constants locally so that we don't need as many qualifiers below.
const ALWAYS_TRUE: ScopedVisibilityConstraintId = ScopedVisibilityConstraintId::ALWAYS_TRUE;
const AMBIGUOUS: ScopedVisibilityConstraintId = ScopedVisibilityConstraintId::AMBIGUOUS;
const ALWAYS_FALSE: ScopedVisibilityConstraintId = ScopedVisibilityConstraintId::ALWAYS_FALSE;
const SMALLEST_TERMINAL: u32 = ALWAYS_FALSE.0;

/// A collection of visibility constraints. This is currently stored in `UseDefMap`, which means we
/// maintain a separate set of visibility constraints for each scope in file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VisibilityConstraints<'db> {
    constraints: Vec<Constraint<'db>>,
    interiors: Vec<InteriorNode>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct VisibilityConstraintsBuilder<'db> {
    constraints: Vec<Constraint<'db>>,
    interiors: Vec<InteriorNode>,
    constraint_cache: FxHashMap<Constraint<'db>, u32>,
    interior_cache: FxHashMap<InteriorNode, u32>,
    not_cache: FxHashMap<ScopedVisibilityConstraintId, ScopedVisibilityConstraintId>,
    and_cache: FxHashMap<
        (ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
        ScopedVisibilityConstraintId,
    >,
    or_cache: FxHashMap<
        (ScopedVisibilityConstraintId, ScopedVisibilityConstraintId),
        ScopedVisibilityConstraintId,
    >,
}

impl<'db> VisibilityConstraintsBuilder<'db> {
    pub(crate) fn build(self) -> VisibilityConstraints<'db> {
        VisibilityConstraints {
            constraints: self.constraints,
            interiors: self.interiors,
        }
    }

    fn interior_node(&self, a: ScopedVisibilityConstraintId) -> InteriorNode {
        debug_assert!(!a.is_terminal());
        self.interiors[a.0 as usize]
    }

    /// Returns whether `a` or `b` has a "larger" atom. TDDs are ordered such that interior nodes
    /// can only have edges to "larger" nodes. Terminals are considered to have a larger atom than
    /// any internal node, since they are leaf nodes.
    fn cmp_atoms(
        &self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> Ordering {
        if a == b {
            Ordering::Equal
        } else if a.is_terminal() {
            Ordering::Greater
        } else if b.is_terminal() {
            Ordering::Less
        } else {
            self.interior_node(a).atom.cmp(&self.interior_node(b).atom)
        }
    }

    /// Adds a constraint, ensuring that we only store any particular constraint once.
    #[allow(clippy::cast_possible_truncation)]
    fn add_constraint(&mut self, constraint: Constraint<'db>, copy: u8) -> Atom {
        let index = *self.constraint_cache.entry(constraint).or_insert_with(|| {
            let index = self.constraints.len() as u32;
            self.constraints.push(constraint);
            index
        });
        Atom::from_index_and_copy(index, copy)
    }

    /// Adds an interior node, ensuring that we always use the same visibility constraint ID for
    /// equal nodes.
    #[allow(clippy::cast_possible_truncation)]
    fn add_interior(&mut self, node: InteriorNode) -> ScopedVisibilityConstraintId {
        // If the true and false branches lead to the same node, we can override the ambiguous
        // branch to go there too. And this node is then redundant and can be reduced.
        if node.if_true == node.if_false {
            return node.if_true;
        }

        let index = *self.interior_cache.entry(node).or_insert_with(|| {
            let index = self.interiors.len() as u32;
            self.interiors.push(node);
            index
        });
        debug_assert!(index < SMALLEST_TERMINAL);
        ScopedVisibilityConstraintId(index)
    }

    /// Adds a new visibility constraint that checks a single [`Constraint`]. Provide different
    /// values for `copy` if you need to model that the constraint can evaluate to different
    /// results at different points in the execution of the program being modeled.
    pub(crate) fn add_atom(
        &mut self,
        constraint: Constraint<'db>,
        copy: u8,
    ) -> ScopedVisibilityConstraintId {
        let atom = self.add_constraint(constraint, copy);
        self.add_interior(InteriorNode {
            atom,
            if_true: ALWAYS_TRUE,
            if_ambiguous: AMBIGUOUS,
            if_false: ALWAYS_FALSE,
        })
    }

    /// Adds a new visibility constraint that is the ternary NOT of an existing one.
    pub(crate) fn add_not_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        if a == ALWAYS_TRUE {
            return ALWAYS_FALSE;
        } else if a == AMBIGUOUS {
            return AMBIGUOUS;
        } else if a == ALWAYS_FALSE {
            return ALWAYS_TRUE;
        }

        if let Some(cached) = self.not_cache.get(&a) {
            return *cached;
        }
        let a_node = self.interiors[a.0 as usize];
        let if_true = self.add_not_constraint(a_node.if_true);
        let if_ambiguous = self.add_not_constraint(a_node.if_ambiguous);
        let if_false = self.add_not_constraint(a_node.if_false);
        let result = self.add_interior(InteriorNode {
            atom: a_node.atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.not_cache.insert(a, result);
        result
    }

    /// Adds a new visibility constraint that is the ternary OR of two existing ones.
    pub(crate) fn add_or_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (a, b) {
            (ALWAYS_TRUE, _) | (_, ALWAYS_TRUE) => return ALWAYS_TRUE,
            (ALWAYS_FALSE, other) | (other, ALWAYS_FALSE) => return other,
            (AMBIGUOUS, AMBIGUOUS) => return AMBIGUOUS,
            _ => {}
        }

        // OR is commutative, which lets us halve the cache requirements
        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.or_cache.get(&(a, b)) {
            return *cached;
        }

        let (atom, if_true, if_ambiguous, if_false) = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interior_node(a);
                let b_node = self.interior_node(b);
                let if_true = self.add_or_constraint(a_node.if_true, b_node.if_true);
                let if_false = self.add_or_constraint(a_node.if_false, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a_node.if_ambiguous, b_node.if_ambiguous)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Less => {
                let a_node = self.interior_node(a);
                let if_true = self.add_or_constraint(a_node.if_true, b);
                let if_false = self.add_or_constraint(a_node.if_false, b);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a_node.if_ambiguous, b)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Greater => {
                let b_node = self.interior_node(b);
                let if_true = self.add_or_constraint(a, b_node.if_true);
                let if_false = self.add_or_constraint(a, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_or_constraint(a, b_node.if_ambiguous)
                };
                (b_node.atom, if_true, if_ambiguous, if_false)
            }
        };

        let result = self.add_interior(InteriorNode {
            atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.or_cache.insert((a, b), result);
        result
    }

    /// Adds a new visibility constraint that is the ternary AND of two existing ones.
    pub(crate) fn add_and_constraint(
        &mut self,
        a: ScopedVisibilityConstraintId,
        b: ScopedVisibilityConstraintId,
    ) -> ScopedVisibilityConstraintId {
        match (a, b) {
            (ALWAYS_FALSE, _) | (_, ALWAYS_FALSE) => return ALWAYS_FALSE,
            (ALWAYS_TRUE, other) | (other, ALWAYS_TRUE) => return other,
            (AMBIGUOUS, AMBIGUOUS) => return AMBIGUOUS,
            _ => {}
        }

        // AND is commutative, which lets us halve the cache requirements
        let (a, b) = if b.0 < a.0 { (b, a) } else { (a, b) };
        if let Some(cached) = self.and_cache.get(&(a, b)) {
            return *cached;
        }

        let (atom, if_true, if_ambiguous, if_false) = match self.cmp_atoms(a, b) {
            Ordering::Equal => {
                let a_node = self.interior_node(a);
                let b_node = self.interior_node(b);
                let if_true = self.add_and_constraint(a_node.if_true, b_node.if_true);
                let if_false = self.add_and_constraint(a_node.if_false, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a_node.if_ambiguous, b_node.if_ambiguous)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Less => {
                let a_node = self.interior_node(a);
                let if_true = self.add_and_constraint(a_node.if_true, b);
                let if_false = self.add_and_constraint(a_node.if_false, b);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a_node.if_ambiguous, b)
                };
                (a_node.atom, if_true, if_ambiguous, if_false)
            }
            Ordering::Greater => {
                let b_node = self.interior_node(b);
                let if_true = self.add_and_constraint(a, b_node.if_true);
                let if_false = self.add_and_constraint(a, b_node.if_false);
                let if_ambiguous = if if_true == if_false {
                    if_true
                } else {
                    self.add_and_constraint(a, b_node.if_ambiguous)
                };
                (b_node.atom, if_true, if_ambiguous, if_false)
            }
        };

        let result = self.add_interior(InteriorNode {
            atom,
            if_true,
            if_ambiguous,
            if_false,
        });
        self.and_cache.insert((a, b), result);
        result
    }
}

impl<'db> VisibilityConstraints<'db> {
    fn interior_node(&self, a: ScopedVisibilityConstraintId) -> InteriorNode {
        debug_assert!(!a.is_terminal());
        self.interiors[a.0 as usize]
    }

    /// Analyze the statically known visibility for a given visibility constraint.
    pub(crate) fn evaluate(
        &self,
        db: &'db dyn Db,
        mut id: ScopedVisibilityConstraintId,
    ) -> Truthiness {
        loop {
            let node = match id {
                ALWAYS_TRUE => return Truthiness::AlwaysTrue,
                AMBIGUOUS => return Truthiness::Ambiguous,
                ALWAYS_FALSE => return Truthiness::AlwaysFalse,
                _ => self.interior_node(id),
            };
            let (index, _) = node.atom.into_index_and_copy();
            let constraint = &self.constraints[index as usize];
            match Self::analyze_single(db, constraint) {
                Truthiness::AlwaysTrue => id = node.if_true,
                Truthiness::Ambiguous => id = node.if_ambiguous,
                Truthiness::AlwaysFalse => id = node.if_false,
            }
        }
    }

    fn analyze_single(db: &dyn Db, constraint: &Constraint) -> Truthiness {
        match constraint.node {
            ConstraintNode::Expression(test_expr) => {
                let inference = infer_expression_types(db, test_expr);
                let scope = test_expr.scope(db);
                let ty = inference
                    .expression_type(test_expr.node_ref(db).scoped_expression_id(db, scope));

                ty.bool(db).negate_if(!constraint.is_positive)
            }
            ConstraintNode::Pattern(inner) => match inner.kind(db) {
                PatternConstraintKind::Value(value, guard) => {
                    let subject_expression = inner.subject(db);
                    let inference = infer_expression_types(db, *subject_expression);
                    let scope = subject_expression.scope(db);
                    let subject_ty = inference.expression_type(
                        subject_expression
                            .node_ref(db)
                            .scoped_expression_id(db, scope),
                    );

                    let inference = infer_expression_types(db, *value);
                    let scope = value.scope(db);
                    let value_ty = inference
                        .expression_type(value.node_ref(db).scoped_expression_id(db, scope));

                    if subject_ty.is_single_valued(db) {
                        let truthiness =
                            Truthiness::from(subject_ty.is_equivalent_to(db, value_ty));

                        if truthiness.is_always_true() && guard.is_some() {
                            // Fall back to ambiguous, the guard might change the result.
                            Truthiness::Ambiguous
                        } else {
                            truthiness
                        }
                    } else {
                        Truthiness::Ambiguous
                    }
                }
                PatternConstraintKind::Singleton(..)
                | PatternConstraintKind::Class(..)
                | PatternConstraintKind::Unsupported => Truthiness::Ambiguous,
            },
        }
    }
}
