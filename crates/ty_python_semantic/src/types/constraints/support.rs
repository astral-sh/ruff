//! Tracks the support of each constraint and interior node in a BDD.
//!
//! The support of a constraint is the set of typevars mentioned anywhere in the constraint
//! (either the subject, or anywhere in the lower or upper bound).
//!
//! The support of a node is the union of the supports of every constraint reachable from that
//! node.

use std::cell::{Cell, RefCell};
use std::ops::{BitOrAssign, Sub};

use crate::types::constraints::variables::{AtomicConstraint, Constraint};
use crate::types::constraints::{ConstraintId, ConstraintSetStorage, NodeId, TypeVarId};
use crate::types::function::FunctionType;
use crate::types::generics::walk_specialization_types;
use crate::types::protocol_class::{ProtocolInterfaceView, walk_protocol_instance_interface};
use crate::types::signatures::{Signature, walk_signature};
use crate::types::typevar::{TypeVarBoundOrConstraints, TypeVarInstance, TypeVarSet};
use crate::types::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    BoundTypeVarInstance, GenericAlias, ProtocolInstanceType, RecursiveType, Type, TypeAliasType,
};
use crate::{Db, ProgramEnvironment};

use ruff_index::newtype_index;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

#[newtype_index]
#[derive(get_size2::GetSize)]
pub(super) struct SupportId;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct Support {
    chunks: SmallVec<[usize; 2]>,
    has_skipped_lazy_attributes: bool,
}

const CHUNK_SIZE: usize = usize::BITS as usize;

impl Support {
    pub(super) fn from_typevars(typevars: impl IntoIterator<Item = TypeVarId>) -> Self {
        let mut result = Self::default();
        for typevar in typevars {
            result.insert(typevar);
        }
        result
    }

