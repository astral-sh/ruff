//! Metadata used to select special return-type handling for known callables.

use std::alloc::Layout;

use salsa::execution_probe::FieldRequest;

use super::effects::BinderEffects;
use super::{Binding, Bindings, CallableBinding};
use crate::Db;
use crate::types::Type;
use crate::types::function::{
    DataclassTransformerFlags, DataclassTransformerParams, FunctionMetadataEffects, FunctionType,
    OverloadLiteral, identity_sealed,
};
use crate::types::tuple::TupleSpec;

#[cfg(all(test, feature = "experimental-analysis"))]
pub(in crate::types) mod observations;

/// Admits a factory's fixed carriers separately from its operation's work and storage.
/// The source provider retains the factory through rejection and pending-child drainage.
pub(super) async fn dataclass_local<'db, E, T, F>(
    effects: &E,
    work: Option<usize>,
    bytes: Option<usize>,
    action: F,
) -> Result<T, E::Error>
where
    E: BinderEffects<'db>,
    F: FnOnce() -> T,
{
    // Construction, transfer and retirement are logical operations, independent of width.
    let work = work.and_then(|work| work.checked_add(6));
    let bytes = bytes
        .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Option<F>>()))
        .and_then(|bytes| bytes.checked_add(size_of::<T>().checked_mul(2)?))
        .and_then(|bytes| bytes.checked_add(size_of::<Result<T, E::Error>>().checked_mul(2)?));
    effects.local(work, bytes, action).await
}

/// Reads a supplied value; a missing name or unsupplied parameter yields `None`.
/// Signature defaults are not evaluated.
async fn supplied_dataclass_argument<'db, E: BinderEffects<'db>>(
    db: &'db dyn Db,
    binding: &Binding<'db>,
    name: &str,
    effects: &E,
) -> Result<Option<Type<'db>>, E::Error> {
    let count = dataclass_local(effects, Some(1), Some(0), || {
        binding.signature.parameters().len()
    })
    .await?;
    // Equality checks lengths first, so the requested name bounds each comparison.
    let work = name
        .len()
        .checked_add(2)
        .and_then(|per_parameter| count.checked_mul(per_parameter))
        .and_then(|work| work.checked_add(2));
    dataclass_local(effects, work, Some(0), || {
        binding.parameter_type_by_name(db, name, false).ok().flatten()
    })
    .await
}

