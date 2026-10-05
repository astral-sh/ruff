//! Shared decisions for moving homogeneous variadic suffixes into parameter prefixes.

use std::convert::Infallible;
use std::iter::Enumerate;
use std::slice::Iter;

use super::{Parameter, Parameters};
use crate::Db;
use crate::types::Type;

/// Supplies admission, alias resolution, and parameter replacement for variadic normalization.
///
/// The caller retains both parameter owners while these operations borrow them across awaits.
pub(in crate::types) trait VariadicNormalizationEffects<'db> {
    type Error;

    /// Admits local work and additional semantic storage before evaluating `action`.
    ///
    /// The provider quotes the action and result representations; `requested_bytes` covers
    /// additional inputs or local state, without quoting those same representations again.
    /// A controlled provider refuses a `None` quotation before evaluating the action.
    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    /// Resolves the annotation before a normalization decision compares or inspects it.
    async fn resolve_alias(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;

    /// Moves the already matched positional suffix before the variadic and rebuilds the list.
    async fn reorder_homogeneous_suffix(
        &self,
        db: &'db dyn Db,
        parameters: &mut Parameters<'db>,
        variadic_index: usize,
        suffix_len: usize,
    ) -> Result<(), Self::Error>;
}

/// Performs ordinary synchronous alias resolution and homogeneous-suffix replacement.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct InlineVariadicNormalizationEffects;

impl<'db> VariadicNormalizationEffects<'db> for InlineVariadicNormalizationEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn resolve_alias(&self, db: &'db dyn Db, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(db))
    }

    async fn reorder_homogeneous_suffix(
        &self,
        _db: &'db dyn Db,
        parameters: &mut Parameters<'db>,
        variadic_index: usize,
        suffix_len: usize,
    ) -> Result<(), Self::Error> {
        let mut reordered = parameters.as_slice().to_vec();
        reordered[variadic_index..=variadic_index + suffix_len].rotate_left(1);
        *parameters = parameters.with_transformed_parameters(reordered);
        Ok(())
    }
}

/// Finds the first variadic, admitting the borrowed scan and its iterator state.
async fn variadic_with<'parameters, 'db, E: VariadicNormalizationEffects<'db>>(
    parameters: &'parameters Parameters<'db>,
    effects: &E,
) -> Result<Option<(usize, &'parameters Parameter<'db>)>, E::Error> {
    effects
        .local(
            parameters.len().checked_add(1).and_then(|len| len.checked_mul(4)),
            Some(size_of::<Enumerate<Iter<'_, Parameter<'db>>>>()),
            || parameters.variadic(),
        )
        .await
}

/// Finds the first named keyword candidate, admitting each borrowed name comparison.
async fn keyword_by_name_with<'parameters, 'db, E: VariadicNormalizationEffects<'db>>(
    parameters: &'parameters Parameters<'db>,
    name: &str,
    effects: &E,
) -> Result<Option<(usize, &'parameters Parameter<'db>)>, E::Error> {
    let mut candidates = effects
        .local(Some(1), Some(0), || parameters.iter().enumerate())
        .await?;
    while let Some((index, parameter)) = effects
        .local(Some(4), Some(0), || candidates.next())
        .await?
    {
        let Some(candidate_name) = effects
            .local(Some(1), Some(0), || parameter.keyword_name())
            .await?
        else {
            continue;
        };
        let work = name
            .len()
            .checked_add(candidate_name.len())
            .and_then(|len| len.checked_add(1));
        let matching = effects
            .local(work, Some(0), || {
                (candidate_name == name).then_some((index, parameter))
            })
            .await?;
        if matching.is_some() {
            return Ok(matching);
        }
    }
    Ok(None)
}

/// Resolves a parameter's annotation after admitting the annotation-handle extraction.
async fn resolved_annotation_with<'db, E: VariadicNormalizationEffects<'db>>(
    db: &'db dyn Db,
    parameter: &Parameter<'db>,
    effects: &E,
) -> Result<Type<'db>, E::Error> {
    let annotation = effects
        .local(Some(1), Some(0), || parameter.annotated_type())
        .await?;
    effects.resolve_alias(db, annotation).await
}

/// Moves matching suffixes into prefixes when both variadics and named parameters permit it.
///
/// Both lists stay owned by the caller, including when alias resolution or replacement suspends.
/// A target keyword can independently supply a name already filled by a source positional
/// argument. For example, the target below accepts a call that the source cannot accept:
///
/// ```python
/// def source(value: int, *args: int) -> None: ...
/// def target(*args: *tuple[*tuple[int, ...], int], value: int) -> None: ...
/// target(1, value=2)
/// ```
///
/// Keep such comparisons in their original suffix form for the subsequent parameter matching.
pub(in crate::types) async fn normalize_variadic_parameters_with<
    'db,
    E: VariadicNormalizationEffects<'db>,
