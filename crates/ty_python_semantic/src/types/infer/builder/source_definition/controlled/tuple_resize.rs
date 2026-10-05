//! Admitted fixed tuple resizing and retention of its element annotations.

use std::alloc::Layout;
use std::fmt;
use std::slice;
use std::vec;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
#[cfg(all(test, feature = "experimental-analysis"))]
use crate::types::infer::source_runtime::tests::contextual_tuple::{Stage, observe_boundary};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::tuple::fixed_resize::{
    FixedUnpackEffects, FixedUnpackFacts, expand_fixed, unpack_fixed_with,
};
use crate::types::tuple::{FixedLengthTuple, TupleSpec, VariableLengthTuple, VariableSegment};
use crate::types::{Type, UnionBuilder};

/// Borrows the caller's source endpoint and the environment used for union reduction.
struct TupleResizeEffects<'a, 'access, 'run, 'db: 'run, A> {
    source: &'a SourceEffects<'access, 'run, 'db, A>,
    env: &'a ProgramEnvironment<'db>,
}

impl<A> fmt::Debug for TupleResizeEffects<'_, '_, '_, '_, A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TupleResizeEffects").finish_non_exhaustive()
    }
}

/// Walks borrowed tuple elements in prefix, variable-element, and suffix order.
#[derive(Debug)]
struct TupleUnionElements<'a, 'db> {
    prefix: slice::Iter<'a, Type<'db>>,
    variable: Option<Type<'db>>,
    suffix: slice::Iter<'a, Type<'db>>,
}

impl<'db> TupleUnionElements<'_, 'db> {
    fn next(&mut self) -> Option<Type<'db>> {
        self.prefix
            .next()
            .copied()
            .or_else(|| self.variable.take())
            .or_else(|| self.suffix.next().copied())
    }
}

/// Quotes a new fixed payload's allocation, element copies, header transfers, and disposal.
fn fixed_payload_quote(length: usize, headers: usize) -> RunResult<(usize, usize)> {
    let layout = Layout::array::<Type<'_>>(length)
        .map_err(|_| RunError::Contract("fixed tuple allocation overflow"))?;
    let work = length
        .checked_mul(2)
        .and_then(|work| work.checked_add(6))
        .ok_or(RunError::Contract("fixed tuple work overflow"))?;
    let bytes = layout
        .size()
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(headers))
        .ok_or(RunError::Contract("fixed tuple bytes overflow"))?;
    Ok((work, bytes))
}

