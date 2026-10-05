//! Contextual tuple preparation borrows canonical inputs and existing relation resources.

use std::borrow::Cow;

use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::{FixedFieldBorrow, FixedFieldCopy};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class::interpret_class_literal_lookup;
use crate::types::class_selection::class_specialization_with;
use crate::types::infer::builder::source_expression::SourceExpressionOperation;
use crate::types::infer::type_context::{
    TypeContextEffects, TypeContextFacts, discard_disjoint_with, filter_tuple_annotation_with,
    known_specialization_with, narrow_targets_with, specialization_of_with, union_for_filter_with,
};
use crate::types::relation::source::{RelationSourceEffects, resources::RelationResourceAccess};
use crate::types::tuple::TupleSpec;
use crate::types::type_expression_conversion::SubclassArgumentEffects;
use crate::types::{
    DiscardDisjointUnionElementsResult, GenericContext, KnownClass, NewType, PropertyDeprecations,
    Specialization, StaticClassLiteral, Type, TypeDispatchEffects, TypeVarSet, UnionType,
    union_like_with,
};

/// Borrows the source capabilities and environment for one annotation preparation.
struct ContextEffects<'a, 'access, 'run, 'db: 'run, A> {
    source: &'a SourceEffects<'access, 'run, 'db, A>,
    env: &'a ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn contextual_tuple_targets(
        &self,
        env: &ProgramEnvironment<'db>,
        annotation: Type<'db>,
    ) -> RunResult<Option<Cow<'db, [Type<'db>]>>> {
        narrow_targets_with(Some(annotation), &ContextEffects { source: self, env }).await
    }

    pub(in crate::types::infer::builder) async fn contextual_tuple_filter_annotation(
        &self,
        env: &ProgramEnvironment<'db>,
        annotation: Type<'db>,
    ) -> RunResult<Type<'db>> {
        filter_tuple_annotation_with(
            annotation,
            TypeContextFacts,
            &ContextEffects { source: self, env },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn contextual_tuple_specialization(
        &self,
        env: &ProgramEnvironment<'db>,
        annotation: Type<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        known_specialization_with(
            annotation,
            KnownClass::Tuple,
            &ContextEffects { source: self, env },
        )
        .await
    }

    /// Borrows the exact tuple payload carried by a selected tuple specialization.
    pub(in crate::types::infer::builder) async fn contextual_tuple_spec(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db TupleSpec<'db>> {
        let tuple = self
            .field_with_profile(
                specialization.tuple_request(self.access.endpoint().field_request_context()),
                &FixedFieldCopy,
            )
            .await?;
        let tuple = self
            .local_with_fixed_transfers(1, size_of::<crate::types::tuple::TupleType<'db>>(), || {
                tuple.ok_or(RunError::Contract(
                    "tuple specialization has no tuple payload",
                ))
            })
            .await??;
        self.field_with_profile(
            tuple
                .field_requests(self.access.endpoint().field_request_context())
                .tuple(),
            &FixedFieldBorrow,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeContextEffects<'db>
    for ContextEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;
    type Constraints = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;

    async fn checkpoint(&self) -> RunResult<()> {
        // Bound the shared selectors' fixed decisions and inline result carriers. Child
        // operations separately admit their field results and any owned payloads.
        self.source
            .local_with_fixed_transfers(
                16,
                size_of::<Option<Cow<'db, [Type<'db>]>>>()
                    + size_of::<DiscardDisjointUnionElementsResult<'db>>()
                    + size_of::<Option<Specialization<'db>>>()
                    + size_of::<Option<UnionType<'db>>>()
                    + size_of::<Option<Type<'db>>>()
                    + size_of::<TypeVarSet<'db>>()
                    + size_of::<Type<'db>>()
                    + 4 * size_of::<bool>(),
                || (),
            )
            .await
    }

    async fn union_like(&self, ty: Type<'db>) -> RunResult<Option<UnionType<'db>>> {
        union_like_with(ty, self).await
    }

    async fn has_aliases(&self, union: UnionType<'db>) -> RunResult<bool> {
        let elements = self.source.union_elements_source(union).await?;
        let mut cursor = self
            .source
            .local_with_fixed_transfers(1, size_of::<std::slice::Iter<'_, Type<'db>>>(), || {
                elements.iter()
            })
            .await?;
        while let Some(alias) = self
            .source
            .local_with_fixed_transfers(2, size_of::<Option<bool>>(), || {
                cursor.next().map(|ty| ty.is_alias_like())
            })
            .await?
        {
            if alias {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn expand_aliases(&self, union: UnionType<'db>) -> RunResult<Type<'db>> {
        SubclassArgumentEffects::expand_union_aliases(self.source, self.env, union).await
    }

    async fn union_targets(&self, union: UnionType<'db>) -> RunResult<Cow<'db, [Type<'db>]>> {
        let elements = self.source.union_elements_source(union).await?;
        self.source
            .local_with_fixed_transfers(1, size_of::<Cow<'db, [Type<'db>]>>(), || {
                Cow::Borrowed(elements)
            })
            .await
    }

    async fn singleton_target(&self, ty: Type<'db>) -> RunResult<Cow<'db, [Type<'db>]>> {
        // The single owned target prepays its element and vector disposal before allocation.
        let bytes = SourceEffects::<A>::checked(
            size_of::<Type<'db>>().checked_add(size_of::<Cow<'db, [Type<'db>]>>()),
        )?;
        self.source
            .local_with_fixed_transfers(5, bytes, || Cow::Owned(vec![ty]))
            .await
    }

    async fn known_class(&self, class: KnownClass) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let program = self.source.environment_program(self.env).await?;
        let result = self
            .source
            .access
            .known_class_lookup(program, class)
            .await?;
        self.source
            .local_with_fixed_transfers(2, size_of::<Option<StaticClassLiteral<'db>>>(), || {
                interpret_class_literal_lookup(result)
            })
            .await
    }

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.source.access.class_generic_context(class).await
    }

    async fn inferable_typevars(&self, context: GenericContext<'db>) -> RunResult<TypeVarSet<'db>> {
        self.source.inferable_typevars(context).await
    }

    async fn homogeneous_unknown_tuple(&self) -> RunResult<Type<'db>> {
        let program = self.source.environment_program(self.env).await?;
        // The homogeneous specification owns no element buffer; pay for its carrier and cleanup.
        let spec = self
            .source
            .local_with_fixed_transfers(6, size_of::<TupleSpec<'db>>(), || {
                TupleSpec::homogeneous(Type::unknown())
            })
            .await?;
        let tuple = self.source.access.intern_tuple(program, spec).await?;
        self.source
            .local_with_fixed_transfers(1, size_of::<Type<'db>>(), || Type::tuple(tuple))
            .await
    }

    async fn new_constraints(&self) -> RunResult<Self::Constraints> {
        self.source
            .resources()
            .invocation_builder(self.source.endpoint())
            .await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.source.resolve_context_alias(ty).await
    }

    async fn union_for_filter(&self, ty: Type<'db>) -> RunResult<Option<UnionType<'db>>> {
        union_for_filter_with(ty, self).await
    }

    async fn filter_disjoint(
        &self,
        _union: UnionType<'db>,
        _target: Type<'db>,
        _constraints: &Self::Constraints,
        _inferable: TypeVarSet<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::ContextualClassSpecialization,
            ))
            .await
    }

    async fn discard_disjoint(
        &self,
        annotation: Type<'db>,
        target: Type<'db>,
        inferable: TypeVarSet<'db>,
    ) -> RunResult<DiscardDisjointUnionElementsResult<'db>> {
        discard_disjoint_with(annotation, target, inferable, TypeContextFacts, self).await
    }

    async fn class_specialization(
        &self,
        annotation: Type<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Specialization<'db>)>> {
        class_specialization_with(annotation, self.source).await
    }

    async fn specialization_of<'expected>(
        &self,
        annotation: Type<'db>,
        expected: StaticClassLiteral<'expected>,
    ) -> RunResult<Option<Specialization<'db>>> {
        specialization_of_with(annotation, expected, TypeContextFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeDispatchEffects<'db>
    for ContextEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.source.resolve_context_alias(ty).await
    }

    async fn newtype_union(&self, _newtype: NewType<'db>) -> RunResult<Option<UnionType<'db>>> {
        self.source
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::Narrowing,
            ))
            .await
    }

    async fn collect_properties(
        &self,
        _ty: Type<'db>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        self.source
            .unavailable(SourceOperation::Expression(
                SourceExpressionOperation::Narrowing,
            ))
            .await
    }
}
