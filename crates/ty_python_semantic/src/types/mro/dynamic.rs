//! Shared dynamic MRO construction and ordered fallback traversal.

use std::collections::VecDeque;
use std::convert::Infallible;

use rustc_hash::FxHashSet;

use super::base::{BaseMroStart, InlineBaseMroEffects, base_mro_start_sync};
use super::collection::base::BaseCursor;
use super::construction::{InlineStaticMroEffects, base_has_cyclic_mro_sync};
use super::iteration::SynchronousMroIterationEffects;
use super::root::InlineMroRootEffects;
use super::{DynamicMroError, DynamicMroErrorKind, Mro};
use crate::types::class::{DynamicClassLiteral, DynamicEnumLiteral, DynamicNamedTupleLiteral};
use crate::types::class_base::ClassBase;
use crate::types::{ClassType, Type};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) trait DynamicMroEffects<'db>:
    SynchronousMroIterationEffects<'db>
{
    fn dynamic_checkpoint(&self) -> Result<(), Self::Error>;

    fn dynamic_bases(
        &self,
        class: DynamicClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error>;

    fn enum_bases(&self, class: DynamicEnumLiteral<'db>) -> Result<Box<[Type<'db>]>, Self::Error>;

    fn convert_base(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error>;

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Self::Error>;

    fn base_is_cycle(&self, base: ClassBase<'db>) -> Result<bool, Self::Error>;

    fn base_start(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
    ) -> Result<BaseMroStart<'db>, Self::Error>;

    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Self::Error>;

    fn tuple_base(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassType<'db>, Self::Error>;
}

pub(in crate::types) fn dynamic_mro_with<'db, E: DynamicMroEffects<'db>>(
    db: &'db dyn Db,
    dynamic: DynamicClassLiteral<'db>,
    effects: &E,
) -> Result<Result<Mro<'db>, DynamicMroError<'db>>, E::Error> {
    effects.dynamic_checkpoint()?;
    let result = 'mro: {
        let env = &ProgramEnvironment::from_scope(dynamic.scope(db));
        let original_bases = effects.dynamic_bases(dynamic)?;

        // Convert Types to ClassBases, tracking any that fail conversion.
        let mut resolved_bases = Vec::with_capacity(original_bases.len());
        let mut invalid_bases = Vec::new();

        for (i, base_type) in original_bases.iter().enumerate() {
            match effects.convert_base(env, *base_type)? {
                Some(class_base) => resolved_bases.push(class_base),
                None => invalid_bases.push((i, *base_type)),
            }
        }

        // If there are any invalid bases, return an error.
        if !invalid_bases.is_empty() {
            break 'mro Err(
                DynamicMroErrorKind::InvalidBases(invalid_bases.into_boxed_slice())
                    .into_error(dynamic_fallback_with(db, env, dynamic, effects)?),
            );
        }

        // Check if any bases are dynamic, like `Unknown` or `Any`.
        let has_dynamic_bases = resolved_bases
            .iter()
            .any(|base| matches!(base, ClassBase::Any | ClassBase::Dynamic(_)));

        let self_base = ClassBase::Class(ClassType::NonGeneric(dynamic.into()));

        // Handle empty bases case: MRO is just [self, object].
        if resolved_bases.is_empty() {
            break 'mro Ok(Mro::from([self_base, effects.object_base(env)?]));
        }

        // Build MRO sequences and check for inheritance cycles.
        let mut seqs = vec![VecDeque::from([self_base])];
        for base in &resolved_bases {
            if effects.base_is_cycle(*base)? {
                break 'mro Err(DynamicMroErrorKind::InheritanceCycle
                    .into_error(dynamic_fallback_with(db, env, dynamic, effects)?));
            }
            seqs.push(collect_base_mro_with(db, env, *base, effects)?);
        }
        seqs.push(resolved_bases.iter().copied().collect());

        // Try C3 merge.
        if let Some(mro) = effects.c3_merge(seqs)? {
            break 'mro Ok(mro);
        }

        // C3 merge failed. Figure out why and report the most specific error.

        // Check for duplicate bases (skip dynamic bases like `Unknown` or `Any`).
        let mut seen = FxHashSet::default();
        let mut duplicates = Vec::new();
        let mut has_duplicate_dynamic_bases = false;
        for base in &resolved_bases {
            if !seen.insert(base.mro_identity(db)) {
                if matches!(base, ClassBase::Any | ClassBase::Dynamic(_)) {
                    has_duplicate_dynamic_bases = true;
                } else {
                    duplicates.push(*base);
                }
            }
        }

        if !duplicates.is_empty() {
            break 'mro Err(
                DynamicMroErrorKind::DuplicateBases(duplicates.into_boxed_slice())
                    .into_error(dynamic_fallback_with(db, env, dynamic, effects)?),
            );
        }

        // No duplicate concrete bases. If there are dynamic bases, use fallback MRO.
        if has_dynamic_bases || has_duplicate_dynamic_bases {
            Ok(dynamic_fallback_with(db, env, dynamic, effects)?)
        } else {
            Err(DynamicMroErrorKind::UnresolvableMro
                .into_error(dynamic_fallback_with(db, env, dynamic, effects)?))
        }
    };
    effects.dynamic_checkpoint()?;
    Ok(result)
}

