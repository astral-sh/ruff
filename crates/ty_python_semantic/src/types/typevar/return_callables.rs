//! Helpers for moving type variables from an enclosing generic context to a returned `Callable`.
//!
//! Python users often write factories whose return annotation is a `Callable` containing type
//! variables that are not mentioned anywhere else in the factory signature:
//!
//! ```python
//! def decorator_factory[T]() -> Callable[[T], T]: ...
//! ```
//!
//! The public type of `decorator_factory` should not be generic itself. Instead, the callable value
//! returned by `decorator_factory()` is generic. This module finds eligible type-variable
//! occurrences that are fully covered by returned callable values, rewrites those callable
//! signatures to use renamed `T'return` type variables, and attaches a generic context to the
//! rewritten callables.
//!
//! The caller supplies the eligibility predicate. That lets the same traversal serve definition-time
//! signature cleanup and, later, post-call-result cleanup without structurally capturing unrelated
//! type variables from enclosing scopes.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::type_alias::{walk_manual_pep_695_type_alias, walk_pep_695_type_alias};
use crate::types::typevar::BoundTypeVarIdentity;
use crate::types::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    ApplySpecialization, ApplyTypeMappingVisitor, BoundTypeVarInstance, CallableType, FunctionType,
    GenericContext, Type, TypeAliasType, TypeContext, TypeMapping,
};
use crate::{Db, FxIndexMap};

/// Where the currently visited type appears relative to the return value being processed.
#[derive(Clone, Copy, Eq, PartialEq)]
enum TraversalPosition {
    /// A type that is not part of the return value. Eligible type-variable occurrences here make
    /// the variable ineligible for returned-callable rebinding.
    Outside,
    /// A value position in the return type. Non-generic callables reached from this position can
    /// become owners of covered type variables.
    ReturnedValue,
    /// A parameter annotation inside a returned callable signature. Nested callables found here do
    /// not create new ownership frames; their type-variable occurrences belong to the surrounding
    /// callable frame.
    Parameter,
}

/// A returned-callable frame while its signature is being traversed.
struct Frame<'db> {
    /// The callable occurrence that may receive a generic context when the frame is finalized.
    callable: CallableType<'db>,
    /// Eligible type variables seen directly in this callable frame, excluding variables that are
    /// currently owned by a nested returned callable.
    direct_typevars: FxHashSet<BoundTypeVarIdentity<'db>>,
    /// Nested returned callables that have been fully traversed.
    children: Vec<CompletedFrame<'db>>,
}

impl<'db> Frame<'db> {
    fn new(callable: CallableType<'db>) -> Self {
        Self {
            callable,
            direct_typevars: FxHashSet::default(),
            children: Vec::new(),
        }
    }

    /// Assign ownership for this callable after all nested returned callables have been seen.
    ///
    /// A type variable is owned by the innermost returned callable that covers all of its eligible
    /// occurrences. Direct occurrences force ownership into this frame. A variable that appears in
    /// multiple nested returned callables is also promoted into this frame, because no single child
    /// covers all occurrences. Once promoted, it is removed from every child frame.
    fn finish(mut self) -> CompletedFrame<'db> {
        let mut seen_once = FxHashSet::default();
        let mut seen_multiple = FxHashSet::default();
        for child in &self.children {
            let mut child_typevars = FxHashSet::default();
            child.collect_subtree_typevars(&mut child_typevars);
            for typevar in child_typevars {
                if !seen_once.insert(typevar) {
                    seen_multiple.insert(typevar);
                }
            }
        }

        self.direct_typevars.extend(seen_multiple);
        for child in &mut self.children {
            child.remove_typevars(&self.direct_typevars);
        }

        let mut all_typevars = self.direct_typevars.clone();
        for child in &self.children {
            all_typevars.extend(child.all_typevars.iter().copied());
        }

        CompletedFrame {
            callable: self.callable,
            typevars: self.direct_typevars,
            all_typevars,
            children: self.children,
        }
    }
}

