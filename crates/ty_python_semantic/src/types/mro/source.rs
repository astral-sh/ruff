//! MRO dependencies derived from a class declaration's own base expressions.

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::ZalsaDatabase;
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::IngredientImpl;
use std::collections::VecDeque;
use std::convert::Infallible;
use ty_python_core::scope::ScopeId;

use super::base::{BaseMroStart, InlineBaseMroEffects, base_mro_start_sync, class_mro_start_sync};
use super::collection::base::{collect_start_sync, collect_start_with_root_sync};
use super::collection::{MroCollectionWork, SynchronousMroCollectionEffects};
use super::construction::{
    StaticMroFacts, StaticMroWork, SynchronousStaticMroEffects, base_has_cyclic_mro_sync,
    static_mro_cycle_sync, static_mro_sync,
};
use super::dynamic::{
    DynamicMroEffects, dynamic_enum_mro_with, dynamic_mro_with, named_tuple_mro_with,
};
use super::error::static_error_details_with;
use super::iteration::{
    MroCursor, MroDirection, MroIterationWork, SynchronousMroIterationEffects, mro_next_sync,
};
use super::root::{
    MroRootFacts, MroRootWork, MroTailRequest, SynchronousMroRootEffects,
    apply_optional_class_specialization_sync, mro_first_sync,
};
use super::{DynamicMroError, Mro, StaticMroError, StaticMroErrorKind, c3_merge};
use crate::types::class::{DynamicClassLiteral, DynamicEnumLiteral, DynamicNamedTupleLiteral};
use crate::types::class_base::conversion::ConversionEffects;
use crate::types::class_base::{ClassBase, ClassBaseConversion};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
type Interrupted = crate::types::constructor::expansion_probe::Incomplete;
#[cfg(not(test))]
type Interrupted = Infallible;

/// Traverses a declaration's own hierarchy without accepting a supplied specialization.
pub(in crate::types) struct DeclarationMroCursor<'db> {
    cursor: MroCursor<'db>,
}

impl<'db> DeclarationMroCursor<'db> {
    pub(in crate::types) fn new(class: StaticClassLiteral<'db>) -> Self {
        Self {
            cursor: MroCursor::new(class.into(), None),
        }
    }

    /// Keeps the exact metaclass derived from the owner, including its type arguments.
    pub(in crate::types) fn for_metaclass_of(
        db: &'db dyn Db,
        owner: StaticClassLiteral<'db>,
    ) -> Result<Option<Self>, Interrupted> {
        let context = Context::new(db);
        let Some(metaclass) = read_source(&context, || owner.metaclass(db).to_class_type(db))?
        else {
            return Ok(None);
        };
        let start = read_source(&context, || {
            infallible(class_mro_start_sync(
                db,
                metaclass,
                None,
                &InlineBaseMroEffects::new(db),
            ))
        })?;
        Ok(Some(Self {
            cursor: MroCursor::new(start.class, start.specialization),
        }))
    }

    pub(in crate::types) fn next(
        &mut self,
        db: &'db dyn Db,
    ) -> Result<Option<ClassBase<'db>>, Interrupted> {
        mro_next_sync(
            db,
            &mut self.cursor,
            MroDirection::Forward,
            &Context::new(db),
        )
    }
}

/// Only the declaration query can create the context that interprets source-derived aliases.
pub(in crate::types) fn compute<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Result<Mro<'db>, StaticMroError<'db>> {
    let context = Context::new(db);
    recover(static_mro_sync(db, class, None, &context))
}

pub(in crate::types) fn cycle<'db>(
    db: &'db dyn Db,
    class: StaticClassLiteral<'db>,
) -> Result<Mro<'db>, Box<StaticMroError<'db>>> {
    let context = Context::new(db);
    recover(static_mro_cycle_sync(db, class, None, &context).map(Err)).map_err(Box::new)
}

