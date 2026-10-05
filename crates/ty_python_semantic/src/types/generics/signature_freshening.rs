//! Freshen signature declarations in order, giving each declaration its own mapping root.

use std::convert::Infallible;

use super::context_construction::ContextVariables;
use crate::types::{BoundTypeVarInstance, GenericContext, Type, TypeContext, TypeMapping};
use crate::{Db, ProgramEnvironment};

/// Supplies declaration mapping and the finite storage used to rebuild its generic context.
pub(in crate::types) trait SignatureFresheningEffects<'db> {
    type Error;

    /// Admits logical work and requested bytes before running `action`, including fixed transfers.
    /// A controlled provider rejects an absent quote and retains captured owners until rejection drains;
    /// the ordinary provider runs the same action without resource limits.
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Self::Error>;
    /// Maps one declaration with an independent visitor, adding `delta` to occurrences selected by `context`.
    async fn map_declaration(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Result<Type<'db>, Self::Error>;
    async fn finish(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> Result<GenericContext<'db>, Self::Error>;
}

/// Rebuilds `context` by freshening declarations whose occurrences belong to `mapping_context`.
/// Each declaration has an independent mapping root and adds `delta` to selected freshness nonces.
/// A nonvariable result retains the original declaration.
/// Context construction then preserves encounter order and deduplicates variable identities.
pub(in crate::types) async fn freshen_signature_context_with<
    'db,
    E: SignatureFresheningEffects<'db>,
>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    context: GenericContext<'db>,
    mapping_context: GenericContext<'db>,
    delta: u32,
    effects: &E,
) -> Result<GenericContext<'db>, E::Error> {
    let variables = effects.variables(db, context).await?;
    let len = effects.local(Some(1), Some(0), || variables.len()).await?;
    let bytes = std::alloc::Layout::array::<BoundTypeVarInstance<'db>>(len)
        .ok()
        .map(|layout| layout.size());
    let work = len.checked_mul(2).and_then(|work| work.checked_add(4));
    let mut mapped = effects
        .local(work, bytes, || Vec::with_capacity(len))
        .await?;
    let mut index = effects.local(Some(1), Some(0), || 0usize).await?;
    loop {
        let next = effects
            .local(
                Some(4),
                Some(size_of::<(
                    &crate::types::BoundTypeVarIdentity<'db>,
                    &BoundTypeVarInstance<'db>,
                )>()),
                || {
                    let next = variables.get_index(index).map(|(_, variable)| *variable);
                    if next.is_some() {
                        index += 1;
                    }
                    next
                },
            )
            .await?;
        let Some(variable) = next else { break };
        let replacement = effects
            .map_declaration(db, env, variable, mapping_context, delta)
            .await?;
        effects
            .local(Some(4), Some(0), || {
                mapped.push(replacement.as_typevar().unwrap_or(variable));
            })
            .await?;
    }
    effects.finish(db, env, &mapped).await
}

/// Uses ordinary roots and the shared canonical generic-context constructor.
#[derive(Debug)]
pub(in crate::types) struct InlineSignatureFreshening;

impl<'db> SignatureFresheningEffects<'db> for InlineSignatureFreshening {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn variables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_inner(db))
    }

    async fn map_declaration(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
        context: GenericContext<'db>,
        delta: u32,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::TypeVar(variable).apply_type_mapping(
            db,
            env,
            &TypeMapping::FreshenBoundTypeVars {
                generic_context: context,
                delta,
            },
            TypeContext::default(),
        ))
    }

    async fn finish(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> Result<GenericContext<'db>, Infallible> {
        Ok(GenericContext::from_typevar_instances(
            db,
            env,
            variables.iter().copied(),
        ))
    }
}