    pub(super) fn from_typevar_set<'db>(
        db: &'db dyn Db,
        storage: &mut ConstraintSetStorage<'db>,
        typevars: TypeVarSet<'db>,
    ) -> Self {
        let mut result = Self::default();
        for typevar in typevars.iter(db) {
            let typevar = storage.intern_typevar(db, typevar);
            result.insert(typevar);
        }
        result
    }

    /// Adds a typevar to this support.
    pub(super) fn insert(&mut self, typevar: TypeVarId) {
        let index = typevar.index();
        let chunks_needed = (index + 1).div_ceil(CHUNK_SIZE);
        if self.chunks.len() < chunks_needed {
            self.chunks.resize(chunks_needed, 0);
        }

        let chunk_index = index / CHUNK_SIZE;
        let bit_index_within_chunk = index % CHUNK_SIZE;
        let bit_mask_within_chunk = 1 << bit_index_within_chunk;
        self.chunks[chunk_index] |= bit_mask_within_chunk;
    }

    /// Removes and returns an arbitrary typevar from this support.
    pub(super) fn pop(&mut self) -> Option<TypeVarId> {
        let (idx, first_nonempty_chunk) = self
            .chunks
            .iter_mut()
            .enumerate()
            .find(|(_, chunk)| **chunk != 0)?;
        let first_set_bit_in_chunk = first_nonempty_chunk.lowest_one()? as usize;

        // Clear out the bit we just found, and then return it
        *first_nonempty_chunk ^= 1 << first_set_bit_in_chunk;
        Some(TypeVarId::from_usize(
            CHUNK_SIZE * idx + first_set_bit_in_chunk,
        ))
    }

    fn iter_chunks(&self) -> impl Iterator<Item = usize> + '_ {
        self.chunks.iter().copied()
    }

    /// Returns an iterator of all of the typevars in this support.
    pub(super) fn iter(&self) -> impl Iterator<Item = TypeVarId> + '_ {
        // Iterate through all of the chunks
        let mut next_chunk_start = 0;
        self.iter_chunks().flat_map(move |mut chunk| {
            // Figure out the starting index of this chunk
            let chunk_start = next_chunk_start;
            next_chunk_start += CHUNK_SIZE;

            // Iterate through the set bits in this chunk
            std::iter::from_fn(move || {
                // Find the lowest set bit, if there is one
                let index = chunk.lowest_one()? as usize;

                // Clear out the bit we just found.
                chunk ^= 1 << index;

                // And then return it, converted into a TypeVarId
                Some(TypeVarId::from_usize(chunk_start + index))
            })
        })
    }

    /// Returns whether this support contains any typevars that are not in `other`.
    pub(super) fn contains_more_than(&self, other: &Self) -> bool {
        let lhs = self.iter_chunks();
        let rhs = std::iter::chain(other.iter_chunks(), std::iter::repeat(0));
        std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (lhs & !rhs) != 0)
    }

    /// Returns whether this support contains any type variables in common with `other`.
    pub(super) fn overlaps_with(&self, other: &Self) -> bool {
        let lhs = self.iter_chunks();
        let rhs = other.iter_chunks();
        std::iter::zip(lhs, rhs).any(|(lhs, rhs)| (lhs & rhs) != 0)
    }

    /// Records that lazy type attributes may contain additional type variables.
    pub(super) fn mark_incomplete(&mut self) {
        self.has_skipped_lazy_attributes = true;
    }

    /// Returns whether all type attributes were inspected while collecting this support.
    pub(super) fn is_complete(&self) -> bool {
        !self.has_skipped_lazy_attributes
    }

    /// Closes this support over a set of constraints.
    ///
    /// We perform a fixed-point loop, where we find the constraints that mention any of the
    /// typevars in the support, and add any _other_ typevars they mention. (That might add
    /// additional typevars that cause more constraints to become eligible, and so on.)
    #[expect(clippy::needless_pass_by_value)]
    pub(super) fn close_over_constraints(
        &mut self,
        storage: &ConstraintSetStorage<'_>,
        constraints: impl Iterator<Item = ConstraintId> + Clone,
    ) {
        loop {
            let mut any_added = false;
            for constraint in constraints.clone() {
                let constraint_support = storage.constraint_support(constraint);
                if constraint_support.overlaps_with(self)
                    && constraint_support.contains_more_than(self)
                {
                    any_added = true;
                    *self |= constraint_support;
                }
            }

            if !any_added {
                return;
            }
        }
    }
}

impl BitOrAssign<&Self> for Support {
    fn bitor_assign(&mut self, rhs: &Self) {
        if self.chunks.len() < rhs.chunks.len() {
            self.chunks.resize(rhs.chunks.len(), 0);
        }
        for (lhs, rhs) in std::iter::zip(&mut self.chunks, &rhs.chunks) {
            *lhs |= *rhs;
        }
        self.has_skipped_lazy_attributes |= rhs.has_skipped_lazy_attributes;
    }
}

impl BitOrAssign<Option<&Self>> for Support {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}

impl Sub<&Support> for &Support {
    type Output = Support;

    fn sub(self, rhs: &Support) -> Support {
        let mut result = self.clone();
        for (lhs, rhs) in std::iter::zip(&mut result.chunks, &rhs.chunks) {
            *lhs &= !(*rhs);
        }
        result
    }
}

/// Whether a quantified body and its declared domains depend only on bound type variables.
///
/// Ordinary support retains quantified locals and can omit lazy protocol members. Neither is
/// suitable for deciding whether an existential can be evaluated independently of its context.
pub(super) fn is_closed<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    storage: &mut ConstraintSetStorage<'db>,
    body: NodeId,
    locals: &Support,
) -> bool {
    let visitor = ClosedBodyVisitor {
        env,
        storage: RefCell::new(storage),
        bound: RefCell::default(),
        closed: Cell::new(true),
        types: RefCell::default(),
        nodes: RefCell::default(),
    };
    visitor.with_locals(db, locals, || visitor.visit_node(db, body));
    visitor.closed.get()
}