fn recover<E>(result: Result<Result<Mro<'_>, E>, Interrupted>) -> Result<Mro<'_>, E> {
    match result {
        Ok(result) => result,
        #[cfg(test)]
        Err(_) => Ok(Mro::incomplete()),
        #[cfg(not(test))]
        Err(never) => match never {},
    }
}

fn recover_mro(result: Result<Mro<'_>, Interrupted>) -> Mro<'_> {
    match result {
        Ok(result) => result,
        #[cfg(test)]
        Err(_) => Mro::incomplete(),
        #[cfg(not(test))]
        Err(never) => match never {},
    }
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

/// Resolve an exact alias derived from a declaration's base expressions. Specialized dependencies
/// need their own cycle seeds even when the class query already has a provisional successful MRO.
#[salsa::tracked(configuration = (pub(in crate::types) SourceAliasMroConfiguration),
    attempt = ReturnOnly,
    returns(as_ref),
    cycle_initial = |db, _, alias: GenericAlias<'db>| {
        let context = Context::new(db);
        recover(static_mro_cycle_sync(db, alias.origin(db), Some(alias.specialization(db)), &context).map(Err))
            .map_err(Box::new)
    },
    heap_size = ruff_memory_usage::heap_size
)]
fn source_alias_mro<'db>(
    db: &'db dyn Db,
    alias: GenericAlias<'db>,
) -> Result<Mro<'db>, Box<StaticMroError<'db>>> {
    let context = Context::new(db);
    recover(static_mro_sync(
        db,
        alias.origin(db),
        Some(alias.specialization(db)),
        &context,
    ))
    .map_err(Box::new)
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn source_alias_mro_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<SourceAliasMroConfiguration> {
    source_alias_mro::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(
    attempt = ReturnOnly,
    returns(ref),
    cycle_initial = |db, _, literal: DynamicClassLiteral<'db>| {
        let context = Context::new(db);
        let env = ProgramEnvironment::from_scope(literal.scope(db));
        recover(context.dynamic_seed(&env, ClassType::NonGeneric(literal.into())).map(Ok))
    },
    heap_size = ruff_memory_usage::heap_size
)]
fn source_dynamic_mro<'db>(
    db: &'db dyn Db,
    literal: DynamicClassLiteral<'db>,
) -> Result<Mro<'db>, DynamicMroError<'db>> {
    recover(dynamic_mro_with(db, literal, &Context::new(db)))
}

#[salsa::tracked(
    attempt = ReturnOnly,
    returns(ref),
    cycle_initial = |db, _, literal: DynamicEnumLiteral<'db>| {
        let context = Context::new(db);
        let env = ProgramEnvironment::from_scope(literal.scope(db));
        recover(context.dynamic_seed(&env, ClassType::NonGeneric(literal.into())).map(Ok))
    },
    heap_size = ruff_memory_usage::heap_size
)]
fn source_enum_mro<'db>(
    db: &'db dyn Db,
    literal: DynamicEnumLiteral<'db>,
) -> Result<Mro<'db>, DynamicMroError<'db>> {
    recover(dynamic_enum_mro_with(db, literal, &Context::new(db)))
}

#[salsa::tracked(
    attempt = ReturnOnly,
    returns(ref),
    cycle_initial = |db, _, literal: DynamicNamedTupleLiteral<'db>| {
        let context = Context::new(db);
        let env = ProgramEnvironment::from_scope(literal.scope(db));
        recover_mro(read_source(&context, || Mro::from_error(db, &env, ClassType::NonGeneric(literal.into()))))
    },
    heap_size = ruff_memory_usage::heap_size
)]
fn source_named_tuple_mro<'db>(
    db: &'db dyn Db,
    literal: DynamicNamedTupleLiteral<'db>,
) -> Mro<'db> {
    recover_mro(named_tuple_mro_with(db, literal, &Context::new(db)))
}

#[derive(Clone, Copy)]
struct DeclaredBase<'db> {
    base: ClassBase<'db>,
    additional: Option<Specialization<'db>>,
}

struct Context<'db> {
    db: &'db dyn Db,
}

