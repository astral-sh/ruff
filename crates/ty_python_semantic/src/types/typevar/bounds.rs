use std::convert::Infallible;

use super::{
    TypeVarBoundOrConstraints, TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints,
    TypeVarInstance,
};
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

pub(super) struct OrdinaryTypeVarBoundsEffects<'db> {
    pub(super) db: &'db dyn Db,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVarBoundsEffects)]
    pub(in crate::types) trait TypeVarBoundsEffects<'db> {
        type Error;

        #[operation(source)]
        async fn stored_bounds(&self, typevar: TypeVarInstance<'db>) -> Result<Option<TypeVarBoundOrConstraintsEvaluation<'db>>, Self::Error>;
        #[operation(child)]
        async fn lazy_upper_bound(&self, typevar: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(child)]
        async fn lazy_constraints(&self, typevar: TypeVarInstance<'db>, env: &ProgramEnvironment<'db>) -> Result<Option<TypeVarConstraints<'db>>, Self::Error>;
    }

    #[synchronous(typevar_bounds_sync)]
    #[capabilities(effects = TypeVarBoundsEffects)]
    #[passive_values(TypeVarBoundOrConstraints::UpperBound, TypeVarBoundOrConstraints::Constraints)]
    pub(in crate::types) async fn typevar_bounds_with<'db, E: TypeVarBoundsEffects<'db>>(
        typevar: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        effects: &E,
    ) -> Result<Option<TypeVarBoundOrConstraints<'db>>, E::Error> {
        match effects.stored_bounds(typevar).await? {
            None => Ok(None),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(bounds)) => Ok(Some(bounds)),
            Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound) => {
                match effects.lazy_upper_bound(typevar, env).await? {
                    Some(bound) => Ok(Some(TypeVarBoundOrConstraints::UpperBound(bound))),
                    None => Ok(None),
                }
            }
            Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints) => {
                match effects.lazy_constraints(typevar, env).await? {
                    Some(constraints) => Ok(Some(TypeVarBoundOrConstraints::Constraints(constraints))),
                    None => Ok(None),
                }
            }
        }
    }
}

