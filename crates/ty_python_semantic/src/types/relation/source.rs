//! Source relations retain their comparison owners across suspended work.

use std::alloc::Layout;
use std::cell::Cell;
use std::future::Future;
use std::ops::{ControlFlow, Deref};

use salsa::execution_probe::{
    BorrowOrCopy, ExecutionWork, FieldRequest, FieldRequestContext, RunError, RunResult, TaskEndpoint,
};
use ty_python_core::Truthiness;

use super::callable::{CallableSourceFacts, check_callable_source_with};
use super::dependencies::OrdinaryDependencies;
use super::guard::with_relation_guard;
use super::pair::PairEvaluation;
use super::pair_effects::PairEffects;
use super::source_intersection::check_source_intersection_with;
use super::target_intersection::check_target_intersection_with;
use super::target_union::check_target_union_with;
use super::typevar_subclass::{TypeVarSubclassFacts, check_typevar_subclass_with};
use super::{EquivalenceChecker, TypeRelationChecker};
use crate::types::callable::CallableTypes;
use crate::types::class::relation::{ClassPairEffects, check_class_pair_with};
use crate::types::class_base::ClassBase;
use crate::types::class_selection::{
    LiteralFallbackEffects, LiteralFallbackFacts, literal_fallback_instance_with,
};
use crate::types::constraints::source::{SourceStructural, SourceStructuralResult};
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::instance::nominal_relation::{NominalPairFacts, check_nominal_pair_with};
use crate::types::known_instance::{
    FieldInstance, FunctoolsPartialInstance, InternedType, MethodWrapper, MethodWrapperKind,
    SentinelInstance,
};
use crate::types::mro::iteration::MroCursor;
use crate::types::set_theoretic::builder::intersection_insertion::Elements;
use crate::types::tuple::buffer::TupleBufferStorageEffects;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::typevar::{BoundTypeVarIdentity, BoundTypeVarInstance, TypeVarDomain};
use crate::types::{
    BoundMethodType, BoundSuperType, BytesLiteralType, CallableSignature, CallableType,
    ClassLiteral, ClassType, EnumComplementType, EnumLiteralType, FunctionType, GenericAlias,
    IntersectionType, KnownBoundMethodType, KnownClass, KnownInstanceType,
    MaterializationEquivalenceVisitor, MaterializationKind, NewType, NominalInstanceType,
    PropertyInstanceType, ProtocolInstanceType, RecursiveType, SpecialFormType, StaticClassLiteral,
    StringLiteralType, SubclassOfInner, SubclassOfType, Type, TypeAliasType, TypeFormType, TypeGuardType, TypeIsType,
    TypeVarBoundOrConstraints, TypedDictType, UnionType, UpcastPolicy,
};
use crate::{Db, ProgramEnvironment};

mod callable_source;
mod disjoint_guard;
mod disjoint_intersection;
mod disjointness;
mod guard;
mod guard_control;
mod nominal;
pub(in crate::types) mod owned_constraints;
#[cfg(test)]
pub(in crate::types) mod receiver_constraint_observations;
pub(in crate::types) mod resources;
pub(in crate::types) mod retained;
mod signature;
mod source_intersection;
mod target_intersection;
mod target_union;
mod tuple;
mod typevar_subclass;

#[cfg(test)]
pub(in crate::types) use guard::observations as guard_observations;
#[cfg(test)]
pub(in crate::types) use signature::observations as signature_observations;
#[cfg(test)]
pub(in crate::types) use target_intersection::materialization_observations;

use callable_source::BorrowedCallableSource;
use nominal::BorrowedNominalPairs;
use resources::RelationResourceAccess;
pub(in crate::types) use retained::RetainedRelationSource;
use retained::{PairChildren, UnavailablePairs};
use source_intersection::BorrowedSourceIntersection;
use target_intersection::BorrowedTargetIntersection;
use target_union::BorrowedTargetUnion;
use typevar_subclass::BorrowedTypeVarSubclass;

pub(in crate::types) use super::source_operations::RelationOperation as RelationSourceOperation;

#[cfg(test)]
macro_rules! owner_observations {
    ($name:ident) => {
        pub(in crate::types) mod $name {
                    use std::cell::Cell;

                    use crate::Db;

                    thread_local! {
                        static LIVE: Cell<usize> = const { Cell::new(0) };
                        static ENTERED: Cell<usize> = const { Cell::new(0) };
                        static REMAINING: Cell<Option<usize>> = const { Cell::new(None) };
                        static CANCEL: Cell<bool> = const { Cell::new(false) };
                    }

                    pub(in crate::types) fn reset(cancel: bool) {
                        assert_eq!(LIVE.get(), 0);
                        ENTERED.set(0);
                        REMAINING.set(None);
                        CANCEL.set(cancel);
                    }

                    pub(in crate::types) fn progress() -> (usize, usize, Option<usize>) {
                        (LIVE.get(), ENTERED.get(), REMAINING.get())
                    }

                    pub(super) struct OwnersLifetime;

                    impl Drop for OwnersLifetime {
                        fn drop(&mut self) {
                            LIVE.set(LIVE.get() - 1);
                        }
                    }

                    pub(super) fn owners_ready(db: &dyn Db) -> OwnersLifetime {
                        LIVE.set(LIVE.get() + 1);
                        ENTERED.set(ENTERED.get() + 1);
                        REMAINING.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                            db,
                        ));
                        if CANCEL.replace(false) {
                            db.cancellation_token().cancel();
                        }
                        OwnersLifetime
                    }
                }
    };
}

