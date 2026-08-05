//! Cycle detection for recursive types.
//!
//! The visitors here ([`TypeTransformer`] and [`PairVisitor`]) are used in methods that
//! recursively visit types to transform them (e.g. [`Type::apply_type_mapping`]) or to
//! decide a relation between a pair of types (e.g. [`Type::has_relation_to`]).
//!
//! The typical pattern is that the "entry" method (e.g. [`Type::apply_type_mapping`]) will create
//! a visitor and pass it to the recursive method (e.g. [`Type::apply_type_mapping_impl`]).
//! Rust types that form part of a complex type (e.g. tuples, protocols, nominal instances, etc)
//! should usually just implement the recursive method, and all recursive calls should call the
//! recursive method and pass along the visitor.
//!
//! Not all recursive calls need to actually call `.visit` on the visitor; only when visiting types
//! that can create a recursive relationship (this includes, for example, type aliases and
//! protocols).
//!
//! There is a risk of double-visiting, for example if [`Type::apply_type_mapping_impl`] calls
//! `visitor.visit` when visiting a protocol type, and then internal `apply_type_mapping_impl`
//! methods of the Rust types implementing protocols also call `visitor.visit`. The best way to
//! avoid this is to prefer always calling `visitor.visit` only in the main recursive method on
//! `Type`.

use std::cell::{Cell, OnceCell, RefCell};
use std::cmp::Eq;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::types::function::FunctionLiteral;
use crate::types::generics::Specialization;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, GenericAlias, ProtocolInstanceType, Type,
    TypeAliasType, TypedDictType,
};
use crate::{Db, ProgramEnvironment};