impl<'db> Context<'db> {
    fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }

    fn static_mro(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Result<&'db Mro<'db>, &'db StaticMroError<'db>>, Interrupted> {
        read_source(self, || match specialization {
            None => class.try_mro(self.db, None),
            Some(specialization) => {
                let alias = GenericAlias::new(self.db, class, specialization);
                source_alias_mro(self.db, alias).map_err(Box::as_ref)
            }
        })
    }

    fn start(
        &self,
        env: &ProgramEnvironment<'db>,
        base: DeclaredBase<'db>,
    ) -> Result<BaseMroStart<'db>, Interrupted> {
        read_source(self, || {
            infallible(base_mro_start_sync(
                self.db,
                env,
                base.base,
                base.additional,
                &InlineBaseMroEffects::new(self.db),
            ))
        })
    }

    fn dynamic_seed(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Result<Mro<'db>, Interrupted> {
        let object = SynchronousStaticMroEffects::object_base(self, env)?;
        Ok(Mro::from([ClassBase::Class(class), object]))
    }
}

impl SourceReadControl for Context<'_> {
    type Error = Interrupted;

    fn check(&self) -> Result<(), Interrupted> {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::active() {
            crate::types::constructor::expansion_probe::continue_work(self.db)?;
        } else if salsa::attempt_probe::is_incomplete(self.db) {
            return Err(Interrupted::Interrupted);
        }
        Ok(())
    }
}

impl super::construction::sealed::Sealed for Context<'_> {}

impl crate::types::class_base::conversion::sealed::Sealed for Context<'_> {}

impl<'db> ConversionEffects<'db> for Context<'db> {
    fn default_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<ClassBase<'db>, Interrupted> {
        mro_first_sync(db, class, None, self)
    }
}

impl<'db> StaticMroFacts<'db> for Context<'db> {
    type Error = Interrupted;
}

impl<'db> SynchronousStaticMroEffects<'db> for Context<'db> {
    fn body_scope(&self, class: StaticClassLiteral<'db>) -> Result<ScopeId<'db>, Interrupted> {
        Ok(class.body_scope(self.db))
    }

    fn is_object(&self, class: ClassType<'db>) -> Result<bool, Interrupted> {
        Ok(crate::types::mro::field_reads::MroFieldReads::new(self.db).is_object(class))
    }

    fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Interrupted> {
        Ok(crate::types::mro::field_reads::MroFieldReads::new(self.db).static_class_literal(class))
    }

    fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> Result<&[Type<'db>], Interrupted> {
        read_source(self, || class.explicit_bases(self.db))
    }

    fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Interrupted> {
        read_source(self, || class.has_pep_695_type_params(self.db))
    }

    fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        _index: usize,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Interrupted> {
        ClassBaseConversion::from_explicit_type(ty).resolve_with(
            self.db,
            env,
            Some(class.into()),
            self,
        )
    }

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Interrupted> {
        read_source(self, || ClassBase::object(self.db, env))
    }

    fn checkpoint(&self, _work: StaticMroWork) -> Result<(), Interrupted> {
        self.check()
    }

    fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Interrupted> {
        apply_optional_class_specialization_sync(self.db, class, specialization, self)
    }

    fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Interrupted> {
        Ok(self
            .static_mro(class, specialization)?
            .is_err_and(StaticMroError::is_cycle))
    }

    fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Interrupted> {
        let start = self.start(env, DeclaredBase { base, additional })?;
        collect_start_with_root_sync(self.db, root, start, self)
    }

    fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Interrupted> {
        let start = self.start(env, DeclaredBase { base, additional })?;
        collect_start_sync(self.db, start, self)
    }

    fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Interrupted> {
        read_source(self, || {
            base.apply_optional_specialization(self.db, specialization)
        })
    }

    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Interrupted> {
        read_source(self, || c3_merge(self.db, sequences))
    }

    fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Interrupted> {
        read_source(self, || kind.into_mro_error(self.db, env, class))
    }

    fn failed_c3(
        &self,
        env: &ProgramEnvironment<'db>,
        class_literal: StaticClassLiteral<'db>,
        class: ClassType<'db>,
        original_bases: &[Type<'db>],
        resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Interrupted> {
        static_error_details_with(
            self.db,
            env,
            class_literal,
            class,
            original_bases,
            resolved_bases,
            self,
        )
    }
}

impl super::root::sealed::Sealed for Context<'_> {}

impl<'db> MroRootFacts<'db> for Context<'db> {
    type Error = Interrupted;
}

impl<'db> SynchronousMroRootEffects<'db> for Context<'db> {
    fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Interrupted> {
        read_source(self, || class.generic_context(self.db))
    }

    fn checkpoint(&self, _work: MroRootWork) -> Result<(), Interrupted> {
        self.check()
    }

    fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Interrupted> {
        crate::types::class::default_class_specialization_with(self.db, class, self)
    }

    fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Interrupted> {
        read_source(self, || {
            specialization.tuple_runtime_element_specialization(self.db)
        })
    }
}