#[cfg(test)]
owner_observations!(redundancy_observations);
#[cfg(test)]
owner_observations!(disjointness_observations);

pub(in crate::types) trait RelationSourceEffects<'run, 'db: 'run>: TupleBufferStorageEffects<'db> {
    type Resources: RelationResourceAccess<'run, 'db>;
    type Retained: RetainedRelationSource<'run, 'db>;

    fn resources(&self) -> Self::Resources;
    fn retained(&self) -> Self::Retained;

    fn endpoint(&self) -> &TaskEndpoint<'run, 'db>;

    async fn unavailable<T>(&self, operation: RelationSourceOperation) -> RunResult<T>;

    async fn type_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Truthiness>;

    async fn nominal_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>>;

    /// Constructs a canonical tuple from a retained specification using the original tuple ingredient.
    async fn tuple_from_spec(
        &self,
        _env: &ProgramEnvironment<'db>,
        _spec: &TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        self.unavailable(RelationSourceOperation::TuplePacking).await
    }

    async fn tuple_spec(&self, _tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        self.unavailable(RelationSourceOperation::TupleFieldRead).await
    }

    async fn tuple_pack_identity(
        &self,
        _pack: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.unavailable(RelationSourceOperation::TupleFieldRead).await
    }

    async fn nominal_is_definition_generic(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool>;

    async fn nominal_known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>>;

    async fn function_runtime_class(&self, function: FunctionType<'db>) -> RunResult<KnownClass>;

    async fn callable_conversion(
        &self,
        _env: &ProgramEnvironment<'db>,
        _source: Type<'db>,
        _policy: UpcastPolicy,
    ) -> RunResult<Option<CallableTypes<'db>>> {
        self.unavailable(RelationSourceOperation::CallableSource)
            .await
    }

    async fn known_class_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>>;

    async fn property_instance_fallback(
        &self,
        _env: &ProgramEnvironment<'db>,
        _property: PropertyInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(RelationSourceOperation::PropertyInstanceFallback)
            .await
    }

    async fn class_literal_metaclass_instance(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(RelationSourceOperation::ClassLiteralMetaclassInstance)
            .await
    }

    async fn class_metaclass_instance(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(RelationSourceOperation::ClassMetaclassInstance)
            .await
    }

    async fn subclass_metaclass_instance(
        &self,
        _env: &ProgramEnvironment<'db>,
        _subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(RelationSourceOperation::SubclassMetaclassInstance)
            .await
    }

    async fn cached_materialization(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>>;

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]>;

    async fn intersection_positive_contains(
        &self,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool>;

    async fn intersection_negative_contains(
        &self,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool>;

    async fn intersection_positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>>;

    async fn intersection_negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>>;

    async fn intersection_next_element(
        &self,
        elements: &mut Elements<'db>,
    ) -> RunResult<Option<Type<'db>>>;

    async fn intersection_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>>;

    async fn intersection_expand(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Type<'db>>;

    async fn class_default_specialization(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>>;

    async fn subclass_inner_class(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Option<ClassType<'db>>>;

    async fn class_mro_start(&self, class: ClassType<'db>) -> RunResult<MroCursor<'db>>;

    async fn class_mro_next(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> RunResult<Option<ClassBase<'db>>>;

    async fn class_is_object(&self, class: ClassType<'db>) -> RunResult<bool>;

    async fn class_is_final(&self, class: ClassType<'db>) -> RunResult<bool>;
}

/// Checks subtyping with fresh inference owners and the ordinary always-satisfied criterion.
pub(in crate::types) async fn subtyping_condition<
    'run,
    'db: 'run,
    E: RelationSourceEffects<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    source: Type<'db>,
    target: Type<'db>,
    effects: &E,
) -> RunResult<bool> {
    fresh_condition(db, env, source, target, FreshRelation::Subtyping, effects).await
}

/// Checks assignability with fresh constraints, returning whether the resulting constraints are always satisfied.
pub(in crate::types) async fn assignability_condition<
    'run,
    'db: 'run,
    E: RelationSourceEffects<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    source: Type<'db>,
    target: Type<'db>,
    effects: &E,
) -> RunResult<bool> {
    fresh_condition(db, env, source, target, FreshRelation::Assignability, effects).await
}

/// Checks canonical redundancy with fresh owners and the ordinary always-satisfied criterion.
pub(in crate::types) async fn redundancy_condition<
    'run,
    'db: 'run,
    E: RelationSourceEffects<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    source: Type<'db>,
    target: Type<'db>,
    effects: &E,
) -> RunResult<bool> {
    fresh_condition(db, env, source, target, FreshRelation::Redundancy, effects).await
}

/// Checks disjointness with fresh inference owners and the ordinary always-satisfied criterion.
pub(in crate::types) async fn disjointness_condition<
    'run,
    'db: 'run,
    E: RelationSourceEffects<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    effects: &E,
) -> RunResult<bool> {
    fresh_condition(
        db,
        env,
        left,
        right,
        FreshRelation::Disjointness {
            perform_expensive_checks: true,
        },
        effects,
    )
    .await
}

#[cfg(test)]
pub(in crate::types) async fn disjointness_condition_with_mode<
    'run,
    'db: 'run,
    E: RelationSourceEffects<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    left: Type<'db>,
    right: Type<'db>,
    perform_expensive_checks: bool,
    effects: &E,
) -> RunResult<bool> {
    fresh_condition(
        db,
        env,
        left,
        right,
        FreshRelation::Disjointness {
            perform_expensive_checks,
        },
        effects,
    )
    .await
}

#[derive(Clone, Copy)]
pub(in crate::types) enum FreshRelation {
    Subtyping,
    Assignability,
    Redundancy,
    Disjointness { perform_expensive_checks: bool },
}

async fn fresh_condition<'run, 'db: 'run, E: RelationSourceEffects<'run, 'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    source: Type<'db>,
    target: Type<'db>,
    relation: FreshRelation,
    effects: &E,
) -> RunResult<bool> {
    effects
        .resources()
        .condition(db, env, source, target, relation, effects)
        .await
}

/// Compare both directions using the ordinary equivalence algorithm and retained owners.
pub(in crate::types) async fn equivalence_condition<
    'run,
    'db: 'run,
    E: RelationSourceEffects<'run, 'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    source: Type<'db>,
    target: Type<'db>,
    effects: &E,
) -> RunResult<bool> {
    effects
        .resources()
        .equivalence(db, env, source, target, effects)
        .await
}

struct BorrowedPairs<'effects, 'run, 'db: 'run, 'c, E, P = UnavailablePairs> {
    children: &'effects P,
    db: &'db dyn Db,
    endpoint: &'effects TaskEndpoint<'run, 'db>,
    effects: &'effects E,
    constraints: &'c ConstraintSetBuilder<'db>,
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    LiteralFallbackEffects<'db> for BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()
            })
            .await;
        Ok(())
    }

    async fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.effects.known_class_instance(env, class).await
    }

    async fn function_runtime_class(&self, function: FunctionType<'db>) -> RunResult<KnownClass> {
        self.effects.function_runtime_class(function).await
    }

    async fn enum_instance(
        &self,
        _env: &ProgramEnvironment<'db>,
        _literal: EnumLiteralType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(RelationSourceOperation::LiteralFallbackEnumInstance)
            .await
    }
}