/// The type identity used for recursive checks/transformations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum TypeIdentity<'db> {
    FunctionLiteral(FunctionLiteral<'db>),
    NewTypeInstance(Definition<'db>),
    RecursiveProtocol(Definition<'db>),
    RecursiveTypeAlias(Definition<'db>),
    RecursiveTypedDict(Definition<'db>),
    NonRecursive(Type<'db>),
}

impl<'db> Type<'db> {
    pub(crate) fn to_type_identity(self, db: &'db dyn Db) -> TypeIdentity<'db> {
        self.recursive_identity(db)
            .unwrap_or(TypeIdentity::NonRecursive(self))
    }

    /// Returns `false` if `self` and `other` cannot have the same [`TypeIdentity`].
    ///
    /// A `true` result is only a candidate match and must be confirmed with
    /// [`Type::to_type_identity`].
    pub(crate) fn may_share_type_identity(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Type::FunctionLiteral(a), Type::FunctionLiteral(b)) => a.literal(db) == b.literal(db),
            (Type::NewTypeInstance(a), Type::NewTypeInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::ProtocolInstance(a), Type::ProtocolInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::TypeAlias(a), Type::TypeAlias(b)) => a.definition(db) == b.definition(db),
            (Type::TypedDict(a), Type::TypedDict(b)) => a.definition(db) == b.definition(db),
            _ => false,
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn recursive_identity(self, db: &'db dyn Db) -> Option<TypeIdentity<'db>> {
        match self {
            // We can create a self-referential function type: e.g. `def f(x: "TypeOf[f]"): reveal_type(x)`
            // To avoid the difficulty of equality checking for function types containing this, we simply use `literal` for equality checking.
            Type::FunctionLiteral(function) => {
                Some(TypeIdentity::FunctionLiteral(function.literal(db)))
            }
            // Similarly, we can create a self-referential NewType: e.g. `T = NewType("T", list["T"])`
            Type::NewTypeInstance(newtype) => {
                Some(TypeIdentity::NewTypeInstance(newtype.definition(db)))
            }
            // Type aliases can be self-referential: e.g. `type RecursiveT = int | tuple[RecursiveT, ...]`
            Type::TypeAlias(alias) if alias.recursive_specializations_may_diverge(db) => {
                Some(TypeIdentity::RecursiveTypeAlias(alias.definition(db)))
            }
            Type::ProtocolInstance(protocol)
                if protocol.recursive_specializations_may_diverge(db) =>
            {
                Some(TypeIdentity::RecursiveProtocol(protocol.definition(db)?))
            }
            Type::TypedDict(typed_dict) if typed_dict.recursive_specializations_may_diverge(db) => {
                let definition = typed_dict.definition(db)?;
                Some(TypeIdentity::RecursiveTypedDict(definition))
            }
            _ => None,
        }
    }
}

struct DefinitionRecursionVisitor<'db> {
    env: ProgramEnvironment<'db>,
    target: Definition<'db>,
    active_definitions: RefCell<Vec<ActiveDefinition<'db>>>,
    may_diverge: Cell<bool>,
}

#[derive(Clone, Copy)]
struct DefinitionType<'db> {
    definition: Definition<'db>,
    identity_type: Type<'db>,
    specialization: Option<Specialization<'db>>,
}

struct ActiveDefinition<'db> {
    definition: Definition<'db>,
    specialization: Option<Specialization<'db>>,
    incoming_specialization: Option<Specialization<'db>>,
    unbounded_parameters: FxHashSet<BoundTypeVarIdentity<'db>>,
    // Paths to the target are retained because a growing cycle can be visited before or after the
    // path that leaves it.
    target_specializations: Vec<Specialization<'db>>,
}

struct ActiveDefinitionGuard<'a, 'db> {
    active_definitions: &'a RefCell<Vec<ActiveDefinition<'db>>>,
    depth: usize,
}

impl Drop for ActiveDefinitionGuard<'_, '_> {
    fn drop(&mut self) {
        self.active_definitions.borrow_mut().truncate(self.depth);
    }
}

struct DefinitionBodyVisitor<'a, 'db> {
    recursion: &'a DefinitionRecursionVisitor<'db>,
    specialization: Option<Specialization<'db>>,
    visited_types: TypeCollector<'db>,
}

impl<'db> DefinitionRecursionVisitor<'db> {
    /// Returns whether recursive specializations of `ty` can fail to reach an exact repetition.
    fn specializations_may_diverge(
        db: &'db dyn Db,
        ty: Type<'db>,
        target: Definition<'db>,
    ) -> bool {
        let visitor = Self::new(target);
        let Some(root) = DefinitionType::from_type(db, &visitor.env, ty) else {
            return false;
        };
        debug_assert_eq!(root.definition, target);
        visitor.visit_definition(db, root, None, root.specialization);
        visitor.may_diverge.get()
    }

    fn new(target: Definition<'db>) -> Self {
        Self {
            env: ProgramEnvironment::from_definition(target),
            target,
            active_definitions: RefCell::default(),
            may_diverge: Cell::new(false),
        }
    }

    fn visit_reference(&self, db: &'db dyn Db, reference: DefinitionType<'db>) {
        if reference.definition == self.target {
            self.record_target_specializations(db, reference.specialization);
        }

        let current_specialization = self
            .active_definitions
            .borrow()
            .last()
            .and_then(|active| active.specialization);
        let specialization = match (reference.specialization, current_specialization) {
            (Some(reference), Some(current)) => Some(reference.apply_specialization(db, current)),
            (reference, _) => reference,
        };
        self.visit_definition(db, reference, reference.specialization, specialization);
    }

    fn visit_definition(
        &self,
        db: &'db dyn Db,
        definition_type: DefinitionType<'db>,
        incoming_specialization: Option<Specialization<'db>>,
        specialization: Option<Specialization<'db>>,
    ) {
        if self.may_diverge.get() {
            return;
        }

        let matching_active = {
            let active_definitions = self.active_definitions.borrow();
            if active_definitions.iter().any(|active| {
                active.definition == definition_type.definition
                    && active.specialization == specialization
            }) {
                return;
            }
            active_definitions
                .iter()
                .rposition(|active| active.definition == definition_type.definition)
        };

        if let Some(active_index) = matching_active {
            let unbounded_parameters =
                self.cycle_unbounded_parameters(db, active_index, incoming_specialization);
            if definition_type.definition == self.target {
                self.may_diverge.set(!unbounded_parameters.is_empty());
                return;
            }
            if !unbounded_parameters.is_empty() {
                let target_specializations = self.active_definitions.borrow()[active_index]
                    .target_specializations
                    .clone();
                if target_specializations
                    .iter()
                    .copied()
                    .any(|specialization| {
                        specialization.references_any_parameter(
                            db,
                            &self.env,
                            &unbounded_parameters,
                        )
                    })
                {
                    self.may_diverge.set(true);
                }
                self.active_definitions.borrow_mut()[active_index]
                    .unbounded_parameters
                    .extend(unbounded_parameters);
                return;
            }
        }

        let depth = self.active_definitions.borrow().len();
        self.active_definitions.borrow_mut().push(ActiveDefinition {
            definition: definition_type.definition,
            specialization,
            incoming_specialization,
            unbounded_parameters: FxHashSet::default(),
            target_specializations: Vec::new(),
        });
        let guard = ActiveDefinitionGuard {
            active_definitions: &self.active_definitions,
            depth,
        };
        let visitor = DefinitionBodyVisitor {
            recursion: self,
            specialization,
            visited_types: TypeCollector::default(),
        };
        visitor.visit_definition_body(db, definition_type.identity_type);
        drop(guard);
    }

    fn record_target_specializations(
        &self,
        db: &'db dyn Db,
        incoming_specialization: Option<Specialization<'db>>,
    ) {
        let active_len = self.active_definitions.borrow().len();
        let target_specializations: Vec<_> = (0..active_len)
            .map(|active_index| {
                self.compose_specializations_from(db, active_index, incoming_specialization)
            })
            .collect();
        let unbounded_parameters: Vec<_> = self
            .active_definitions
            .borrow()
            .iter()
            .map(|active| active.unbounded_parameters.clone())
            .collect();

        if target_specializations
            .iter()
            .copied()
            .zip(&unbounded_parameters)
            .any(|(specialization, parameters)| {
                specialization.is_some_and(|specialization| {
                    specialization.references_any_parameter(db, &self.env, parameters)
                })
            })
        {
            self.may_diverge.set(true);
        }

        for (active, specialization) in self
            .active_definitions
            .borrow_mut()
            .iter_mut()
            .zip(target_specializations)
        {
            active.target_specializations.extend(specialization);
        }
    }

    /// Returns the parameters whose values can grow without bound by repeatedly traversing the
    /// cycle that returns to `active_index`.
    fn cycle_unbounded_parameters(
        &self,
        db: &'db dyn Db,
        active_index: usize,
        incoming_specialization: Option<Specialization<'db>>,
    ) -> FxHashSet<BoundTypeVarIdentity<'db>> {
        self.compose_specializations_from(db, active_index, incoming_specialization)
            .map_or_else(FxHashSet::default, |specialization| {
                specialization.unbounded_parameters(db, &self.env)
            })
    }

    fn compose_specializations_from(
        &self,
        db: &'db dyn Db,
        active_index: usize,
        incoming_specialization: Option<Specialization<'db>>,
    ) -> Option<Specialization<'db>> {
        let incoming_specializations: Vec<_> = self.active_definitions.borrow()[active_index + 1..]
            .iter()
            .map(|active| active.incoming_specialization)
            .chain([incoming_specialization])
            .collect();

        let mut cycle_specialization = None;
        for incoming in incoming_specializations {
            cycle_specialization = match (cycle_specialization, incoming) {
                (Some(current), Some(incoming)) => Some(incoming.apply_specialization(db, current)),
                (_, incoming) => incoming,
            };
        }
        cycle_specialization
    }
}

impl<'db> DefinitionType<'db> {
    fn from_type(db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> Option<Self> {
        if let Type::TypeAlias(alias) = ty {
            let specialization = alias.generic_context(db).map(|generic_context| {
                alias
                    .specialization(db)
                    .unwrap_or_else(|| generic_context.default_specialization(db, None))
            });
            let identity_alias = alias
                .unspecialized(db)
                .apply_specialization(db, |context| context.identity_specialization(db));
            return Some(Self {
                definition: alias.definition(db),
                identity_type: Type::TypeAlias(identity_alias),
                specialization,
            });
        }

        let (class, is_typed_dict) = match ty {
            Type::ProtocolInstance(protocol) => (*protocol.class_origin(db)?, false),
            Type::TypedDict(typed_dict) => (typed_dict.defining_class()?, true),
            _ => return None,
        };
        let (origin, specialization) = class.static_class_literal(db)?;
        let specialization = origin.generic_context(db).map(|generic_context| {
            specialization.unwrap_or_else(|| generic_context.default_specialization(db, None))
        });
        let identity_class = origin.identity_specialization(db);
        Some(Self {
            definition: origin.definition(db),
            identity_type: if is_typed_dict {
                Type::typed_dict(identity_class)
            } else {
                Type::instance(db, env, identity_class)
            },
            specialization,
        })
    }
}

impl<'db> DefinitionBodyVisitor<'_, 'db> {
    fn visit_definition_body(&self, db: &'db dyn Db, ty: Type<'db>) {
        match ty {
            Type::TypeAlias(alias) => self.visit_type_alias_type(db, alias),
            Type::ProtocolInstance(protocol) => {
                self.visit_protocol_instance_type(db, protocol);
            }
            Type::TypedDict(typed_dict) => self.visit_typed_dict_type(db, typed_dict),
            _ => {}
        }
    }
}

impl<'db> TypeVisitor<'db> for DefinitionBodyVisitor<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        &self.recursion.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if self.recursion.may_diverge.get() {
            return;
        }

        if let Type::TypeVar(typevar) = ty {
            if let Some(mapped) = self
                .specialization
                .and_then(|specialization| specialization.get(db, typevar))
                && mapped != ty
            {
                self.visit_type(db, mapped);
            }
            return;
        }

        if let Some(reference) = DefinitionType::from_type(db, &self.recursion.env, ty) {
            self.recursion.visit_reference(db, reference);
        } else {
            walk_type_with_recursion_guard(db, ty, self, &self.visited_types);
        }
    }

    fn visit_bound_type_var_type(
        &self,
        _db: &'db dyn Db,
        _bound_typevar: BoundTypeVarInstance<'db>,
    ) {
    }

    fn visit_protocol_instance_type(&self, db: &'db dyn Db, protocol: ProtocolInstanceType<'db>) {
        if let Some(class) = protocol.class_origin(db) {
            class.walk_recursive_member_types(db, self);
        }
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        self.visit_type(db, alias.raw_value_type(db));
    }

    fn visit_typed_dict_type(&self, db: &'db dyn Db, typed_dict: TypedDictType<'db>) {
        for field in typed_dict.items(db).values() {
            self.visit_type(db, field.declared_ty);
        }
        if let Some(extra_items) = typed_dict.explicit_extra_items(db) {
            self.visit_type(db, extra_items.declared_ty);
        }
    }
}