impl<'db> SynchronousMroIterationEffects<'db> for Context<'db> {
    fn iteration_checkpoint(&self, _work: MroIterationWork) -> Result<(), Interrupted> {
        self.check()
    }

    fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Interrupted> {
        Ok(match request {
            MroTailRequest::Static(class, specialization) => self
                .static_mro(class, specialization)?
                .unwrap_or_else(StaticMroError::fallback_mro),
            MroTailRequest::Dynamic(literal) => {
                read_source(self, || source_dynamic_mro(self.db, literal))?
                    .as_ref()
                    .unwrap_or_else(DynamicMroError::fallback_mro)
            }
            MroTailRequest::DynamicEnum(literal) => {
                read_source(self, || source_enum_mro(self.db, literal))?
                    .as_ref()
                    .unwrap_or_else(DynamicMroError::fallback_mro)
            }
            MroTailRequest::DynamicNamedTuple(literal) => {
                read_source(self, || source_named_tuple_mro(self.db, literal))?
            }
            MroTailRequest::DynamicTypedDict(literal) => {
                read_source(self, || literal.mro(self.db))?
            }
        })
    }
}

impl<'db> SynchronousMroCollectionEffects<'db> for Context<'db> {
    fn collection_checkpoint(&self, _work: MroCollectionWork) -> Result<(), Interrupted> {
        self.check()
    }
}

impl<'db> DynamicMroEffects<'db> for Context<'db> {
    fn dynamic_checkpoint(&self) -> Result<(), Interrupted> {
        self.check()
    }

    fn dynamic_bases(
        &self,
        literal: DynamicClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Interrupted> {
        read_source(self, || literal.explicit_bases(self.db))
    }

    fn enum_bases(
        &self,
        literal: DynamicEnumLiteral<'db>,
    ) -> Result<Box<[Type<'db>]>, Interrupted> {
        read_source(self, || literal.explicit_bases(self.db))
    }

    fn convert_base(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Interrupted> {
        ClassBaseConversion::from_explicit_type(ty).resolve_with(self.db, env, None, self)
    }

    fn object_base(&self, env: &ProgramEnvironment<'db>) -> Result<ClassBase<'db>, Interrupted> {
        SynchronousStaticMroEffects::object_base(self, env)
    }

    fn base_is_cycle(&self, base: ClassBase<'db>) -> Result<bool, Interrupted> {
        base_has_cyclic_mro_sync(self.db, base, self)
    }

    fn base_start(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
    ) -> Result<BaseMroStart<'db>, Interrupted> {
        self.start(
            env,
            DeclaredBase {
                base,
                additional: None,
            },
        )
    }

    fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Interrupted> {
        SynchronousStaticMroEffects::c3_merge(self, sequences)
    }

    fn tuple_base(
        &self,
        literal: DynamicNamedTupleLiteral<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassType<'db>, Interrupted> {
        read_source(self, || literal.tuple_base_class(self.db, env))
    }
}