/// A returned-callable frame after its ownership set has been finalized.
struct CompletedFrame<'db> {
    /// The original callable occurrence in the return type.
    callable: CallableType<'db>,
    /// Type variables owned by this callable after applying the innermost-cover rule.
    typevars: FxHashSet<BoundTypeVarIdentity<'db>>,
    /// All type variables owned anywhere in this completed subtree. This is maintained alongside
    /// `typevars` so parent frames can inspect a child subtree without recursively walking it.
    all_typevars: FxHashSet<BoundTypeVarIdentity<'db>>,
    /// Finalized nested returned callables whose remaining type variables were not promoted into
    /// this frame.
    children: Vec<CompletedFrame<'db>>,
}

impl<'db> CompletedFrame<'db> {
    /// Collect all type variables owned anywhere in this completed subtree.
    fn collect_subtree_typevars(&self, typevars: &mut FxHashSet<BoundTypeVarIdentity<'db>>) {
        typevars.extend(self.all_typevars.iter().copied());
    }

    /// Remove type variables that have been claimed by an enclosing frame.
    fn remove_typevars(&mut self, removed_typevars: &FxHashSet<BoundTypeVarIdentity<'db>>) {
        self.typevars
            .retain(|typevar| !removed_typevars.contains(typevar));
        self.all_typevars
            .retain(|typevar| !removed_typevars.contains(typevar));
        for child in &mut self.children {
            child.remove_typevars(removed_typevars);
        }
    }

    /// Build callable and type-variable replacement maps for this completed subtree.
    ///
    /// Per-callable replacement maps are created by iterating `representatives`, not by iterating
    /// hash sets, so generic-context display order follows first occurrence order in the original
    /// return type.
    fn collect_replacements(
        &self,
        db: &'db dyn Db,
        representatives: &FxIndexMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
        rebound_typevars: &mut FxIndexMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
        callable_replacements: &mut FxHashMap<CallableType<'db>, CallableType<'db>>,
    ) {
        // Build child replacements first so a rewritten parent callable can include rewritten
        // returned callables nested inside its own signature.
        for child in &self.children {
            child.collect_replacements(
                db,
                representatives,
                rebound_typevars,
                callable_replacements,
            );
        }

        if !self.typevars.is_empty() {
            let typevar_replacements: FxIndexMap<_, _> = representatives
                .iter()
                .filter_map(|(identity, typevar)| {
                    if self.typevars.contains(identity) {
                        rebound_typevars.entry(*identity).or_insert(*typevar);
                        Some((*typevar, typevar.with_name_suffix(db, "return")))
                    } else {
                        None
                    }
                })
                .collect();

            if !typevar_replacements.is_empty() {
                let signatures = if callable_replacements.is_empty() {
                    self.callable.signatures(db).clone()
                } else {
                    self.callable.signatures(db).apply_type_mapping_impl(
                        db,
                        &TypeMapping::RescopeReturnCallables(&*callable_replacements),
                        TypeContext::default(),
                        &ApplyTypeMappingVisitor::default(),
                    )
                };
                let apply = ApplySpecialization::ReturnCallables(&typevar_replacements);
                let signatures = signatures.apply_type_mapping_impl(
                    db,
                    &TypeMapping::ApplySpecialization(apply),
                    TypeContext::default(),
                    &ApplyTypeMappingVisitor::default(),
                );
                let generic_context = GenericContext::from_typevar_instances(
                    db,
                    typevar_replacements.values().copied(),
                );
                let signatures = signatures.with_inherited_generic_context(db, generic_context);
                let replacement = CallableType::new(
                    db,
                    signatures,
                    self.callable.kind(db),
                    self.callable.provenance(db),
                );
                callable_replacements.insert(self.callable, replacement);
            }
        }
    }
}