struct ClosedBodyVisitor<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    storage: RefCell<&'a mut ConstraintSetStorage<'db>>,
    bound: RefCell<Support>,
    closed: Cell<bool>,
    // A type or node can occur inside and outside the scope of the same binder.
    types: RefCell<FxHashMap<Type<'db>, SmallVec<[Support; 1]>>>,
    nodes: RefCell<FxHashSet<(NodeId, Support)>>,
}

impl<'db> ClosedBodyVisitor<'_, 'db> {
    fn already_visited(&self, ty: Type<'db>) -> bool {
        let bound = self.bound.borrow();
        let mut types = self.types.borrow_mut();
        let contexts = types.entry(ty).or_default();
        // Binding additional variables cannot introduce a free occurrence. This also stops
        // recursive definitions whose methods add their own binders on the way back to the type.
        if contexts
            .iter()
            .any(|context| !context.contains_more_than(&bound))
        {
            return true;
        }
        contexts.push(bound.clone());
        false
    }

    fn with_locals(&self, db: &'db dyn Db, locals: &Support, visit: impl FnOnce()) {
        let previous = self.bound.borrow().clone();
        *self.bound.borrow_mut() |= locals;
        for local in (locals - &previous).iter() {
            let local = self.storage.borrow().typevar_data(local);
            match local.typevar(db).bound_or_constraints(db, self.env) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => self.visit_type(db, bound),
                Some(TypeVarBoundOrConstraints::Constraints(bounds)) => {
                    for bound in bounds.elements(db) {
                        self.visit_type(db, *bound);
                    }
                }
                None => {}
            }
        }
        visit();
        self.bound.replace(previous);
    }

    fn locals(
        &self,
        db: &'db dyn Db,
        locals: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
    ) -> Support {
        let mut storage = self.storage.borrow_mut();
        Support::from_typevars(
            locals
                .into_iter()
                .map(|local| storage.intern_typevar(db, local)),
        )
    }

    fn visit_node(&self, db: &'db dyn Db, node: NodeId) {
        if !self.closed.get()
            || node.is_terminal()
            || !self
                .nodes
                .borrow_mut()
                .insert((node, self.bound.borrow().clone()))
        {
            return;
        }
        let interior = self.storage.borrow().interior_node_data(node);
        let constraint = self
            .storage
            .borrow()
            .constraint_data(interior.constraint)
            .clone();
        match constraint {
            Constraint::Existential(existential) => {
                self.with_locals(db, &existential.locals, || {
                    self.visit_node(db, existential.body);
                });
            }
            Constraint::Atomic(atomic) => {
                let (left, right) = match atomic {
                    AtomicConstraint::ConcreteLower(bound) => {
                        (Type::TypeVar(bound.typevar), bound.bound)
                    }
                    AtomicConstraint::ConcreteUpper(bound) => {
                        (Type::TypeVar(bound.typevar), bound.bound)
                    }
                    AtomicConstraint::ConcreteEquivalence(bound) => {
                        (Type::TypeVar(bound.typevar), bound.bound)
                    }
                    AtomicConstraint::TypeVarRange(bound) => {
                        (Type::TypeVar(bound.left), Type::TypeVar(bound.right))
                    }
                    AtomicConstraint::TypeVarEquivalence(bound) => {
                        (Type::TypeVar(bound.left), Type::TypeVar(bound.right))
                    }
                };
                self.visit_type(db, left);
                self.visit_type(db, right);
            }
        }
        self.visit_node(db, interior.if_true);
        self.visit_node(db, interior.if_uncertain);
        self.visit_node(db, interior.if_false);
    }
}