/// Constructs the metadata returned by a dataclass-transform factory and updates its binding.
/// Exact tuples contribute their fixed prefix and suffix in order; missing or non-tuple
/// field-specifier arguments contribute no elements. Parameter defaults are not inferred.
pub(super) async fn dataclass_transform_with<'db, E: BinderEffects<'db>>(
    db: &'db dyn Db,
    binding: &mut Binding<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    // Use named parameter lookup to handle custom
    // `__dataclass_transform__` functions that follow older versions
    // of the spec.
    let eq = supplied_dataclass_argument(db, binding, "eq_default", effects).await?;
    let order = supplied_dataclass_argument(db, binding, "order_default", effects).await?;
    let kw_only = supplied_dataclass_argument(db, binding, "kw_only_default", effects).await?;
    let frozen = supplied_dataclass_argument(db, binding, "frozen_default", effects).await?;
    let flags = dataclass_local(effects, Some(12), Some(0), || {
        let mut flags = DataclassTransformerFlags::empty();
        flags.set(
            DataclassTransformerFlags::EQ_DEFAULT,
            eq.and_then(Type::as_bool_literal).unwrap_or(true),
        );
        flags.set(
            DataclassTransformerFlags::ORDER_DEFAULT,
            order.and_then(Type::as_bool_literal).unwrap_or(false),
        );
        flags.set(
            DataclassTransformerFlags::KW_ONLY_DEFAULT,
            kw_only.and_then(Type::as_bool_literal).unwrap_or(false),
        );
        flags.set(
            DataclassTransformerFlags::FROZEN_DEFAULT,
            frozen.and_then(Type::as_bool_literal).unwrap_or(false),
        );
        flags
    })
    .await?;
    // Accept both `field_specifiers` (current name) and
    // `field_descriptors` (legacy name).
    let modern = supplied_dataclass_argument(db, binding, "field_specifiers", effects).await?;
    let supplied = match modern {
        Some(ty) => Some(ty),
        None => supplied_dataclass_argument(db, binding, "field_descriptors", effects).await?,
    };
    let exact = dataclass_local(effects, Some(4), Some(size_of::<Option<&TupleSpec<'db>>>() * 2), || {
        supplied
            .and_then(Type::as_nominal_instance)
            .and_then(|instance| instance.exact_tuple())
    })
    .await?;
    let spec = match exact {
        Some(tuple) => Some(effects.field(tuple.field_requests(db).tuple()).await?),
        None => None,
    };
    let count = dataclass_local(effects, Some(2), Some(0), || {
        spec.map(|spec| spec.fixed_elements().len()).unwrap_or(0)
    })
    .await?;
    let mut cursor = dataclass_local(effects, Some(1), Some(0), || {
        spec.into_iter()
            .flat_map(|spec| spec.fixed_elements())
            .copied()
    })
    .await?;

    #[cfg(all(test, feature = "experimental-analysis"))]
    observations::stage(db, observations::Stage::BeforeBuffer, count);
    // Declare the observation before the buffer so it retires after that local owner.
    #[cfg(all(test, feature = "experimental-analysis"))]
    let lifetime;
    let buffer_bytes = Layout::array::<Type<'db>>(count)
        .ok()
        .map(|layout| layout.size());
    let buffer_work = count.checked_mul(2).and_then(|work| work.checked_add(4));
    let mut elements = dataclass_local(effects, buffer_work, buffer_bytes, || {
        let mut elements = Vec::new();
        elements.reserve_exact(count);
        elements
    })
    .await?;
    #[cfg(all(test, feature = "experimental-analysis"))]
    {
        lifetime = observations::BufferLifetime::new(db);
        observations::stage(db, observations::Stage::BufferReady, elements.len());
    }
    while let Some(element) = dataclass_local(effects, Some(2), Some(0), || cursor.next()).await? {
        dataclass_local(effects, Some(3), Some(size_of::<Type<'db>>()), || {
            elements.push(element);
        })
        .await?;
    }
    #[cfg(all(test, feature = "experimental-analysis"))]
    observations::stage(db, observations::Stage::Populated, elements.len());
    let finalization_work = count.checked_mul(3).and_then(|work| work.checked_add(4));
    let elements = dataclass_local(effects, finalization_work, buffer_bytes, move || {
        elements.into_boxed_slice()
    })
    .await?;
    #[cfg(all(test, feature = "experimental-analysis"))]
    observations::stage(db, observations::Stage::BeforeInterner, elements.len());
    let future = dataclass_local(effects, Some(1), Some(0), move || {
        effects.dataclass_transformer_params(db, flags, elements)
    })
    .await?;
    let params = future.await?;
    #[cfg(all(test, feature = "experimental-analysis"))]
    observations::stage(db, observations::Stage::BeforeReturn, count);
    dataclass_local(effects, Some(2), Some(size_of::<Type<'db>>() * 2), || {
        binding.set_return_type(Type::DataclassTransformer(params));
    })
    .await?;
    #[cfg(all(test, feature = "experimental-analysis"))]
    {
        observations::stage(db, observations::Stage::Returned, count);
        drop(lifetime);
    }
    Ok(())
}

struct KnownCaseFunctionMetadata<'a, E> {
    effects: &'a E,
}

impl<E> identity_sealed::Sealed for KnownCaseFunctionMetadata<'_, E> {}

impl<'db, E: BinderEffects<'db>> FunctionMetadataEffects<'db> for KnownCaseFunctionMetadata<'_, E> {
    type Error = E::Error;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        self.effects.field(request).await
    }

    async fn overloads_and_implementation(
        &self,
        db: &'db dyn Db,
        last_definition: OverloadLiteral<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error> {
        self.effects
            .function_overloads(db, last_definition)
            .await
    }
}

pub(super) async fn transformer_params_with<'db, E: BinderEffects<'db>>(
    db: &'db dyn Db,
    function: FunctionType<'db>,
    effects: &E,
) -> Result<Option<DataclassTransformerParams<'db>>, E::Error> {
    let literal = effects.field(function.field_requests(db).literal()).await?;
    let (overloads, implementation) = literal
        .overloads_and_implementation_with(db, &KnownCaseFunctionMetadata { effects })
        .await?;
    effects
        .local(overloads.len().checked_add(1), Some(0), || ())
        .await?;
    for overload in overloads.iter().copied().chain(implementation).rev() {
        if let Some(params) = effects
            .field(overload.field_requests(db).dataclass_transformer_params())
            .await?
        {
            return Ok(Some(params));
        }
    }
    Ok(None)
}

impl Bindings<'_> {
    pub(super) fn known_case_traversal_work(&self) -> Option<usize> {
        self.elements
            .iter()
            .try_fold(self.elements.len(), |work, element| {
                work.checked_add(element.items.len())
            })
    }
}

impl CallableBinding<'_> {
    pub(super) fn known_case_matching_work(&self) -> Option<usize> {
        self.overloads
            .iter()
            .try_fold(self.overloads.len(), |work, overload| {
                work.checked_add(overload.errors.len())?
                    .checked_add(overload.return_type().inline_payload_bytes())
            })
    }
}
