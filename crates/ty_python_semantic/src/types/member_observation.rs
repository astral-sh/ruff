//! Member evaluation retains the declaration and call expressions that produce its result.

use ruff_python_ast::name::Name;

use super::call::{Bindings, CallArguments, CallDunderError, CallError};
use super::class::{ClassMemberResult, InstanceMemberResult, MroLookup};
use super::constraints::ConstraintSetBuilder;
use super::projection::{ObservationEdge, ObservedType};
use super::relation::RelationContext;
use super::set_theoretic::IntersectionBuilder;
use super::{
    ApplyTypeMappingVisitor, AttributeKind, ClassBase, ClassType, DescriptorAccess,
    DescriptorGetCallContext, InstanceFallbackShadowsNonDataDescriptor, KnownClass,
    MemberLookupErrorKind, MemberLookupPolicy, MemberLookupResult, MemberSelection, PromotionKind,
    PromotionMode, SelfBinding, SubclassOfInner, Type, TypeContext, TypeMapping, TypeNormalization,
    TypeQualifiers, TypeVarBoundOrConstraints, UnionBuilder, map_member_lookup_type,
    member_lookup_result,
};
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy, TypeOrigin,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(super) enum MemberStorage {
    Class,
    Instance,
}

/// A contribution reported by the MRO walk that selected the actual member.
#[derive(Clone, Copy)]
pub(super) struct MroMemberSource<'db> {
    pub(super) class: ClassType<'db>,
    pub(super) member: PlaceAndQualifiers<'db>,
    pub(super) storage: MemberStorage,
    pub(super) replace: bool,
}

/// A completed lookup and its value occurrence. `None` means that no member exists; failure to
/// finish evaluation is represented by the enclosing `Option`, separately from missing members.
#[derive(Clone)]
pub(super) struct ObservedMember<'db> {
    pub(super) result: MemberLookupResult<'db>,
    pub(super) value: Option<ObservedType<'db>>,
}

impl<'db> ObservedMember<'db> {
    fn from_result(
        db: &'db dyn Db,
        result: MemberLookupResult<'db>,
        input: &ObservedType<'db>,
    ) -> Self {
        let value = result
            .unwrap_or_else(|error| error.fallback_member(db))
            .member(db)
            .place
            .ignore_possibly_undefined()
            .map(|ty| input.unchanged_or_unresolved(ty));
        Self { result, value }
    }

    fn from_value(
        db: &'db dyn Db,
        result: MemberLookupResult<'db>,
        value: ObservedType<'db>,
    ) -> Self {
        let ty = result
            .unwrap_or_else(|error| error.fallback_member(db))
            .member(db)
            .place
            .ignore_possibly_undefined();
        let value = ty.map(|ty| {
            if ty == value.ty {
                value
            } else {
                value.unchanged_or_unresolved(ty)
            }
        });
        Self { result, value }
    }

    pub(super) fn place(&self, db: &'db dyn Db) -> PlaceAndQualifiers<'db> {
        self.result
            .unwrap_or_else(|error| error.fallback_member(db))
            .member(db)
    }

    fn map(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        let Some(value) = self.value else {
            return self;
        };
        let value = value.apply_mapping(
            db,
            mapping,
            &ApplyTypeMappingVisitor::new_for_type_construction(env),
        );
        Self::from_value(
            db,
            map_member_lookup_type(db, self.result, |_| value.ty),
            value,
        )
    }

    fn bind_self_typevars(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Self {
        if !self
            .value
            .as_ref()
            .is_some_and(|value| value.ty.supports_self_binding(db, env))
        {
            return self;
        }
        self.map(
            db,
            &TypeMapping::BindSelf(SelfBinding::new(db, env, receiver, None)),
            env,
        )
    }

    fn promote_inferred_class_literals(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        let place = self.place(db);
        if matches!(
            place.place,
            Place::Defined(DefinedPlace {
                origin: TypeOrigin::Inferred,
                ..
            })
        ) && !place.qualifiers.contains(TypeQualifiers::FINAL)
        {
            self.map(
                db,
                &TypeMapping::Promote(PromotionMode::On, PromotionKind::ClassLiteralsOnly),
                env,
            )
        } else {
            self
        }
    }

    fn with_definedness(mut self, db: &'db dyn Db, definedness: Definedness) -> Self {
        self.result = member_lookup_result(
            db,
            Type::with_definedness(self.place(db), definedness),
            self.result.err().map(|error| error.kind(db)),
            self.result
                .unwrap_or_else(|error| error.fallback_member(db))
                .deprecated_properties(db),
        );
        self
    }
}

/// Which part of a member lookup the caller needs. A presence result cannot be used as the
/// member's value: an instance assignment may shadow the class value that established presence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum MemberLookupDemand {
    Presence,
    Value,
}

/// A lookup's search rules and the evidence that must be produced. Both affect whether a
/// repeated lookup is the same proof operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct MemberLookupOptions {
    pub(super) policy: MemberLookupPolicy,
    pub(super) demand: MemberLookupDemand,
}

pub(super) fn lookup_member<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    input: &ObservedType<'db>,
    receiver: &ObservedType<'db>,
    name: &str,
    policy: MemberLookupPolicy,
    context: &RelationContext<'db>,
) -> Option<ObservedMember<'db>> {
    lookup_member_with_options(
        db,
        env,
        input,
        receiver,
        name,
        MemberLookupOptions {
            policy,
            demand: MemberLookupDemand::Value,
        },
        context,
    )
}

pub(super) fn lookup_member_with_options<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    input: &ObservedType<'db>,
    receiver: &ObservedType<'db>,
    name: &str,
    options: MemberLookupOptions,
    context: &RelationContext<'db>,
) -> Option<ObservedMember<'db>> {
    MemberEvaluator {
        db,
        env,
        context,
        demand: options.demand,
    }
    .lookup(input, receiver, name, options.policy)
}

#[derive(Clone, Copy)]
struct MemberEvaluator<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
    context: &'a RelationContext<'db>,
    demand: MemberLookupDemand,
}

