//! Applies class-base substitutions before their optional, separate materialization pass.

use crate::types::generics::{GenericContext, Specialization};
use crate::types::mapping::OwnedTypeMapping;
use crate::types::mapping::effects::{MappingEffects, SynchronousMappingEffects};
#[cfg(any(test, feature = "experimental-analysis"))]
use crate::types::storage_quote::StorageQuote;
use crate::types::{
    ApplyTypeMappingVisitor, ClassBase, ClassType, GenericAlias, MaterializationKind, TypeContext,
};
use crate::{Db, Program, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum ClassBaseMapping<'db> {
    Specialize(Specialization<'db>),
    Materialize(MaterializationKind),
}

impl<'db> ClassBaseMapping<'db> {
    pub(in crate::types) fn into_owned(self) -> OwnedTypeMapping<'db, 'db> {
        match self {
            Self::Specialize(specialization) => OwnedTypeMapping::Specialization {
                specialization,
                specialize_self_domain: false,
                materialization_kind: None,
            },
            Self::Materialize(kind) => OwnedTypeMapping::Materialize(kind),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types) enum ClassBaseSpecializationWork {
    OptionalDispatch,
    MappingRequest,
    BaseDispatch,
    Publish,
}

impl ClassBaseSpecializationWork {
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) const fn quote(self) -> StorageQuote {
        match self {
            Self::OptionalDispatch => StorageQuote {
                work: 3,
                bytes: 2 * size_of::<ClassBase<'_>>() + size_of::<Option<Specialization<'_>>>(),
            },
            Self::MappingRequest | Self::BaseDispatch => StorageQuote {
                work: 4,
                bytes: 2 * size_of::<ClassBase<'_>>() + 2 * size_of::<ClassBaseMapping<'_>>(),
            },
            Self::Publish => StorageQuote {
                work: 1,
                bytes: 2 * size_of::<ClassBase<'_>>(),
            },
        }
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassBaseSpecializationEffects)]
    pub(in crate::types) trait ClassBaseSpecializationEffects<'db> {
        type Error;
        type Environment;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ClassBaseSpecializationWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn generic_context(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn program(&self, db: &'db dyn Db, context: GenericContext<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error>;
        #[operation(source)]
        async fn materialization_kind(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(child)]
        async fn map_base_fresh(&self, db: &'db dyn Db, base: ClassBase<'db>, env: &Self::Environment, mapping: ClassBaseMapping<'db>) -> Result<ClassBase<'db>, Self::Error>;
    }

    #[synchronous(SynchronousClassBaseMappingEffects)]
    pub(in crate::types) trait ClassBaseMappingEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self, work: ClassBaseSpecializationWork) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn map_alias(&self, db: &'db dyn Db, alias: GenericAlias<'db>, mapping: ClassBaseMapping<'db>, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Result<GenericAlias<'db>, Self::Error>;
    }

    #[synchronous(apply_optional_base_specialization_sync)]
    #[capabilities(effects = ClassBaseSpecializationEffects)]
    #[passive_values(ClassBaseSpecializationWork::OptionalDispatch, ClassBaseSpecializationWork::MappingRequest, ClassBaseSpecializationWork::Publish, ClassBaseMapping::Specialize, ClassBaseMapping::Materialize)]
    pub(in crate::types) async fn apply_optional_base_specialization_with<'db, E: ClassBaseSpecializationEffects<'db>>(
        db: &'db dyn Db,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
        effects: &E,
    ) -> Result<ClassBase<'db>, E::Error> {
        effects.checkpoint(ClassBaseSpecializationWork::OptionalDispatch).await?;
        let Some(specialization) = specialization else {
            effects.checkpoint(ClassBaseSpecializationWork::Publish).await?;
            return Ok(base);
        };
        let context = effects.generic_context(db, specialization).await?;
        let program = effects.program(db, context).await?;
        let env = effects.environment(program).await?;
        effects.checkpoint(ClassBaseSpecializationWork::MappingRequest).await?;
        let mapped = effects.map_base_fresh(db, base, &env, ClassBaseMapping::Specialize(specialization)).await?;
        let result = match effects.materialization_kind(db, specialization).await? {
            None => mapped,
            Some(kind) => {
                effects.checkpoint(ClassBaseSpecializationWork::MappingRequest).await?;
                effects.map_base_fresh(db, mapped, &env, ClassBaseMapping::Materialize(kind)).await?
            }
        };
        effects.checkpoint(ClassBaseSpecializationWork::Publish).await?;
        Ok(result)
    }

    #[synchronous(map_class_base_sync)]
    #[capabilities(effects = ClassBaseMappingEffects)]
    #[passive_values(ClassBaseSpecializationWork::BaseDispatch, ClassBaseSpecializationWork::Publish, ClassBase::Class, ClassType::Generic)]
    pub(in crate::types) async fn map_class_base_with<'db, E: ClassBaseMappingEffects<'db>>(
        db: &'db dyn Db,
        base: ClassBase<'db>,
        mapping: ClassBaseMapping<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<ClassBase<'db>, E::Error> {
        effects.checkpoint(ClassBaseSpecializationWork::BaseDispatch).await?;
        let result = match base {
            ClassBase::Class(ClassType::Generic(alias)) => {
                ClassBase::Class(ClassType::Generic(effects.map_alias(db, alias, mapping, visitor).await?))
            }
            ClassBase::Class(ClassType::NonGeneric(_))
            | ClassBase::Any
            | ClassBase::Dynamic(_)
            | ClassBase::Divergent(_)
            | ClassBase::Generic
            | ClassBase::Protocol
            | ClassBase::TypedDict(_) => base,
        };
        effects.checkpoint(ClassBaseSpecializationWork::Publish).await?;
        Ok(result)
    }
}

pub(super) struct MappingClassBaseEffects<'a, E>(pub(super) &'a E);