impl<'db> TypeVisitor<'db> for ClosedBodyVisitor<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn notify_skipped_lazy_type_attributes(&self) {
        self.closed.set(false);
    }

    fn visit_type_var_type(&self, _db: &'db dyn Db, _typevar: TypeVarInstance<'db>) {}

    fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
        walk_specialization_types(db, alias.specialization(db), self);
    }

    fn visit_signature(&self, db: &'db dyn Db, signature: &Signature<'db>) {
        let locals = self.locals(
            db,
            signature
                .generic_context
                .into_iter()
                .flat_map(|context| context.variables(db)),
        );
        self.with_locals(db, &locals, || walk_signature(db, signature, self));
    }

    fn visit_function_type(&self, db: &'db dyn Db, function: FunctionType<'db>) {
        self.visit_callable_type(db, function.into_callable_type(db));
        if let Some(specialization) = function.class_specialization(db) {
            walk_specialization_types(db, specialization, self);
        }
    }

    fn visit_protocol_instance_type(&self, db: &'db dyn Db, protocol: ProtocolInstanceType<'db>) {
        if let Some((class, specialization)) = protocol
            .class_origin(db)
            .and_then(|class| class.static_class_literal(db))
        {
            if let Some(specialization) = specialization {
                walk_specialization_types(db, specialization, self);
            }
            let Some(origin) = class.into_protocol_class(db) else {
                self.closed.set(false);
                return;
            };
            let locals = self.locals(
                db,
                class
                    .generic_context(db)
                    .into_iter()
                    .flat_map(|context| context.variables(db)),
            );
            self.with_locals(db, &locals, || {
                // Inspect the definition separately from its arguments: recursively specializing
                // a protocol such as P[list[T]] need not converge to a single Type identity.
                if self.closed.get() && !self.already_visited(Type::ClassLiteral(class.into())) {
                    walk_protocol_instance_interface(
                        db,
                        ProtocolInterfaceView::new(origin.interface(db), None),
                        Type::ProtocolInstance(protocol),
                        self,
                    );
                }
            });
        } else {
            walk_protocol_instance_interface(
                db,
                protocol.interface(db),
                Type::ProtocolInstance(protocol),
                self,
            );
        }
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        let specialization = alias.specialization(db).or_else(|| {
            alias
                .generic_context(db)
                .map(|context| context.default_specialization(db, None))
        });
        if let Some(specialization) = specialization {
            walk_specialization_types(db, specialization, self);
        }
        let locals = self.locals(
            db,
            alias
                .generic_context(db)
                .into_iter()
                .flat_map(|context| context.variables(db)),
        );
        self.with_locals(db, &locals, || {
            if self.closed.get() && !self.already_visited(Type::TypeAlias(alias.unspecialized(db)))
            {
                self.visit_type(db, alias.raw_value_type(db));
            }
        });
    }

    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(arguments) = recursive.arguments(db) {
            walk_specialization_types(db, arguments, self);
        }
        let constructor = recursive.constructor(db);
        let locals = self.locals(
            db,
            recursive
                .parameters(db)
                .into_iter()
                .flat_map(|context| context.variables(db)),
        );
        self.with_locals(db, &locals, || {
            if self.closed.get() && !self.already_visited(Type::Recursive(constructor)) {
                self.visit_type(db, constructor.unfold(db, self.env).into_type());
            }
        });
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if !self.closed.get() {
            return;
        }
        if let Type::TypeVar(typevar) = ty {
            let id = self.storage.borrow_mut().intern_typevar(db, typevar);
            if !self.bound.borrow().iter().any(|bound| bound == id) {
                self.closed.set(false);
            }
            return;
        }
        if matches!(ty, Type::Divergent(_)) {
            self.closed.set(false);
            return;
        }
        let TypeKind::NonAtomic(non_atomic) = TypeKind::from(ty) else {
            return;
        };
        // Aliases have their own guard for their unspecialized definition.
        if matches!(ty, Type::TypeAlias(_) | Type::Recursive(_)) || !self.already_visited(ty) {
            walk_non_atomic_type(db, non_atomic, self);
        }
    }
}