struct SpecializationParameterVisitor<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    parameters: &'a FxHashSet<BoundTypeVarIdentity<'db>>,
    visited_types: TypeCollector<'db>,
    in_growing_type: Cell<bool>,
    dependencies: RefCell<FxHashMap<BoundTypeVarIdentity<'db>, bool>>,
}

impl<'db> Specialization<'db> {
    /// Returns the parameters whose values can grow without bound as this specialization is
    /// repeatedly applied to itself.
    fn unbounded_parameters(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> FxHashSet<BoundTypeVarIdentity<'db>> {
        let generic_context = self.generic_context(db);
        let parameters: Vec<_> = generic_context
            .variables(db)
            .map(|parameter| SpecializationParameterVisitor::parameter_identity(db, parameter))
            .collect();
        let parameter_set: FxHashSet<_> = parameters.iter().copied().collect();
        let mut dependencies = vec![vec![false; parameters.len()]; parameters.len()];
        let mut growing_edges = Vec::new();

        for (output_index, ty) in self.types(db).iter().copied().enumerate() {
            let parameter_dependencies =
                SpecializationParameterVisitor::collect(db, env, &parameter_set, ty);
            for (input_index, parameter) in parameters.iter().enumerate() {
                let Some(&is_growing) = parameter_dependencies.get(parameter) else {
                    continue;
                };
                dependencies[output_index][input_index] = true;
                if is_growing {
                    growing_edges.push((output_index, input_index));
                }
            }
        }

        for intermediate in 0..parameters.len() {
            let intermediate_dependencies = dependencies[intermediate].clone();
            for output_dependencies in &mut dependencies {
                if output_dependencies[intermediate] {
                    for (input, is_reachable) in
                        intermediate_dependencies.iter().copied().enumerate()
                    {
                        output_dependencies[input] |= is_reachable;
                    }
                }
            }
        }

        // A structural wrapper accumulates only when its dependency edge belongs to a cycle. Any
        // parameter that depends on such a cycle can then grow along with it.
        let growing_cycles: Vec<_> = growing_edges
            .into_iter()
            .filter_map(|(output, input)| {
                (output == input || dependencies[input][output]).then_some(output)
            })
            .collect();

        parameters
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(output, parameter)| {
                growing_cycles
                    .iter()
                    .any(|&cycle| output == cycle || dependencies[output][cycle])
                    .then_some(parameter)
            })
            .collect()
    }

    fn references_any_parameter(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        parameters: &FxHashSet<BoundTypeVarIdentity<'db>>,
    ) -> bool {
        self.types(db)
            .iter()
            .copied()
            .any(|ty| !SpecializationParameterVisitor::collect(db, env, parameters, ty).is_empty())
    }
}