pub(in crate::types) fn dynamic_enum_mro_with<'db, E: DynamicMroEffects<'db>>(
    db: &'db dyn Db,
    dynamic_enum: DynamicEnumLiteral<'db>,
    effects: &E,
) -> Result<Result<Mro<'db>, DynamicMroError<'db>>, E::Error> {
    effects.dynamic_checkpoint()?;
    let result = 'mro: {
        let env = &ProgramEnvironment::from_scope(dynamic_enum.scope(db));
        let self_base = ClassBase::Class(ClassType::NonGeneric(dynamic_enum.into()));

        // Convert the functional enum bases (`type=` mixin first, enum base second)
        // into `ClassBase`s, skipping any invalid mixin that we already diagnosed
        // during call inference.
        let original_bases = effects.enum_bases(dynamic_enum)?;
        let mut resolved_bases: Vec<ClassBase<'db>> = Vec::with_capacity(original_bases.len());
        for base_type in original_bases.iter().copied() {
            if let Some(base) = effects.convert_base(env, base_type)? {
                resolved_bases.push(base);
            }
        }

        // When C3 linearization fails (e.g. a bad `type=` mixin), we still need a
        // usable MRO for downstream type inference. Rather than falling back to the
        // generic `[self, Unknown, object]`, we chain the bases' MROs with
        // deduplication. This preserves type information from the known bases so that
        // member lookups can still find attributes from the mixin and enum base class.
        //
        // For example, if `Enum("Foo", ..., type=BadMixin)` fails C3, the fallback
        // produces `[Foo, BadMixin, ..., Enum, object]` (deduped), so lookups for
        // `Foo.some_method` can still resolve methods from `BadMixin` or `Enum`.
        // With `[Foo, Unknown, object]`, those lookups would silently return Unknown.
        //
        // This matches the fallback approach used for dynamic classes.
        let fallback_mro = || {
            fallback_from_bases_with(
                db,
                env,
                ClassType::NonGeneric(dynamic_enum.into()),
                resolved_bases.iter().copied().map(Ok),
                effects,
            )
        };

        // Standard C3 linearization: build sequences from each base's MRO, plus the
        // bases list itself, then merge. Static and dynamic classes use the same pattern.
        let mut seqs = vec![VecDeque::from([self_base])];
        for base in &resolved_bases {
            if effects.base_is_cycle(*base)? {
                break 'mro Err(DynamicMroError {
                    kind: DynamicMroErrorKind::InheritanceCycle,
                    fallback_mro: fallback_mro()?,
                });
            }
            seqs.push(collect_base_mro_with(db, env, *base, effects)?);
        }
        seqs.push(resolved_bases.iter().copied().collect());

        match effects.c3_merge(seqs)? {
            Some(mro) => Ok(mro),
            None => Err(DynamicMroError {
                kind: DynamicMroErrorKind::UnresolvableMro,
                fallback_mro: fallback_mro()?,
            }),
        }
    };
    effects.dynamic_checkpoint()?;
    Ok(result)
}