struct BorrowedClassPairs<'pairs, 'effects, 'run, 'db: 'run, 'c, E, P> {
    pairs: &'pairs BorrowedPairs<'effects, 'run, 'db, 'c, E, P>,
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    ClassPairEffects<'c, 'db> for BorrowedClassPairs<'_, '_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;
    type MroCursor<'state>
        = MroCursor<'db>
    where
        Self: 'state;
    type Ancestors<'state>
        = ()
    where
        Self: 'state;
    type Fold<'state>
        = ConstraintFold<'db, 'c>
    where
        Self: 'state;

    async fn same_literal(
        &self,
        source: ClassLiteral<'db>,
        target: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                self.pairs.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<ClassLiteral<'db>>()
                        .checked_mul(2)
                        .and_then(|bytes| bytes.checked_add(size_of::<bool>().checked_mul(2)?))
                        .ok_or(RunError::Contract("class literal comparison quotation overflow"))?,
                })?;
                Ok(source == target)
            })
            .await)
    }

    async fn same_origin(
        &self,
        source: GenericAlias<'db>,
        target: GenericAlias<'db>,
    ) -> RunResult<bool> {
        self.pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(4)?;
                self.pairs.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<StaticClassLiteral<'db>>()
                        .checked_mul(2)
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<GenericAlias<'db>>().checked_mul(2)?)
                        })
                        .and_then(|bytes| bytes.checked_add(size_of::<bool>().checked_mul(2)?))
                        .ok_or(RunError::Contract("class origin comparison quotation overflow"))?,
                })
            })
            .await;
        let source_origin = self
            .pairs
            .read_field_from(
                source,
                |alias, context| alias.field_requests(context),
                |fields| fields.origin(),
            )
            .await;
        let target_origin = self
            .pairs
            .read_field_from(
                target,
                |alias, context| alias.field_requests(context),
                |fields| fields.origin(),
            )
            .await;
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(1)?;
                Ok(source_origin == target_origin)
            })
            .await)
    }

    async fn specialization_pair(
        &self,
        _source: GenericAlias<'db>,
        _target: GenericAlias<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .unavailable(RelationSourceOperation::ClassSpecialization)
            .await
    }

    async fn constant(&self, value: bool) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                self.pairs.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<ConstraintSet<'db, 'c>>()
                        .checked_mul(2)
                        .ok_or(RunError::Contract("class constraint quotation overflow"))?,
                })?;
                Ok(ConstraintSet::from_bool(self.pairs.constraints, value))
            })
            .await)
    }

    async fn is_object(&self, class: ClassType<'db>) -> RunResult<bool> {
        self.pairs.effects.class_is_object(class).await
    }

    async fn is_final(&self, class: ClassType<'db>) -> RunResult<bool> {
        self.pairs.effects.class_is_final(class).await
    }

    async fn mro_start(&self, class: ClassType<'db>) -> RunResult<Self::MroCursor<'_>> {
        self.pairs.effects.class_mro_start(class).await
    }

    async fn mro_next<'state>(
        &'state self,
        cursor: &mut Self::MroCursor<'state>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        self.pairs.effects.class_mro_next(cursor).await
    }

    async fn fold_start(&self) -> RunResult<Self::Fold<'_>> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                self.pairs.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<ConstraintFold<'db, 'c>>()
                        .checked_mul(2)
                        .ok_or(RunError::Contract("class constraint fold quotation overflow"))?,
                })?;
                Ok(ConstraintFold::new(
                    self.pairs.constraints,
                    ConstraintFoldKind::Any,
                ))
            })
            .await)
    }

    async fn fold_push<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        self.pairs.push_constraints(fold, next).await
    }

    async fn fold_finish<'state>(
        &'state self,
        fold: &mut Self::Fold<'state>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs.finish_constraints(fold).await
    }

    async fn is_always(&self, value: ConstraintSet<'db, 'c>) -> RunResult<bool> {
        Ok(self
            .pairs
            .endpoint
            .local_call(|| {
                self.pairs.endpoint.admit_work(2)?;
                self.pairs.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<bool>()
                        .checked_mul(2)
                        .and_then(|bytes| bytes.checked_add(size_of::<ConstraintSet<'db, 'c>>()))
                        .ok_or(RunError::Contract("class constraint check quotation overflow"))?,
                })?;
                value.verify_builder(self.pairs.constraints);
                Ok(value.is_trivially_always_satisfied())
            })
            .await)
    }

    async fn ancestors_start(&self, _class: ClassType<'db>) -> RunResult<Self::Ancestors<'_>> {
        self.pairs
            .unavailable(RelationSourceOperation::ClassAncestors)
            .await
    }

    async fn ancestors_next<'state>(
        &'state self,
        _cursor: &mut Self::Ancestors<'state>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.pairs
            .unavailable(RelationSourceOperation::ClassAncestors)
            .await
    }

    async fn disjoin(
        &self,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.pairs
            .combine_constraints(self.pairs.constraints, ConstraintFoldKind::Any, left, right)
            .await
    }
}