impl<'a, 'db> SpecializationParameterVisitor<'a, 'db> {
    fn collect(
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        parameters: &'a FxHashSet<BoundTypeVarIdentity<'db>>,
        ty: Type<'db>,
    ) -> FxHashMap<BoundTypeVarIdentity<'db>, bool> {
        let visitor = Self {
            env,
            parameters,
            visited_types: TypeCollector::default(),
            in_growing_type: Cell::new(false),
            dependencies: RefCell::default(),
        };
        visitor.visit_type(db, ty);
        visitor.dependencies.into_inner()
    }

    fn parameter_identity(
        db: &'db dyn Db,
        parameter: BoundTypeVarInstance<'db>,
    ) -> BoundTypeVarIdentity<'db> {
        let identity = parameter.identity(db);
        if identity.is_paramspec(db) {
            identity.without_paramspec_attr(db)
        } else {
            identity
        }
    }
}

impl<'db> TypeVisitor<'db> for SpecializationParameterVisitor<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if let Type::TypeVar(typevar) = ty {
            let parameter = Self::parameter_identity(db, typevar);
            if self.parameters.contains(&parameter) {
                self.dependencies
                    .borrow_mut()
                    .entry(parameter)
                    .and_modify(|is_growing| {
                        *is_growing |= self.in_growing_type.get();
                    })
                    .or_insert_with(|| self.in_growing_type.get());
            }
            return;
        }

        // Unions and intersections are normalized set operations, so nesting a parameter inside
        // either does not by itself add another structural layer on every substitution.
        match ty {
            Type::Union(union) => {
                self.visit_union_type(db, union);
                return;
            }
            Type::Intersection(intersection) => {
                self.visit_intersection_type(db, intersection);
                return;
            }
            _ => {}
        }

        let was_in_growing_type = self.in_growing_type.replace(true);
        walk_type_with_recursion_guard(db, ty, self, &self.visited_types);
        self.in_growing_type.set(was_in_growing_type);
    }

    fn visit_bound_type_var_type(
        &self,
        _db: &'db dyn Db,
        _bound_typevar: BoundTypeVarInstance<'db>,
    ) {
    }

    fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
        for ty in alias.specialization(db).types(db) {
            self.visit_type(db, *ty);
        }
    }

    fn visit_protocol_instance_type(&self, db: &'db dyn Db, protocol: ProtocolInstanceType<'db>) {
        if let Some((_, Some(specialization))) = protocol
            .class_origin(db)
            .and_then(|class| class.static_class_literal(db))
        {
            for ty in specialization.types(db) {
                self.visit_type(db, *ty);
            }
        }
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(specialization) = alias.specialization(db) {
            for ty in specialization.types(db) {
                self.visit_type(db, *ty);
            }
        }
    }
}