pub(in crate::types) fn named_tuple_mro_with<'db, E: DynamicMroEffects<'db>>(
    db: &'db dyn Db,
    named_tuple: DynamicNamedTupleLiteral<'db>,
    effects: &E,
) -> Result<Mro<'db>, E::Error> {
    effects.dynamic_checkpoint()?;
    let env = ProgramEnvironment::from_scope(named_tuple.scope(db));
    let self_base = ClassBase::Class(ClassType::NonGeneric(named_tuple.into()));
    let tuple_class = effects.tuple_base(named_tuple, &env)?;
    let mut result = vec![self_base];
    let start = effects.base_start(&env, ClassBase::Class(tuple_class))?;
    let mut cursor = BaseCursor::new(start);
    while let Some(base) = cursor.next(db, effects)? {
        result.push(base);
    }
    let result = Mro::from(result);
    effects.dynamic_checkpoint()?;
    Ok(result)
}

fn collect_base_mro_with<'db, E: DynamicMroEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    base: ClassBase<'db>,
    effects: &E,
) -> Result<VecDeque<ClassBase<'db>>, E::Error> {
    let start = effects.base_start(env, base)?;
    let mut cursor = BaseCursor::new(start);
    let mut output = VecDeque::new();
    while let Some(base) = cursor.next(db, effects)? {
        output.push_back(base);
    }
    Ok(output)
}

/// Compute a fallback MRO for a dynamic class when C3 cannot resolve its bases.
///
/// Preserves known bases even when an invalid base must be replaced with `Unknown`.
fn dynamic_fallback_with<'db, E: DynamicMroEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    dynamic: DynamicClassLiteral<'db>,
    effects: &E,
) -> Result<Mro<'db>, E::Error> {
    fallback_from_bases_with(
        db,
        env,
        ClassType::NonGeneric(dynamic.into()),
        effects.dynamic_bases(dynamic)?.iter().map(|base_type| {
            effects
                .convert_base(env, *base_type)
                .map(|base| base.unwrap_or_else(ClassBase::unknown))
        }),
        effects,
    )
}

/// Retain the first specialization of each class when C3 cannot determine a valid MRO.
///
/// Visit the bases' MROs in order, but defer `object` until every other base has been added.
/// This preserves known members while keeping class identities unique and `object` last,
/// including when subclasses inherit this fallback MRO.
fn fallback_from_bases_with<'db, E: DynamicMroEffects<'db>>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    bases: impl IntoIterator<Item = Result<ClassBase<'db>, E::Error>>,
    effects: &E,
) -> Result<Mro<'db>, E::Error> {
    let self_base = ClassBase::Class(class);
    let object_base = effects.object_base(env)?;
    let mut result = vec![self_base];
    let mut seen = FxHashSet::from_iter([self_base.mro_identity(db), object_base.mro_identity(db)]);
    for base in bases {
        let start = effects.base_start(env, base?)?;
        let mut cursor = BaseCursor::new(start);
        while let Some(item) = cursor.next(db, effects)? {
            if seen.insert(item.mro_identity(db)) {
                result.push(item);
            }
        }
    }
    result.push(object_base);
    Ok(Mro::from(result))
}

impl<'db> DynamicMroEffects<'db> for InlineMroRootEffects<'db> {
    #[inline]
    fn dynamic_checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn dynamic_bases(
        &self,
        class: DynamicClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Infallible> {
        Ok(class.explicit_bases(self.db))
    }

    #[inline]
    fn enum_bases(&self, class: DynamicEnumLiteral<'db>) -> Result<Box<[Type<'db>]>, Infallible> {
        Ok(class.explicit_bases(self.db))
    }

    #[inline]
    fn convert_base(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Infallible> {
        Ok(ClassBase::try_from_explicit_base(self.db, env, ty, None))
    }

    #[inline]
    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Infallible> {
        Ok(ClassBase::object(self.db, env))
    }

    #[inline]
    fn base_is_cycle(&self, base: ClassBase<'db>) -> Result<bool, Infallible> {
        base_has_cyclic_mro_sync(self.db, base, &InlineStaticMroEffects::new(self.db))
    }

    #[inline]
    fn base_start(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
    ) -> Result<BaseMroStart<'db>, Infallible> {
        base_mro_start_sync(
            self.db,
            env,
            base,
            None,
            &InlineBaseMroEffects::new(self.db),
        )
    }

    #[inline]
    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Infallible> {
        Ok(super::c3_merge(self.db, sequences))
    }

    #[inline]
    fn tuple_base(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassType<'db>, Infallible> {
        Ok(class.tuple_base_class(self.db, env))
    }
}
