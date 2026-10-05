use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{SourceAccess, SourceEffects, SourceOperation};
#[cfg(test)]
use crate::Db;
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::normalization::source::{
    NormalizationSourceEffects, RetainedNormalizationSource, recursive_normalize_with_retained,
};
use crate::types::normalization::{
    NormalizationEffects, NormalizationFacts, NormalizationSearch, RecursiveNormalizationOperation,
    RecursiveNormalizationRequest, cycle_normalized_with, recursive_type_normalized_with_cycle,
};
use crate::types::relation::source::resources::EnvironmentResourceAccess;
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::set_theoretic::widening::{TupleWideningEffects, recovery_union_with};
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::visitor::runtime::{RuntimeTypeSearch, RuntimeTypeWalk};
use crate::types::visitor::{TypeSearchMode, TypeWalkFacts, search_type_with};
use crate::types::{
    DynamicType, GenericAlias, RecursivelyDefined, Type, TypeFormType, UnionBuilder, UnionType,
};
use crate::{Program, ProgramEnvironment};

#[derive(Clone)]
struct OwnedNormalizationSource<'db, A> {
    access: A,
    program: Program<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RetainedNormalizationSource<'run, 'db>
    for OwnedNormalizationSource<'db, A>
{
    type Effects<'call>
        = SourceEffects<'call, 'run, 'db, A>
    where
        Self: 'call;

    fn effects(&self) -> Self::Effects<'_> {
        SourceEffects::new(&self.access, self.program)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn cycle_normalize(
        &self,
        env: &ProgramEnvironment<'db>,
        current: Type<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| {
            cycle_normalized_with(current, env, previous, cycle, NormalizationFacts, self)
        })
        .await?
        .await
    }

    pub(in crate::types::infer) async fn normalize_cycle_heads(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| {
            recursive_type_normalized_with_cycle(ty, env, cycle, NormalizationFacts, self)
        })
        .await?
        .await
    }

    pub(in crate::types::infer) async fn recursive_normalize(
        &self,
        env: &ProgramEnvironment<'db>,
        request: RecursiveNormalizationRequest<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| async {
            let source = self
                .local(
                    size_of::<OwnedNormalizationSource<'db, A>>() * 2 + 1,
                    0,
                    || OwnedNormalizationSource {
                        access: A::clone(self.access),
                        program: self.program,
                    },
                )
                .await?;
            let env = EnvironmentResourceAccess::retain_environment(
                self.access.resources(),
                self.access.endpoint(),
                env,
            )
            .await?;
            recursive_normalize_with_retained(request, env, source).await
        })
        .await?
        .await
    }
}

struct NormalizationPredicate(NormalizationSearch);

impl<'db> RuntimeTypeSearch<'db> for NormalizationPredicate {
    fn predicate(&self, ty: Type<'db>) -> bool {
        match self.0 {
            NormalizationSearch::AmbiguousOverload => {
                matches!(ty, Type::Dynamic(DynamicType::AmbiguousOverload))
            }
            NormalizationSearch::Divergent => ty.is_divergent(),
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NormalizationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn contains(
        &self,
        ty: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        search: NormalizationSearch,
    ) -> RunResult<bool> {
        self.allocate_future(|| async {
            let mut effects = RuntimeTypeWalk {
                db: self.db(),
                endpoint: self.access.endpoint(),
                query: NormalizationPredicate(search),
                unavailable: self,
            };
            search_type_with(
                ty,
                TypeSearchMode::SkipLazyAttributes,
                TypeWalkFacts,
                &mut effects,
            )
            .await
        })
        .await?
        .await
    }

    async fn merge_aliases(
        &self,
        _current: GenericAlias<'db>,
        _previous: GenericAlias<'db>,
    ) -> RunResult<Option<GenericAlias<'db>>> {
        self.unavailable(SourceOperation::GenericAliasCycleMerge)
            .await
    }

    async fn recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| recovery_union_with(previous, current, env, self))
            .await?
            .await
    }

    async fn widen_tuples(
        &self,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.widen_growing_tuples(env, previous, current).await
    }

    async fn normalize_heads(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> RunResult<Type<'db>> {
        self.normalize_cycle_heads(env, ty, cycle).await
    }

    async fn next_head(
        &self,
        heads: &mut salsa::CycleHeadCandidates<'_>,
    ) -> RunResult<Option<salsa::CycleHeadCandidate>> {
        self.local(1, 0, || heads.next()).await
    }

    async fn recursive_normalize(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        divergent: Type<'db>,
        nested: bool,
    ) -> RunResult<Option<Type<'db>>> {
        SourceEffects::recursive_normalize(
            self,
            env,
            RecursiveNormalizationRequest {
                ty,
                divergent,
                nested,
            },
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NormalizationSourceEffects<'run, 'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    #[cfg(test)]
    fn db(&self) -> &'db dyn Db {
        SourceEffects::db(self)
    }

    fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        self.access.endpoint()
    }

    async fn unavailable<T>(&self, operation: RecursiveNormalizationOperation) -> RunResult<T> {
        self.unavailable(SourceOperation::RecursiveNormalization(operation))
            .await
    }

    async fn retain_environment(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<&'run ProgramEnvironment<'db>> {
        EnvironmentResourceAccess::retain_environment(
            self.access.resources(),
            self.access.endpoint(),
            env,
        )
        .await
    }

    async fn program(&self, env: &ProgramEnvironment<'db>) -> RunResult<Program<'db>> {
        self.environment_program(env).await
    }

    async fn tuple_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        TupleSpecEffects::exact_spec(self, tuple).await
    }

    async fn intern_tuple(
        &self,
        program: Program<'db>,
        spec: TupleSpec<'db>,
    ) -> RunResult<TupleType<'db>> {
        self.access.intern_tuple(program, spec).await
    }

    async fn type_form_argument(&self, form: TypeFormType<'db>) -> RunResult<Type<'db>> {
        self.field(form.field_requests(self.db()).type_argument())
            .await
    }

    async fn intern_type_form(&self, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.access.intern_typeform(argument).await
    }

    async fn new_recovery_union(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        TupleWideningEffects::new_recovery_union(self, env).await
    }

    async fn union_recursion(&self, union: UnionType<'db>) -> RunResult<RecursivelyDefined> {
        self.union_recursion_source(union).await
    }

    async fn merge_recursion(
        &self,
        builder: &mut UnionBuilder<'db>,
        recursion: RecursivelyDefined,
    ) -> RunResult<()> {
        TupleWideningEffects::merge_recursion(self, builder, recursion).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.union_elements_source(union).await
    }

    async fn union_add(&self, builder: &mut UnionBuilder<'db>, ty: Type<'db>) -> RunResult<()> {
        PairUnionEffects::union_add(self, builder, ty).await
    }

    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::union_build(self, builder).await
    }
}