impl<'db> TypeAliasType<'db> {
    fn recursive_specializations_may_diverge(self, db: &'db dyn Db) -> bool {
        let identity = self
            .unspecialized(db)
            .apply_specialization(db, |context| context.identity_specialization(db));
        DefinitionRecursionVisitor::specializations_may_diverge(
            db,
            Type::TypeAlias(identity),
            self.definition(db),
        )
    }
}

impl<'db> ProtocolInstanceType<'db> {
    fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        let (origin, _) = self.class_origin(db)?.static_class_literal(db)?;
        Some(origin.definition(db))
    }

    fn recursive_specializations_may_diverge(self, db: &'db dyn Db) -> bool {
        let Some(class) = self.class_origin(db) else {
            return false;
        };
        let Some((origin, _)) = class.static_class_literal(db) else {
            return false;
        };
        let definition = origin.definition(db);
        let env = ProgramEnvironment::from_definition(definition);
        let identity = Type::instance(db, &env, origin.identity_specialization(db));
        DefinitionRecursionVisitor::specializations_may_diverge(db, identity, definition)
    }
}

impl<'db> TypedDictType<'db> {
    fn recursive_specializations_may_diverge(self, db: &'db dyn Db) -> bool {
        let Some(class) = self.defining_class() else {
            return false;
        };
        let Some((origin, _)) = class.static_class_literal(db) else {
            return false;
        };
        let definition = origin.definition(db);
        let identity = Type::typed_dict(origin.identity_specialization(db));
        DefinitionRecursionVisitor::specializations_may_diverge(db, identity, definition)
    }
}

/// An item that provides the identity used to detect active recursive cycles.
pub trait HasIdentity<'db> {
    type Id: PartialEq;

    /// Returns `false` if `self` and `other` cannot have the same identity.
    ///
    /// Implementations can use this to avoid constructing an expensive identity. Returning
    /// `true` does not imply that the identities match; [`HasIdentity::to_identity`] confirms it.
    fn may_share_identity(&self, _db: &'db dyn Db, _other: &Self) -> bool {
        true
    }

    /// Returns an identity that remains stable while this item is active in a [`CycleDetector`].
    fn to_identity(&self, db: &'db dyn Db) -> Self::Id;
}

impl<'db> HasIdentity<'db> for Type<'db> {
    type Id = TypeIdentity<'db>;

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.may_share_type_identity(db, *other)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        Type::to_type_identity(*self, db)
    }
}

pub(crate) type PairVisitor<'db, Tag, C> = CycleDetector<'db, Tag, (Type<'db>, Type<'db>), C, 1>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>) {
    type Id = (TypeIdentity<'db>, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0) && self.1.may_share_type_identity(db, other.1)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (self.0.to_type_identity(db), self.1.to_type_identity(db))
    }
}

impl<'db, Context> HasIdentity<'db> for (Type<'db>, Context, Type<'db>)
where
    Context: Copy + PartialEq,
{
    type Id = (TypeIdentity<'db>, Context, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0)
            && self.1 == other.1
            && self.2.may_share_type_identity(db, other.2)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (
            self.0.to_type_identity(db),
            self.1,
            self.2.to_type_identity(db),
        )
    }
}

/// `CycleDetector` is temporary, so callers should choose the capacity that keeps observed cycle
/// paths inline even when that makes `seen` slightly larger than an `FxIndexSet<T>`.
#[derive(Debug)]
pub struct CycleDetector<'db, Tag, T: HasIdentity<'db>, R, const INLINE_CAPACITY: usize> {
    /// The active recursion stack and the lazily-computed identity of each item.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveCycleDetectorVisit<'db, T>; INLINE_CAPACITY]>>,

    /// Memoized results from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<T, R>>,

    fallback: R,

