use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::ProgramFile;
use ty_python_core::definition::Definition;

use super::{BindingContext, BoundTypeVarInstance, TypeVarDefaultEvaluation, TypeVarInstance};
use crate::types::{Type, TypeContext, TypeMapping};
use crate::{Db, ProgramEnvironment};

pub(in crate::types) mod evaluation;
pub(in crate::types) mod lazy;
pub(in crate::types) mod self_reference;

pub(in crate::types) struct BoundDefaultFacts;

pub(super) struct OrdinaryBoundDefaultEffects<'db> {
    pub(super) db: &'db dyn Db,
}

shared_semantic_family! {
    #[synchronous(SynchronousBoundDefaultEffects)]
    pub(in crate::types) trait BoundDefaultEffects<'db> {
        type Error;

        #[operation(source)]
        async fn typevar(&self, bound: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn stored_default(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Self::Error>;
        #[operation(source)]
        async fn definition(&self, typevar: TypeVarInstance<'db>) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn default_type(&self, typevar: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>, stored: TypeVarDefaultEvaluation<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn binding_context(&self, bound: BoundTypeVarInstance<'db>) -> Result<BindingContext<'db>, Self::Error>;
        #[operation(child)]
        async fn bind_default(&self, default: Type<'db>, env: &ProgramEnvironment<'db>, binding: BindingContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn definition_file(&self, definition: Definition<'db>) -> Result<ProgramFile<'db>, Self::Error>;
        #[operation(child)]
        async fn cycle_normalize(&self, default: Type<'db>, env: &ProgramEnvironment<'db>, previous: Type<'db>, cycle: &salsa::Cycle<'_>) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn recursive_normalize(&self, default: Type<'db>, env: &ProgramEnvironment<'db>, cycle: &salsa::Cycle<'_>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl BoundDefaultFacts {
        fn definition_environment<'db>(&self, definition: Definition<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_definition(definition)
        }

        fn file_environment<'db>(&self, file: ProgramFile<'db>) -> ProgramEnvironment<'db> {
            ProgramEnvironment::from_file(file)
        }
    }

    #[synchronous(bound_typevar_default_sync)]
    #[capabilities(effects = BoundDefaultEffects, facts = BoundDefaultFacts)]
    #[passive_values()]
    pub(in crate::types) async fn bound_typevar_default_with<'db, E: BoundDefaultEffects<'db>>(
        bound: BoundTypeVarInstance<'db>,
        facts: BoundDefaultFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let typevar = effects.typevar(bound).await?;
        let Some(stored) = effects.stored_default(typevar).await? else {
            return Ok(None);
        };
        let definition = effects.definition(typevar).await?;
        let env = facts.definition_environment(definition);
        let Some(default) = effects.default_type(typevar, &env, stored).await? else {
            return Ok(None);
        };
        let binding = effects.binding_context(bound).await?;
        let default = effects.bind_default(default, &env, binding).await?;
        Ok(Some(default))
    }

    #[synchronous(bound_typevar_default_recover_sync)]
    #[capabilities(effects = BoundDefaultEffects, facts = BoundDefaultFacts)]
    #[passive_values()]
    pub(in crate::types) async fn bound_typevar_default_recover_with<'db, E: BoundDefaultEffects<'db>>(
        cycle: &salsa::Cycle<'_>,
        previous: Option<Type<'db>>,
        value: Option<Type<'db>>,
        bound: BoundTypeVarInstance<'db>,
        facts: BoundDefaultFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        let Some(default) = value else { return Ok(None); };
        let typevar = effects.typevar(bound).await?;
        let definition = effects.definition(typevar).await?;
        let file = effects.definition_file(definition).await?;
        let env = facts.file_environment(file);
        let default = match previous {
            Some(previous) => effects.cycle_normalize(default, &env, previous, cycle).await?,
            None => effects.recursive_normalize(default, &env, cycle).await?,
        };
        Ok(Some(default))
    }
}

impl<'db> SynchronousBoundDefaultEffects<'db> for OrdinaryBoundDefaultEffects<'db> {
    type Error = Infallible;

    fn typevar(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(bound.typevar(self.db))
    }

    fn stored_default(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Infallible> {
        Ok(typevar._default(self.db))
    }

    fn definition(&self, typevar: TypeVarInstance<'db>) -> Result<Definition<'db>, Infallible> {
        Ok(typevar
            .definition(self.db)
            .expect("a bound TypeVar with a default must have a source definition"))
    }

    fn default_type(
        &self,
        typevar: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        stored: TypeVarDefaultEvaluation<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        evaluation::typevar_default_sync(
            typevar,
            env,
            Some(stored),
            &evaluation::OrdinaryTypeVarDefaultEffects {
                db: self.db,
                visitor: None,
            },
        )
    }

    fn binding_context(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<BindingContext<'db>, Infallible> {
        Ok(bound.binding_context(self.db))
    }

    fn bind_default(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        binding: BindingContext<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(default.apply_type_mapping(
            self.db,
            env,
            &TypeMapping::BindLegacyTypevars(binding),
            TypeContext::default(),
        ))
    }

    fn definition_file(&self, definition: Definition<'db>) -> Result<ProgramFile<'db>, Infallible> {
        Ok(definition.program_file(self.db))
    }

    fn cycle_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(default.cycle_normalized(self.db, env, previous, cycle))
    }

    fn recursive_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(default.recursive_type_normalized(self.db, env, cycle))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::name::Name;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder};
    use crate::place::global_symbol;
    use crate::types::typevar::{TypeVarIdentity, TypeVarNonce};
    use crate::types::{ClassLiteral, TypeVarKind};

    struct Trace<'db> {
        ordinary: OrdinaryBoundDefaultEffects<'db>,
        events: RefCell<Vec<&'static str>>,
        evaluated: Option<Type<'db>>,
        refuse: Option<&'static str>,
    }

    impl<'db> Trace<'db> {
        fn new(db: &'db dyn Db, evaluated: Option<Type<'db>>) -> Self {
            Self {
                ordinary: OrdinaryBoundDefaultEffects { db },
                events: RefCell::new(Vec::new()),
                evaluated,
                refuse: None,
            }
        }

        fn record(&self, event: &'static str) -> Result<(), &'static str> {
            self.events.borrow_mut().push(event);
            if self.refuse == Some(event) {
                Err(event)
            } else {
                Ok(())
            }
        }
    }

    fn infallible<T>(result: Result<T, Infallible>) -> T {
        match result {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    impl<'db> SynchronousBoundDefaultEffects<'db> for Trace<'db> {
        type Error = &'static str;

        fn typevar(
            &self,
            bound: BoundTypeVarInstance<'db>,
        ) -> Result<TypeVarInstance<'db>, Self::Error> {
            self.record("typevar")?;
            Ok(infallible(self.ordinary.typevar(bound)))
        }
        fn stored_default(
            &self,
            typevar: TypeVarInstance<'db>,
        ) -> Result<Option<TypeVarDefaultEvaluation<'db>>, Self::Error> {
            self.record("stored_default")?;
            Ok(infallible(self.ordinary.stored_default(typevar)))
        }
        fn definition(
            &self,
            typevar: TypeVarInstance<'db>,
        ) -> Result<Definition<'db>, Self::Error> {
            self.record("definition")?;
            Ok(infallible(self.ordinary.definition(typevar)))
        }
        fn default_type(
            &self,
            _typevar: TypeVarInstance<'db>,
            _env: &ProgramEnvironment<'db>,
            _stored: TypeVarDefaultEvaluation<'db>,
        ) -> Result<Option<Type<'db>>, Self::Error> {
            self.record("default_type")?;
            Ok(self.evaluated)
        }
        fn binding_context(
            &self,
            bound: BoundTypeVarInstance<'db>,
        ) -> Result<BindingContext<'db>, Self::Error> {
            self.record("binding_context")?;
            Ok(infallible(self.ordinary.binding_context(bound)))
        }
        fn bind_default(
            &self,
            default: Type<'db>,
            env: &ProgramEnvironment<'db>,
            binding: BindingContext<'db>,
        ) -> Result<Type<'db>, Self::Error> {
            self.record("bind_default")?;
            Ok(infallible(
                self.ordinary.bind_default(default, env, binding),
            ))
        }
        fn definition_file(
            &self,
            definition: Definition<'db>,
        ) -> Result<ProgramFile<'db>, Self::Error> {
            self.record("definition_file")?;
            Ok(infallible(self.ordinary.definition_file(definition)))
        }
        fn cycle_normalize(
            &self,
            default: Type<'db>,
            _env: &ProgramEnvironment<'db>,
            previous: Type<'db>,
            _cycle: &salsa::Cycle<'_>,
        ) -> Result<Type<'db>, Self::Error> {
            self.record("cycle_normalize")?;
            assert_eq!(previous, Type::int_literal(7));
            assert_eq!(default, Type::int_literal(8));
            Ok(Type::int_literal(9))
        }
        fn recursive_normalize(
            &self,
            default: Type<'db>,
            _env: &ProgramEnvironment<'db>,
            _cycle: &salsa::Cycle<'_>,
        ) -> Result<Type<'db>, Self::Error> {
            self.record("recursive_normalize")?;
            assert_eq!(default, Type::int_literal(8));
            Ok(Type::int_literal(10))
        }
    }

    fn database() -> anyhow::Result<TestDb> {
        TestDbBuilder::new()
            .with_file("/src/bound_default.py", "class Marker: ...\n")
            .build()
    }

    fn variable<'db>(
        db: &'db TestDb,
        stored: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> anyhow::Result<BoundTypeVarInstance<'db>> {
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/bound_default.py")?,
            db.program_environment().program(db),
        );
        let Some(Type::ClassLiteral(ClassLiteral::Static(class))) =
            global_symbol(db, file, "Marker")
                .place
                .ignore_possibly_undefined()
        else {
            anyhow::bail!("missing Marker class");
        };
        let definition = class.definition(db);
        let typevar = TypeVarInstance::new(
            db,
            TypeVarIdentity::new(
                db,
                Name::new_static("T"),
                Some(definition),
                TypeVarKind::LegacyTypeVar,
            ),
            None,
            None,
            stored,
        );
        Ok(BoundTypeVarInstance::new(
            db,
            typevar,
            BindingContext::Definition(definition),
            None,
            TypeVarNonce::NONE,
        ))
    }

    #[test]
    fn absent_default_stops_before_definition_and_evaluation() -> anyhow::Result<()> {
        let db = database()?;
        let bound = variable(&db, None)?;
        let effects = Trace::new(&db, Some(Type::int_literal(1)));
        assert_eq!(
            bound_typevar_default_sync(bound, BoundDefaultFacts, &effects),
            Ok(None)
        );
        assert_eq!(*effects.events.borrow(), ["typevar", "stored_default"]);
        Ok(())
    }

    #[test]
    fn present_default_retains_evaluation_binding_and_refusal_order() -> anyhow::Result<()> {
        let db = database()?;
        for stored in [
            TypeVarDefaultEvaluation::Lazy,
            TypeVarDefaultEvaluation::Eager(Type::int_literal(1)),
        ] {
            let bound = variable(&db, Some(stored))?;
            let effects = Trace::new(&db, Some(Type::int_literal(2)));
            assert_eq!(
                bound_typevar_default_sync(bound, BoundDefaultFacts, &effects),
                Ok(Some(Type::int_literal(2)))
            );
            assert_eq!(
                *effects.events.borrow(),
                [
                    "typevar",
                    "stored_default",
                    "definition",
                    "default_type",
                    "binding_context",
                    "bind_default"
                ]
            );

            let effects = Trace::new(&db, None);
            assert_eq!(
                bound_typevar_default_sync(bound, BoundDefaultFacts, &effects),
                Ok(None)
            );
            assert_eq!(
                *effects.events.borrow(),
                ["typevar", "stored_default", "definition", "default_type"]
            );

            let mut effects = Trace::new(&db, Some(Type::int_literal(2)));
            effects.refuse = Some("default_type");
            assert_eq!(
                bound_typevar_default_sync(bound, BoundDefaultFacts, &effects),
                Err("default_type")
            );
            assert_eq!(
                *effects.events.borrow(),
                ["typevar", "stored_default", "definition", "default_type"]
            );
        }
        Ok(())
    }

    #[salsa::interned]
    struct RecoveryInput<'db> {
        #[returns(copy)]
        bound: BoundTypeVarInstance<'db>,
    }

    #[salsa::tracked(cycle_initial = |_, _, _| false, cycle_fn = recover_contract)]
    fn recovery_contract<'db>(db: &'db dyn Db, input: RecoveryInput<'db>) -> bool {
        let _ = recovery_contract(db, input);
        true
    }

    fn recover_contract<'db>(
        db: &'db dyn Db,
        cycle: &salsa::Cycle<'_>,
        _last: &bool,
        value: bool,
        input: RecoveryInput<'db>,
    ) -> bool {
        let bound = input.bound(db);
        let effects = Trace::new(db, None);
        assert_eq!(
            bound_typevar_default_recover_sync(
                cycle,
                Some(Type::int_literal(7)),
                None,
                bound,
                BoundDefaultFacts,
                &effects
            ),
            Ok(None)
        );
        assert!(effects.events.borrow().is_empty());

        for (previous, event, expected) in [
            (
                Some(Type::int_literal(7)),
                "cycle_normalize",
                Type::int_literal(9),
            ),
            (None, "recursive_normalize", Type::int_literal(10)),
        ] {
            let effects = Trace::new(db, None);
            assert_eq!(
                bound_typevar_default_recover_sync(
                    cycle,
                    previous,
                    Some(Type::int_literal(8)),
                    bound,
                    BoundDefaultFacts,
                    &effects
                ),
                Ok(Some(expected))
            );
            assert_eq!(
                *effects.events.borrow(),
                ["typevar", "definition", "definition_file", event]
            );

            let mut effects = Trace::new(db, None);
            effects.refuse = Some(event);
            assert_eq!(
                bound_typevar_default_recover_sync(
                    cycle,
                    previous,
                    Some(Type::int_literal(8)),
                    bound,
                    BoundDefaultFacts,
                    &effects
                ),
                Err(event)
            );
        }
        value
    }

    #[test]
    fn recovery_selects_normalization_only_for_present_results() -> anyhow::Result<()> {
        let db = database()?;
        let bound = variable(&db, None)?;
        assert!(recovery_contract(&db, RecoveryInput::new(&db, bound)));
        Ok(())
    }
}
