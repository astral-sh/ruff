//! Instance construction within an installed semantic attempt.

use std::future::{Future, ready};
use std::task::Poll;

use super::effects::{InstanceEffects, InstanceWork};
use crate::types::constructor::expansion_probe::{self, Incomplete};
use crate::types::generics::Specialization;
use crate::types::signatures::effects::{sealed, try_poll_immediate};
use crate::types::source_read::{SourceReadControl, read_source};
use crate::types::tuple::TupleType;
use crate::types::{ClassLiteral, ClassType, KnownClass, StaticClassLiteral, Type};
use crate::{Db, ProgramEnvironment};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod initializer_probe;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedInstanceOperation {
    UncontrolledMro,
    DynamicInheritance,
    TupleNormalization,
    MetaclassAttributeClassification,
    InvalidSuspension,
}

pub(in crate::types) struct AttemptInstanceEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> AttemptInstanceEffects<'db> {
    pub(in crate::types) fn new(db: &'db dyn Db) -> Self {
        Self { db }
    }

    fn unsupported<T>(&self, operation: UnsupportedInstanceOperation) -> Result<T, Incomplete> {
        self.check()?;
        Err(expansion_probe::refuse(
            self.db,
            Incomplete::UnsupportedInstanceOperation(operation),
        ))
    }

    #[cfg_attr(test, track_caller)]
    fn admit(&self, units: usize) -> Result<(), Incomplete> {
        expansion_probe::charge_work(self.db, units)?;
        if !expansion_probe::mro_effects_enabled() {
            return self.unsupported(UnsupportedInstanceOperation::UncontrolledMro);
        }
        Ok(())
    }
}

impl SourceReadControl for AttemptInstanceEffects<'_> {
    type Error = Incomplete;

    fn check(&self) -> Result<(), Incomplete> {
        expansion_probe::continue_work(self.db)
    }
}

impl sealed::Sealed for AttemptInstanceEffects<'_> {}

impl<'db> InstanceEffects<'db> for AttemptInstanceEffects<'db> {
    type Error = Incomplete;

    async fn checkpoint(&self, work: InstanceWork) -> Result<(), Incomplete> {
        #[cfg(test)]
        let _charge = expansion_probe::charge_ledger::scope(&work);
        self.admit(1)
    }

    fn class_literal_and_specialization(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> impl Future<Output = Result<(ClassLiteral<'db>, Option<Specialization<'db>>), Incomplete>>
    {
        ready(
            self.admit(2)
                .and_then(|()| read_source(self, || class.class_literal_and_specialization(db))),
        )
    }

    fn known_class(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<Option<KnownClass>, Incomplete>> {
        ready(
            self.admit(1)
                .and_then(|()| read_source(self, || class.known(db))),
        )
    }

    fn is_typed_dict(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Incomplete>> {
        ready(
            self.admit(1)
                .and_then(|()| read_source(self, || class.is_typed_dict(db))),
        )
    }

    fn is_protocol(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Incomplete>> {
        // Protocol classification examines at most the last three explicit bases.
        ready(
            self.admit(4)
                .and_then(|()| read_source(self, || class.is_protocol(db))),
        )
    }

    fn inherits_from_explicit_any(
        &self,
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> impl Future<Output = Result<bool, Incomplete>> {
        ready(self.admit(1).and_then(|()| {
            if !matches!(class, ClassLiteral::Static(_)) {
                return self.unsupported(UnsupportedInstanceOperation::DynamicInheritance);
            }
            read_source(self, || class.inherits_from_explicit_any(db))
        }))
    }

    fn tuple(
        &self,
        _: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        _: Option<Specialization<'db>>,
    ) -> impl Future<Output = Result<TupleType<'db>, Incomplete>> {
        ready(self.unsupported(UnsupportedInstanceOperation::TupleNormalization))
    }
}

pub(in crate::types) fn instance<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
) -> Result<Type<'db>, Incomplete> {
    let effects = AttemptInstanceEffects::new(db);
    match try_poll_immediate(Type::instance_with(db, env, &effects, class)) {
        Poll::Ready(result) => result,
        Poll::Pending => effects.unsupported(UnsupportedInstanceOperation::InvalidSuspension),
    }
}