    _tag: PhantomData<fn() -> &'db Tag>,
}

impl<'db, Tag, T, R, const INLINE_CAPACITY: usize> CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    pub(crate) fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(SmallVec::new()),
            cache: RefCell::new(CycleDetectorCache::new()),
            fallback,
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag, T, R: Clone, const INLINE_CAPACITY: usize>
    CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    #[inline]
    pub fn visit(&self, db: &'db dyn Db, item: T, compute: impl FnOnce() -> R) -> R {
        match self.begin_visit(db, item) {
            CycleDetectorVisit::Ready(result) => result,
            CycleDetectorVisit::Cycle(_) => self.fallback.clone(),
            CycleDetectorVisit::Pending(item) => {
                let result = compute();
                self.finish_visit(item, result)
            }
        }
    }

    /// Visits `item`, returning it in `Err` if another active item has the same identity.
    ///
    /// The caller must convert `Err(item)` into an operation-specific conservative result. An
    /// exact recursive reentry uses the detector's configured fallback and is returned as `Ok`.
    #[inline]
    pub(super) fn try_visit(
        &self,
        db: &'db dyn Db,
        item: T,
        compute: impl FnOnce() -> R,
    ) -> Result<R, T> {
        match self.begin_visit(db, item) {
            CycleDetectorVisit::Ready(result) => Ok(result),
            CycleDetectorVisit::Cycle(item) => Err(item),
            CycleDetectorVisit::Pending(item) => {
                let result = compute();
                Ok(self.finish_visit(item, result))
            }
        }
    }

    fn begin_visit(&self, db: &'db dyn Db, item: T) -> CycleDetectorVisit<T, R> {
        if let Some(result) = self.cache.borrow().get(&item) {
            return CycleDetectorVisit::Ready(result.clone());
        }

        let seen = self.seen.borrow();
        if seen.iter().any(|active| active.item == item) {
            return CycleDetectorVisit::Ready(self.fallback.clone());
        }

        let mut candidates = seen
            .iter()
            .filter(|active| item.may_share_identity(db, &active.item))
            .peekable();
        let identity = if candidates.peek().is_none() {
            OnceCell::new()
        } else {
            // Deriving an identity can require a structural definition walk. Defer it until a
            // cheap candidate match shows that another active item could form a cycle.
            let identity = item.to_identity(db);
            if candidates.any(|active| {
                active.identity.get_or_init(|| active.item.to_identity(db)) == &identity
            }) {
                return CycleDetectorVisit::Cycle(item);
            }
            OnceCell::from(identity)
        };
        drop(seen);

        self.seen.borrow_mut().push(ActiveCycleDetectorVisit {
            item: item.clone(),
            identity,
        });
        CycleDetectorVisit::Pending(item)
    }

    /// Finish a [`CycleDetectorVisit::Pending`] visit and cache its result.
    fn finish_visit(&self, item: T, result: R) -> R {
        let active = self.seen.borrow_mut().pop();
        debug_assert!(active.as_ref().is_some_and(|active| active.item == item));
        self.cache
            .borrow_mut()
            .insert_completed(item, result.clone());
        result
    }
}

struct ActiveCycleDetectorVisit<'db, T: HasIdentity<'db>> {
    item: T,
    identity: OnceCell<T::Id>,
}

impl<'db, T: fmt::Debug + HasIdentity<'db>> fmt::Debug for ActiveCycleDetectorVisit<'db, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.item.fmt(f)
    }
}

/// Result of starting a cycle-detector visit.
pub(super) enum CycleDetectorVisit<T, R> {
    /// The item already has a completed result or hit an exact recursive edge.
    Ready(R),
    /// A different item with the same abstract identity is already pending.
    Cycle(T),
    /// The caller should compute the result and finish the pending visit.
    Pending(T),
}

/// Guards recursive type transformations.
pub(crate) struct TypeTransformer<'db, Tag> {
    /// The active transformation stack and its recursive identities.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveTypeTransformation<'db>; 3]>>,

    /// Memoized transformations from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<Type<'db>, Type<'db>>>,

    _tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Default for TypeTransformer<'_, Tag> {
    fn default() -> Self {
        Self {
            seen: RefCell::default(),
            cache: RefCell::default(),
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag> TypeTransformer<'db, Tag> {
    #[inline]
    pub(crate) fn visit_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        match self.begin_visit(db, ty) {
            TypeTransformerVisit::Ready(result) => result,
            TypeTransformerVisit::Pending(ty) => {
                let result = compute();
                self.finish_visit(ty, result)
            }
        }
    }

    fn begin_visit(&self, db: &'db dyn Db, ty: Type<'db>) -> TypeTransformerVisit<'db> {
        if let Some(result) = self.cache.borrow().get(&ty) {
            return TypeTransformerVisit::Ready(*result);
        }

        let identity = ty.to_type_identity(db);
        let seen = self.seen.borrow();
        if seen
            .iter()
            .any(|active| active.ty == ty || active.identity == identity)
        {
            return TypeTransformerVisit::Ready(ty);
        }
        drop(seen);

        self.seen
            .borrow_mut()
            .push(ActiveTypeTransformation { ty, identity });
        TypeTransformerVisit::Pending(ty)
    }

    fn finish_visit(&self, ty: Type<'db>, result: Type<'db>) -> Type<'db> {
        let active = self.seen.borrow_mut().pop();
        debug_assert_eq!(active.map(|active| active.ty), Some(ty));
        self.cache.borrow_mut().insert_completed(ty, result);
        result
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveTypeTransformation<'db> {
    ty: Type<'db>,
    identity: TypeIdentity<'db>,
}

