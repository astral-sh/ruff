//! Generic intersection reductions use shared decisions and canonical pair operations.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class_selection;
use crate::types::set_theoretic::generic_gradual_intersections::{
    GenericIntersection, GenericIntersectionEffects, GenericIntersectionFacts,
    base_top_intersection_with, dynamic_generalization_intersection_with,
    generic_gradual_intersection_with,
};
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, ClassBase, ClassType, GenericAlias, GenericContext,
    KnownClass, MaterializationKind, Specialization, StaticClassLiteral, Type, TypeVarVariance,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn generic_intersection_source(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Option<GenericIntersection<'db>>> {
        self.environment_program(env).await?;
        generic_gradual_intersection_with(first, second, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GenericIntersectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Variables = ();
    type Types = ();
    type Mro = ();

    async fn dynamic_generalization(
        &self,
        general: Type<'db>,
        specific: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        dynamic_generalization_intersection_with(general, specific, GenericIntersectionFacts, self)
            .await
    }

    async fn base_top(
        &self,
        base: Type<'db>,
        subclass: Type<'db>,
    ) -> RunResult<Option<GenericIntersection<'db>>> {
        base_top_intersection_with(base, subclass, GenericIntersectionFacts, self).await
    }

    async fn has_dynamic(&self, ty: Type<'db>) -> RunResult<bool> {
        self.has_dynamic_source(&ProgramEnvironment::from_program(self.program), ty)
            .await
    }

    async fn class_specialization(
        &self,
        ty: Type<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Specialization<'db>)>> {
        class_selection::class_specialization_with(ty, self).await
    }

    async fn materialization(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.field(
            specialization
                .field_requests(self.db())
                .materialization_kind(),
        )
        .await
    }

    async fn known_class(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.field(class.field_requests(self.db()).known()).await
    }

    async fn tuple(
        &self,
        _specialization: Specialization<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn has_fixed_elements(&self, _tuple: &'db TupleSpec<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn fixed_tuple(
        &self,
        _tuple: &'db TupleSpec<'db>,
    ) -> RunResult<Option<&'db FixedLengthTuple<Type<'db>>>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn same_tuple_length(
        &self,
        _first: &'db FixedLengthTuple<Type<'db>>,
        _second: &'db FixedLengthTuple<Type<'db>>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn tuple_elements(
        &self,
        _tuple: &'db FixedLengthTuple<Type<'db>>,
    ) -> RunResult<Self::Types> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn homogeneous_tuple(&self, _ty: Type<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn heterogeneous_tuple_intersections(
        &self,
        _specific: &'db FixedLengthTuple<Type<'db>>,
        _general: &'db FixedLengthTuple<Type<'db>>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.field(specialization.field_requests(self.db()).generic_context())
            .await
    }

    async fn variables(&self, _context: GenericContext<'db>) -> RunResult<Self::Variables> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn next_variable(
        &self,
        _variables: &mut Self::Variables,
    ) -> RunResult<Option<(usize, BoundTypeVarInstance<'db>)>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn specialization_types(
        &self,
        _specialization: Specialization<'db>,
    ) -> RunResult<Self::Types> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn next_type(&self, _types: &mut Self::Types) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn types_equal(&self, first: Type<'db>, second: Type<'db>) -> RunResult<bool> {
        let work = Self::checked(
            first
                .inline_payload_bytes()
                .checked_add(second.inline_payload_bytes())
                .and_then(|work| work.checked_add(1)),
        )?;
        self.local(work, 0, || first == second).await
    }

    async fn variance(&self, _variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarVariance> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn intersection(&self, first: Type<'db>, second: Type<'db>) -> RunResult<Type<'db>> {
        self.access
            .intersection_from_two_elements(first, second)
            .await
    }

    async fn union(&self, first: Type<'db>, second: Type<'db>) -> RunResult<Type<'db>> {
        self.access.union_from_two_elements(first, second).await
    }

    async fn new_types(&self) -> RunResult<Vec<Type<'db>>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn reserve_types(
        &self,
        _types: &mut Vec<Type<'db>>,
        _variables: &Self::Variables,
        _first: &Self::Types,
        _second: &Self::Types,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn copy_specialization_types(
        &self,
        _specialization: Specialization<'db>,
        _types: &mut Vec<Type<'db>>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn append_type(&self, _types: &mut Vec<Type<'db>>, _ty: Type<'db>) -> RunResult<()> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn get_type(&self, _types: &[Type<'db>], _index: usize) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn replace_type(
        &self,
        _types: &mut [Type<'db>],
        _index: usize,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn specialize(
        &self,
        _context: GenericContext<'db>,
        _types: &mut Vec<Type<'db>>,
    ) -> RunResult<Specialization<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn apply_specialization(
        &self,
        _class: StaticClassLiteral<'db>,
        _specialization: Specialization<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn instance(&self, _class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn identity_specialization(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn mro(&self, _class: ClassType<'db>) -> RunResult<Self::Mro> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn next_ancestor(&self, _mro: &mut Self::Mro) -> RunResult<Option<ClassBase<'db>>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        self.field(alias.field_requests(self.db()).origin()).await
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.field(alias.field_requests(self.db()).specialization())
            .await
    }

    async fn contains_growing_type(&self, _ty: Type<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn fully_static(&self, _ty: Type<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn variable_identity(
        &self,
        _variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }

    async fn top_materialization(&self, _ty: Type<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::GenericIntersection).await
    }
}