impl<'run, 'db: 'run, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    /// Constructs a generated field request after admitting its fixed carriers, then reads it.
    /// The factories only build the generated wrapper and request; `read_field` separately
    /// admits canonical field selection and conversion through `BorrowOrCopy`.
    async fn read_field_from<Value, Fields, R, MakeFields, MakeRequest>(
        &self,
        value: Value,
        make_fields: MakeFields,
        make_request: MakeRequest,
    ) -> R::Output
    where
        Value: Copy,
        R: FieldRequest<'db>,
        MakeFields: FnOnce(Value, FieldRequestContext<'db>) -> Fields + Copy,
        MakeRequest: FnOnce(&Fields) -> R + Copy,
    {
        let endpoint = self.endpoint;
        endpoint
            .local_call(|| {
                let requested_bytes = size_of::<FieldRequestContext<'db>>()
                    .checked_mul(4)
                    .and_then(|bytes| bytes.checked_add(size_of::<Fields>().checked_mul(2)?))
                    .and_then(|bytes| bytes.checked_add(size_of::<R>().checked_mul(4)?))
                    .and_then(|bytes| bytes.checked_add(size_of::<Value>().checked_mul(4)?))
                    .and_then(|bytes| bytes.checked_add(size_of::<MakeFields>().checked_mul(4)?))
                    .and_then(|bytes| bytes.checked_add(size_of::<MakeRequest>().checked_mul(4)?))
                    .and_then(|bytes| {
                        bytes.checked_add(size_of::<&TaskEndpoint<'run, 'db>>().checked_mul(2)?)
                    })
                    .ok_or(RunError::Contract("field request carrier quotation overflow"))?;
                endpoint.admit_work(24)?;
                endpoint.admit(ExecutionWork::Resource { requested_bytes })
            })
            .await;
        let request = endpoint
            .local_call(move || {
                let fields = make_fields(value, endpoint.field_request_context());
                Ok(make_request(&fields))
            })
            .await;
        endpoint.read_field(request, &BorrowOrCopy).await
    }

    async fn callable_runtime_class(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        let kind = self
            .endpoint
            .read_field(
                callable
                    .field_requests(self.endpoint.field_request_context())
                    .kind(),
                &BorrowOrCopy,
            )
            .await;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(kind.runtime_class())
            })
            .await)
    }

    async fn intersection_positive_any(
        &self,
        intersection: IntersectionType<'db>,
        predicate: fn(&Type<'db>) -> bool,
    ) -> RunResult<bool> {
        let mut elements = self
            .effects
            .intersection_positive_elements(intersection)
            .await?;
        while let Some(element) = self
            .effects
            .intersection_next_element(&mut elements)
            .await?
        {
            if self
                .endpoint
                .local_call(|| {
                    self.endpoint.admit_work(2)?;
                    Ok(predicate(&element))
                })
                .await
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn pair(
        &self,
        db: &'db dyn Db,
        checker: &TypeRelationChecker<'_, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        let endpoint = self.endpoint;
        let mut pair = None;
        endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<PairEvaluation<'_, '_, '_, 'db>>() + 2)?;
                pair = Some(
                    PairEvaluation::start(db, checker, source, target, &OrdinaryDependencies)
                        .unwrap_or_else(|never| match never {}),
                );
                Ok(())
            })
            .await;
        if matches!(source, Type::RecursiveVar(_)) || matches!(target, Type::RecursiveVar(_)) {
            return self.unavailable(RelationSourceOperation::Operands).await;
        }
        let result = endpoint
            .child_call(|| checker.check_type_pair_inner_with(source, target, self))
            .await;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                let pair = pair
                    .as_ref()
                    .ok_or(RunError::Contract("relation entry was not installed"))?;
                Ok(pair
                    .finish_borrowed(db, result, &OrdinaryDependencies)
                    .unwrap_or_else(|never| match never {}))
            })
            .await)
    }

    async fn unavailable<T>(&self, operation: RelationSourceOperation) -> RunResult<T> {
        self.effects.unavailable(operation).await
    }

    async fn satisfy(&self, constraints: ConstraintSet<'db, 'c>, always: bool) -> RunResult<bool> {
        let terminal = self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(2)?;
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<Option<bool>>()
                        .checked_mul(2)
                        .and_then(|bytes| bytes.checked_add(size_of::<ConstraintSet<'db, 'c>>()))
                        .and_then(|bytes| bytes.checked_add(size_of::<bool>().checked_mul(2)?))
                        .ok_or(RunError::Contract("constraint satisfaction quotation overflow"))?,
                })?;
                constraints.verify_builder(self.constraints);
                Ok(constraints.satisfaction_start(always))
            })
            .await;
        match terminal {
            Some(result) => Ok(result),
            None => {
                self.unavailable(RelationSourceOperation::ConstraintSatisfaction)
                    .await
            }
        }
    }
}