impl<'db> MemberEvaluator<'_, 'db> {
    fn lookup(
        &self,
        input: &ObservedType<'db>,
        receiver: &ObservedType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<ObservedMember<'db>> {
        self.context.member_lookup(
            self.db,
            input,
            receiver,
            name,
            MemberLookupOptions {
                policy,
                demand: self.demand,
            },
            || self.lookup_impl(input, receiver, name, policy),
        )
    }

    fn missing(&self, input: &ObservedType<'db>) -> ObservedMember<'db> {
        ObservedMember::from_result(self.db, Place::Undefined.into(), input)
    }

    fn class_view(&self, input: &ObservedType<'db>) -> Option<(ObservedType<'db>, ClassType<'db>)> {
        let view = input.project(self.db, self.env, ObservationEdge::ClassView)?;
        let class = view.ty.to_class_type(self.db)?;
        Some((view, class))
    }

    /// Runtime storage used when a type does not itself carry a nominal class. This is also
    /// the class on which descriptor slots are searched.
    fn runtime_lookup_target(&self, input: &ObservedType<'db>) -> Option<ObservedType<'db>> {
        let db = self.db;
        let env = self.env;
        let runtime = match input.ty {
            Type::FunctionLiteral(function) => function.runtime_class(db).to_instance(db, env),
            Type::KnownBoundMethod(method) => method.class().to_instance(db, env),
            Type::WrapperDescriptor(_) => KnownClass::WrapperDescriptorType.to_instance(db, env),
            Type::DataclassDecorator(_) => KnownClass::FunctionType.to_instance(db, env),
            Type::Callable(callable) => callable
                .runtime_class(db)
                .map_or_else(Type::object, |class| class.to_instance(db, env)),
            Type::DataclassTransformer(_)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeForm(_)
            | Type::TypeVar(_) => Type::object(),
            Type::KnownInstance(super::KnownInstanceType::MethodWrapper(wrapper)) => {
                wrapper.instance_fallback(db, env)
            }
            Type::SpecialForm(_) | Type::KnownInstance(_) => input
                .ty
                .to_meta_type(db, env)
                .to_instance_approximation(db, env)?,
            Type::TypeIs(_) | Type::TypeGuard(_) => KnownClass::Bool.to_instance(db, env),
            _ => return None,
        };
        Some(input.unchanged_or_unresolved(runtime))
    }

    fn source_value(
        &self,
        input: &ObservedType<'db>,
        name: &str,
        source: MroMemberSource<'db>,
    ) -> Option<ObservedType<'db>> {
        let value = source.member.place.ignore_possibly_undefined()?;
        let Some((owner, specialization)) = source.class.static_class_literal(self.db) else {
            return Some(input.unchanged_or_unresolved(value));
        };
        let raw = match source.storage {
            MemberStorage::Class => ClassType::NonGeneric(owner.into()).own_class_member(
                self.db,
                self.env,
                owner.generic_context(self.db),
                name,
            ),
            MemberStorage::Instance => {
                ClassType::NonGeneric(owner.into()).own_instance_member(self.db, self.env, name)
            }
        };
        let Some(raw) = raw.ignore_possibly_undefined() else {
            return Some(input.unchanged_or_unresolved(value));
        };
        Some(
            input
                .declaration_member(
                    self.db,
                    self.env,
                    owner,
                    ObservationEdge::ProtocolMemberRead {
                        name: Name::new(name),
                        class_access: matches!(source.storage, MemberStorage::Class),
                    },
                    raw,
                    specialization,
                )
                .unchanged_or_unresolved(value),
        )
    }

    fn mro_member(
        &self,
        input: &ObservedType<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        storage: MemberStorage,
    ) -> ObservedMember<'db> {
        self.mro_member_from(input, class, name, policy, storage, class.iter_mro(self.db))
    }

    fn mro_member_from(
        &self,
        input: &ObservedType<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        storage: MemberStorage,
        mro: impl Iterator<Item = ClassBase<'db>>,
    ) -> ObservedMember<'db> {
        let mut sources = Vec::new();
        let mut observe = |source: MroMemberSource<'db>| {
            if source.replace {
                sources.clear();
            }
            sources.push(source);
        };
        let member = match storage {
            MemberStorage::Class => {
                let inherited = if policy.no_inherited_generic_context() {
                    None
                } else {
                    class
                        .static_class_literal(self.db)
                        .and_then(|(class, _)| class.generic_context(self.db))
                };
                match MroLookup::new(self.db, self.env, mro).class_member_with_observer(
                    name,
                    policy,
                    inherited,
                    class.is_object(self.db),
                    &mut observe,
                ) {
                    ClassMemberResult::Done(result) => result.finalize(self.db, self.env),
                    // TypedDict member declarations have a shared synthesized constructor.
                    ClassMemberResult::TypedDict(_) => {
                        class.class_member(self.db, self.env, name, policy)
                    }
                }
            }
            MemberStorage::Instance => match MroLookup::new(self.db, self.env, mro)
                .instance_member_with_observer(name, &mut observe)
            {
                InstanceMemberResult::Done(result) => result,
                InstanceMemberResult::TypedDict => Place::Undefined.into(),
            },
        };
        let Some(ty) = member.place.ignore_possibly_undefined() else {
            return self.missing(input);
        };
        let values: Vec<_> = sources
            .into_iter()
            .filter_map(|source| self.source_value(input, name, source))
            .collect();
        let value = match values.as_slice() {
            [value] => value.unchanged_or_unresolved(ty),
            [] => input.unchanged_or_unresolved(ty),
            _ => input.normalized(ty, values),
        };
        ObservedMember::from_value(self.db, member.into(), value)
    }

    /// Descriptor slots are searched on the type of the original attribute value. A type
    /// variable uses its upper bound (object when absent) or each constraint for this search;
    /// calling a found slot still receives the original attribute as its descriptor argument.
    fn descriptor_slot(
        &self,
        descriptor: &ObservedType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<ObservedMember<'db>> {
        let db = self.db;
        let env = self.env;
        if let Type::TypeVar(variable) = descriptor.ty {
            return match variable.require_bound_or_constraints(db, env) {
                TypeVarBoundOrConstraints::UpperBound(bound) => {
                    let bound =
                        descriptor.child_at(db, env, bound, ObservationEdge::TypeVarUpperBound);
                    self.descriptor_slot(&bound, name, policy)
                }
                TypeVarBoundOrConstraints::Constraints(constraints) => {
                    let mut members = Vec::new();
                    for (index, &constraint) in constraints.elements(db).iter().enumerate() {
                        let constraint = descriptor.child_at(
                            db,
                            env,
                            constraint,
                            ObservationEdge::TypeVarConstraint(index),
                        );
                        members.push(self.descriptor_slot(&constraint, name, policy)?);
                    }
                    self.combine_members(descriptor, members, false)
                }
            };
        }
        if let Some(runtime) = self.runtime_lookup_target(descriptor) {
            return self.descriptor_slot(&runtime, name, policy);
        }
        let (view, class) = self.class_view(descriptor)?;
        let (view, class) = if matches!(
            descriptor.ty,
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
        ) {
            let meta = view.project(db, env, ObservationEdge::ClassMetaclassInstance)?;
            let (_, class) = self.class_view(&meta)?;
            (meta, class)
        } else {
            (view, class)
        };
        Some(self.mro_member(&view, class, name, policy, MemberStorage::Class))
    }

    fn instance_class_namespace(
        &self,
        input: &ObservedType<'db>,
        class_view: &ObservedType<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<ObservedMember<'db>> {
        let db = self.db;
        let class_attr = self.mro_member(input, class, name, policy, MemberStorage::Class);
        let Some(meta) = class_view.project(db, self.env, ObservationEdge::ClassMetaclassInstance)
        else {
            return Some(class_attr);
        };
        let Some((_, metaclass)) = self.class_view(&meta) else {
            return Some(class_attr);
        };
        let mut stored = self.mro_member(&meta, metaclass, name, policy, MemberStorage::Instance);
        if stored.place(db).place.is_undefined() {
            return Some(class_attr);
        }
        let implicit = stored
            .place(db)
            .qualifiers
            .contains(TypeQualifiers::IMPLICIT_INSTANCE_ATTRIBUTE);
        let mut own = self.mro_member_from(
            input,
            class,
            name,
            policy,
            MemberStorage::Class,
            class.iter_mro(db).take(1),
        );
        if !Type::has_own_class_namespace_value(db, self.env, class, name, own.place(db)) {
            own = self.missing(input);
        }
        if implicit {
            stored = stored.with_definedness(db, Definedness::PossiblyUndefined);
        }
        let member = self.or_fall_back_to(input, own, || Some(stored))?;
        let member = self.or_fall_back_to(input, member, || {
            Some(self.mro_member_from(
                input,
                class,
                name,
                policy,
                MemberStorage::Class,
                class.iter_mro(db).skip(1),
            ))
        })?;
        Some(if implicit {
            member.with_definedness(db, Definedness::AlwaysDefined)
        } else {
            member
        })
    }

    fn lookup_impl(
        &self,
        input: &ObservedType<'db>,
        receiver: &ObservedType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<ObservedMember<'db>> {
        let db = self.db;
        let env = self.env;
        let declaration_protocol = matches!(input.ty, Type::Recursive(recursive)
            if recursive.materialization_kind(db).is_none() && input.ty.as_protocol_instance(db).is_some());
        if !declaration_protocol && let Some(unfolded) = input.unfold(db, env) {
            return self.lookup(&unfolded, receiver, name, policy);
        }
        if matches!(input.ty, Type::Deferred(_)) {
            let resolved = input.unfold_in_context(db, env, self.context)?;
            return self.lookup(&resolved, receiver, name, policy);
        }
        if input.ty.is_dynamic() || input.ty.is_never() || input.ty.is_divergent() {
            return Some(ObservedMember::from_value(
                db,
                Place::bound(input.ty).into(),
                input.clone(),
            ));
        }
        if let Some(member) = input.ty.intrinsic_member(db, env, name) {
            let value =
                input.project(db, env, ObservationEdge::IntrinsicMember(Name::new(name)))?;
            return Some(ObservedMember::from_value(db, member.into(), value));
        }
        if let Type::Union(union) = input.ty {
            let mut children = Vec::new();
            for (index, _) in union.elements(db).iter().enumerate() {
                let child = input.project(db, env, ObservationEdge::UnionElement(index))?;
                let receiver = IntersectionBuilder::bounded_from_observed_elements(
                    db,
                    env,
                    [receiver.clone(), child.clone()],
                    TypeNormalization::Semantic,
                    Some(self.context.clone()),
                )?;
                children.push(self.lookup(&child, &receiver, name, policy)?);
            }
            return self.combine_members(input, children, false);
        }
        if let Type::Intersection(intersection) = input.ty {
            let mut children = Vec::new();
            if intersection.positive(db).is_empty() {
                let child = input.unchanged_or_unresolved(Type::object());
                children.push(self.lookup(&child, receiver, name, policy)?);
            } else {
                for index in 0..intersection.positive(db).len() {
                    let child =
                        input.project(db, env, ObservationEdge::IntersectionPositive(index))?;
                    children.push(self.lookup(&child, receiver, name, policy)?);
                }
            }
            return self.combine_members(input, children, true);
        }
        if let Type::TypedDict(super::typed_dict::TypedDictType::Synthesized(synthesized)) =
            input.ty
        {
            let declaration = super::class::synthesized_typed_dict_class_member(
                db,
                env,
                synthesized,
                policy,
                name,
            );
            let declaration = ObservedMember::from_result(db, declaration.into(), input);
            let owner = receiver.unchanged_or_unresolved(receiver.ty.to_meta_type(db, env));
            let result = self.resolve_descriptor(
                declaration,
                Some(receiver),
                &owner,
                self.missing(input),
                InstanceFallbackShadowsNonDataDescriptor::No,
            )?;
            if result.place(db).is_class_var() {
                return Some(self.missing(input));
            }
            return Some(result.bind_self_typevars(db, env, receiver.ty));
        }
        if let Type::TypeVar(variable) = input.ty {
            if let Some(bound) = input.project(db, env, ObservationEdge::TypeVarUpperBound) {
                return self.lookup(&bound, receiver, name, policy);
            }
            if let Some(constraints) = variable.typevar(db).constraints(db, env) {
                let mut children = Vec::new();
                for index in 0..constraints.len() {
                    let child =
                        input.project(db, env, ObservationEdge::TypeVarConstraint(index))?;
                    let mut member = self.lookup(&child, &child, name, policy)?;
                    if let Some(value) = &member.value
                        && let Type::BoundMethod(method) = value.ty
                    {
                        let ty = Type::BoundMethod(method.with_constrained_receiver(
                            db,
                            receiver.ty,
                            child.ty,
                        ));
                        let value =
                            ObservedType::dependent_on(ty, &[value.clone(), receiver.clone()]);
                        member = ObservedMember::from_value(
                            db,
                            map_member_lookup_type(db, member.result, |_| ty),
                            value,
                        );
                    }
                    children.push(member);
                }
                return self.combine_members(input, children, false);
            }
        }
        if let Type::SubclassOf(subclass) = input.ty {
            if subclass.into_type_var().is_some() {
                let transposed =
                    input.project(db, env, ObservationEdge::TransposedSubclassVariable)?;
                return self.lookup(&transposed, receiver, name, policy);
            }
            if subclass.is_dynamic() {
                // Every metaclass inherits type's real members. Names absent from that base
                // remain gradual unless the caller explicitly requires a concrete declaration.
                let base = input.project(db, env, ObservationEdge::GradualMetaclassBase)?;
                let known = self.lookup(&base, receiver, name, policy)?;
                if policy.require_concrete() {
                    return Some(known);
                }
                return self.or_fall_back_to(input, known, || {
                    let value = input.project(db, env, ObservationEdge::SubclassInstance)?;
                    Some(ObservedMember::from_value(
                        db,
                        Place::bound(value.ty).into(),
                        value,
                    ))
                });
            }
        }
        if let Type::ModuleLiteral(module) = input.ty {
            return Some(ObservedMember::from_result(
                db,
                module.static_member(db, env, name),
                input,
            ));
        }
        if let Type::BoundMethod(method) = input.ty {
            let runtime =
                input.unchanged_or_unresolved(KnownClass::MethodType.to_instance(db, env));
            let result = self.lookup(&runtime, receiver, name, policy)?;
            return self.or_fall_back_to(input, result, || {
                let function = input.unchanged_or_unresolved(method.func(db));
                self.lookup(&function, &function, name, policy)
            });
        }
        if let Some(runtime) = self.runtime_lookup_target(input) {
            return self.lookup(&runtime, receiver, name, policy);
        }
        let (class_view, class) = self.class_view(input)?;
        if policy.no_instance_fallback() {
            let member = if matches!(
                input.ty,
                Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
            ) {
                let meta = class_view.project(db, env, ObservationEdge::ClassMetaclassInstance)?;
                let (_, class) = self.class_view(&meta)?;
                self.mro_member(&meta, class, name, policy, MemberStorage::Class)
            } else {
                // A metaclass can store values in its instances' class namespace. They remain
                // visible to implicit method lookup even though instance storage is excluded.
                let class_policy = receiver
                    .ty
                    .instance_class_member_policy(db, env, name, policy);
                self.instance_class_namespace(input, &class_view, class, name, class_policy)?
            };
            let owner = receiver.unchanged_or_unresolved(receiver.ty.to_meta_type(db, env));
            let result = self.resolve_descriptor(
                member,
                Some(receiver),
                &owner,
                self.missing(input),
                InstanceFallbackShadowsNonDataDescriptor::No,
            )?;
            return Some(result.bind_self_typevars(db, env, receiver.ty));
        }
        if let Some(value) = input.project(db, env, ObservationEdge::EnumMember(Name::new(name))) {
            return Some(ObservedMember::from_value(
                db,
                Place::bound(value.ty).into(),
                value,
            ));
        }
        if matches!(
            input.ty,
            Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)
        ) {
            self.class_object(input, receiver, &class_view, class, name, policy)
        } else {
            let class_policy = receiver
                .ty
                .instance_class_member_policy(db, env, name, policy);
            let member =
                self.instance_class_namespace(input, &class_view, class, name, class_policy)?;
            let owner = receiver.unchanged_or_unresolved(receiver.ty.to_meta_type(db, env));
            if self.demand == MemberLookupDemand::Presence {
                // An instance assignment can change an existing class attribute's value, but
                // cannot remove its presence. Do not infer that assignment merely to decide
                // whether a value is readable: its initializer can itself depend on this guard.
                let class_result = self.resolve_descriptor(
                    member.clone(),
                    Some(receiver),
                    &owner,
                    self.missing(input),
                    InstanceFallbackShadowsNonDataDescriptor::No,
                )?;
                if class_result.place(db).place.is_definitely_bound() {
                    if input.ty.is_typed_dict() && class_result.place(db).is_class_var() {
                        return Some(self.missing(input));
                    }
                    return self.fallback(input, receiver, name, policy, class_result);
                }
            }
            let fallback = self.mro_member(input, class, name, policy, MemberStorage::Instance);
            let result = self.resolve_descriptor(
                member,
                Some(receiver),
                &owner,
                fallback,
                InstanceFallbackShadowsNonDataDescriptor::No,
            )?;
            let result = self.fallback(input, receiver, name, policy, result)?;
            if input.ty.is_typed_dict() && result.place(db).is_class_var() {
                return Some(self.missing(input));
            }
            Some(
                result
                    .bind_self_typevars(db, env, receiver.ty)
                    .promote_inferred_class_literals(db, env),
            )
        }
    }

    fn combine_members(
        &self,
        input: &ObservedType<'db>,
        members: Vec<ObservedMember<'db>>,
        intersection: bool,
    ) -> Option<ObservedMember<'db>> {
        let db = self.db;
        let mut values = Vec::new();
        let mut qualifiers = TypeQualifiers::empty();
        let mut origin = TypeOrigin::Declared;
        let mut provenance = Provenance::Unknown;
        let mut all_defined = true;
        let mut any_defined = false;
        let mut error = None;
        let mut properties = None;
        let mut all_deprecated = true;
        for member in members {
            let resolved = member
                .result
                .unwrap_or_else(|error| error.fallback_member(db));
            let place = resolved.member(db);
            qualifiers |= place.qualifiers;
            error = error.or_else(|| member.result.err().map(|error| error.kind(db)));
            if let Some(deprecated) = resolved.deprecated_properties(db) {
                properties = Some(properties.map_or(
                    deprecated,
                    |previous: super::PropertyDeprecations<'db>| {
                        if intersection {
                            previous.intersection(db, deprecated)
                        } else {
                            previous.union(db, deprecated)
                        }
                    },
                ));
            } else if !place.place.is_undefined() {
                all_deprecated = false;
            }
            match place.place {
                Place::Undefined => all_defined = false,
                Place::Defined(place) => {
                    origin = origin.merge(place.origin);
                    provenance = provenance.or(place.provenance);
                    all_defined &= place.definedness == Definedness::AlwaysDefined;
                    any_defined |= place.definedness == Definedness::AlwaysDefined;
                }
            }
            if let Some(value) = member.value {
                values.push(value);
            }
        }
        if values.is_empty() {
            return Some(self.missing(input));
        }
        let value = if intersection {
            IntersectionBuilder::bounded_from_observed_elements(
                db,
                self.env,
                values,
                TypeNormalization::Semantic,
                Some(self.context.clone()),
            )?
        } else {
            let mut builder =
                UnionBuilder::new(db, self.env).with_observed_context(self.context.clone());
            for value in values {
                builder.add_observed_in_place(value);
            }
            builder.build_observed()
        };
        let definedness = if if intersection {
            any_defined
        } else {
            all_defined
        } {
            Definedness::AlwaysDefined
        } else {
            Definedness::PossiblyUndefined
        };
        let result = member_lookup_result(
            db,
            Place::Defined(DefinedPlace {
                ty: value.ty,
                origin,
                definedness,
                provenance,
                public_type_policy: PublicTypePolicy::Raw,
            })
            .with_qualifiers(qualifiers),
            error,
            properties.filter(|_| !intersection || all_deprecated),
        );
        Some(ObservedMember::from_value(db, result, value))
    }

    fn or_fall_back_to(
        &self,
        input: &ObservedType<'db>,
        member: ObservedMember<'db>,
        fallback: impl FnOnce() -> Option<ObservedMember<'db>>,
    ) -> Option<ObservedMember<'db>> {
        match member.place(self.db).place {
            Place::Undefined => fallback(),
            Place::Defined(place) if place.definedness == Definedness::AlwaysDefined => {
                Some(member)
            }
            Place::Defined(_) => {
                let fallback = fallback()?;
                if fallback.place(self.db).place.is_undefined() {
                    return Some(member);
                }
                let definedness = fallback.place(self.db).place.is_definitely_bound();
                let mut result = self.combine_members(input, vec![member, fallback], false)?;
                if definedness {
                    let mut place = result.place(self.db);
                    if let Place::Defined(ref mut value) = place.place {
                        value.definedness = Definedness::AlwaysDefined;
                    }
                    result.result = member_lookup_result(
                        self.db,
                        place,
                        result.result.err().map(|error| error.kind(self.db)),
                        result
                            .result
                            .unwrap_or_else(|error| error.fallback_member(self.db))
                            .deprecated_properties(self.db),
                    );
                }
                Some(result)
            }
        }
    }

    fn class_object(
        &self,
        input: &ObservedType<'db>,
        receiver: &ObservedType<'db>,
        class_view: &ObservedType<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Option<ObservedMember<'db>> {
        let db = self.db;
        let env = self.env;
        let namespace = if let Type::SubclassOf(subclass) = input.ty
            && let SubclassOfInner::Protocol(protocol) = subclass.subclass_of()
            && let Some(member) = protocol.interface(db).meta_member(db, env, input.ty, name)
        {
            let result = member.into();
            if let Some(ty) = member.place.ignore_possibly_undefined() {
                let value = input.child_at(
                    db,
                    env,
                    ty,
                    ObservationEdge::ProtocolMemberRead {
                        name: Name::new(name),
                        class_access: true,
                    },
                );
                ObservedMember::from_value(db, result, value)
            } else {
                self.missing(input)
            }
        } else {
            self.mro_member(input, class, name, policy, MemberStorage::Class)
        };
        let own = class.own_class_member(db, env, None, name).inner;
        let own_definedness = match own.place {
            Place::Defined(place) if place.origin.is_declared() => Some(place.definedness),
            _ => None,
        };
        let meta = class_view.project(db, env, ObservationEdge::ClassMetaclassInstance)?;
        let (_, metaclass) = self.class_view(&meta)?;
        let namespace = if own_definedness == Some(Definedness::AlwaysDefined) {
            namespace
        } else {
            let stored = self.mro_member(&meta, metaclass, name, policy, MemberStorage::Instance);
            if own_definedness.is_some() {
                self.or_fall_back_to(input, namespace, || Some(stored))?
            } else {
                self.or_fall_back_to(input, stored, || Some(namespace))?
            }
        };
        let instance = receiver.ty.to_instance_approximation(db, env)?;
        let namespace = namespace.bind_self_typevars(db, env, instance);
        let (fallback, _, _) = self.get_attribute(namespace, None, receiver)?;
        let member = self.mro_member(&meta, metaclass, name, policy, MemberStorage::Class);
        let owner = receiver.unchanged_or_unresolved(receiver.ty.to_meta_type(db, env));
        let result = self.resolve_descriptor(
            member,
            Some(receiver),
            &owner,
            fallback,
            InstanceFallbackShadowsNonDataDescriptor::Yes,
        )?;
        let result = self.fallback(input, receiver, name, policy, result)?;
        Some(
            if let Type::SubclassOf(subclass) = input.ty
                && subclass.exact_typevar_upper_bound(db, env).is_none()
            {
                result.promote_inferred_class_literals(db, env)
            } else {
                result
            },
        )
    }

    fn resolve_descriptor(
        &self,
        member: ObservedMember<'db>,
        instance: Option<&ObservedType<'db>>,
        owner: &ObservedType<'db>,
        fallback: ObservedMember<'db>,
        policy: InstanceFallbackShadowsNonDataDescriptor,
    ) -> Option<ObservedMember<'db>> {
        let db = self.db;
        let env = self.env;
        let original = member.place(db).place.ignore_possibly_undefined();
        let (meta, kind, error) = self.get_attribute(member, instance, owner)?;
        let mut combined = None;
        let (result, selected) = DescriptorAccess {
            member: meta.place(db),
            kind,
            error,
            properties: original.and_then(|ty| ty.property_deprecations(db)),
            slot: matches!(original, Some(Type::SlotDescriptor(_))),
            policy,
        }
        .select(db, fallback.result, |_, _| {
            let mut builder =
                UnionBuilder::new(db, env).with_observed_context(self.context.clone());
            for child in meta.value.iter().chain(fallback.value.iter()) {
                builder.add_observed_in_place(child.clone());
            }
            let value = builder.build_observed();
            let ty = value.ty;
            combined = Some(value);
            ty
        });
        let value = match selected {
            MemberSelection::Meta => meta.value,
            MemberSelection::Fallback => fallback.value,
            MemberSelection::Both => combined,
        };
        Some(match value {
            Some(value) => ObservedMember::from_value(db, result, value),
            None => self.missing(owner),
        })
    }

    fn get_attribute(
        &self,
        member: ObservedMember<'db>,
        instance: Option<&ObservedType<'db>>,
        owner: &ObservedType<'db>,
    ) -> Option<(
        ObservedMember<'db>,
        AttributeKind,
        Option<DescriptorGetCallContext<'db>>,
    )> {
        let db = self.db;
        let env = self.env;
        let Some(descriptor) = &member.value else {
            return Some((member, AttributeKind::NormalOrNonDataDescriptor, None));
        };
        if let Some(unfolded) = descriptor.unfold_in_context(db, env, self.context) {
            let result = map_member_lookup_type(db, member.result, |_| unfolded.ty);
            return self.get_attribute(
                ObservedMember::from_value(db, result, unfolded),
                instance,
                owner,
            );
        }
        // These values already carry their descriptor outcome, even when no nominal runtime
        // class is available. Ordinary descriptor invocation treats them as data descriptors.
        if matches!(
            descriptor.ty,
            Type::Dynamic(_) | Type::Divergent(_) | Type::Never
        ) {
            return Some((member, AttributeKind::DataDescriptor, None));
        }
        if let Type::Union(union) = descriptor.ty {
            let mut members = Vec::new();
            let mut all_data = true;
            let mut error = None;
            for index in 0..union.elements(db).len() {
                let child = descriptor.project(db, env, ObservationEdge::UnionElement(index))?;
                let result = map_member_lookup_type(db, member.result, |_| child.ty);
                let (result, kind, child_error) = self.get_attribute(
                    ObservedMember::from_value(db, result, child),
                    instance,
                    owner,
                )?;
                members.push(result);
                all_data &= kind.is_data();
                error = error.or(child_error);
            }
            let kind = if all_data {
                AttributeKind::DataDescriptor
            } else {
                AttributeKind::NormalOrNonDataDescriptor
            };
            return Some((
                self.combine_members(descriptor, members, false)?,
                kind,
                error,
            ));
        }
        if let Type::Intersection(intersection) = descriptor.ty {
            if intersection.positive(db).is_empty() {
                return Some((member, AttributeKind::NormalOrNonDataDescriptor, None));
            }
            let mut members = Vec::new();
            let mut error = None;
            for index in 0..intersection.positive(db).len() {
                let child =
                    descriptor.project(db, env, ObservationEdge::IntersectionPositive(index))?;
                let result = map_member_lookup_type(db, member.result, |_| child.ty);
                let (result, _, child_error) = self.get_attribute(
                    ObservedMember::from_value(db, result, child),
                    instance,
                    owner,
                )?;
                members.push(result);
                error = error.or(child_error);
            }
            return Some((
                self.combine_members(descriptor, members, true)?,
                AttributeKind::NormalOrNonDataDescriptor,
                error,
            ));
        }
        if let Some(dynamic) = descriptor.ty.dynamic_descriptor_type() {
            let value = descriptor.unchanged_or_unresolved(dynamic);
            return Some((
                ObservedMember::from_value(
                    db,
                    map_member_lookup_type(db, member.result, |_| dynamic),
                    value,
                ),
                AttributeKind::DataDescriptor,
                None,
            ));
        }
        if matches!(descriptor.ty, Type::BoundMethod(_)) {
            return Some((member, AttributeKind::NormalOrNonDataDescriptor, None));
        }
        if let Type::PropertyInstance(property) = descriptor.ty {
            let Some(instance) = instance else {
                return Some((member, AttributeKind::DataDescriptor, None));
            };
            let Some(getter) = descriptor.project(db, env, ObservationEdge::PropertyGetter) else {
                let value = descriptor.unchanged_or_unresolved(Type::unknown());
                let context = DescriptorGetCallContext::new(
                    db,
                    descriptor.ty,
                    Type::KnownBoundMethod(super::KnownBoundMethodType::PropertyDunderGet(
                        property,
                    )),
                    Some(instance.ty),
                    owner.ty,
                );
                return Some((
                    ObservedMember::from_value(
                        db,
                        map_member_lookup_type(db, member.result, |_| value.ty),
                        value,
                    ),
                    AttributeKind::DataDescriptor,
                    Some(context),
                ));
            };
            let arguments = CallArguments::positional([instance.ty]);
            let (bindings, error) = match self.call(&getter, &arguments)? {
                Ok(bindings) => (bindings, None),
                Err(CallError(_, bindings)) => (
                    *bindings,
                    Some(DescriptorGetCallContext::new(
                        db,
                        descriptor.ty,
                        Type::KnownBoundMethod(super::KnownBoundMethodType::PropertyDunderGet(
                            property,
                        )),
                        Some(instance.ty),
                        owner.ty,
                    )),
                ),
            };
            let value = bindings.observed_return_type(db, env, self.context);
            return Some((
                ObservedMember::from_value(
                    db,
                    map_member_lookup_type(db, member.result, |_| value.ty),
                    value,
                ),
                AttributeKind::DataDescriptor,
                error,
            ));
        }
        if let Some(bound) =
            descriptor
                .ty
                .function_like_dunder_get(db, env, instance.map(|v| v.ty), Some(owner.ty))
        {
            let function = descriptor.project(db, env, ObservationEdge::UnderlyingFunction)?;
            let value = if let Type::BoundMethod(method) = bound {
                let receiver = if descriptor.ty.is_classmethod(db) {
                    owner
                } else {
                    instance?
                };
                ObservedType::constructed(
                    bound,
                    [
                        (
                            ObservationEdge::IntrinsicMember(Name::new_static("__func__")),
                            function.unchanged_or_unresolved(method.func(db)),
                        ),
                        (
                            ObservationEdge::IntrinsicMember(Name::new_static("__self__")),
                            receiver.unchanged_or_unresolved(method.self_instance(db)),
                        ),
                    ],
                )
            } else {
                function.unchanged_or_unresolved(bound)
            };
            return Some((
                ObservedMember::from_value(
                    db,
                    map_member_lookup_type(db, member.result, |_| bound),
                    value,
                ),
                AttributeKind::NormalOrNonDataDescriptor,
                None,
            ));
        }
        if let Type::SlotDescriptor(slot) = descriptor.ty {
            let value = descriptor
                .unchanged_or_unresolved(instance.map_or(descriptor.ty, |_| slot.value_type(db)));
            return Some((
                ObservedMember::from_value(
                    db,
                    map_member_lookup_type(db, member.result, |_| value.ty),
                    value,
                ),
                AttributeKind::DataDescriptor,
                None,
            ));
        }
        if self
            .descriptor_slot(descriptor, "__get__", MemberLookupPolicy::REQUIRE_CONCRETE)?
            .place(db)
            .place
            .is_undefined()
        {
            return Some((member, AttributeKind::NormalOrNonDataDescriptor, None));
        }
        let get = self.descriptor_slot(
            descriptor,
            "__get__",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        )?;
        let getter_definitely_defined = get.place(db).place.is_definitely_bound();
        let Some(getter) = get.value else {
            return Some((member, AttributeKind::NormalOrNonDataDescriptor, None));
        };
        if getter.ty.is_divergent() {
            return Some((member, AttributeKind::NormalOrNonDataDescriptor, None));
        }
        let kind = if !self
            .descriptor_slot(descriptor, "__set__", MemberLookupPolicy::REQUIRE_CONCRETE)?
            .place(db)
            .place
            .is_undefined()
            || !self
                .descriptor_slot(
                    descriptor,
                    "__delete__",
                    MemberLookupPolicy::REQUIRE_CONCRETE,
                )?
                .place(db)
                .place
                .is_undefined()
        {
            AttributeKind::DataDescriptor
        } else {
            AttributeKind::NormalOrNonDataDescriptor
        };
        let instance_ty = instance.map_or_else(|| Type::none(db, env), |v| v.ty);
        let arguments = CallArguments::positional([descriptor.ty, instance_ty, owner.ty]);
        let result = self.call(&getter, &arguments)?;
        let (bindings, error) = match result {
            Ok(bindings) => (bindings, None),
            Err(CallError(_, bindings)) => (
                *bindings,
                Some(DescriptorGetCallContext::new(
                    db,
                    descriptor.ty,
                    getter.ty,
                    instance.map(|v| v.ty),
                    owner.ty,
                )),
            ),
        };
        let mut value = bindings.observed_return_type(db, env, self.context);
        if !getter_definitely_defined {
            let mut builder =
                UnionBuilder::new(db, env).with_observed_context(self.context.clone());
            builder.add_observed_in_place(value);
            builder.add_observed_in_place(descriptor.clone());
            value = builder.build_observed();
        }
        let result = map_member_lookup_type(db, member.result, |_| value.ty);
        Some((ObservedMember::from_value(db, result, value), kind, error))
    }

    fn call(
        &self,
        callable: &ObservedType<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Option<Result<Bindings<'db>, CallError<'db>>> {
        let epoch = self.context.session().incomplete_epoch();
        let constraints = ConstraintSetBuilder::with_relation_context(self.context.clone());
        let result =
            Type::bindings_observed(self.db, self.env, callable.clone(), self.context.clone())
                .match_parameters(self.db, self.env, arguments)
                .check_types(
                    self.db,
                    self.env,
                    &constraints,
                    arguments,
                    TypeContext::default(),
                    &[],
                );
        (self.context.session().incomplete_epoch() == epoch).then_some(result)
    }

    /// Apply the same interception and error precedence as ordinary attribute lookup, while
    /// keeping each fallback call within the member operation that requested it.
    fn fallback(
        &self,
        input: &ObservedType<'db>,
        receiver: &ObservedType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        result: ObservedMember<'db>,
    ) -> Option<ObservedMember<'db>> {
        if policy.no_instance_fallback() {
            return Some(result);
        }
        let db = self.db;
        let name_type = Type::string_literal(db, name);
        let arguments = CallArguments::positional([name_type]);
        let returned = |bindings: &Bindings<'db>, error| {
            let value = bindings.observed_return_type(db, self.env, self.context);
            ObservedMember::from_value(
                db,
                member_lookup_result(db, Place::bound(value.ty).into(), error, None),
                value,
            )
        };
        let custom_getattr = || {
            if policy.no_getattr_lookup() {
                return Some(self.missing(input));
            }
            if matches!(
                input.ty,
                Type::KnownInstance(super::KnownInstanceType::TypeGenericAlias(_))
            ) {
                // A runtime type[T] alias delegates to its origin, type, independently of T.
                let origin =
                    input.unchanged_or_unresolved(KnownClass::Type.to_class_literal(db, self.env));
                return self.lookup(&origin, &origin, name, policy);
            }
            Some(
                match self.call_dunder(
                    input,
                    receiver,
                    "__getattr__",
                    &arguments,
                    MemberLookupPolicy::empty(),
                )? {
                    Ok(bindings) => returned(&bindings, None),
                    Err(CallDunderError::CallError(_, bindings, _)) => returned(
                        &bindings,
                        Some(MemberLookupErrorKind::GetAttr {
                            receiver: input.ty,
                            name: name_type,
                        }),
                    ),
                    Err(
                        CallDunderError::PossiblyUnbound { .. }
                        | CallDunderError::MethodNotAvailable,
                    ) => self.missing(input),
                },
            )
        };
        if !input
            .ty
            .custom_getattribute_may_affect_lookup(db, self.env, result.result)
        {
            return self.or_fall_back_to(input, result, custom_getattr);
        }

        let getattribute_policy = MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
            | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK;
        let custom_getattribute = match self.call_dunder(
            input,
            receiver,
            "__getattribute__",
            &arguments,
            getattribute_policy,
        )? {
            Ok(bindings) => returned(&bindings, None),
            Err(CallDunderError::CallError(_, bindings, _)) => returned(
                &bindings,
                Some(MemberLookupErrorKind::GetAttribute {
                    receiver: input.ty,
                    name: name_type,
                }),
            ),
            Err(CallDunderError::PossiblyUnbound { .. }) => self.missing(input),
            Err(CallDunderError::MethodNotAvailable) => {
                return self.or_fall_back_to(input, result, custom_getattr);
            }
        };
        if let Err(error) = custom_getattribute.result {
            let properties = result
                .result
                .unwrap_or_else(|error| error.fallback_member(db))
                .deprecated_properties(db);
            let mut selected = self.or_fall_back_to(input, result, || Some(custom_getattribute))?;
            selected.result =
                member_lookup_result(db, selected.place(db), Some(error.kind(db)), properties);
            return Some(selected);
        }

        // A custom interceptor can bypass a descriptor whose own call failed, including when
        // the interceptor is only possibly defined. A missing interceptor takes the branch above.
        let mut result = result;
        if matches!(
            result.result.err().map(|error| error.kind(db)),
            Some(MemberLookupErrorKind::DescriptorGet(_))
        ) {
            result.result = Ok(result
                .result
                .unwrap_or_else(|error| error.fallback_member(db)));
        }
        let result = self.or_fall_back_to(input, result, || Some(custom_getattribute))?;
        self.or_fall_back_to(input, result, custom_getattr)
    }

    /// Invoke an implicit method without instance storage or attribute fallback. Missing and
    /// possibly-defined methods remain distinct, as they affect interceptor error precedence.
    fn call_dunder(
        &self,
        input: &ObservedType<'db>,
        receiver: &ObservedType<'db>,
        name: &str,
        arguments: &CallArguments<'_, 'db>,
        policy: MemberLookupPolicy,
    ) -> Option<Result<Bindings<'db>, CallDunderError<'db>>> {
        // The dunder is about to be called, so it always needs its actual callable value even
        // when its result is used only to establish the presence of the original member.
        let method = Self {
            demand: MemberLookupDemand::Value,
            ..*self
        }
        .lookup(
            input,
            receiver,
            name,
            policy
                | MemberLookupPolicy::NO_INSTANCE_FALLBACK
                | MemberLookupPolicy::NO_GETATTR_LOOKUP,
        )?;
        let Place::Defined(place) = method.place(self.db).place else {
            return Some(Err(CallDunderError::MethodNotAvailable));
        };
        let callable = method.value.as_ref()?;
        let bindings = match self.call(callable, arguments)? {
            Ok(bindings) => bindings,
            Err(CallError(kind, bindings)) => {
                return Some(Err(CallDunderError::CallError(
                    kind,
                    bindings,
                    place.provenance,
                )));
            }
        };
        if place.definedness == Definedness::PossiblyUndefined {
            return Some(Err(CallDunderError::PossiblyUnbound {
                bindings: Box::new(bindings),
                unbound_on: None,
            }));
        }
        Some(Ok(bindings))
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ty_python_core::ProgramFile;

    use super::{ObservedMember, lookup_member};
    use crate::db::tests::{TestDb, setup_db};
    use crate::place::{Definedness, Place, global_symbol};
    use crate::types::projection::ObservedType;
    use crate::types::relation::RelationContext;
    use crate::types::{KnownClass, MemberLookupPolicy, Type, TypeQualifiers};

    fn assert_lookup_parity<'db>(
        db: &'db TestDb,
        receiver: Type<'db>,
        name: &str,
    ) -> anyhow::Result<ObservedMember<'db>> {
        let env = db.program_environment();
        let policy = MemberLookupPolicy::default();
        let legacy = receiver.member_lookup_with_policy_and_receiver(db, &env, name, policy, None);
        let input = ObservedType::root(receiver);
        let observed = lookup_member(
            db,
            &env,
            &input,
            &input,
            name,
            policy,
            &RelationContext::default(),
        )
        .ok_or_else(|| anyhow::anyhow!("finite member {name} did not complete"))?;
        // Compare the full result: the member's place includes definedness, declaration origin,
        // provenance, and qualifiers; an error includes its recovery member and call context.
        let place = receiver.member_lookup_with_policy(db, &env, name, policy);
        assert_eq!(observed.result, legacy, "lookup of {name}");
        assert_eq!(observed.place(db), place, "public lookup of {name}");
        assert_eq!(
            observed.value.as_ref().map(|value| value.ty),
            place.place.ignore_possibly_undefined()
        );
        Ok(observed)
    }

    #[test]
    fn finite_lookup_preserves_member_precedence_and_metadata() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from typing import ClassVar, Final, overload

