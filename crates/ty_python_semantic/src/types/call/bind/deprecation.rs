//! Deprecation discovery preserves call selection and intersection suppression across effects.

use std::convert::Infallible;

use itertools::Either;
use smallvec::SmallVec;

use super::{Bindings, CallableBinding, OverloadCallResult};
use crate::Db;
use crate::types::Type;
use crate::types::function::{
    FunctionMetadataEffects, LegacyFunctionIdentityEffects, OverloadLiteral,
};
use crate::types::signatures::effects::legacy_inline;

#[derive(Clone, Copy)]
pub(in crate::types) enum DeprecationDependency {
    BoundMethodType,
    BoundMethodFunction,
    DownstreamConstructor,
}

#[derive(Clone, Copy)]
pub(in crate::types) struct DeprecationQuote {
    pub work: usize,
    pub requested_bytes: usize,
}

impl DeprecationQuote {
    fn scan(items: usize) -> Option<Self> {
        Some(Self {
            work: items.checked_add(1)?,
            requested_bytes: 0,
        })
    }
}

pub(in crate::types) trait DeprecationEffects<'db>:
    FunctionMetadataEffects<'db>
{
    async fn local<T>(
        &self,
        quote: Option<DeprecationQuote>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn dependency<T>(
        &self,
        dependency: DeprecationDependency,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
}

#[derive(Clone, Copy)]
enum DeprecationCandidate<'db> {
    Deprecated(OverloadLiteral<'db>),
    Overload(OverloadLiteral<'db>),
}

/// The caller retains this owner until deprecation collection and its pending effects finish.
#[derive(Default)]
pub(in crate::types) struct DeprecatedFunctions<'a, 'db> {
    functions: SmallVec<[(&'a CallableBinding<'db>, OverloadLiteral<'db>); 1]>,
}

impl<'a, 'db> DeprecatedFunctions<'a, 'db> {
    pub(in crate::types) fn as_slice(&self) -> &[(&'a CallableBinding<'db>, OverloadLiteral<'db>)] {
        &self.functions
    }

    pub(super) fn into_iter(
        self,
    ) -> impl Iterator<Item = (&'a CallableBinding<'db>, OverloadLiteral<'db>)> {
        self.functions.into_iter()
    }

    fn push_quote(&self) -> Option<DeprecationQuote> {
        let capacity = self.functions.capacity();
        let next_len = self.functions.len().checked_add(1)?;
        let requested_bytes = if next_len > capacity {
            next_len.checked_mul(size_of::<(&CallableBinding<'_>, OverloadLiteral<'_>)>())?
        } else {
            0
        };
        // Include relocation and eventual retirement before extending this flat owner.
        Some(DeprecationQuote {
            work: capacity.checked_add(next_len)?.checked_add(4)?,
            requested_bytes,
        })
    }
}

impl<'db> DeprecationEffects<'db> for LegacyFunctionIdentityEffects {
    async fn local<T>(
        &self,
        _quote: Option<DeprecationQuote>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn dependency<T>(
        &self,
        _dependency: DeprecationDependency,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }
}

impl<'db> Bindings<'db> {
    /// Append deprecations without discarding earlier union alternatives when a
    /// non-deprecated intersection member suppresses the current alternative's warnings.
    pub(in crate::types) async fn collect_deprecated_functions_with<'a, E>(
        &'a self,
        db: &'db dyn Db,
        functions: &mut DeprecatedFunctions<'a, 'db>,
        effects: &E,
    ) -> Result<(), E::Error>
    where
        E: DeprecationEffects<'db>,
    {
        effects.local(DeprecationQuote::scan(1), || ()).await?;
        for element in &self.elements {
            let start = effects
                .local(DeprecationQuote::scan(1), || functions.functions.len())
                .await?;
            for item in &element.items {
                let (item_start, callable) = effects
                    .local(DeprecationQuote::scan(1), || {
                        (functions.functions.len(), item.callable())
                    })
                    .await?;
                let (mut deprecated, iteration_quote) =
                    callable.deprecated_functions_with(db, effects).await?;
                while let Some(candidate) =
                    effects.local(iteration_quote, || deprecated.next()).await?
                {
                    let function = match candidate {
                        DeprecationCandidate::Deprecated(function) => function,
                        DeprecationCandidate::Overload(function) => {
                            if effects
                                .field(function.field_requests(db).deprecated())
                                .await?
                                .is_none()
                            {
                                continue;
                            }
                            function
                        }
                    };
                    effects
                        .local(functions.push_quote(), || {
                            functions.functions.reserve_exact(1);
                            functions.functions.push((callable, function));
                        })
                        .await?;
                }
                let downstream = effects
                    .local(DeprecationQuote::scan(1), || {
                        item.as_constructor()
                            .and_then(|constructor| constructor.downstream_constructor())
                    })
                    .await?;
                if let Some(downstream) = downstream {
                    effects
                        .dependency(DeprecationDependency::DownstreamConstructor, || {
                            legacy_inline(downstream.collect_deprecated_functions_with(
                                db,
                                functions,
                                &LegacyFunctionIdentityEffects,
                            ));
                        })
                        .await?;
                }
                let suppressed = effects
                    .local(DeprecationQuote::scan(functions.functions.len()), || {
                        if functions.functions.len() == item_start {
                            // This intersection member provides a non-deprecated alternative.
                            functions.functions.truncate(start);
                            true
                        } else {
                            false
                        }
                    })
                    .await?;
                if suppressed {
                    break;
                }
            }
        }
        Ok(())
    }
}

impl<'db> CallableBinding<'db> {
    /// Returns the deprecated implementation, taking precedence over any deprecated overloads.
    /// Otherwise, returns overloads selected by this call for deprecation checks, using their
    /// original source indexes to preserve their identities after receiver compatibility filtering.
    async fn deprecated_functions_with<E: DeprecationEffects<'db>>(
        &self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<
        (
            impl Iterator<Item = DeprecationCandidate<'db>> + Clone,
            Option<DeprecationQuote>,
        ),
        E::Error,
    > {
        let signature_type = effects
            .local(DeprecationQuote::scan(1), || self.signature_type)
            .await?;
        let signature_type = match signature_type {
            Type::BoundMethod(bound) => {
                effects
                    .dependency(DeprecationDependency::BoundMethodType, || bound.func(db))
                    .await?
            }
            ty => ty,
        };
        if let Type::Callable(callable) = signature_type {
            let deprecated = effects
                .field(callable.field_requests(db).deprecated())
                .await?;
            return Ok((
                Either::Left(deprecated.map(DeprecationCandidate::Deprecated).into_iter()),
                DeprecationQuote::scan(1),
            ));
        }
        let function = match signature_type {
            Type::FunctionLiteral(function) => Some(function),
            Type::BoundMethod(bound) => {
                effects
                    .dependency(DeprecationDependency::BoundMethodFunction, || {
                        bound.function(db)
                    })
                    .await?
            }
            _ => None,
        };
        let (overloads, implementation) = if let Some(function) = function {
            effects.local(DeprecationQuote::scan(1), || ()).await?;
            function
                .overloads_and_implementation_with(db, effects)
                .await?
        } else {
            (&[][..], None)
        };
        let implementation = effects
            .local(DeprecationQuote::scan(1), || implementation)
            .await?;
        if let Some(implementation) = implementation
            && effects
                .field(implementation.field_requests(db).deprecated())
                .await?
                .is_some()
        {
            return Ok((
                Either::Left(Some(DeprecationCandidate::Deprecated(implementation)).into_iter()),
                DeprecationQuote::scan(1),
            ));
        }

        let iteration_quote = effects
            .local(DeprecationQuote::scan(self.overloads.len()), || {
                self.deprecation_iteration_quote()
            })
            .await?;
        Ok((
            Either::Right(self.selected_overloads().filter_map(move |(_, binding)| {
                overloads
                    .get(binding.source_overload_index())
                    .copied()
                    .map(DeprecationCandidate::Overload)
            })),
            iteration_quote,
        ))
    }

    fn deprecation_iteration_quote(&self) -> Option<DeprecationQuote> {
        let selection = match &self.overload_call_result {
            Some(OverloadCallResult::ArgumentTypeExpansion(expanded)) => {
                expanded.selected_overloads.len()
            }
            _ => 0,
        };
        let work = self.overloads.iter().try_fold(1usize, |work, overload| {
            work.checked_add(overload.errors.len())?
                .checked_add(selection)?
                .checked_add(4)
        })?;
        Some(DeprecationQuote {
            work,
            requested_bytes: 0,
        })
    }
}
