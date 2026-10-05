//! Independently supported declaration inputs for canonical static MRO construction.

use super::{
    ClassInput, DeclarationKey, Fact, MissingDeclaration, PreparationError, PreparedDeclarations,
};
use crate::types::class_base::ClassBase;
use crate::types::{GenericContext, StaticClassLiteral, Type};
use crate::{Db, Program, ProgramEnvironment};
use salsa::prepared_source_probe::CaptureError;
use ty_python_core::ProgramFile;

pub(super) struct MroConstructionFacts<'db> {
    pub(super) context: Fact<Option<GenericContext<'db>>>,
    pub(super) explicit_bases: Fact<Box<[Type<'db>]>>,
    pub(super) has_pep_695_type_params: Fact<bool>,
    pub(super) converted_explicit_bases: Box<[Option<Fact<Option<ClassBase<'db>>>>]>,
}

impl<'db> PreparedDeclarations<'db> {
    pub(in crate::types::callable::scheduled_probe) fn prepare_construction_only(
        db: &'db dyn Db,
        file: ProgramFile<'db>,
        origins: impl IntoIterator<Item = StaticClassLiteral<'db>>,
    ) -> Result<Self, PreparationError<'db>> {
        let mut prepared = Self::empty(db, file);
        prepared.prepare_construction_origins(origins)?;
        Ok(prepared)
    }

    pub(in crate::types::callable::scheduled_probe) fn prepare_construction_origins(
        &mut self,
        origins: impl IntoIterator<Item = StaticClassLiteral<'db>>,
    ) -> Result<(), PreparationError<'db>> {
        for class in origins {
            self.prepare_construction(class)?;
        }
        self.check_construction_stamp()
    }

    pub(in crate::types::callable::scheduled_probe) fn prepare_construction(
        &mut self,
        class: StaticClassLiteral<'db>,
    ) -> Result<(), PreparationError<'db>> {
        self.check_construction_stamp()?;
        let db = self.db;
        let env = ProgramEnvironment::from_scope(class.body_scope(db));
        let program = self.file.program(db);
        if env.program(db) != program {
            return Err(PreparationError::ProgramDomain(class));
        }
        if self.mro_construction.contains_key(&class) {
            return Ok(());
        }

        let context = match self.classes.get(&class) {
            Some(facts) => facts.context.stored(facts.context.value),
            None => Fact::read(db, DeclarationKey::Context(class), || {
                class.generic_context(db)
            })?,
        };
        if context.stamp != self.stamp {
            return Err(PreparationError::Observation(
                DeclarationKey::Context(class),
                CaptureError::ChangedDatabaseStamp,
            ));
        }
        let explicit_bases: Fact<Box<[Type<'db>]>> =
            context.derive(db, DeclarationKey::ExplicitBases(class), || {
                class.explicit_bases(db).into()
            })?;
        let (facts, object_base) =
            self.capture_construction_remainder(class, &env, context, explicit_bases)?;
        self.publish_construction(class, facts, object_base)
    }

    pub(super) fn capture_construction_remainder(
        &self,
        class: StaticClassLiteral<'db>,
        env: &ProgramEnvironment<'db>,
        context: Fact<Option<GenericContext<'db>>>,
        explicit_bases: Fact<Box<[Type<'db>]>>,
    ) -> Result<(MroConstructionFacts<'db>, Option<Fact<ClassBase<'db>>>), PreparationError<'db>>
    {
        let db = self.db;
        let has_pep_695_type_params = context.derive(
            db,
            DeclarationKey::ClassInput(class, ClassInput::HasPep695TypeParams),
            || class.has_pep_695_type_params(db),
        )?;
        let converted_explicit_bases = explicit_bases
            .value
            .iter()
            .copied()
            .enumerate()
            .map(|(index, raw)| {
                explicit_bases
                    .derive(
                        db,
                        DeclarationKey::ConvertedExplicitBase(class, index),
                        || ClassBase::try_from_explicit_base(db, env, raw, Some(class.into())),
                    )
                    .map(Some)
            })
            .collect::<Result<Box<[_]>, _>>()?;
        let object_base = if self.object_base.is_none() {
            Some(context.derive(
                db,
                DeclarationKey::ObjectBase(self.file.program(db)),
                || ClassBase::object(db, env),
            )?)
        } else {
            None
        };
        let facts = MroConstructionFacts {
            context,
            explicit_bases,
            has_pep_695_type_params,
            converted_explicit_bases,
        };
        Ok((facts, object_base))
    }

    pub(super) fn publish_construction(
        &mut self,
        class: StaticClassLiteral<'db>,
        facts: MroConstructionFacts<'db>,
        object_base: Option<Fact<ClassBase<'db>>>,
    ) -> Result<(), PreparationError<'db>> {
        // Capture failure or cancellation must not expose a partial packet or its object fact.
        self.check_construction_stamp()?;
        self.mro_construction.insert(class, facts);
        if let Some(object_base) = object_base {
            self.object_base = Some(object_base);
        }
        Ok(())
    }

    fn check_construction_stamp(&self) -> Result<(), PreparationError<'db>> {
        if self.stamp.belongs_to(self.db) {
            Ok(())
        } else {
            Err(PreparationError::Observation(
                DeclarationKey::File(self.file),
                CaptureError::ChangedDatabaseStamp,
            ))
        }
    }

    pub(in crate::types::callable::scheduled_probe) fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&[Type<'db>], MissingDeclaration<'db>> {
        self.mro_construction
            .get(&class)
            .map(|facts| facts.explicit_bases.value.as_ref())
            .ok_or(MissingDeclaration(DeclarationKey::ExplicitBases(class)))
    }

    pub(in crate::types::callable::scheduled_probe) fn has_pep_695_type_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, MissingDeclaration<'db>> {
        self.mro_construction
            .get(&class)
            .map(|facts| facts.has_pep_695_type_params.value)
            .ok_or(MissingDeclaration(DeclarationKey::ClassInput(
                class,
                ClassInput::HasPep695TypeParams,
            )))
    }

    pub(in crate::types::callable::scheduled_probe) fn converted_explicit_base(
        &self,
        class: StaticClassLiteral<'db>,
        index: usize,
    ) -> Result<Option<ClassBase<'db>>, MissingDeclaration<'db>> {
        self.mro_construction
            .get(&class)
            .and_then(|facts| facts.converted_explicit_bases.get(index))
            .and_then(Option::as_ref)
            .map(|fact| fact.value)
            .ok_or(MissingDeclaration(DeclarationKey::ConvertedExplicitBase(
                class, index,
            )))
    }

    pub(in crate::types::callable::scheduled_probe) fn object_base(
        &self,
        program: Program<'db>,
    ) -> Result<ClassBase<'db>, MissingDeclaration<'db>> {
        self.object_base
            .as_ref()
            .filter(|_| self.file.program(self.db) == program)
            .map(|fact| fact.value)
            .ok_or(MissingDeclaration(DeclarationKey::ObjectBase(program)))
    }
}