/// Traverses outside roots and a return type to find returned-callable type-variable ownership.
struct ReturnedCallableTypeVarCollector<'db, 'a> {
    /// Use-case-specific filter for type variables that may be rebound.
    eligible: &'a dyn Fn(BoundTypeVarInstance<'db>) -> bool,
    /// The position of the type currently being visited. The standard type visitor APIs do not
    /// carry this distinction, so this visitor maintains it explicitly while manually traversing
    /// callable signatures.
    position: Cell<TraversalPosition>,
    /// Recursion guard for the current traversal path. Types are removed when leaving the path so
    /// repeated sibling occurrences are still counted.
    active_types: RefCell<FxHashSet<Type<'db>>>,
    /// Stack of returned callables whose signatures are currently being traversed.
    active_frames: RefCell<Vec<Frame<'db>>>,
    /// Top-level returned-callable frames that have been fully traversed.
    completed_frames: RefCell<Vec<CompletedFrame<'db>>>,
    /// Eligible type variables that were seen with no active returned-callable owner.
    ineligible_typevars: RefCell<FxHashSet<BoundTypeVarIdentity<'db>>>,
    /// First normalized occurrence of each eligible type variable, used for stable replacement
    /// ordering and for constructing renamed type variables.
    representatives: RefCell<FxIndexMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>>,
}

impl<'db, 'a> ReturnedCallableTypeVarCollector<'db, 'a> {
    fn new(eligible: &'a dyn Fn(BoundTypeVarInstance<'db>) -> bool) -> Self {
        Self {
            eligible,
            position: Cell::new(TraversalPosition::Outside),
            active_types: RefCell::default(),
            active_frames: RefCell::default(),
            completed_frames: RefCell::default(),
            ineligible_typevars: RefCell::default(),
            representatives: RefCell::default(),
        }
    }

    fn visit_outside_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        self.visit_with_position(db, ty, TraversalPosition::Outside);
    }

    fn visit_returned_value_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        self.visit_with_position(db, ty, TraversalPosition::ReturnedValue);
    }

    fn visit_with_position(&self, db: &'db dyn Db, ty: Type<'db>, position: TraversalPosition) {
        let previous = self.position.replace(position);
        self.visit_type(db, ty);
        self.position.set(previous);
    }

    /// Manually traverse callable signatures without visiting generic-context metadata or
    /// parameter default types.
    fn visit_callable_signatures(&self, db: &'db dyn Db, callable: CallableType<'db>) {
        let signature_position = self.position.get();
        for signature in callable.signatures(db) {
            self.visit_signature(db, signature, signature_position);
        }
    }

    /// Traverse one signature, preserving the distinction between parameter annotations and return
    /// annotations.
    fn visit_signature(
        &self,
        db: &'db dyn Db,
        signature: &crate::types::Signature<'db>,
        signature_position: TraversalPosition,
    ) {
        let (parameter_position, return_position) = match signature_position {
            TraversalPosition::ReturnedValue => (
                TraversalPosition::Parameter,
                TraversalPosition::ReturnedValue,
            ),
            TraversalPosition::Parameter => {
                (TraversalPosition::Parameter, TraversalPosition::Parameter)
            }
            TraversalPosition::Outside => (TraversalPosition::Outside, TraversalPosition::Outside),
        };

        for parameter in signature.parameters() {
            self.visit_with_position(db, parameter.annotated_type(), parameter_position);
        }
        self.visit_with_position(db, signature.return_ty, return_position);
    }

    /// Record one eligible type-variable occurrence at the current traversal position.
    ///
    /// `P.args` and `P.kwargs` are normalized to their base `P`, because rebinding moves the
    /// `ParamSpec` itself to the returned callable's generic context.
    fn record_typevar(&self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) {
        let typevar = if typevar.is_paramspec(db) {
            typevar.without_paramspec_attr(db)
        } else {
            typevar
        };

        if !(self.eligible)(typevar) {
            return;
        }

        let identity = typevar.identity(db);
        self.representatives
            .borrow_mut()
            .entry(identity)
            .or_insert(typevar);

        if let Some(frame) = self.active_frames.borrow_mut().last_mut() {
            frame.direct_typevars.insert(identity);
        } else {
            self.ineligible_typevars.borrow_mut().insert(identity);
        }
    }
}

impl<'db> TypeVisitor<'db> for ReturnedCallableTypeVarCollector<'db, '_> {
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if !self.active_types.borrow_mut().insert(ty) {
            return;
        }

        // We dispatch manually instead of using `walk_type_with_recursion_guard`, because this
        // traversal needs to pop the recursion guard when leaving the current path. Otherwise, a
        // repeated callable or typevar in a sibling branch would be skipped and ownership could be
        // assigned to a child that does not cover all occurrences.
        match TypeKind::from(ty) {
            TypeKind::Atomic => {}
            TypeKind::NonAtomic(non_atomic_type) => {
                walk_non_atomic_type(db, non_atomic_type, self);
            }
        }