impl<'db, E: MappingEffects<'db>> ClassBaseSpecializationEffects<'db>
    for MappingClassBaseEffects<'_, E>
{
    type Error = E::Error;
    type Environment = ProgramEnvironment<'db>;

    async fn checkpoint(&self, _work: ClassBaseSpecializationWork) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }

    async fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<Program<'db>, Self::Error> {
        Ok(context.program(db))
    }

    async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }

    async fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }

    async fn map_base_fresh(
        &self,
        db: &'db dyn Db,
        base: ClassBase<'db>,
        env: &Self::Environment,
        mapping: ClassBaseMapping<'db>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        map_class_base_with(db, base, mapping, &ApplyTypeMappingVisitor::new(env), self).await
    }
}

impl<'db, E: MappingEffects<'db>> ClassBaseMappingEffects<'db> for MappingClassBaseEffects<'_, E> {
    type Error = E::Error;

    async fn checkpoint(&self, _work: ClassBaseSpecializationWork) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn map_alias(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: ClassBaseMapping<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<GenericAlias<'db>, Self::Error> {
        alias
            .apply_type_mapping_with(
                db,
                &mapping.into_owned().into_mapping(),
                TypeContext::default(),
                visitor,
                self.0,
            )
            .await
    }
}

impl<'db, E: SynchronousMappingEffects<'db>> SynchronousClassBaseSpecializationEffects<'db>
    for MappingClassBaseEffects<'_, E>
{
    type Error = E::Error;
    type Environment = ProgramEnvironment<'db>;

    fn checkpoint(&self, _work: ClassBaseSpecializationWork) -> Result<(), Self::Error> {
        Ok(())
    }

    fn generic_context(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<GenericContext<'db>, Self::Error> {
        Ok(specialization.generic_context(db))
    }

    fn program(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> Result<Program<'db>, Self::Error> {
        Ok(context.program(db))
    }

    fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error> {
        Ok(ProgramEnvironment::from_program(program))
    }

    fn materialization_kind(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Option<MaterializationKind>, Self::Error> {
        Ok(specialization.materialization_kind(db))
    }

    fn map_base_fresh(
        &self,
        db: &'db dyn Db,
        base: ClassBase<'db>,
        env: &Self::Environment,
        mapping: ClassBaseMapping<'db>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        map_class_base_sync(db, base, mapping, &ApplyTypeMappingVisitor::new(env), self)
    }
}

impl<'db, E: SynchronousMappingEffects<'db>> SynchronousClassBaseMappingEffects<'db>
    for MappingClassBaseEffects<'_, E>
{
    type Error = E::Error;

    fn checkpoint(&self, _work: ClassBaseSpecializationWork) -> Result<(), Self::Error> {
        Ok(())
    }

    fn map_alias(
        &self,
        db: &'db dyn Db,
        alias: GenericAlias<'db>,
        mapping: ClassBaseMapping<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Result<GenericAlias<'db>, Self::Error> {
        alias.apply_type_mapping_sync(
            db,
            &mapping.into_owned().into_mapping(),
            TypeContext::default(),
            visitor,
            self.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::types::{ClassLiteral, DivergentType, DynamicType, Type, TypingModule};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Step {
        Context,
        Program,
        Environment,
        Specialize,
        MaterializationKind,
        Materialize,
        Alias,
    }

    struct Recording<'db> {
        specialization: Specialization<'db>,
        program: Program<'db>,
        steps: RefCell<Vec<Step>>,
        refuse: Option<Step>,
    }

    impl Recording<'_> {
        fn record(&self, step: Step) -> Result<(), Step> {
            self.steps.borrow_mut().push(step);
            if self.refuse == Some(step) {
                Err(step)
            } else {
                Ok(())
            }
        }
    }

    impl<'db> ClassBaseSpecializationEffects<'db> for Recording<'db> {
        type Error = Step;
        type Environment = ProgramEnvironment<'db>;

        async fn checkpoint(&self, _work: ClassBaseSpecializationWork) -> Result<(), Step> {
            Ok(())
        }

        async fn generic_context(
            &self,
            db: &'db dyn Db,
            specialization: Specialization<'db>,
        ) -> Result<GenericContext<'db>, Step> {
            self.record(Step::Context)?;
            assert_eq!(specialization, self.specialization);
            Ok(specialization.generic_context(db))
        }

        async fn program(
            &self,
            db: &'db dyn Db,
            context: GenericContext<'db>,
        ) -> Result<Program<'db>, Step> {
            self.record(Step::Program)?;
            assert_eq!(context, self.specialization.generic_context(db));
            Ok(context.program(db))
        }

        async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Step> {
            self.record(Step::Environment)?;
            assert_eq!(program, self.program);
            Ok(ProgramEnvironment::from_program(program))
        }

        async fn materialization_kind(
            &self,
            db: &'db dyn Db,
            specialization: Specialization<'db>,
        ) -> Result<Option<MaterializationKind>, Step> {
            self.record(Step::MaterializationKind)?;
            assert_eq!(specialization, self.specialization);
            Ok(specialization.materialization_kind(db))
        }

        async fn map_base_fresh(
            &self,
            db: &'db dyn Db,
            base: ClassBase<'db>,
            env: &Self::Environment,
            mapping: ClassBaseMapping<'db>,
        ) -> Result<ClassBase<'db>, Step> {
            assert_eq!(env.program(db), self.program);
            match mapping {
                ClassBaseMapping::Specialize(specialization) => {
                    assert_eq!(specialization, self.specialization);
                    self.record(Step::Specialize)?;
                }
                ClassBaseMapping::Materialize(kind) => {
                    assert_eq!(Some(kind), self.specialization.materialization_kind(db));
                    self.record(Step::Materialize)?;
                }
            }
            map_class_base_with(db, base, mapping, &ApplyTypeMappingVisitor::new(env), self).await
        }
    }

    impl<'db> ClassBaseMappingEffects<'db> for Recording<'db> {
        type Error = Step;

        async fn checkpoint(&self, _work: ClassBaseSpecializationWork) -> Result<(), Step> {
            Ok(())
        }

        async fn map_alias(
            &self,
            db: &'db dyn Db,
            _alias: GenericAlias<'db>,
            _mapping: ClassBaseMapping<'db>,
            visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        ) -> Result<GenericAlias<'db>, Step> {
            assert_eq!(visitor.env.program(db), self.program);
            self.record(Step::Alias)?;
            Err(Step::Alias)
        }
    }

    fn declared_base<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<ClassBase<'db>> {
        let file = system_path_to_file(db, "/src/base.py")?;
        let Type::ClassLiteral(ClassLiteral::Static(class)) =
            global_symbol(db, db.program_file(file), name)
                .place
                .expect_type()
        else {
            anyhow::bail!("fixture class {name} was not inferred");
        };
        Ok(ClassBase::Class(class.default_specialization(db)))
    }

    #[test]
    fn identity_bases_keep_the_metadata_and_materialization_order() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/base.py", "class Plain: ...\n")
            .build()?;
        let env = db.program_environment();
        let context = GenericContext::from_typevar_instances(&db, &env, []);
        let bases = [
            declared_base(&db, "Plain")?,
            ClassBase::Any,
            ClassBase::Dynamic(DynamicType::Any),
            ClassBase::Dynamic(DynamicType::Unknown),
            ClassBase::Divergent(DivergentType::new(salsa::plumbing::Id::from_bits(1))),
            ClassBase::Generic,
            ClassBase::Protocol,
            ClassBase::TypedDict(TypingModule::Typing),
            ClassBase::TypedDict(TypingModule::TypingExtensions),
        ];
        for kind in [
            None,
            Some(MaterializationKind::Top),
            Some(MaterializationKind::Bottom),
        ] {
            let specialization = Specialization::new(&db, context, &[][..], kind, None);
            for base in bases {
                let effects = Recording {
                    specialization,
                    program: env.program(&db),
                    steps: RefCell::default(),
                    refuse: None,
                };
                assert_eq!(
                    try_poll_immediate(apply_optional_base_specialization_with(
                        &db, base, None, &effects,
                    )),
                    Poll::Ready(Ok(base)),
                );
                assert!(effects.steps.borrow().is_empty());
                assert_eq!(
                    try_poll_immediate(apply_optional_base_specialization_with(
                        &db,
                        base,
                        Some(specialization),
                        &effects,
                    )),
                    Poll::Ready(Ok(base)),
                );
                let mut expected = vec![
                    Step::Context,
                    Step::Program,
                    Step::Environment,
                    Step::Specialize,
                    Step::MaterializationKind,
                ];
                if kind.is_some() {
                    expected.push(Step::Materialize);
                }
                assert_eq!(*effects.steps.borrow(), expected);
                assert_eq!(
                    base.apply_optional_specialization(&db, Some(specialization)),
                    base
                );
            }
        }
        Ok(())
    }

    #[test]
    fn refusal_stops_before_later_metadata_and_mapping_passes() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let env = db.program_environment();
        let context = GenericContext::from_typevar_instances(&db, &env, []);
        let specialization =
            Specialization::new(&db, context, &[][..], Some(MaterializationKind::Top), None);
        let steps = [
            Step::Context,
            Step::Program,
            Step::Environment,
            Step::Specialize,
            Step::MaterializationKind,
            Step::Materialize,
        ];
        for (index, refuse) in steps.into_iter().enumerate() {
            let effects = Recording {
                specialization,
                program: env.program(&db),
                steps: RefCell::default(),
                refuse: Some(refuse),
            };
            assert_eq!(
                try_poll_immediate(apply_optional_base_specialization_with(
                    &db,
                    ClassBase::Generic,
                    Some(specialization),
                    &effects,
                )),
                Poll::Ready(Err(refuse)),
            );
            assert_eq!(*effects.steps.borrow(), steps[..=index]);
        }
        Ok(())
    }

    #[test]
    fn alias_child_refusal_precedes_materialization_metadata() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/base.py", "class Box[T]: ...\n")
            .build()?;
        let env = db.program_environment();
        let base = declared_base(&db, "Box")?;
        assert!(matches!(base, ClassBase::Class(ClassType::Generic(_))));
        let context = GenericContext::from_typevar_instances(&db, &env, []);
        let specialization = Specialization::new(
            &db,
            context,
            &[][..],
            Some(MaterializationKind::Bottom),
            None,
        );
        let effects = Recording {
            specialization,
            program: env.program(&db),
            steps: RefCell::default(),
            refuse: None,
        };
        assert_eq!(
            try_poll_immediate(apply_optional_base_specialization_with(
                &db,
                base,
                Some(specialization),
                &effects,
            )),
            Poll::Ready(Err(Step::Alias)),
        );
        assert_eq!(
            *effects.steps.borrow(),
            [
                Step::Context,
                Step::Program,
                Step::Environment,
                Step::Specialize,
                Step::Alias
            ],
        );
        Ok(())
    }
}
