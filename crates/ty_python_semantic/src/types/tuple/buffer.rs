use super::{FixedLengthTuple, Tuple, TupleSpec, VariableLengthTuple, VariableSegment};
use crate::types::Type;

pub(in crate::types) fn fixed_spec(elements: Vec<Type<'_>>) -> TupleSpec<'_> {
    Tuple::Fixed(FixedLengthTuple(elements.into_boxed_slice()))
}

pub(in crate::types) fn variable_spec<'db>(
    mut elements: Vec<Type<'db>>,
    prefix_len: usize,
    variable_segment: VariableSegment<'db>,
) -> TupleSpec<'db> {
    elements.shrink_to_fit();
    Tuple::Variable(VariableLengthTuple {
        fixed_elements: smallvec::SmallVec::from_vec(elements),
        prefix_len,
        variable_segment,
    })
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) use admitted::{TupleBuffer, TupleBufferStorageEffects};

#[cfg(any(test, feature = "experimental-analysis"))]
mod admitted {
    use salsa::execution_probe::{RunResult, TaskEndpoint};

    use super::{TupleSpec, Type, VariableSegment};
    use crate::types::local_transfer::local_with_fixed_transfers_at;

    /// Admits tuple-element storage operations, including backing construction and retirement.
    /// Each stored type is a copied handle; these operations do not visit its semantic children.
    pub(in crate::types) trait TupleBufferStorageEffects<'db> {
        async fn new_elements(&self, capacity: usize) -> RunResult<Vec<Type<'db>>>;
        async fn push_element(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()>;
        async fn finish_elements(&self, elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> RunResult<TupleSpec<'db>>;
    }

    pub(in crate::types) struct TupleBuffer<'db, O = ()> {
        elements: Vec<Type<'db>>,
        variable: Option<(usize, VariableSegment<'db>)>,
        observer: O,
    }

    impl<'db, O> TupleBuffer<'db, O> {
        /// Creates storage for fixed tuple elements, admitting its allocation and retirement.
        pub(in crate::types) async fn new(
            endpoint: &TaskEndpoint<'_, 'db>,
            storage: &impl TupleBufferStorageEffects<'db>,
            capacity: usize,
            make_observer: impl FnOnce() -> O,
        ) -> RunResult<Self> {
            let elements = storage.new_elements(capacity).await?;
            local(endpoint, 4, 0, || {
                Ok(Self {
                    elements,
                    variable: None,
                    observer: make_observer(),
                })
            })
            .await
        }

        /// Borrows the stored fixed elements in prefix-then-suffix order.
        ///
        /// The caller admits this read and any cursor construction; element visits need
        /// separate admission.
        pub(in crate::types) const fn elements(&self) -> &[Type<'db>] {
            self.elements.as_slice()
        }

        /// Appends one element after admitting any relocation and the element write.
        pub(in crate::types) async fn push(
            &mut self,
            endpoint: &TaskEndpoint<'_, 'db>,
            storage: &impl TupleBufferStorageEffects<'db>,
            ty: Type<'db>,
            on_push: impl FnOnce(&mut O, usize),
        ) -> RunResult<()> {
            storage.push_element(&mut self.elements, ty).await?;
            // Run the fixed observer bookkeeping after the admitted element write.
            local(endpoint, 3, 0, || {
                on_push(&mut self.observer, self.elements.len());
                Ok(())
            })
            .await
        }

        /// Records where the variable segment separates the stored prefix and suffix.
        pub(in crate::types) async fn start_variable(
            &mut self,
            endpoint: &TaskEndpoint<'_, 'db>,
            variable: VariableSegment<'db>,
        ) -> RunResult<()> {
            local(endpoint, 2, 0, || {
                self.variable = Some((self.elements.len(), variable));
                Ok(())
            })
            .await
        }

        /// Moves the accumulated elements into a tuple specification after admitting any compaction.
        pub(in crate::types) async fn finish(
            &mut self,
            endpoint: &TaskEndpoint<'_, 'db>,
            storage: &impl TupleBufferStorageEffects<'db>,
            on_finish: impl FnOnce(&mut O),
        ) -> RunResult<TupleSpec<'db>> {
            let variable = local(endpoint, 1, 0, || Ok(self.variable)).await?;
            let result = storage.finish_elements(&mut self.elements, variable).await?;
            local(endpoint, 3, 0, || {
                on_finish(&mut self.observer);
                Ok(result)
            })
            .await
        }
    }

    /// Admits operation payloads and fixed transfers before invoking a local callback.
    async fn local<'db, T, F: FnOnce() -> RunResult<T>>(
        endpoint: &TaskEndpoint<'_, 'db>,
        work: usize,
        bytes: usize,
        operation: F,
    ) -> RunResult<T> {
        local_with_fixed_transfers_at(endpoint, work, bytes, operation).await?
    }
}
