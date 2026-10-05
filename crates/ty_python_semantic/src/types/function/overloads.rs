//! Overload collection shared by ordinary and controlled inference.

use super::{
    FunctionDecorators, FunctionIdentityEffects, LegacyFunctionIdentityEffects, OverloadLiteral,
};
use crate::Db;

/// Storage operations for collecting a function's overloads in source order.
pub(in crate::types) trait OverloadCollectionEffects<'db>:
    FunctionIdentityEffects<'db>
{
    async fn append_overload(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
        overload: OverloadLiteral<'db>,
    ) -> Result<(), Self::Error>;

    async fn reverse_overloads(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
    ) -> Result<(), Self::Error>;

    async fn finish_overloads(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
    ) -> Result<Box<[OverloadLiteral<'db>]>, Self::Error>;
}

pub(in crate::types) async fn collect_overloads_with<'db, E: OverloadCollectionEffects<'db>>(
    db: &'db dyn Db,
    self_overload: OverloadLiteral<'db>,
    effects: &E,
) -> Result<(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>), E::Error> {
    let mut current = self_overload;
    let mut overloads = vec![];

    while let Some(previous) = current.previous_overload_with(db, effects).await? {
        let overload = previous.last_definition;
        effects.append_overload(&mut overloads, overload).await?;
        current = overload;
    }

    // Overloads are inserted in reverse order, from bottom to top.
    effects.reverse_overloads(&mut overloads).await?;

    let implementation = if effects
        .field(self_overload.field_requests(db).decorators())
        .await?
        .contains(FunctionDecorators::OVERLOAD)
    {
        effects
            .append_overload(&mut overloads, self_overload)
            .await?;
        None
    } else {
        Some(self_overload)
    };

    Ok((
        effects.finish_overloads(&mut overloads).await?,
        implementation,
    ))
}

impl<'db> OverloadCollectionEffects<'db> for LegacyFunctionIdentityEffects {
    async fn append_overload(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
        overload: OverloadLiteral<'db>,
    ) -> Result<(), Self::Error> {
        overloads.push(overload);
        Ok(())
    }

    async fn reverse_overloads(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
    ) -> Result<(), Self::Error> {
        overloads.reverse();
        Ok(())
    }

    async fn finish_overloads(
        &self,
        overloads: &mut Vec<OverloadLiteral<'db>>,
    ) -> Result<Box<[OverloadLiteral<'db>]>, Self::Error> {
        Ok(std::mem::take(overloads).into_boxed_slice())
    }
}
