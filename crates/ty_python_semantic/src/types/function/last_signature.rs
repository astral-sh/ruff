//! Selects the effective last-definition signature for the existing canonical function query.

use std::convert::Infallible;

use salsa::execution_probe::FieldRequest;

use super::{FunctionLiteral, FunctionType};
use crate::Db;
use crate::types::signatures::Signature;

/// Supplies retained fields, signature cloning and source inference for last-definition selection.
pub(in crate::types) trait FunctionLastSignatureEffects<'db> {
    type Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;

    /// Admits fixed selection and transfer work before invoking its borrowed factory.
    async fn local<T>(&self, work: usize, action: impl FnOnce() -> T) -> Result<T, Self::Error>;

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error>;

    /// Clones a selected stored signature, including admission for its eventual destruction.
    async fn clone_signature(
        &self,
        signature: &Signature<'db>,
    ) -> Result<Signature<'db>, Self::Error>;

    /// Infers the last definition through the shared ordinary source-signature algorithm.
    async fn definition_signature(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<Signature<'db>, Self::Error>;
}

impl<'db> FunctionType<'db> {
    /// Selects the effective last-definition signature. A separate implementation uses a stored
    /// update only when it contains exactly one callable with exactly one signature. Otherwise
    /// that branch infers the last source definition. Without a separate implementation, the last
    /// stored public overload is used when present, with the same source-definition fallback.
    pub(in crate::types) async fn last_definition_signature_with<
        E: FunctionLastSignatureEffects<'db>,
    >(
        self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Signature<'db>, E::Error> {
        let literal = effects.field(self.field_requests(db).literal()).await?;
        let separate = effects.has_separate_implementation(db, literal).await?;
        let updated = effects
            .field(self.field_requests(db).updated_signatures())
            .await?;
        let signature = if separate {
            let callable = effects
                .local(3, || {
                    let updated = updated.as_deref()?;
                    let [callable] = updated.implementation_callables.as_deref()? else {
                        return None;
                    };
                    Some(*callable)
                })
                .await?;
            if let Some(callable) = callable {
                let signatures = effects
                    .field(callable.field_requests(db).signatures())
                    .await?;
                effects
                    .local(2, || {
                        let [signature] = signatures.overloads.as_slice() else {
                            return None;
                        };
                        Some(signature)
                    })
                    .await?
            } else {
                None
            }
        } else {
            effects
                .local(3, || {
                    let updated = updated.as_deref()?;
                    let signature = updated.signature.as_ref()?;
                    signature.overloads.last()
                })
                .await?
        };

        if let Some(signature) = signature {
            effects.clone_signature(signature).await
        } else {
            effects.definition_signature(db, literal).await
        }
    }
}

/// Executes last-definition selection synchronously inside the ordinary Salsa query body.
#[derive(Debug)]
pub(super) struct InlineFunctionLastSignatureEffects;

impl<'db> FunctionLastSignatureEffects<'db> for InlineFunctionLastSignatureEffects {
    type Error = Infallible;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn local<T>(&self, _work: usize, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(literal.has_separate_implementation(db))
    }

    async fn clone_signature(
        &self,
        signature: &Signature<'db>,
    ) -> Result<Signature<'db>, Self::Error> {
        Ok(signature.clone())
    }

    async fn definition_signature(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> Result<Signature<'db>, Self::Error> {
        Ok(literal.last_definition_signature(db))
    }
}
