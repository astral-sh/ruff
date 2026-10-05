//! Error classification and recovery after a static class's C3 merge fails.

use std::collections::VecDeque;

use itertools::Itertools;

use super::construction::{SynchronousStaticMroEffects, base_has_cyclic_mro_sync};
use super::{DuplicateBaseError, Mro, StaticMroError, StaticMroErrorKind};
use crate::types::class_base::ClassBase;
use crate::types::{ClassType, KnownInstanceType, SpecialFormType, StaticClassLiteral, Type};
use crate::{Db, FxIndexMap, ProgramEnvironment};

#[cfg(test)]
mod tests;

/// Classifies a completed C3 failure while preserving fallible declaration dependencies.
/// Collection work here belongs to the source owner; this helper does not admit generated work.
pub(super) fn static_error_details_with<'db, E: SynchronousStaticMroEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class_literal: StaticClassLiteral<'db>,
    class: ClassType<'db>,
    original_bases: &[Type<'db>],
    resolved_bases: &[ClassBase<'db>],
    effects: &E,
) -> Result<Result<Mro<'db>, StaticMroError<'db>>, E::Error> {
    // We now know that the MRO is unresolvable through the C3-merge algorithm.
    // The rest of this function is dedicated to figuring out the best error message
    // to report to the user.

    if let Some(literal) = class.class_literal(db).as_static()
        && effects.has_pep_695_type_params(literal)?
        && original_bases.iter().any(|base| {
            matches!(
                base,
                Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_))
                    | Type::SpecialForm(SpecialFormType::Generic)
            )
        })
    {
        return Ok(Err(effects.make_error(
            env,
            class,
            StaticMroErrorKind::Pep695ClassWithGenericInheritance,
        )?));
    }

    let mut duplicate_dynamic_bases = false;

    let duplicate_bases: Vec<DuplicateBaseError<'db>> = {
        let mut base_to_indices = FxIndexMap::<Type<'db>, (ClassBase<'db>, Vec<usize>)>::default();

        // We need to iterate over `original_bases` here rather than `resolved_bases`
        // so that we get the correct index of the duplicate bases if there were any
        // (`resolved_bases` may be a longer list than `original_bases`!). However, we
        // need to use the base's MRO identity rather than its inferred type as the key
        // for the `base_to_indices` map so that a class such as
        // `class Foo(Protocol[T], Protocol): ...` correctly causes us to emit a
        // `duplicate-base` diagnostic (matching the runtime behaviour) rather than an
        // `inconsistent-mro` diagnostic (which would be accurate -- but not nearly as
        // precise!).
        for (index, base) in original_bases.iter().enumerate() {
            let Some(base) = effects.converted_explicit_base(env, class_literal, index, *base)?
            else {
                continue;
            };
            let (_, indices) = base_to_indices
                .entry(base.mro_identity(db))
                .or_insert_with(|| (base, Vec::new()));
            indices.push(index);
        }

        let mut errors = vec![];

        for (base, indices) in base_to_indices.into_values() {
            let Some((first_index, later_indices)) = indices.split_first() else {
                continue;
            };
            if later_indices.is_empty() {
                continue;
            }
            match base {
                ClassBase::Class(_)
                | ClassBase::Generic
                | ClassBase::Protocol
                | ClassBase::TypedDict(_) => {
                    errors.push(DuplicateBaseError {
                        duplicate_base: base,
                        first_index: *first_index,
                        later_indices: later_indices.iter().copied().collect(),
                    });
                }
                ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                    duplicate_dynamic_bases = true;
                }
            }
        }

        errors
    };

    if duplicate_bases.is_empty() {
        if duplicate_dynamic_bases {
            Ok(Ok(Mro::from_error_with_object(
                class,
                effects.object_base(env)?,
            )))
        } else {
            let kind = StaticMroErrorKind::UnresolvableMro {
                bases_list: original_bases.iter().copied().collect(),
                generic_index: check_generic_reorder_fixes_mro_with(
                    db,
                    env,
                    resolved_bases,
                    original_bases,
                    effects,
                )?,
            };
            Ok(Err(effects.make_error(env, class, kind)?))
        }
    } else {
        Ok(Err(effects.make_error(
            env,
            class,
            StaticMroErrorKind::DuplicateBases(duplicate_bases.into_boxed_slice()),
        )?))
    }
}

/// Determine if an `inconsistent-mro` error could be resolved by moving
/// a `Generic[]` base to the end of the bases list.
///
/// If so, this function will return `Some(i)`, where `i` is the index of
/// the `Generic[]` base. If not, this function will return `None`.
fn check_generic_reorder_fixes_mro_with<'db, E: SynchronousStaticMroEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    resolved_bases: &[ClassBase<'db>],
    original_bases: &[Type<'db>],
    effects: &E,
) -> Result<Option<usize>, E::Error> {
    // Only attempt an autofix if `Generic[]` appears exactly once in the original bases list.
    let Ok(single_index) = original_bases
        .iter()
        .enumerate()
        .filter_map(|(i, base)| {
            matches!(
                base,
                Type::KnownInstance(KnownInstanceType::SubscriptedGeneric(_))
            )
            .then_some(i)
        })
        .exactly_one()
    else {
        return Ok(None);
    };

    // This should always be true if the original bases list contains exactly one
    // subscripted `Generic`, but return `None` here just to be safe.
    if resolved_bases.get(single_index) != Some(&ClassBase::Generic) {
        return Ok(None);
    }

    let mut reordered: VecDeque<ClassBase<'db>> = resolved_bases.iter().copied().collect();
    let Some(generic) = reordered.remove(single_index) else {
        return Ok(None);
    };
    reordered.push_back(generic);
    let mut seqs: Vec<VecDeque<ClassBase<'db>>> = Vec::with_capacity(reordered.len() + 1);
    for base in &reordered {
        if base_has_cyclic_mro_sync(db, *base, effects)? {
            return Ok(None);
        }
        seqs.push(effects.collect_base_mro(env, *base, None)?);
    }
    seqs.push(reordered);
    if effects.c3_merge(seqs)?.is_none() {
        return Ok(None);
    }
    Ok(Some(single_index))
}
