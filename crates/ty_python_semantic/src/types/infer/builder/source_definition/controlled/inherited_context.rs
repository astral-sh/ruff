//! Admitted overload cloning and inherited-context merging for callable signatures.

use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;

use super::{SourceAccess, SourceEffects};
use crate::types::function::last_signature::FunctionLastSignatureEffects;
use crate::types::generics::GenericContext;
use crate::types::signatures::inherited_context::{
    InheritedGenericContextEffects, with_inherited_generic_context_with,
};
use crate::types::signatures::{CallableSignature, Signature};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Clones a callable's overloads and merges the inherited context into each one in order.
    ///
    /// The caller admits this bridge's future before constructing it. The shared traversal retains
    /// its partial output and current clone while a merge suspends; the execution driver drains
    /// queued children before retiring those owners after refusal or cancellation.
    pub(super) async fn inherit_callable_generic_context(
        &self,
        signatures: &CallableSignature<'db>,
        inherited: GenericContext<'db>,
    ) -> RunResult<CallableSignature<'db>> {
        let captures = Self::checked(
            size_of::<(&Self, &CallableSignature<'db>, GenericContext<'db>)>().checked_mul(4),
        )?;
        self.local_with_fixed_transfers(8, captures, || ()).await?;
        self.type_parameter_future(|| {
            with_inherited_generic_context_with(signatures, inherited, self)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> InheritedGenericContextEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn new_overloads(
        &self,
        signatures: &CallableSignature<'db>,
    ) -> RunResult<SmallVec<[Signature<'db>; 1]>> {
        let count = self
            .local_with_fixed_transfers(4, 0, || signatures.overloads.len())
            .await?;
        let bytes = if count > 1 {
            Self::checked(count.checked_mul(size_of::<Signature<'db>>()))?
        } else {
            0
        };
        // One overload is inline. Reserving the complete output avoids relocation; each clone
        // separately funds its payload retirement, including shared storage's final-owner drop.
        let work = Self::checked(count.checked_add(8))?;
        self.local_with_fixed_transfers(work, bytes, || SmallVec::with_capacity(count))
            .await
    }

    async fn next_overload<'a>(
        &self,
        signatures: &'a CallableSignature<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<&'a Signature<'db>>> {
        // These are the next iteration's fixed clone, context, cursor, and push carriers. Their
        // storage may be reused, but each initialization and transfer is admitted before it occurs.
        type Carriers<'a, 'db> = (
            &'a Signature<'db>,
            Option<&'a Signature<'db>>,
            Signature<'db>,
            GenericContext<'db>,
            Option<GenericContext<'db>>,
            usize,
        );
        let bytes = Self::checked(size_of::<Carriers<'_, 'db>>().checked_mul(4))?;
        self.local_with_fixed_transfers(24, bytes, || {
            let signature = signatures.overloads.get(*cursor);
            if signature.is_some() {
                *cursor += 1;
            }
            signature
        })
        .await
    }

    async fn clone_signature(&self, signature: &Signature<'db>) -> RunResult<Signature<'db>> {
        let captures = Self::checked(size_of::<(&Self, &Signature<'db>)>().checked_mul(4))?;
        self.local_with_fixed_transfers(4, captures, || ()).await?;
        self.type_parameter_future(|| {
            FunctionLastSignatureEffects::clone_signature(self, signature)
        })
        .await?
        .await
    }

    async fn generic_context(
        &self,
        signature: &Signature<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.local_with_fixed_transfers(2, 0, || signature.generic_context)
            .await
    }

    async fn merge_context(
        &self,
        existing: GenericContext<'db>,
        inherited: GenericContext<'db>,
    ) -> RunResult<GenericContext<'db>> {
        let captures = Self::checked(
            size_of::<(&Self, GenericContext<'db>, GenericContext<'db>)>().checked_mul(4),
        )?;
        self.local_with_fixed_transfers(4, captures, || ()).await?;
        self.type_parameter_future(|| self.merge_return_context(existing, inherited))
            .await?
            .await
    }

    async fn push_overload(
        &self,
        overloads: &mut SmallVec<[Signature<'db>; 1]>,
        signature: Signature<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(4, size_of::<Option<Signature<'db>>>(), || ())
            .await?;
        let mut signature = Some(signature);
        // Keep the clone outside the rejecting callback. Only successful admission transfers it
        // into the output, whose complete capacity was reserved before the first overload.
        self.local_with_fixed_transfers(8, size_of::<Signature<'db>>(), || {
            if overloads.len() == overloads.capacity() {
                return Err(RunError::Contract("inherited overload capacity exhausted"));
            }
            let Some(mut signature) = signature.take() else {
                return Err(RunError::Contract("inherited overload was consumed"));
            };
            signature.generic_context = Some(context);
            overloads.push(signature);
            Ok(())
        })
        .await?
    }

    async fn finish_overloads(
        &self,
        overloads: &mut SmallVec<[Signature<'db>; 1]>,
    ) -> RunResult<CallableSignature<'db>> {
        self.local_with_fixed_transfers(6, size_of::<SmallVec<[Signature<'db>; 1]>>(), || {
            CallableSignature {
                overloads: std::mem::take(overloads),
            }
        })
        .await
    }
}