        self.active_types.borrow_mut().remove(&ty);
    }

    fn visit_bound_type_var_type(&self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) {
        self.record_typevar(db, typevar);
    }

    fn visit_callable_type(&self, db: &'db dyn Db, callable: CallableType<'db>) {
        // Only callables that are themselves returned values can own type variables. If a returned
        // callable's parameter annotation contains another callable, the nested callable is part of
        // the surrounding callable's signature and should not become independently generic.
        let should_create_frame = self.position.get() == TraversalPosition::ReturnedValue
            && callable
                .signatures(db)
                .iter()
                .all(|signature| signature.generic_context.is_none());

        if should_create_frame {
            self.active_frames.borrow_mut().push(Frame::new(callable));
            self.visit_callable_signatures(db, callable);
            let completed = self.active_frames.borrow_mut().pop().map(Frame::finish);
            if let Some(completed) = completed {
                if let Some(parent) = self.active_frames.borrow_mut().last_mut() {
                    parent.children.push(completed);
                } else {
                    self.completed_frames.borrow_mut().push(completed);
                }
            }
        } else {
            self.visit_callable_signatures(db, callable);
        }
    }

    fn visit_function_type(&self, db: &'db dyn Db, function: FunctionType<'db>) {
        // Function types are visited for type-variable occurrences, but they are not rewritten by
        // this helper and never create returned-callable ownership frames.
        let signature_position = self.position.get();
        function.visit_updated_signatures(db, |signature| {
            self.visit_signature(db, signature, signature_position);
        });
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, type_alias: TypeAliasType<'db>) {
        // Keep broad lazy-attribute traversal disabled, but preserve the historical special case
        // for PEP 695 type aliases. This lets aliases hide returned callables without forcing all
        // lazy type attributes during signature construction.
        match type_alias {
            TypeAliasType::PEP695(type_alias) => {
                walk_pep_695_type_alias(db, type_alias, self);
            }
            TypeAliasType::ManualPEP695(type_alias) => {
                walk_manual_pep_695_type_alias(db, type_alias, self);
            }
        }
    }
}

/// Rebind eligible type variables that are fully covered by returned callable values.
///
/// `outside_roots` are traversed with no active callable frame; any eligible type variable found
/// there is left in its original scope. `return_type` is traversed as a returned-value root, so
/// non-generic callables found structurally inside it can own type variables according to the
/// innermost-cover rule.
///
/// Returns the rewritten return type and a map containing the original type variables that were
/// rebound, keyed by identity.
pub(crate) fn rebind_return_callables<'db>(
    db: &'db dyn Db,
    outside_roots: impl IntoIterator<Item = Type<'db>>,
    return_type: Type<'db>,
    eligible: impl Fn(BoundTypeVarInstance<'db>) -> bool,
) -> (
    Type<'db>,
    FxIndexMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
) {
    let collector = ReturnedCallableTypeVarCollector::new(&eligible);
    for root in outside_roots {
        collector.visit_outside_type(db, root);
    }
    collector.visit_returned_value_type(db, return_type);

    let ineligible_typevars = collector.ineligible_typevars.into_inner();
    let mut completed_frames = collector.completed_frames.into_inner();
    for frame in &mut completed_frames {
        frame.remove_typevars(&ineligible_typevars);
    }

    let representatives = collector.representatives.into_inner();
    let mut rebound_typevars = FxIndexMap::default();
    let mut callable_replacements = FxHashMap::default();
    for frame in &completed_frames {
        frame.collect_replacements(
            db,
            &representatives,
            &mut rebound_typevars,
            &mut callable_replacements,
        );
    }

    if callable_replacements.is_empty() {
        return (return_type, rebound_typevars);
    }

    let type_mapping = TypeMapping::RescopeReturnCallables(&callable_replacements);
    let return_type = return_type.apply_type_mapping(db, &type_mapping, TypeContext::default());

    (return_type, rebound_typevars)
}