>(
    db: &'db dyn Db,
    source: &mut Parameters<'db>,
    target: &mut Parameters<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let source_variadic = variadic_with(source, effects).await?;
    let target_variadic = variadic_with(target, effects).await?;
    let (Some((_, source_variadic)), Some((_, target_variadic))) =
        (source_variadic, target_variadic)
    else {
        return Ok(());
    };
    if !effects
        .local(Some(2), Some(0), || {
            !source_variadic.has_starred_annotation() && !target_variadic.has_starred_annotation()
        })
        .await?
    {
        return Ok(());
    }

    let source_annotation = resolved_annotation_with(db, source_variadic, effects).await?;
    let target_annotation = resolved_annotation_with(db, target_variadic, effects).await?;
    if !effects
        .local(Some(1), Some(0), || source_annotation == target_annotation)
        .await?
    {
        return Ok(());
    }
    let source_annotation = resolved_annotation_with(db, source_variadic, effects).await?;
    if effects
        .local(Some(1), Some(0), || source_annotation.is_dynamic())
        .await?
    {
        return Ok(());
    }

    {
        let mut positional = effects
            .local(Some(1), Some(0), || source.positional())
            .await?;
        while let Some(source_parameter) = effects
            .local(Some(4), Some(0), || positional.next())
            .await?
        {
            let Some(name) = effects
                .local(Some(2), Some(0), || {
                    if source_parameter.is_positional_only() {
                        None
                    } else {
                        source_parameter.name()
                    }
                })
                .await?
            else {
                continue;
            };
            let target_parameter = match keyword_by_name_with(target, name, effects).await? {
                candidate @ Some((_, parameter)) => {
                    if !effects
                        .local(Some(1), Some(0), || parameter.is_keyword_only())
                        .await?
                    {
                        continue;
                    }
                    candidate
                }
                None => effects
                    .local(
                        target.len().checked_add(1).and_then(|len| len.checked_mul(4)),
                        Some(size_of::<Enumerate<Iter<'_, Parameter<'db>>>>()),
                        || target.keyword_variadic(),
                    )
                    .await?,
            };
            if let Some((_, target_parameter)) = target_parameter {
                let annotation = resolved_annotation_with(db, target_parameter, effects).await?;
                if !effects
                    .local(Some(1), Some(0), || annotation.is_never())
                    .await?
                {
                    return Ok(());
                }
            }
        }
    }

    with_homogeneous_variadic_suffix_in_prefix_with(db, source, effects).await?;
    with_homogeneous_variadic_suffix_in_prefix_with(db, target, effects).await
}

/// Moves required suffix elements that match a homogeneous variadic into its prefix.
async fn with_homogeneous_variadic_suffix_in_prefix_with<
    'db,
    E: VariadicNormalizationEffects<'db>,
>(
    db: &'db dyn Db,
    parameters: &mut Parameters<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let Some((variadic_index, variadic)) = variadic_with(parameters, effects).await? else {
        return Ok(());
    };
    let (mut suffix, mut matching_suffix_len) = effects
        .local(Some(5), Some(0), || {
            (parameters.as_slice()[variadic_index + 1..].iter(), 0_usize)
        })
        .await?;
    while let Some(parameter) = effects
        .local(Some(4), Some(0), || suffix.next())
        .await?
    {
        if !effects
            .local(Some(1), Some(0), || parameter.is_positional_only())
            .await?
        {
            break;
        }
        let annotation = resolved_annotation_with(db, parameter, effects).await?;
        let variadic_annotation = resolved_annotation_with(db, variadic, effects).await?;
        if !effects
            .local(Some(1), Some(0), || annotation == variadic_annotation)
            .await?
        {
            break;
        }
        effects
            .local(Some(1), Some(0), || matching_suffix_len += 1)
            .await?;
    }
    if effects
        .local(Some(4), Some(0), || {
            matching_suffix_len == 0
                || parameters
                    .as_slice()
                    .get(variadic_index + matching_suffix_len + 1)
                    .is_some_and(Parameter::is_positional)
        })
        .await?
    {
        return Ok(());
    }
    effects
        .reorder_homogeneous_suffix(db, parameters, variadic_index, matching_suffix_len)
        .await
}
