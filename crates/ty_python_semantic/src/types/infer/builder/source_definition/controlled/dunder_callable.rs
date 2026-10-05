//! Admitted callable-kind changes and ordered dunder-member set traversal.

use std::future::Future;
use std::pin::Pin;
use std::slice;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::callable::CallableTypeKind;
use crate::types::class::dunder_callable::{
    DunderCallableEffects, DunderCallableFacts, DunderCallableMapping, DunderCallableTransform,
    dunder_callable_with,
};
use crate::types::infer::builder::function::application::DecoratorApplicationEffects;
use crate::types::set_theoretic::builder::controlled_union::UnionEffects;
use crate::types::set_theoretic::builder::intersection_insertion::{Elements, InsertionEffects};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::{
    BoundTypeVarInstance, CallableType, IntersectionBuilder, IntersectionType, RecursivelyDefined,
    Type, UnionBuilder, UnionType,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Boxes a dunder traversal after admitting its future storage and fixed transfers.
    /// Four output representations cover the child return, await, effect return, and shared
    /// caller. Builder and signature payloads acquired while polling have separate admission.
    pub(super) async fn dunder_callable_future<F: Future>(
        &self,
        make: impl FnOnce() -> F,
    ) -> RunResult<Pin<Box<F>>> {
        let quote = size_of::<F::Output>()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(size_of::<F>()))
            .map(|bytes| (6, bytes))
            .ok_or(RunError::Contract(
                "dunder continuation byte quotation overflow",
            ));
        self.local_quoted_with_fixed_transfers(quote, || Box::pin(make()))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DunderCallableEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Union = UnionBuilder<'db>;
    type Intersection = IntersectionBuilder<'db>;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local_with_fixed_transfers(
            8,
            2 * size_of::<DunderCallableMapping<'db>>() + 2 * size_of::<bool>(),
            || (),
        )
        .await
    }

    async fn callable_kind(&self, callable: CallableType<'db>) -> RunResult<CallableTypeKind> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || callable.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.kind())
            .await?;
        self.field(request).await
    }

    async fn signatures(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || callable.field_requests(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || fields.signatures())
            .await?;
        let signatures = self.field(request).await?;
        // The shared FunctionLike arm then creates its borrowed overload cursor.
        self.local_with_fixed_transfers(
            6,
            size_of::<&[Signature<'db>]>() + size_of::<slice::Iter<'db, Signature<'db>>>(),
            || signatures,
        )
        .await
    }

    async fn single_paramspec(&self, signatures: &CallableSignature<'db>) -> RunResult<bool> {
        let bytes = size_of::<&[Signature<'db>]>()
            + size_of::<&Parameters<'db>>()
            + size_of::<&Signature<'db>>()
            + size_of::<(BoundTypeVarInstance<'db>, &Signature<'db>)>()
            + 2 * size_of::<Option<BoundTypeVarInstance<'db>>>()
            + 2 * size_of::<Option<(BoundTypeVarInstance<'db>, &Signature<'db>)>>();
        self.local_with_fixed_transfers(14, bytes, || signatures.is_single_paramspec().is_some())
            .await
    }

    async fn next_signature(
        &self,
        cursor: &mut slice::Iter<'db, Signature<'db>>,
    ) -> RunResult<Option<&'db Signature<'db>>> {
        // A returned signature is inspected by the shared has_parameters decision.
        let bytes = size_of::<&Parameters<'db>>()
            + size_of::<&[Parameter<'db>]>()
            + size_of::<usize>()
            + 2 * size_of::<bool>();
        self.local_with_fixed_transfers(12, bytes, || cursor.next())
            .await
    }

    async fn with_kind(
        &self,
        callable: CallableType<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<Type<'db>> {
        DecoratorApplicationEffects::callable_with_kind(self, callable, kind).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        let elements = self.union_elements_source(union).await?;
        self.local_with_fixed_transfers(4, size_of::<slice::Iter<'db, Type<'db>>>(), || elements)
            .await
    }

    async fn next_union(
        &self,
        cursor: &mut slice::Iter<'_, Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        // The lazy traversal can compare the result and test its alias variant before rebuilding.
        self.local_with_fixed_transfers(10, 3 * size_of::<bool>(), || cursor.next().copied())
            .await
    }

    async fn new_union(&self) -> RunResult<Self::Union> {
        // The shared body next derives the untouched-prefix slice and its cursor.
        let prefix_bytes = size_of::<&[Type<'db>]>()
            + size_of::<slice::Iter<'db, Type<'db>>>()
            + 4 * size_of::<usize>();
        let env = self
            .local_with_fixed_transfers(12, prefix_bytes, || {
                ProgramEnvironment::from_program(self.program)
            })
            .await?;
        PairUnionEffects::new_union(self, &env).await
    }

    async fn union_add(&self, builder: &mut Self::Union, ty: Type<'db>) -> RunResult<()> {
        self.dunder_callable_future(|| PairUnionEffects::union_add(self, builder, ty))
            .await?
            .await
    }

    async fn union_recursion(&self, union: UnionType<'db>) -> RunResult<RecursivelyDefined> {
        self.union_recursion_source(union).await
    }

    async fn finish_union(
        &self,
        mut builder: Self::Union,
        recursion: RecursivelyDefined,
    ) -> RunResult<Type<'db>> {
        UnionEffects::merge_recursion(self, &mut builder, recursion).await?;
        self.dunder_callable_future(|| PairUnionEffects::union_build(self, builder))
            .await?
            .await
    }

    async fn new_intersection(&self) -> RunResult<Self::Intersection> {
        let env = self
            .local_with_fixed_transfers(2, 0, || ProgramEnvironment::from_program(self.program))
            .await?;
        SourceEffects::new_intersection(self, &env).await
    }

    async fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::positive_elements(self, intersection).await
    }

    async fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Elements<'db>> {
        InsertionEffects::negative_elements(self, intersection).await
    }

    async fn next_intersection(&self, cursor: &mut Elements<'db>) -> RunResult<Option<Type<'db>>> {
        InsertionEffects::next_element(self, cursor).await
    }

    async fn add_positive(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> RunResult<()> {
        self.dunder_callable_future(|| self.intersection_add_positive(builder, ty))
            .await?
            .await
    }

    async fn add_negative(&self, builder: &mut Self::Intersection, ty: Type<'db>) -> RunResult<()> {
        self.dunder_callable_future(|| self.intersection_add_negative(builder, ty))
            .await?
            .await
    }

    async fn finish_intersection(&self, mut builder: Self::Intersection) -> RunResult<Type<'db>> {
        let result = self
            .dunder_callable_future(|| self.intersection_build(&mut builder))
            .await?
            .await?;
        self.retire_intersection(builder).await?;
        Ok(result)
    }

    async fn transform(
        &self,
        ty: Type<'db>,
        transform: DunderCallableTransform,
    ) -> RunResult<Type<'db>> {
        self.dunder_callable_future(|| {
            dunder_callable_with(ty, transform, DunderCallableFacts, self)
        })
        .await?
        .await
    }
}
