//! Inherit a generic context into each overload without changing its other signature metadata.

use std::convert::Infallible;

use smallvec::SmallVec;

use super::{CallableSignature, Signature};
use crate::Db;
use crate::types::generics::GenericContext;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousInheritedGenericContextEffects)]
    /// Supplies signature cloning, ordered context merging, and overload storage.
    ///
    /// Controlled providers fund each cloned signature's eventual retirement before cloning it.
    /// Partial output and the current clone remain alive across a suspended context merge.
    pub(in crate::types) trait InheritedGenericContextEffects<'db> {
        type Error;

        #[operation(local)]
        /// Allocates room for the complete output before cloning the first overload.
        async fn new_overloads(&self, signatures: &CallableSignature<'db>) -> Result<SmallVec<[Signature<'db>; 1]>, Self::Error>;

        #[operation(local)]
        #[progress]
        /// Advances through the stored overloads, preserving their order.
        async fn next_overload<'a>(&self, signatures: &'a CallableSignature<'db>, cursor: &mut usize) -> Result<Option<&'a Signature<'db>>, Self::Error>;

        #[operation(local)]
        /// Clones all signature metadata and funds destruction of the clone's owned payload.
        async fn clone_signature(&self, signature: &Signature<'db>) -> Result<Signature<'db>, Self::Error>;

        #[operation(local)]
        /// Reads whether the cloned overload already owns a generic context.
        async fn generic_context(&self, signature: &Signature<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;

        #[operation(child)]
        /// Merges existing variables before inherited variables using ordinary context deduplication.
        async fn merge_context(&self, existing: GenericContext<'db>, inherited: GenericContext<'db>) -> Result<GenericContext<'db>, Self::Error>;

        #[operation(local)]
        /// Assigns the completed context and appends the clone without modifying its other fields.
        async fn push_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>, context: GenericContext<'db>) -> Result<(), Self::Error>;

        #[operation(local)]
        /// Transfers every completed overload into the callable's final storage.
        async fn finish_overloads(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>) -> Result<CallableSignature<'db>, Self::Error>;
    }

    #[synchronous(with_inherited_generic_context_sync)]
    #[capabilities(effects = InheritedGenericContextEffects)]
    #[passive_values()]
    /// Clones each overload, then merges or assigns its inherited context before advancing.
    ///
    /// Existing variables precede inherited variables when both contexts are present. An overload
    /// without a context directly uses the inherited handle. An empty callable remains empty.
    pub(in crate::types) async fn with_inherited_generic_context_with<'db, E: InheritedGenericContextEffects<'db>>(
        signatures: &CallableSignature<'db>,
        inherited: GenericContext<'db>,
        effects: &E,
    ) -> Result<CallableSignature<'db>, E::Error> {
        let mut overloads = effects.new_overloads(signatures).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(signature) = effects.next_overload(signatures, &mut cursor).await? {
            let signature = effects.clone_signature(signature).await?;
            let context = match effects.generic_context(&signature).await? {
                Some(existing) => effects.merge_context(existing, inherited).await?,
                None => inherited,
            };
            effects.push_overload(&mut overloads, signature, context).await?;
        }
        effects.finish_overloads(&mut overloads).await
    }
}

/// Uses ordinary context merging and signature cloning for synchronous inheritance.
struct OrdinaryInheritedGenericContextEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousInheritedGenericContextEffects<'db>
    for OrdinaryInheritedGenericContextEffects<'db>
{
    type Error = Infallible;

    fn new_overloads(
        &self,
        signatures: &CallableSignature<'db>,
    ) -> Result<SmallVec<[Signature<'db>; 1]>, Infallible> {
        Ok(SmallVec::with_capacity(signatures.overloads.len()))
    }

    fn next_overload<'a>(
        &self,
        signatures: &'a CallableSignature<'db>,
        cursor: &mut usize,
    ) -> Result<Option<&'a Signature<'db>>, Infallible> {
        let signature = signatures.overloads.get(*cursor);
        if signature.is_some() {
            *cursor += 1;
        }
        Ok(signature)
    }

    fn clone_signature(&self, signature: &Signature<'db>) -> Result<Signature<'db>, Infallible> {
        Ok(signature.clone())
    }

    fn generic_context(
        &self,
        signature: &Signature<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(signature.generic_context)
    }

    fn merge_context(
        &self,
        existing: GenericContext<'db>,
        inherited: GenericContext<'db>,
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(existing.merge(self.db, inherited))
    }

    fn push_overload(
        &self,
        overloads: &mut SmallVec<[Signature<'db>; 1]>,
        mut signature: Signature<'db>,
        context: GenericContext<'db>,
    ) -> Result<(), Infallible> {
        signature.generic_context = Some(context);
        overloads.push(signature);
        Ok(())
    }

    fn finish_overloads(
        &self,
        overloads: &mut SmallVec<[Signature<'db>; 1]>,
    ) -> Result<CallableSignature<'db>, Infallible> {
        Ok(CallableSignature {
            overloads: std::mem::take(overloads),
        })
    }
}

/// Synchronously clones each overload and adds inherited variables after its existing declarations.
pub(super) fn with_inherited_generic_context<'db>(
    db: &'db dyn Db,
    signatures: &CallableSignature<'db>,
    inherited: GenericContext<'db>,
) -> CallableSignature<'db> {
    match with_inherited_generic_context_sync(
        signatures,
        inherited,
        &OrdinaryInheritedGenericContextEffects { db },
    ) {
        Ok(signatures) => signatures,
        Err(never) => match never {},
    }
}
