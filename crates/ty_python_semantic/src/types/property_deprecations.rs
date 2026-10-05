//! Retains deprecated accessor implementations after descriptor lookup replaces a property with
//! its value type. Collection preserves union alternatives and intersection suppression before
//! deduplicating declarations in their first-seen order.

use std::convert::Infallible;

use crate::types::function::{
    FunctionMetadataEffects, LegacyFunctionIdentityEffects, OverloadLiteral,
};
use crate::types::storage_quote::buffer_push_quote;
use crate::types::{IntersectionType, PropertyDeprecations, Type, UnionType, legacy_inline};
use crate::{Db, FxOrderSet};

pub(in crate::types) trait PropertyDeprecationEffects<'db>:
    FunctionMetadataEffects<'db>
{
    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(Some(1), Some(size_of::<T>()), action).await
    }

    async fn properties(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Self::Error>;

    async fn accessor(
        &self,
        db: &'db dyn Db,
        accessor: Type<'db>,
        functions: &mut Vec<OverloadLiteral<'db>>,
    ) -> Result<(), Self::Error>;

    async fn union_elements(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error>;

    async fn intersection_elements(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Self::Error>;

    async fn deduplicate(
        &self,
        functions: Vec<OverloadLiteral<'db>>,
    ) -> Result<Box<[OverloadLiteral<'db>]>, Self::Error>;

    async fn insert(
        &self,
        functions: &mut FxOrderSet<OverloadLiteral<'db>>,
        function: OverloadLiteral<'db>,
    ) -> Result<(), Self::Error>;

    async fn finish(
        &self,
        functions: FxOrderSet<OverloadLiteral<'db>>,
    ) -> Result<Box<[OverloadLiteral<'db>]>, Self::Error>;

    async fn intern(
        &self,
        db: &'db dyn Db,
        functions: &mut [Box<[OverloadLiteral<'db>]>; 3],
    ) -> Result<PropertyDeprecations<'db>, Self::Error>;

    async fn combine(
        &self,
        db: &'db dyn Db,
        left: PropertyDeprecations<'db>,
        right: PropertyDeprecations<'db>,
        intersection: bool,
    ) -> Result<PropertyDeprecations<'db>, Self::Error>;
}

pub(in crate::types) async fn collect_with<'db, E: PropertyDeprecationEffects<'db>>(
    db: &'db dyn Db,
    ty: Type<'db>,
    effects: &E,
) -> Result<Option<PropertyDeprecations<'db>>, E::Error> {
    effects.local(Some(1), Some(0), || ()).await?;
    match ty {
        Type::PropertyInstance(property) => {
            let getter = effects.field(property.field_requests(db).getter()).await?;
            let setter = effects.field(property.field_requests(db).setter()).await?;
            let deleter = effects.field(property.field_requests(db).deleter()).await?;
            let accessors = effects
                .local(Some(1), Some(size_of::<[Option<Type<'db>>; 3]>()), || {
                    [getter, setter, deleter]
                })
                .await?;
            let mut declarations = effects
                .local(
                    Some(1),
                    Some(size_of::<[Box<[OverloadLiteral<'db>]>; 3]>()),
                    || [Box::default(), Box::default(), Box::default()],
                )
                .await?;
            let mut entries = effects
                .step(|| accessors.into_iter().zip(&mut declarations))
                .await?;
            while let Some((accessor, result)) = effects.step(|| entries.next()).await? {
                let mut functions = effects
                    .local(
                        Some(1),
                        Some(size_of::<Vec<OverloadLiteral<'db>>>()),
                        Vec::new,
                    )
                    .await?;
                if let Some(accessor) = accessor {
                    effects.accessor(db, accessor, &mut functions).await?;
                }
                let functions = effects.deduplicate(functions).await?;
                effects
                    .local(
                        Some(1),
                        Some(size_of::<Box<[OverloadLiteral<'db>]>>()),
                        || {
                            *result = functions;
                        },
                    )
                    .await?;
            }
            drop(entries);
            if effects
                .local(Some(4), Some(0), || {
                    declarations.iter().all(|functions| functions.is_empty())
                })
                .await?
            {
                Ok(None)
            } else {
                Ok(Some(effects.intern(db, &mut declarations).await?))
            }
        }
        Type::Union(union) => {
            let elements = effects.union_elements(db, union).await?;
            let mut elements = effects.step(|| elements.iter()).await?;
            let mut properties = effects
                .local(
                    Some(1),
                    Some(size_of::<Option<PropertyDeprecations<'db>>>()),
                    || None,
                )
                .await?;
            while let Some(element) = effects.step(|| elements.next().copied()).await? {
                if let Some(next) = effects.properties(db, element).await? {
                    let next = match properties {
                        Some(previous) => effects.combine(db, previous, next, false).await?,
                        None => next,
                    };
                    effects
                        .local(
                            Some(1),
                            Some(size_of::<Option<PropertyDeprecations<'db>>>()),
                            || {
                                properties = Some(next);
                            },
                        )
                        .await?;
                }
            }
            Ok(properties)
        }
        Type::Intersection(intersection) => {
            let elements = effects.intersection_elements(db, intersection).await?;
            let mut elements = effects.step(|| elements.iter()).await?;
            let Some(first) = effects.step(|| elements.next().copied()).await? else {
                return Ok(None);
            };
            let Some(mut properties) = effects.properties(db, first).await? else {
                return Ok(None);
            };
            while let Some(element) = effects.step(|| elements.next().copied()).await? {
                let Some(next) = effects.properties(db, element).await? else {
                    return Ok(None);
                };
                let combined = effects.combine(db, properties, next, true).await?;
                effects
                    .local(
                        Some(1),
                        Some(size_of::<PropertyDeprecations<'db>>()),
                        || {
                            properties = combined;
                        },
                    )
                    .await?;
            }
            Ok(Some(properties))
        }
        _ => Ok(None),
    }
}

/// Append deprecated implementations, preserving earlier entries if a non-deprecated
/// intersection alternative suppresses this accessor's deprecations.
pub(in crate::types) async fn collect_accessor_with<'db, E: PropertyDeprecationEffects<'db>>(
    db: &'db dyn Db,
    accessor: Type<'db>,
    functions: &mut Vec<OverloadLiteral<'db>>,
    effects: &E,
) -> Result<(), E::Error> {
    effects.local(Some(1), Some(0), || ()).await?;
    match accessor {
        Type::FunctionLiteral(function) => {
            let (_, implementation) = function
                .overloads_and_implementation_with(db, effects)
                .await?;
            if let Some(implementation) = implementation
                && effects
                    .field(implementation.field_requests(db).deprecated())
                    .await?
                    .is_some()
            {
                let quote = effects
                    .local(Some(3), Some(0), || {
                        buffer_push_quote::<OverloadLiteral<'db>>((
                            functions.len(),
                            functions.capacity(),
                            true,
                        ))
                    })
                    .await?;
                effects
                    .local(
                        quote.map(|quote| quote.work),
                        quote.and_then(|quote| {
                            quote.bytes.checked_add(size_of::<OverloadLiteral<'db>>())
                        }),
                        || functions.push(implementation),
                    )
                    .await?;
            }
        }
        Type::BoundMethod(method) => {
            let function = effects.field(method.field_requests(db).func()).await?;
            effects.accessor(db, function, functions).await?;
        }
        Type::Union(union) => {
            let elements = effects.union_elements(db, union).await?;
            let mut elements = effects.step(|| elements.iter()).await?;
            while let Some(element) = effects.step(|| elements.next().copied()).await? {
                effects.accessor(db, element, functions).await?;
            }
        }
        Type::Intersection(intersection) => {
            let start = effects.step(|| functions.len()).await?;
            let elements = effects.intersection_elements(db, intersection).await?;
            let mut elements = effects.step(|| elements.iter()).await?;
            while let Some(element) = effects.step(|| elements.next().copied()).await? {
                let element_start = effects.step(|| functions.len()).await?;
                effects.accessor(db, element, functions).await?;
                let end = effects.step(|| functions.len()).await?;
                if end == element_start {
                    // A non-deprecated intersection member can supply the accessor.
                    effects
                        .local(
                            end.checked_sub(start)
                                .and_then(|count| count.checked_add(1)),
                            Some(0),
                            || functions.truncate(start),
                        )
                        .await?;
                    break;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(in crate::types) async fn deduplicate_with<'db, E: PropertyDeprecationEffects<'db>>(
    functions: Vec<OverloadLiteral<'db>>,
    effects: &E,
) -> Result<Box<[OverloadLiteral<'db>]>, E::Error> {
    let mut unique = effects
        .local(
            Some(1),
            Some(size_of::<FxOrderSet<OverloadLiteral<'db>>>()),
            FxOrderSet::default,
        )
        .await?;
    let mut functions = effects
        .local(
            Some(1),
            Some(size_of::<std::vec::IntoIter<OverloadLiteral<'db>>>()),
            || functions.into_iter(),
        )
        .await?;
    while let Some(function) = effects
        .local(
            Some(1),
            Some(size_of::<Option<OverloadLiteral<'db>>>()),
            || functions.next(),
        )
        .await?
    {
        effects.insert(&mut unique, function).await?;
    }
    effects.finish(unique).await
}

impl<'db> PropertyDeprecationEffects<'db> for LegacyFunctionIdentityEffects {
    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn properties(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Infallible> {
        Ok(ty.property_deprecations(db))
    }

    async fn accessor(
        &self,
        db: &'db dyn Db,
        accessor: Type<'db>,
        functions: &mut Vec<OverloadLiteral<'db>>,
    ) -> Result<(), Infallible> {
        Ok(legacy_inline(collect_accessor_with(
            db, accessor, functions, self,
        )))
    }

    async fn union_elements(
        &self,
        db: &'db dyn Db,
        union: UnionType<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(union.elements(db))
    }

    async fn intersection_elements(
        &self,
        db: &'db dyn Db,
        intersection: IntersectionType<'db>,
    ) -> Result<&'db FxOrderSet<Type<'db>>, Infallible> {
        Ok(intersection.positive(db))
    }

    async fn deduplicate(
        &self,
        functions: Vec<OverloadLiteral<'db>>,
    ) -> Result<Box<[OverloadLiteral<'db>]>, Infallible> {
        deduplicate_with(functions, self).await
    }

    async fn insert(
        &self,
        functions: &mut FxOrderSet<OverloadLiteral<'db>>,
        function: OverloadLiteral<'db>,
    ) -> Result<(), Infallible> {
        functions.insert(function);
        Ok(())
    }

    async fn finish(
        &self,
        functions: FxOrderSet<OverloadLiteral<'db>>,
    ) -> Result<Box<[OverloadLiteral<'db>]>, Infallible> {
        Ok(functions.into_iter().collect())
    }

    async fn intern(
        &self,
        db: &'db dyn Db,
        functions: &mut [Box<[OverloadLiteral<'db>]>; 3],
    ) -> Result<PropertyDeprecations<'db>, Infallible> {
        let [getters, setters, deleters] = std::mem::take(functions);
        Ok(PropertyDeprecations::new(db, getters, setters, deleters))
    }

    async fn combine(
        &self,
        db: &'db dyn Db,
        left: PropertyDeprecations<'db>,
        right: PropertyDeprecations<'db>,
        intersection: bool,
    ) -> Result<PropertyDeprecations<'db>, Infallible> {
        Ok(if intersection {
            left.intersection(db, right)
        } else {
            left.union(db, right)
        })
    }
}