enum TypeTransformerVisit<'db> {
    Ready(Type<'db>),
    Pending(Type<'db>),
}

impl<'db, Tag, T, R: Default, const INLINE_CAPACITY: usize> Default
    for CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    fn default() -> Self {
        CycleDetector::new(R::default())
    }
}

/// The memoized results for a [`CycleDetector`].
///
/// Most populated cycle-detector caches contain at most two results. Keep those results inline,
/// but spill on the third distinct result so lookups in wider caches remain hashed.
#[derive(Debug, Default)]
enum CycleDetectorCache<T, R> {
    #[default]
    Empty,
    One((T, R)),
    Two([(T, R); 2]),
    Spilled(FxHashMap<T, R>),
}

impl<T, R> CycleDetectorCache<T, R> {
    const fn new() -> Self {
        Self::Empty
    }

    fn get(&self, item: &T) -> Option<&R>
    where
        T: Hash + Eq,
    {
        match self {
            Self::Empty => None,
            Self::One((cached_item, result)) => (cached_item == item).then_some(result),
            Self::Two(entries) => entries
                .iter()
                .find_map(|(cached_item, result)| (cached_item == item).then_some(result)),
            Self::Spilled(cache) => cache.get(item),
        }
    }

    /// Inserts a completed item after the caller has checked that `item` is not already cached.
    fn insert_completed(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        debug_assert!(self.get(&item).is_none());
        self.insert_new(item, result);
    }

    fn insert_new(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        let entry = (item, result);
        *self = match mem::replace(self, Self::Empty) {
            Self::Empty => Self::One(entry),
            Self::One(first) => Self::Two([first, entry]),
            Self::Two(entries) => Self::spill(entries, entry),
            Self::Spilled(mut cache) => {
                cache.insert(entry.0, entry.1);
                Self::Spilled(cache)
            }
        };
    }

    #[cold]
    fn spill(entries: [(T, R); 2], third: (T, R)) -> Self
    where
        T: Hash + Eq,
    {
        Self::Spilled(entries.into_iter().chain([third]).collect())
    }

    #[cfg(test)]
    const fn is_spilled(&self) -> bool {
        matches!(self, Self::Spilled(_))
    }
}

/// Recursion detection without memoization.
///
/// This is useful when a recursive relation needs a coinductive-style "we're already proving this
/// goal, assume it for now" step, but completed results are not safe to reuse for future visits to
/// the same abstract key.
#[derive(Debug)]
pub(crate) struct ActiveRecursionDetector<T> {
    seen: RefCell<FxHashSet<T>>,
}

impl<T> Default for ActiveRecursionDetector<T> {
    fn default() -> Self {
        Self {
            seen: RefCell::new(FxHashSet::default()),
        }
    }
}

impl<T: Hash + Eq + Clone> ActiveRecursionDetector<T> {
    pub(crate) fn visit<R>(
        &self,
        item: &T,
        on_cycle: impl FnOnce() -> R,
        func: impl FnOnce() -> R,
    ) -> R {
        if !self.seen.borrow_mut().insert(item.clone()) {
            return on_cycle();
        }

        // Keep the active-recursion state scoped even if `func` unwinds. In some cases, we catch
        // panics and continue handling later work on the same thread.
        let _guard = ActiveRecursionGuard {
            seen: &self.seen,
            item,
        };

        func()
    }
}

struct ActiveRecursionGuard<'a, T: Hash + Eq> {
    seen: &'a RefCell<FxHashSet<T>>,
    item: &'a T,
}

impl<T: Hash + Eq> Drop for ActiveRecursionGuard<'_, T> {
    fn drop(&mut self) {
        self.seen.borrow_mut().remove(self.item);
    }
}

#[cfg(test)]
mod tests {
    use super::{CycleDetector, CycleDetectorVisit, Db, HasIdentity, TypeIdentity};
    use crate::ProgramEnvironment;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::Type;
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use std::cell::Cell;
    use std::hash::{Hash, Hasher};
    use ty_python_core::ProgramFile;

    struct TestVisit;