impl<'db> SynchronousTypeVarBoundsEffects<'db> for OrdinaryTypeVarBoundsEffects<'db> {
    type Error = Infallible;

    fn stored_bounds(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<TypeVarBoundOrConstraintsEvaluation<'db>>, Infallible> {
        Ok(typevar._bound_or_constraints(self.db))
    }

    fn lazy_upper_bound(
        &self,
        typevar: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<Type<'db>>, Infallible> {
        Ok(typevar.lazy_bound(self.db, env))
    }

    fn lazy_constraints(
        &self,
        typevar: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Option<TypeVarConstraints<'db>>, Infallible> {
        Ok(typevar.lazy_constraints(self.db, env))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use ruff_python_ast::name::Name;

    use super::*;
    use crate::db::tests::TestDbBuilder;
    use crate::types::TypeVarKind;
    use crate::types::typevar::TypeVarIdentity;

    struct Trace<'db> {
        ordinary: OrdinaryTypeVarBoundsEffects<'db>,
        events: RefCell<Vec<&'static str>>,
        evaluated: Option<TypeVarBoundOrConstraints<'db>>,
        refuse: Option<&'static str>,
    }

    impl<'db> Trace<'db> {
        fn new(db: &'db dyn Db, evaluated: Option<TypeVarBoundOrConstraints<'db>>) -> Self {
            Self {
                ordinary: OrdinaryTypeVarBoundsEffects { db },
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

    impl<'db> SynchronousTypeVarBoundsEffects<'db> for Trace<'db> {
        type Error = &'static str;

        fn stored_bounds(
            &self,
            typevar: TypeVarInstance<'db>,
        ) -> Result<Option<TypeVarBoundOrConstraintsEvaluation<'db>>, Self::Error> {
            self.record("stored_bounds")?;
            match self.ordinary.stored_bounds(typevar) {
                Ok(bounds) => Ok(bounds),
                Err(never) => match never {},
            }
        }

        fn lazy_upper_bound(
            &self,
            _typevar: TypeVarInstance<'db>,
            _env: &ProgramEnvironment<'db>,
        ) -> Result<Option<Type<'db>>, Self::Error> {
            self.record("lazy_upper_bound")?;
            Ok(match self.evaluated {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => Some(bound),
                _ => None,
            })
        }

        fn lazy_constraints(
            &self,
            _typevar: TypeVarInstance<'db>,
            _env: &ProgramEnvironment<'db>,
        ) -> Result<Option<TypeVarConstraints<'db>>, Self::Error> {
            self.record("lazy_constraints")?;
            Ok(match self.evaluated {
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => Some(constraints),
                _ => None,
            })
        }
    }

    fn variable<'db>(
        db: &'db dyn Db,
        stored: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
    ) -> TypeVarInstance<'db> {
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new_static("T"), None, TypeVarKind::Pep695TypeVar),
            stored,
            None,
            None,
        )
    }

    #[test]
    fn absent_bounds_stop_before_lazy_evaluation() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let typevar = variable(&db, None);
        let effects = Trace::new(
            &db,
            Some(TypeVarBoundOrConstraints::UpperBound(Type::int_literal(1))),
        );
        assert_eq!(
            typevar_bounds_sync(typevar, &db.program_environment(), &effects),
            Ok(None)
        );
        assert_eq!(*effects.events.borrow(), ["stored_bounds"]);
        Ok(())
    }

    #[test]
    fn eager_bounds_preserve_the_stored_value_without_lazy_evaluation() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let constraints =
            TypeVarConstraints::new(&db, [Type::int_literal(1), Type::int_literal(2)].as_slice());
        for bounds in [
            TypeVarBoundOrConstraints::UpperBound(Type::int_literal(3)),
            TypeVarBoundOrConstraints::Constraints(constraints),
        ] {
            let typevar = variable(
                &db,
                Some(TypeVarBoundOrConstraintsEvaluation::Eager(bounds)),
            );
            let effects = Trace::new(&db, None);
            assert_eq!(
                typevar_bounds_sync(typevar, &db.program_environment(), &effects),
                Ok(Some(bounds))
            );
            assert_eq!(*effects.events.borrow(), ["stored_bounds"]);
        }
        Ok(())
    }

    #[test]
    fn lazy_bounds_retain_evaluation_and_refusal_order() -> anyhow::Result<()> {
        let db = TestDbBuilder::new().build()?;
        let constraints =
            TypeVarConstraints::new(&db, [Type::int_literal(1), Type::int_literal(2)].as_slice());
        for (stored, bounds, event) in [
            (
                TypeVarBoundOrConstraintsEvaluation::LazyUpperBound,
                TypeVarBoundOrConstraints::UpperBound(Type::int_literal(3)),
                "lazy_upper_bound",
            ),
            (
                TypeVarBoundOrConstraintsEvaluation::LazyConstraints,
                TypeVarBoundOrConstraints::Constraints(constraints),
                "lazy_constraints",
            ),
        ] {
            let typevar = variable(&db, Some(stored));
            for evaluated in [Some(bounds), None] {
                let effects = Trace::new(&db, evaluated);
                assert_eq!(
                    typevar_bounds_sync(typevar, &db.program_environment(), &effects),
                    Ok(evaluated)
                );
                assert_eq!(*effects.events.borrow(), ["stored_bounds", event]);
            }

            let expected_events = ["stored_bounds", event];
            for (index, refusal) in expected_events.into_iter().enumerate() {
                let mut effects = Trace::new(&db, Some(bounds));
                effects.refuse = Some(refusal);
                assert_eq!(
                    typevar_bounds_sync(typevar, &db.program_environment(), &effects),
                    Err(refusal)
                );
                assert_eq!(*effects.events.borrow(), expected_events[..=index]);
            }
        }
        Ok(())
    }
}