class Meta(type):
    @property
    def value(cls) -> int:
        return 1

class Descriptor:
    @overload
    def __get__(self, instance: None, owner: type) -> str: ...
    @overload
    def __get__(self, instance: object, owner: type) -> bytes: ...
    def __get__(self, instance: object, owner: type) -> str | bytes:
        raise NotImplementedError

class Subject(metaclass=Meta):
    value: str = "text"
    shared: ClassVar[int] = 1
    constant: Final[int] = 1
    descriptor = Descriptor()

    @property
    def property_value(self) -> bytes:
        return b"value"

class Other:
    value: bytes

class Broad:
    value: object

class Missing: ...

class Fallback:
    def __getattr__(self, name: str) -> bytes:
        raise NotImplementedError

instance: Subject
class_object: type[Subject]
combined: Subject | Other
possibly_missing: Subject | Missing
intersection: Subject & Broad
fallback: Fallback
"#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        let symbol = |name| global_symbol(db, file, name).place.expect_type();
        for (receiver, member) in [
            ("instance", "value"),
            ("Subject", "value"),
            ("class_object", "value"),
            ("instance", "property_value"),
            ("Subject", "property_value"),
            ("instance", "descriptor"),
            ("Subject", "descriptor"),
            ("combined", "value"),
            ("possibly_missing", "value"),
            ("intersection", "value"),
            ("fallback", "unknown"),
            ("Missing", "unknown"),
        ] {
            assert_lookup_parity(db, symbol(receiver), member)?;
        }
        let instance = assert_lookup_parity(db, symbol("instance"), "value")?;
        let class = assert_lookup_parity(db, symbol("class_object"), "value")?;
        assert_eq!(
            instance.value.map(|value| value.ty),
            Some(KnownClass::Str.to_instance(db, &env))
        );
        assert_eq!(
            class.value.map(|value| value.ty),
            Some(KnownClass::Int.to_instance(db, &env))
        );
        let possibly_missing = assert_lookup_parity(db, symbol("possibly_missing"), "value")?;
        assert!(
            matches!(possibly_missing.place(db).place, Place::Defined(place) if place.definedness == Definedness::PossiblyUndefined)
        );
        for (member, qualifier) in [
            ("shared", TypeQualifiers::CLASS_VAR),
            ("constant", TypeQualifiers::FINAL),
        ] {
            let observed = assert_lookup_parity(db, symbol("instance"), member)?;
            assert!(observed.place(db).qualifiers.contains(qualifier));
        }
        Ok(())
    }

    #[test]
    fn callable_annotation_is_a_member_without_runtime_descriptor_binding() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from collections.abc import Callable