    type Detector<'db> = CycleDetector<'db, TestVisit, u8, u8, 1>;

    impl<'db> HasIdentity<'db> for u8 {
        type Id = Self;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            *self
        }
    }

    #[derive(Clone)]
    struct CountingIdentityItem<'a> {
        value: u8,
        identity_calls: &'a Cell<usize>,
    }

    impl<'a> CountingIdentityItem<'a> {
        const fn new(value: u8, identity_calls: &'a Cell<usize>) -> Self {
            Self {
                value,
                identity_calls,
            }
        }
    }

    impl PartialEq for CountingIdentityItem<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }

    impl Eq for CountingIdentityItem<'_> {}

    impl Hash for CountingIdentityItem<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    impl<'db> HasIdentity<'db> for CountingIdentityItem<'_> {
        type Id = u8;

        fn may_share_identity(&self, _db: &'db dyn Db, other: &Self) -> bool {
            self.value % 2 == other.value % 2
        }

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            self.identity_calls.set(self.identity_calls.get() + 1);
            self.value
        }
    }

    #[derive(Clone, Eq, Hash, PartialEq)]
    struct ConstantIdentityItem(u8);

    impl<'db> HasIdentity<'db> for ConstantIdentityItem {
        type Id = ();

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {}
    }

    fn global_instance_type<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Type<'db> {
        let file = system_path_to_file(db, "/src/a.py").unwrap();
        let file = ProgramFile::new(db, file, env.program(db));
        global_symbol(db, file, name)
            .place
            .expect_type()
            .to_instance_approximation(db, env)
            .unwrap()
    }

    #[test]
    fn property_receiver_does_not_make_protocol_recursive() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from __future__ import annotations

from typing import Protocol

class GenericProperty[T](Protocol):
    @property
    def value(self) -> T: ...

class RecursiveProperty[T](Protocol):
    @property
    def child(self) -> RecursiveProperty[list[T]]: ...

class RecursivePropertySetter[T](Protocol):
    @property
    def child(self) -> int: ...

    @child.setter
    def child(self, value: RecursivePropertySetter[list[T]]) -> None: ...
"#,
        )
        .unwrap();

        let env = db.program_environment();
        assert_eq!(
            global_instance_type(&db, &env, "GenericProperty").recursive_identity(&db),
            None
        );
        assert!(matches!(
            global_instance_type(&db, &env, "RecursiveProperty").recursive_identity(&db),
            Some(TypeIdentity::RecursiveProtocol(_))
        ));
        assert!(matches!(
            global_instance_type(&db, &env, "RecursivePropertySetter").recursive_identity(&db),
            Some(TypeIdentity::RecursiveProtocol(_))
        ));
    }

    #[test]
    fn caches_results_and_spills_after_two_entries() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(detector.visit(db, 1, || 10), 10);
        assert_eq!(detector.visit(db, 1, || 40), 10);
        assert_eq!(detector.visit(db, 2, || 20), 20);
        assert!(!detector.cache.borrow().is_spilled());
        assert_eq!(detector.visit(db, 3, || 30), 30);
        assert!(detector.cache.borrow().is_spilled());

        assert_eq!(detector.visit(db, 2, || 40), 20);
        assert_eq!(detector.visit(db, 3, || 40), 30);
    }

    #[test]
    fn nested_visit_short_circuits_on_cycle() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(
            detector.visit(db, 1, || detector.visit(db, 1, || 20) + 10),
            10
        );
    }

    #[test]
    fn computes_each_active_identity_once() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || {
                detector.visit(db, CountingIdentityItem::new(3, &identity_calls), || 1)
            }),
            1
        );
        assert_eq!(identity_calls.get(), 2);
    }

    #[test]
    fn skips_identity_for_distinct_candidates() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || {
                detector.visit(db, CountingIdentityItem::new(2, &identity_calls), || 1)
            }),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn skips_identity_without_a_distinct_active_item() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || 1),
            1
        );
        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || 2),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn different_items_with_same_identity_form_cycle() {
        let db = setup_db();
        let db = &db;
        let detector = CycleDetector::<TestVisit, ConstantIdentityItem, u8, 1>::new(0);

        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be pending");
        };
        let CycleDetectorVisit::Cycle(item) = detector.begin_visit(db, ConstantIdentityItem(2))
        else {
            panic!("a different item with the same identity should form a cycle");
        };
        assert_eq!(item.0, 2);
        detector.finish_visit(pending, 1);

        let CycleDetectorVisit::Ready(seen) = detector.begin_visit(db, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 1);
        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, ConstantIdentityItem(2))
        else {
            panic!("the second identity should be pending after the first is finished");
        };
        detector.finish_visit(pending, 2);
        let CycleDetectorVisit::Ready(seen) = detector.begin_visit(db, ConstantIdentityItem(2))
        else {
            panic!("the second identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 2);
    }
}
