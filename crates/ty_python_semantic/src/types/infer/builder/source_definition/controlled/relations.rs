//! Relation effects borrow the source invocation's existing execution endpoint.

use std::marker::PhantomData;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};
use ty_python_core::Truthiness;

use super::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::callable::CallableTypes;
use crate::types::class::class_default_specialization_with;
use crate::types::class_base::ClassBase;
use crate::types::instance::{self, NominalClassFacts};
use crate::types::member_lookup::class_dispatch::property_class_instance_with;
use crate::types::mro::base::class_mro_start_with;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::property_provenance::PropertyProvenanceEffects;
use crate::types::relation::redundancy::{
    self, RedundancyComparisonEffects, RedundancyProducerEffects,
};
use crate::types::relation::source::{
    RelationSourceEffects, RelationSourceOperation, RetainedRelationSource, redundancy_condition,
};
use crate::types::set_theoretic::builder::controlled_union::UnionEffects;
use crate::types::set_theoretic::builder::intersection_insertion::{Elements, InsertionEffects};
use crate::types::subclass_of::{
    SubclassConstructionEffects, SubclassInnerClassEffects, subclass_inner_into_class_with,
};
use crate::types::tuple::construction::tuple_type;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, ClassLiteral, ClassType, FunctionType, IntersectionType,
    KnownClass,
    MaterializationKind, NominalInstanceType, PropertyInstanceType, SubclassOfInner, SubclassOfType,
    Type, TypePair, TypeVarBoundOrConstraints, UnionType, UpcastPolicy,
};
use crate::{Program, ProgramEnvironment};

/// Owns source access so each queued relation task can borrow it for its own effects.
#[derive(Clone)]
pub(in crate::types::infer) struct OwnedRelationSource<'db, A> {
    access: A,
    program: Program<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RetainedRelationSource<'run, 'db>
    for OwnedRelationSource<'db, A>
{
    type Effects<'call>
        = SourceEffects<'call, 'run, 'db, A>
    where
        Self: 'call;

    fn effects(&self) -> Self::Effects<'_> {
        SourceEffects::new(&self.access, self.program)
    }
}

pub(super) async fn is_redundant_with<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    first: Type<'db>,
    second: Type<'db>,
) -> RunResult<bool> {
    redundancy::compare_with(
        first,
        second,
        &RedundancyComparison {
            access,
            lifetime: PhantomData,
        },
    )
    .await
}