fn materialization_guard_bytes() -> RunResult<usize> {
    Layout::new::<[Cell<usize>; 2]>()
        .extend(Layout::new::<
            <MaterializationEquivalenceVisitor<'_> as Deref>::Target,
        >())
        .map(|(layout, _)| layout.pad_to_align().size())
        .map_err(|_| RunError::Contract("materialization guard size overflow"))
}

macro_rules! unavailable_pair_effects {
    ($(fn $name:ident($($parameter:ident: $argument:ty),* $(,)?) -> $result:ty => $operation:ident;)*) => {
        $(
            async fn $name(&self, $($parameter: $argument),*) -> RunResult<$result> {
                $(let _ = $parameter;)*
                self.unavailable(RelationSourceOperation::$operation).await
            }
        )*
    };
}

impl<'run, 'db: 'run, 'a, 'c, E: RelationSourceEffects<'run, 'db>, P: PairChildren<'run, 'db, 'c>>
    PairEffects<'a, 'c, 'db> for BorrowedPairs<'_, 'run, 'db, 'c, E, P>
{
    type Error = RunError;

    async fn type_form_argument(&self, value: TypeFormType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .type_argument(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn field_default(&self, value: FieldInstance<'db>) -> RunResult<Option<Type<'db>>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .default_type(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn field_converter(
        &self,
        value: FieldInstance<'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .converter(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn method_wrapper_kind(&self, value: MethodWrapper<'db>) -> RunResult<MethodWrapperKind> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .kind(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn method_wrapper_type(&self, value: MethodWrapper<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .wrapped(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn partial_wrapped(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> RunResult<InternedType<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .wrapped(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn partial_callable(
        &self,
        value: FunctoolsPartialInstance<'db>,
    ) -> RunResult<CallableType<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .partial(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn interned_type(&self, value: InternedType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .inner(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn type_is_argument(&self, value: TypeIsType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .type_argument(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn type_guard_return(&self, value: TypeGuardType<'db>) -> RunResult<Type<'db>> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .return_type(),
                &BorrowOrCopy,
            )
            .await)
    }

    async fn step<T>(&self, _operation: impl FnOnce() -> T) -> RunResult<T> {
        self.unavailable(RelationSourceOperation::LocalStep).await
    }

    async fn type_is_always_falsy(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let truthiness = self.effects.type_truthiness(checker.env, ty).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(truthiness.is_always_false())
            })
            .await)
    }

    async fn type_is_always_truthy(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let truthiness = self.effects.type_truthiness(checker.env, ty).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                Ok(truthiness.is_always_true())
            })
            .await)
    }

    async fn combine_constraints(
        &self,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        match SourceStructural::new(self.endpoint)
            .combine(builder, kind, left, right)
            .await?
        {
            SourceStructuralResult::Complete(value) => Ok(value),
            SourceStructuralResult::Unsupported => {
                self.unavailable(RelationSourceOperation::ConstraintCombination)
                    .await
            }
        }
    }

    async fn push_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> RunResult<ControlFlow<ConstraintSet<'db, 'c>>> {
        match SourceStructural::new(self.endpoint)
            .push(fold, next)
            .await?
        {
            SourceStructuralResult::Complete(value) => Ok(value),
            SourceStructuralResult::Unsupported => {
                self.unavailable(RelationSourceOperation::ConstraintFold)
                    .await
            }
        }
    }

    async fn finish_constraints(
        &self,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        match SourceStructural::new(self.endpoint).finish(fold).await? {
            SourceStructuralResult::Complete(value) => Ok(value),
            SourceStructuralResult::Unsupported => {
                self.unavailable(RelationSourceOperation::ConstraintFold)
                    .await
            }
        }
    }

    async fn check_type_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        self.children
            .pair(self.db, self.effects, checker, source, target)
            .await
    }

    async fn is_never_satisfied(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> RunResult<bool> {
        self.satisfy(constraints, false).await
    }

    async fn guard<F>(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
        work: impl FnOnce() -> F,
    ) -> RunResult<ConstraintSet<'db, 'c>>
    where
        F: Future<Output = RunResult<ConstraintSet<'db, 'c>>>,
    {
        with_relation_guard(checker, source, target, work, self).await
    }

    async fn check_callable_source(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: CallableType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        with_relation_guard(
            checker,
            source,
            Type::Callable(target),
            || async {
                check_callable_source_with(
                    source,
                    target,
                    checker.relation,
                    CallableSourceFacts,
                    &BorrowedCallableSource::new(self, checker),
                )
                .await
            },
            self,
        )
        .await
    }

    async fn check_nominal_instance_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: NominalInstanceType<'db>,
        target: NominalInstanceType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_nominal_pair_with(
            source,
            target,
            NominalPairFacts,
            &BorrowedNominalPairs {
                pairs: self,
                checker,
            },
        )
        .await
    }

    async fn check_class_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: ClassType<'db>,
        target: ClassType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_class_pair_with(
            source,
            target,
            checker.relation,
            &BorrowedClassPairs { pairs: self },
        )
        .await
    }

    async fn class_default_specialization(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.effects.class_default_specialization(class).await
    }

    async fn subclass_inner_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.effects.subclass_inner_class(inner).await
    }

    async fn intersection_positive_contains(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.effects
            .intersection_positive_contains(intersection, ty)
            .await
    }

    async fn intersection_negative_contains(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.effects
            .intersection_negative_contains(intersection, ty)
            .await
    }

    async fn intersection_contains_nondivergent_dynamic(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<bool> {
        self.intersection_positive_any(intersection, Type::is_non_divergent_dynamic)
            .await
    }

    async fn intersection_contains_dynamic(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<bool> {
        self.intersection_positive_any(intersection, Type::is_dynamic)
            .await
    }

    async fn union_has_aliases(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
    ) -> RunResult<bool> {
        BorrowedTargetUnion::new(self, checker)
            .has_aliases(union)
            .await
    }

    async fn union_contains_type(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        union: UnionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        BorrowedTargetUnion::new(self, checker)
            .contains(union, ty)
            .await
    }

    async fn check_target_union(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: UnionType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_target_union_with(source, target, &BorrowedTargetUnion::new(self, checker)).await
    }

    async fn check_target_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
        target: IntersectionType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_target_intersection_with(
            source,
            target,
            checker.relation,
            &BorrowedTargetIntersection::new(self, checker),
        )
        .await
    }

    async fn check_source_intersection(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: IntersectionType<'db>,
        target: Type<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        check_source_intersection_with(
            source,
            target,
            &BorrowedSourceIntersection::new(self, checker),
        )
        .await
    }

    async fn nominal_has_known_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> RunResult<bool> {
        let known = self.effects.nominal_known_class(instance).await?;
        Ok(self
            .endpoint
            .local_call(|| {
                self.endpoint.admit_work(1)?;
                self.endpoint.check_completion()?;
                Ok(known == Some(class))
            })
            .await)
    }

    async fn known_class_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.effects.known_class_instance(checker.env, class).await
    }

    async fn property_instance_fallback(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        property: PropertyInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.effects
            .property_instance_fallback(checker.env, property)
            .await
    }

    async fn callable_runtime_class(
        &self,
        _checker: &TypeRelationChecker<'a, 'c, 'db>,
        callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        BorrowedPairs::callable_runtime_class(self, callable).await
    }

    async fn class_literal_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        self.effects
            .class_literal_metaclass_instance(checker.env, class)
            .await
    }

    async fn class_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.effects
            .class_metaclass_instance(checker.env, class)
            .await
    }

    async fn subclass_metaclass_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        self.effects
            .subclass_metaclass_instance(checker.env, subclass)
            .await
    }

    async fn literal_fallback_instance(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        literal_fallback_instance_with(source, checker.env, LiteralFallbackFacts, self).await
    }

    async fn check_callable_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .child_call(|| async {
                checker
                    .check_callable_pair_with(
                        self.db,
                        &signature::SourceSignatureEffects::new(self),
                        source,
                        target,
                    )
                    .await
            })
            .await)
    }

    async fn check_callable_signature_pair(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: &CallableSignature<'db>,
        target: &CallableSignature<'db>,
    ) -> RunResult<ConstraintSet<'db, 'c>> {
        Ok(self
            .endpoint
            .child_call(|| async {
                checker
                    .check_callable_signature_pair_with(
                        self.db,
                        &signature::SourceSignatureEffects::new(self),
                        source,
                        target,
                    )
                    .await
            })
            .await)
    }

    async fn check_typevar_subclass_relation_to_target(
        &self,
        checker: &TypeRelationChecker<'a, 'c, 'db>,
        source: SubclassOfType<'db>,
        target: Type<'db>,
    ) -> RunResult<Option<ConstraintSet<'db, 'c>>> {
        Ok(self
            .endpoint
            .child_call(|| async {
                check_typevar_subclass_with(
                    source,
                    target,
                    TypeVarSubclassFacts,
                    &BorrowedTypeVarSubclass { pairs: self, checker },
                )
                .await
            })
            .await)
    }

    unavailable_pair_effects! {
        fn check_newtype_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: NewType<'db>,
            target: NewType<'db>,
        ) -> ConstraintSet<'db, 'c> => NewType;
        fn check_source_union(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: UnionType<'db>,
            target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => SourceUnion;
        fn check_source_typevar_bounds(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: TypeVarBoundOrConstraints<'db>,
            target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => TypeVarBounds;
        fn check_function_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: FunctionType<'db>,
            target: FunctionType<'db>,
        ) -> ConstraintSet<'db, 'c> => Function;
        fn check_bound_method_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: BoundMethodType<'db>,
            target: BoundMethodType<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundMethod;
        fn check_known_bound_method_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: KnownBoundMethodType<'db>,
            target: KnownBoundMethodType<'db>,
        ) -> ConstraintSet<'db, 'c> => KnownBoundMethod;
        fn check_type_satisfies_protocol(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: Type<'db>,
            target: ProtocolInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Protocol;
        fn check_meta_type_satisfies_protocol(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: Type<'db>,
            target: ProtocolInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => MetaProtocol;
        fn check_typeddict_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: TypedDictType<'db>,
            target: TypedDictType<'db>,
        ) -> ConstraintSet<'db, 'c> => TypedDict;
        fn check_typeddict_fallback(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: TypedDictType<'db>,
            target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => TypedDictFallback;
        fn check_subclassof_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: SubclassOfType<'db>,
            target: SubclassOfType<'db>,
        ) -> ConstraintSet<'db, 'c> => Subclass;
        fn check_property_instance_pair(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: PropertyInstanceType<'db>,
            target: PropertyInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => Property;
        fn when_recursive_types_relate_by_arguments(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: RecursiveType<'db>,
            target: RecursiveType<'db>,
        ) -> ConstraintSet<'db, 'c> => RecursiveArguments;
        fn check_bound_super_pair(
            checker: &EquivalenceChecker<'a, 'c, 'db>,
            source: BoundSuperType<'db>,
            target: BoundSuperType<'db>,
        ) -> ConstraintSet<'db, 'c> => BoundSuper;
        fn recursive_type_pair_fallback(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: Type<'db>,
            target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => RecursiveFallback;
        fn implied_typevar_relation(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: Type<'db>,
            target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => ImpliedTypevarRelation;
        fn lazy_typevar_upper_constraint(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
            target: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => LazyTypevarUpperConstraint;
        fn lazy_typevar_lower_constraint(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
            source: Type<'db>,
        ) -> ConstraintSet<'db, 'c> => LazyTypevarLowerConstraint;
        fn protocol_is_equivalent_to_object(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            protocol: ProtocolInstanceType<'db>,
        ) -> bool => ProtocolObjectEquivalence;
        fn same_typevar_occurrence(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: BoundTypeVarInstance<'db>,
            target: BoundTypeVarInstance<'db>,
        ) -> bool => SameTypevarOccurrence;
        fn unfold_recursive(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            recursive: RecursiveType<'db>,
        ) -> Option<Type<'db>> => UnfoldRecursive;
        fn alias_value(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            alias: TypeAliasType<'db>,
        ) -> Type<'db> => AliasValue;
        fn expand_union_aliases(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            union: UnionType<'db>,
        ) -> Type<'db> => ExpandUnionAliases;
        fn subclass_instance(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            subclass: SubclassOfType<'db>,
        ) -> Type<'db> => SubclassInstance;
        fn class_instance(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            class: ClassType<'db>,
        ) -> Type<'db> => ClassInstance;
        fn known_instance_type_form_argument(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            instance: KnownInstanceType<'db>,
        ) -> Option<Type<'db>> => KnownInstanceTypeFormArgument;
        fn special_form_type_form_argument(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            form: SpecialFormType,
        ) -> Option<Type<'db>> => SpecialFormTypeFormArgument;
        fn enum_remaining_literals(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            complement: EnumComplementType<'db>,
        ) -> Type<'db> => EnumRemainingLiterals;
        fn enum_intersection(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            complement: EnumComplementType<'db>,
        ) -> Type<'db> => EnumIntersection;
        fn same_sentinel(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: SentinelInstance<'db>,
            target: SentinelInstance<'db>,
        ) -> bool => SameSentinel;
        fn wrapper_matches_nominal(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            wrapper: MethodWrapper<'db>,
            instance: NominalInstanceType<'db>,
        ) -> bool => WrapperMatchesNominal;
        fn lookup_wrapped_function(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            target: Type<'db>,
        ) -> Option<Type<'db>> => LookupWrappedFunction;
        fn nominal_class_is_known(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            instance: NominalInstanceType<'db>,
            class: KnownClass,
        ) -> bool => NominalClassIsKnown;
        fn specialize_partial_instance(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            callable: CallableType<'db>,
        ) -> Type<'db> => SpecializePartialInstance;
        fn union_contains_dynamic(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            union: UnionType<'db>,
        ) -> bool => UnionContainsDynamic;
        fn instance_approximation(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            ty: Type<'db>,
        ) -> Option<Type<'db>> => InstanceApproximation;
        fn typevar_is_inferable(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> bool => TypevarIsInferable;
        fn typevar_is_typevartuple(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> bool => TypevarIsTypevartuple;
        fn is_exact_tuple_instance(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            ty: Type<'db>,
        ) -> bool => IsExactTupleInstance;
        fn is_variadic_exact_tuple_instance(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            ty: Type<'db>,
        ) -> bool => IsVariadicExactTupleInstance;
        fn unpacked_typevartuple(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> Type<'db> => UnpackedTypevartuple;
        fn typevar_domain(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> TypeVarDomain => TypevarDomain;
        fn callable_is_gradual_paramspec_value(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            callable: CallableType<'db>,
        ) -> bool => CallableIsGradualParamspecValue;
        fn callable_is_top_paramspec_value(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            callable: CallableType<'db>,
        ) -> bool => CallableIsTopParamspecValue;
        fn callable_is_bottom_paramspec_value(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            callable: CallableType<'db>,
        ) -> bool => CallableIsBottomParamspecValue;
        fn typevar_constraints(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> Option<&'db [Type<'db>]> => TypevarConstraints;
        fn typevar_upper_bound(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> Option<Type<'db>> => TypevarUpperBound;
        fn typevar_bound_or_constraints(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            typevar: BoundTypeVarInstance<'db>,
        ) -> Option<TypeVarBoundOrConstraints<'db>> => TypevarBoundOrConstraints;
        fn newtype_concrete_base(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            newtype: NewType<'db>,
        ) -> Type<'db> => NewtypeConcreteBase;
        fn callable_signatures(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            callable: CallableType<'db>,
        ) -> &'db CallableSignature<'db> => CallableSignatures;
        fn function_callable_signatures(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            function: FunctionType<'db>,
        ) -> &'db CallableSignature<'db> => FunctionCallableSignatures;
        fn check_string_literal_nominal(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            value: StringLiteralType<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => StringLiteralNominal;
        fn check_bytes_literal_nominal(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            value: BytesLiteralType<'db>,
            instance: NominalInstanceType<'db>,
        ) -> ConstraintSet<'db, 'c> => BytesLiteralNominal;
        fn check_enum_instance_literal(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            source: Type<'db>,
            literal: EnumLiteralType<'db>,
        ) -> ConstraintSet<'db, 'c> => EnumInstanceLiteral;
        fn special_form_instance_fallback(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            form: SpecialFormType,
        ) -> Type<'db> => SpecialFormInstanceFallback;
        fn known_instance_fallback(
            checker: &TypeRelationChecker<'a, 'c, 'db>,
            instance: KnownInstanceType<'db>,
        ) -> Type<'db> => KnownInstanceFallback;
    }
}
