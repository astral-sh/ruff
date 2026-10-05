//! Inherit a class context into a function's public and separate implementation signatures.

use std::convert::Infallible;

use super::{FunctionLiteral, FunctionType, UpdatedFunctionSignatures};
use crate::Db;
use crate::types::callable::{CallableType, CallableTypeKind};
use crate::types::generics::GenericContext;
use crate::types::signatures::CallableSignature;

#[cfg(all(test, feature = "experimental-analysis"))]
pub(in crate::types) mod fixtures;
#[cfg(all(test, feature = "experimental-analysis"))]
pub(in crate::types) mod observations;

/// Retains either the complete stored implementation list or its single inferred fallback.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ImplementationInput<'db> {
    Stored(&'db [CallableType<'db>]),
    Fallback(CallableType<'db>),
}

impl<'db> ImplementationInput<'db> {
    /// Reports the number of implementation callables to transform without resolving their signatures.
    pub(in crate::types) const fn len(self) -> usize {
        match self {
            Self::Stored(callables) => callables.len(),
            Self::Fallback(_) => 1,
        }
    }

    /// Advances through retained callable handles in order, including an empty stored list.
    pub(in crate::types) fn next(self, cursor: &mut usize) -> Option<CallableType<'db>> {
        let callable = match self {
            Self::Stored(callables) => callables.get(*cursor).copied(),
            Self::Fallback(callable) => (*cursor == 0).then_some(callable),
        };
        if callable.is_some() {
            *cursor += 1;
        }
        callable
    }
}

/// Builds the native retained-payload request without exposing the function's private storage.
#[cfg(feature = "experimental-analysis")]
pub(in crate::types) fn updated_request<'db>(
    function: FunctionType<'db>,
    fields: salsa::execution_probe::FieldRequestContext<'db>,
) -> impl salsa::execution_probe::FieldRequest<
    'db,
    Output = &'db Option<Box<UpdatedFunctionSignatures<'db>>>,
> {
    function.field_requests(fields).updated_signatures()
}

/// Selects retained implementation storage without conflating absence with a present empty list.
pub(in crate::types) fn stored_implementations<'db>(
    updated: &'db Option<Box<UpdatedFunctionSignatures<'db>>>,
) -> Option<ImplementationInput<'db>> {
    updated
        .as_deref()
        .and_then(|updated| updated.implementation_callables.as_deref())
        .map(ImplementationInput::Stored)
}