struct RedundancyComparison<'a, 'run, A: ?Sized> {
    access: &'a A,
    lifetime: PhantomData<&'run ()>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RedundancyComparisonEffects<'db>
    for RedundancyComparison<'_, 'run, A>
{
    type Error = RunError;

    async fn same_type(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        let endpoint = self.access.endpoint();
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                let work = first
                    .inline_payload_bytes()
                    .checked_add(second.inline_payload_bytes())
                    .and_then(|work| work.checked_add(2))
                    .ok_or(RunError::Contract(
                        "redundancy comparison quotation overflow",
                    ))?;
                endpoint.admit_work(work)?;
                endpoint.check_completion()?;
                Ok(first == second)
            })
            .await)
    }

    async fn canonical(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        self.access.canonical_redundancy(first, second).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) fn retained_relation_source(
        &self,
    ) -> impl RetainedRelationSource<'run, 'db> + use<'run, 'db, A> {
        OwnedRelationSource {
            access: A::clone(self.access),
            program: self.program,
        }
    }

    pub(in crate::types::infer) async fn type_pair_redundancy(
        &self,
        pair: TypePair<'db>,
    ) -> RunResult<bool> {
        redundancy::produce_with(pair, self).await
    }

    async fn intersection_elements_contain(
        &self,
        mut elements: Elements<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        while let Some(element) = InsertionEffects::next_element(self, &mut elements).await? {
            if UnionEffects::same_type(self, ty, element).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RedundancyProducerEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn program(&self, pair: TypePair<'db>) -> RunResult<Program<'db>> {
        let fields = pair.field_requests(self.access.endpoint().field_request_context());
        let program = self.field(fields.program()).await?;
        self.check_program(program)?;
        Ok(program)
    }

    async fn environment(&self, program: Program<'db>) -> RunResult<ProgramEnvironment<'db>> {
        self.local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
            ProgramEnvironment::from_program(program)
        })
        .await
    }

    async fn first(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        let fields = pair.field_requests(self.access.endpoint().field_request_context());
        self.field(fields.first()).await
    }

    async fn second(&self, pair: TypePair<'db>) -> RunResult<Type<'db>> {
        let fields = pair.field_requests(self.access.endpoint().field_request_context());
        self.field(fields.second()).await
    }

    async fn relate(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<bool> {
        redundancy_condition(self.db(), env, first, second, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RelationSourceEffects<'run, 'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Resources = A::Resources;
    type Retained = OwnedRelationSource<'db, A>;

    fn resources(&self) -> Self::Resources {
        self.access.resources()
    }

    fn retained(&self) -> Self::Retained {
        OwnedRelationSource {
            access: A::clone(self.access),
            program: self.program,
        }
    }

    fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        self.access.endpoint()
    }

    async fn unavailable<T>(&self, operation: RelationSourceOperation) -> RunResult<T> {
        SourceEffects::unavailable(self, SourceOperation::Relation(operation)).await
    }

    async fn type_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Truthiness> {
        SourceEffects::type_truthiness(self, env, ty).await
    }

    async fn tuple_from_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        tuple_type(self.db(), env, spec, self).await
    }

    async fn tuple_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        let context = self
            .local_with_fixed_transfers(1, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(1, 0, || tuple.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(1, 0, || fields.tuple())
            .await?;
        self.field_with_profile(request, &FixedFieldBorrow).await
    }

    async fn tuple_pack_identity(
        &self,
        pack: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        let context = self
            .local_with_fixed_transfers(1, 0, || self.access.endpoint().field_request_context())
            .await?;
        let request = self
            .local_with_fixed_transfers(2, 0, || pack.identity_request(context))
            .await?;
        self.field_with_profile(request, &FixedFieldCopy).await
    }

    async fn nominal_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.environment_program(env).await?;
        instance::nominal_class_with(instance, NominalClassFacts, self).await
    }

    async fn nominal_is_definition_generic(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        instance::nominal_is_definition_generic_with(instance, NominalClassFacts, self).await
    }

    async fn nominal_known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        instance::nominal_known_class_with(instance, NominalClassFacts, self).await
    }

    async fn function_runtime_class(&self, function: FunctionType<'db>) -> RunResult<KnownClass> {
        function.runtime_class_with(self.db(), self).await
    }

    async fn callable_conversion(
        &self,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        policy: UpcastPolicy,
    ) -> RunResult<Option<CallableTypes<'db>>> {
        self.callables_with_policy(env, source, policy, None).await
    }

    async fn known_class_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        self.access.known_class_instance(program, class).await
    }

    async fn property_instance_fallback(
        &self,
        env: &ProgramEnvironment<'db>,
        property: PropertyInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        let class = PropertyProvenanceEffects::instance_class(self, property).await?;
        self.allocate_future(|| property_class_instance_with(class, self))
            .await?
            .await
    }

    async fn class_literal_metaclass_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        let class = self
            .class_object_local(3, 0, || ClassType::NonGeneric(class))
            .await?;
        self.class_object_child(|| self.class_metaclass_instance_value(env, class))
            .await
    }

    async fn class_metaclass_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.class_object_child(|| self.class_metaclass_instance_value(env, class))
            .await
    }

    async fn subclass_metaclass_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        subclass: SubclassOfType<'db>,
    ) -> RunResult<Type<'db>> {
        self.subclass_metaclass_instance_value(env, subclass).await
    }

    async fn cached_materialization(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        self.access.cached_materialization(program, ty, kind).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn intersection_positive_contains(
        &self,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let elements = InsertionEffects::positive_elements(self, intersection).await?;
        self.intersection_elements_contain(elements, ty).await
    }

    async fn intersection_negative_contains(
        &self,
        intersection: IntersectionType<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let elements = InsertionEffects::negative_elements(self, intersection).await?;
        self.intersection_elements_contain(elements, ty).await
    }

    async fn intersection_positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::positive_elements(self, intersection).await
    }

    async fn intersection_negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::negative_elements(self, intersection).await
    }

    async fn intersection_next_element(
        &self,
        elements: &mut Elements<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        InsertionEffects::next_element(self, elements).await
    }

    async fn intersection_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        SourceEffects::intersection_alternatives(self, env, intersection).await
    }

    async fn intersection_expand(
        &self,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Type<'db>> {
        SourceEffects::intersection_expand(self, env, intersection).await
    }

    async fn class_default_specialization(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        let ClassLiteral::Static(class) = class else {
            return RelationSourceEffects::unavailable(
                self,
                RelationSourceOperation::ClassDefaultSpecialization,
            )
            .await;
        };
        class_default_specialization_with(class, self).await
    }

    async fn subclass_inner_class(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        subclass_inner_into_class_with(inner, self).await
    }

    async fn class_mro_start(&self, class: ClassType<'db>) -> RunResult<MroCursor<'db>> {
        let start = class_mro_start_with(MroFieldReads::new(self.db()), class, None, self).await?;
        self.local(1, size_of::<MroCursor<'db>>() * 2, || {
            MroCursor::new(start.class, start.specialization)
        })
        .await
    }

    async fn class_mro_next(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn class_is_object(&self, class: ClassType<'db>) -> RunResult<bool> {
        SubclassConstructionEffects::is_object(self, class).await
    }

    async fn class_is_final(&self, class: ClassType<'db>) -> RunResult<bool> {
        SubclassConstructionEffects::is_final(self, class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassInnerClassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = salsa::execution_probe::RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn require_bound_or_constraints(
        &self,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarBoundOrConstraints<'db>> {
        RelationSourceEffects::unavailable(self, RelationSourceOperation::SubclassInnerClass).await
    }

    async fn bound_into_class(&self, _bound: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        RelationSourceEffects::unavailable(self, RelationSourceOperation::SubclassInnerClass).await
    }

    async fn object_class(&self) -> RunResult<ClassType<'db>> {
        RelationSourceEffects::unavailable(self, RelationSourceOperation::SubclassInnerClass).await
    }
}