/// Quotes collection through `Vec`, including its minimum nonempty capacity and optional boxing.
fn collected_payload_quote(
    length: usize,
    headers: usize,
    result: CollectionResult,
) -> RunResult<(usize, usize)> {
    let capacity = if length == 0 { 0 } else { length.max(4) };
    let layout = Layout::array::<Type<'_>>(capacity)
        .map_err(|_| RunError::Contract("tuple collection allocation overflow"))?;
    let relocation = match result {
        CollectionResult::Boxed if capacity != length => length,
        CollectionResult::Boxed | CollectionResult::Vector => 0,
    };
    let relocated_payload = relocation
        .checked_mul(2)
        .ok_or(RunError::Contract("tuple collection relocation overflow"))?;
    let work = length
        .checked_add(capacity)
        .and_then(|work| work.checked_add(relocated_payload))
        .and_then(|work| work.checked_add(8))
        .ok_or(RunError::Contract("tuple collection work overflow"))?;
    let bytes = length
        .checked_add(relocated_payload)
        .and_then(|copies| copies.checked_mul(size_of::<Type<'_>>()))
        .and_then(|bytes| bytes.checked_add(layout.size()))
        .and_then(|bytes| bytes.checked_add(headers))
        .ok_or(RunError::Contract("tuple collection bytes overflow"))?;
    Ok((work, bytes))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CollectionResult {
    Vector,
    Boxed,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Resizes a tuple annotation to a fixed length, retaining length mismatches as `None`.
    pub(in crate::types::infer) async fn resize_tuple_to_fixed(
        &self,
        env: &ProgramEnvironment<'db>,
        spec: &TupleSpec<'db>,
        length: usize,
    ) -> RunResult<Option<TupleSpec<'db>>> {
        let effects = self
            .local_with_fixed_transfers(1, 0, || TupleResizeEffects { source: self, env })
            .await?;
        let resized = self
            .allocate_future(|| unpack_fixed_with(spec, length, FixedUnpackFacts, &effects))
            .await?
            .await?;
        self.local_with_fixed_transfers(3, size_of::<Option<TupleSpec<'db>>>(), || {
            resized.map(TupleSpec::Fixed).ok()
        })
        .await
    }

    /// Collects a successfully resized annotation's fixed elements for ordered child inference.
    pub(in crate::types::infer) async fn tuple_annotation_elements(
        &self,
        spec: &TupleSpec<'db>,
    ) -> RunResult<vec::IntoIter<Type<'db>>> {
        let fixed = self
            .local_with_fixed_transfers(
                2,
                size_of::<Option<&FixedLengthTuple<Type<'db>>>>(),
                || spec.as_fixed_length(),
            )
            .await?
            .ok_or(RunError::Contract("resized tuple annotation is not fixed"))?;
        let length = self
            .local_with_fixed_transfers(1, size_of::<usize>(), || fixed.len())
            .await?;
        let headers = Self::checked(
            size_of::<Vec<Type<'db>>>().checked_add(size_of::<vec::IntoIter<Type<'db>>>()),
        )?;
        #[cfg(all(test, feature = "experimental-analysis"))]
        observe_boundary(self.db(), Stage::BeforeAnnotations);
        self.local_quoted_with_fixed_transfers(
            collected_payload_quote(length, headers, CollectionResult::Vector),
            || {
                let annotations = fixed.iter_all_elements().collect::<Vec<_>>().into_iter();
                #[cfg(all(test, feature = "experimental-analysis"))]
                observe_boundary(self.db(), Stage::AfterAnnotations);
                annotations
            },
        )
        .await
    }

    /// Builds the union of tuple elements in order without expanding type aliases.
    pub(in crate::types::infer) async fn tuple_elements_union(
        &self,
        env: &ProgramEnvironment<'db>,
        prefix: &[Type<'db>],
        variable: Option<Type<'db>>,
        suffix: &[Type<'db>],
    ) -> RunResult<Type<'db>> {
        let mut builder = self
            .local_with_fixed_transfers(4, size_of::<UnionBuilder<'db>>() * 2, || {
                UnionBuilder::new(self.db(), env).unpack_aliases(false)
            })
            .await?;
        let mut elements = self
            .local_with_fixed_transfers(5, size_of::<TupleUnionElements<'_, 'db>>(), || {
                TupleUnionElements {
                    prefix: prefix.iter(),
                    variable,
                    suffix: suffix.iter(),
                }
            })
            .await?;
        while let Some(element) = self
            .local_with_fixed_transfers(5, size_of::<Option<Type<'db>>>(), || elements.next())
            .await?
        {
            self.allocate_future(|| PairUnionEffects::union_add(self, &mut builder, element))
                .await?
                .await?;
        }
        // Keep the builder outside the child factory until the child and its transfer are admitted.
        let mut builder = self
            .local_with_fixed_transfers(2, size_of::<Option<UnionBuilder<'db>>>(), || Some(builder))
            .await?;
        self.allocate_future(|| async {
            let owned = self
                .local_with_fixed_transfers(2, size_of::<UnionBuilder<'db>>(), || builder.take())
                .await?
                .ok_or(RunError::Contract(
                    "tuple element union was already consumed",
                ))?;
            PairUnionEffects::union_build(self, owned).await
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    FixedUnpackEffects<Type<'db>, VariableSegment<'db>>
    for TupleResizeEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(5).await
    }

    async fn clone_fixed(
        &self,
        values: &FixedLengthTuple<Type<'db>>,
    ) -> RunResult<FixedLengthTuple<Type<'db>>> {
        let length = self
            .source
            .local_with_fixed_transfers(1, size_of::<usize>(), || values.len())
            .await?;
        #[cfg(all(test, feature = "experimental-analysis"))]
        observe_boundary(self.source.db(), Stage::BeforeResize);
        self.source
            .local_quoted_with_fixed_transfers(
                fixed_payload_quote(length, size_of::<FixedLengthTuple<Type<'db>>>() * 2),
                || {
                    let resized = values.clone();
                    #[cfg(all(test, feature = "experimental-analysis"))]
                    observe_boundary(self.source.db(), Stage::AfterResize);
                    resized
                },
            )
            .await
    }

    async fn variable_elements(&self, segment: &VariableSegment<'db>) -> RunResult<Vec<Type<'db>>> {
        self.source
            .local_quoted_with_fixed_transfers(
                fixed_payload_quote(1, size_of::<Vec<Type<'db>>>() * 2),
                || vec![segment.element_type(self.source.db())],
            )
            .await
    }

    async fn combine(&self, elements: &[Type<'db>]) -> RunResult<Type<'db>> {
        self.source
            .allocate_future(|| {
                self.source
                    .tuple_elements_union(self.env, elements, None, &[])
            })
            .await?
            .await
    }

    async fn expand_fixed(
        &self,
        values: &VariableLengthTuple<Type<'db>, VariableSegment<'db>>,
        variable: Type<'db>,
        count: usize,
    ) -> RunResult<FixedLengthTuple<Type<'db>>> {
        let length = self
            .source
            .local_with_fixed_transfers(3, size_of::<Option<usize>>(), || {
                values
                    .prefix_elements()
                    .len()
                    .checked_add(count)
                    .and_then(|length| length.checked_add(values.suffix_elements().len()))
            })
            .await?
            .ok_or(RunError::Contract("fixed tuple expansion length overflow"))?;
        let headers = size_of::<FixedLengthTuple<Type<'db>>>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<Vec<Type<'db>>>()))
            .ok_or(RunError::Contract("fixed tuple expansion headers overflow"))?;
        #[cfg(all(test, feature = "experimental-analysis"))]
        observe_boundary(self.source.db(), Stage::BeforeResize);
        self.source
            .local_quoted_with_fixed_transfers(
                collected_payload_quote(length, headers, CollectionResult::Boxed),
                || {
                    let resized = expand_fixed(values, variable, count);
                    #[cfg(all(test, feature = "experimental-analysis"))]
                    observe_boundary(self.source.db(), Stage::AfterResize);
                    resized
                },
            )
            .await
    }
}