/// Assembles the completed public signature and optional implementation list for function interning.
pub(in crate::types) fn updated_signatures<'db>(
    signature: CallableSignature<'db>,
    implementations: Option<Box<[CallableType<'db>]>>,
) -> Option<Box<UpdatedFunctionSignatures<'db>>> {
    UpdatedFunctionSignatures::new(Some(signature), implementations)
}

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies effective signature reads, ordered context inheritance, storage, and canonical rebuilding.
    /// Stored implementation presence is preserved even when its list is empty. Each inherited
    /// signature retains all other metadata, and callable replacement retains kind and deprecation.
    #[synchronous(SynchronousFunctionInheritedContextEffects)]
    pub(in crate::types) trait FunctionInheritedContextEffects<'db> {
        type Error;

        #[operation(child)]
        async fn signature(&self, function: FunctionType<'db>) -> Result<&'db CallableSignature<'db>, Self::Error>;
        #[operation(child)]
        async fn inherit(&self, signature: &CallableSignature<'db>, context: GenericContext<'db>) -> Result<CallableSignature<'db>, Self::Error>;
        #[operation(child)]
        async fn literal(&self, function: FunctionType<'db>) -> Result<FunctionLiteral<'db>, Self::Error>;
        #[operation(child)]
        async fn has_separate_implementation(&self, literal: FunctionLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn stored_implementations(&self, function: FunctionType<'db>) -> Result<Option<ImplementationInput<'db>>, Self::Error>;
        #[operation(child)]
        async fn fallback_implementation(&self, function: FunctionType<'db>) -> Result<ImplementationInput<'db>, Self::Error>;
        #[operation(local)]
        async fn new_output(&self, input: ImplementationInput<'db>) -> Result<Vec<CallableType<'db>>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_callable(&self, input: ImplementationInput<'db>, cursor: &mut usize) -> Result<Option<CallableType<'db>>, Self::Error>;
        #[operation(child)]
        async fn callable_signatures(&self, callable: CallableType<'db>) -> Result<&'db CallableSignature<'db>, Self::Error>;
        #[operation(child)]
        async fn replace_callable(&self, callable: CallableType<'db>, signatures: CallableSignature<'db>) -> Result<CallableType<'db>, Self::Error>;
        #[operation(local)]
        async fn push(&self, output: &mut Vec<CallableType<'db>>, callable: CallableType<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_output(&self, output: Vec<CallableType<'db>>) -> Result<Box<[CallableType<'db>]>, Self::Error>;
        #[operation(local)]
        async fn updated(&self, signature: CallableSignature<'db>, implementations: Option<Box<[CallableType<'db>]>>) -> Result<Option<Box<UpdatedFunctionSignatures<'db>>>, Self::Error>;
        #[operation(child)]
        async fn descriptor_kind(&self, function: FunctionType<'db>) -> Result<Option<CallableTypeKind>, Self::Error>;
        #[operation(child)]
        async fn intern(&self, literal: FunctionLiteral<'db>, updated: Option<Box<UpdatedFunctionSignatures<'db>>>, descriptor: Option<CallableTypeKind>) -> Result<FunctionType<'db>, Self::Error>;
    }

    /// Adds the inherited context to the public signature, then to every separate implementation.
    /// A present implementation list is used exactly as stored; absence requests the effective
    /// last-definition fallback. Functions without a separate implementation ignore that payload.
    #[synchronous(with_inherited_generic_context_sync)]
    #[capabilities(effects = FunctionInheritedContextEffects)]
    #[passive_values()]
    pub(in crate::types) async fn with_inherited_generic_context_with<'db, E: FunctionInheritedContextEffects<'db>>(
        function: FunctionType<'db>,
        context: GenericContext<'db>,
        effects: &E,
    ) -> Result<FunctionType<'db>, E::Error> {
        let signature = effects.signature(function).await?;
        let signature = effects.inherit(signature, context).await?;
        let literal = effects.literal(function).await?;
        let implementations = if effects.has_separate_implementation(literal).await? {
            let input = match effects.stored_implementations(function).await? {
                Some(input) => input,
                None => effects.fallback_implementation(function).await?,
            };
            let mut output = effects.new_output(input).await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(callable) = effects.next_callable(input, &mut cursor).await? {
                let signatures = effects.callable_signatures(callable).await?;
                let signatures = effects.inherit(signatures, context).await?;
                let callable = effects.replace_callable(callable, signatures).await?;
                effects.push(&mut output, callable).await?;
            }
            Some(effects.finish_output(output).await?)
        } else {
            None
        };
        let updated = effects.updated(signature, implementations).await?;
        let descriptor = effects.descriptor_kind(function).await?;
        effects.intern(literal, updated, descriptor).await
    }
}

/// Uses ordinary reads and canonical constructors for the shared function transformation.
pub(super) struct OrdinaryFunctionInheritedContext<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousFunctionInheritedContextEffects<'db>
    for OrdinaryFunctionInheritedContext<'db>
{
    type Error = Infallible;

    fn signature(
        &self,
        function: FunctionType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Infallible> {
        Ok(function.signature(self.db))
    }

    fn inherit(
        &self,
        signature: &CallableSignature<'db>,
        context: GenericContext<'db>,
    ) -> Result<CallableSignature<'db>, Infallible> {
        Ok(signature.with_inherited_generic_context(self.db, context))
    }

    fn literal(&self, function: FunctionType<'db>) -> Result<FunctionLiteral<'db>, Infallible> {
        Ok(function.literal(self.db))
    }

    fn has_separate_implementation(
        &self,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Infallible> {
        Ok(literal.has_separate_implementation(self.db))
    }

    fn stored_implementations(
        &self,
        function: FunctionType<'db>,
    ) -> Result<Option<ImplementationInput<'db>>, Infallible> {
        Ok(stored_implementations(function.updated_signatures(self.db)))
    }

    fn fallback_implementation(
        &self,
        function: FunctionType<'db>,
    ) -> Result<ImplementationInput<'db>, Infallible> {
        Ok(ImplementationInput::Fallback(CallableType::single(
            self.db,
            function.last_definition_signature(self.db).clone(),
        )))
    }

    fn new_output(
        &self,
        input: ImplementationInput<'db>,
    ) -> Result<Vec<CallableType<'db>>, Infallible> {
        Ok(Vec::with_capacity(input.len()))
    }

    fn next_callable(
        &self,
        input: ImplementationInput<'db>,
        cursor: &mut usize,
    ) -> Result<Option<CallableType<'db>>, Infallible> {
        Ok(input.next(cursor))
    }

    fn callable_signatures(
        &self,
        callable: CallableType<'db>,
    ) -> Result<&'db CallableSignature<'db>, Infallible> {
        Ok(callable.signatures(self.db))
    }

    fn replace_callable(
        &self,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> Result<CallableType<'db>, Infallible> {
        Ok(callable.with_signatures(self.db, signatures))
    }

    fn push(
        &self,
        output: &mut Vec<CallableType<'db>>,
        callable: CallableType<'db>,
    ) -> Result<(), Infallible> {
        output.push(callable);
        Ok(())
    }

    fn finish_output(
        &self,
        output: Vec<CallableType<'db>>,
    ) -> Result<Box<[CallableType<'db>]>, Infallible> {
        Ok(output.into_boxed_slice())
    }

    fn updated(
        &self,
        signature: CallableSignature<'db>,
        implementations: Option<Box<[CallableType<'db>]>>,
    ) -> Result<Option<Box<UpdatedFunctionSignatures<'db>>>, Infallible> {
        Ok(updated_signatures(signature, implementations))
    }

    fn descriptor_kind(
        &self,
        function: FunctionType<'db>,
    ) -> Result<Option<CallableTypeKind>, Infallible> {
        Ok(function.descriptor_kind(self.db))
    }

    fn intern(
        &self,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor: Option<CallableTypeKind>,
    ) -> Result<FunctionType<'db>, Infallible> {
        Ok(FunctionType::new_internal(
            self.db, literal, updated, descriptor,
        ))
    }
}