from typing import ClassVar

class Subject:
    callback: ClassVar[Callable[[int], str]]

instance: Subject
"#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        for receiver in ["Subject", "instance"] {
            let receiver = global_symbol(db, file, receiver).place.expect_type();
            let observed = assert_lookup_parity(db, receiver, "callback")?;
            assert!(
                matches!(observed.value.map(|value| value.ty), Some(Type::Callable(callable)) if callable.is_regular(db))
            );
        }
        Ok(())
    }

    #[test]
    fn bottom_protocol_class_members_keep_materialized_values() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from typing import Any, ClassVar, Never, Protocol
from ty_extensions import Bottom

class P(Protocol):
    value: ClassVar[Any]

class Subject:
    missing: ClassVar[Never]
    empty: ClassVar[None]

bottom: type[Bottom[P]]
"#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        for (receiver, name) in [
            ("Subject", "empty"),
            ("Subject", "missing"),
            ("bottom", "value"),
        ] {
            let receiver = global_symbol(db, file, receiver).place.expect_type();
            assert_lookup_parity(db, receiver, name)?;
        }
        Ok(())
    }

    #[test]
    fn failed_implicit_calls_preserve_error_and_recovery_value() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
class Descriptor:
    def __get__(self, instance: int, owner: type) -> bytes:
        raise NotImplementedError

class Subject:
    value = Descriptor()

class Fallback:
    def __getattr__(self, name: int) -> str:
        raise NotImplementedError

subject: Subject
fallback: Fallback
"#,
        )?;
        let db = &db;
        let env = db.program_environment();
        let file = system_path_to_file(db, "/src/a.py")?;
        let file = ProgramFile::new(db, file, env.program(db));
        for (receiver, member, expected) in [
            ("subject", "value", KnownClass::Bytes),
            ("fallback", "missing", KnownClass::Str),
        ] {
            let receiver = global_symbol(db, file, receiver).place.expect_type();
            let observed = assert_lookup_parity(db, receiver, member)?;
            assert!(observed.result.is_err());
            assert_eq!(
                observed.value.map(|value| value.ty),
                Some(expected.to_instance(db, &env))
            );
        }
        Ok(())
    }
}
