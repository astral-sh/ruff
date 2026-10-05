use crate::Db;
use crate::types::mapping::effects::MappingWork;
use crate::types::{
    BoundTypeVarInstance, KnownBoundMethodType, KnownInstanceType, MaterializationKind,
    NominalInstanceType, Specialization, Type,
};

pub(crate) struct SpecializationStartFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSpecializationStartEffects)]
    pub(crate) trait SpecializationStartEffects<'db> {
        type Error;
        #[operation(checkpoint)]
        async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn nominal_is_definition_generic(&self, db: &'db dyn Db, instance: NominalInstanceType<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn typevar_is_paramspec(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn lookup_typevar(&self, db: &'db dyn Db, specialization: Specialization<'db>, variable: BoundTypeVarInstance<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn materialization_kind(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(source)]
        async fn typevar_is_self(&self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) -> Result<bool, Self::Error>;
    }

    #[finite_capability]
    impl SpecializationStartFacts {
        fn no_materialization(&self, kind: Option<MaterializationKind>) -> bool { kind.is_none() }
        fn terminal<'db>(&self, ty: Type<'db>) -> bool {
        matches!(
            ty,
            Type::Dynamic(_)
                | Type::Divergent(_)
                | Type::Never
                | Type::WrapperDescriptor(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::SpecialForm(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::LiteralValue(_)
                | Type::BoundSuper(_)
                | Type::KnownInstance(
                    KnownInstanceType::SubscriptedProtocol(_)
                        | KnownInstanceType::SubscriptedGeneric(_)
                        | KnownInstanceType::TypeAliasType(_)
                        | KnownInstanceType::Deprecated(_)
                        | KnownInstanceType::Field(_)
                        | KnownInstanceType::ConstraintSet(_)
                        | KnownInstanceType::ConstraintSetSolution(_)
                        | KnownInstanceType::GenericContext(_)
                        | KnownInstanceType::Specialization(_)
                        | KnownInstanceType::Literal(_)
                        | KnownInstanceType::NewType(_)
                        | KnownInstanceType::Sentinel(_)
                        | KnownInstanceType::NamedTupleSpec(_),
                )
                | Type::KnownBoundMethod(
                    KnownBoundMethodType::StrStartswith(_)
                        | KnownBoundMethodType::ConstraintSetLowerBound
                        | KnownBoundMethodType::ConstraintSetUpperBound
                        | KnownBoundMethodType::ConstraintSetEquality
                        | KnownBoundMethodType::ConstraintSetRange
                        | KnownBoundMethodType::ConstraintSetAlways
                        | KnownBoundMethodType::ConstraintSetNever
                        | KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)
                        | KnownBoundMethodType::ConstraintSetSatisfies(_)
                        | KnownBoundMethodType::ConstraintSetExists(_)
                        | KnownBoundMethodType::ConstraintSetForAll(_)
                        | KnownBoundMethodType::ConstraintSetSolutionsFor(_)
                        | KnownBoundMethodType::ConstraintSetSolutions(_)
                        | KnownBoundMethodType::ConstraintSetWithDetailedDisplay(_)
                )
        )
        }
    }

    #[synchronous(specialization_start_sync)]
    #[capabilities(effects = SpecializationStartEffects, facts = SpecializationStartFacts)]
    #[passive_values(MappingWork::RootAdmission, MappingWork::TypeVarLookup)]
    pub(crate) async fn specialization_start_with<'db, E: SpecializationStartEffects<'db>>(
        db: &'db dyn Db,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        facts: SpecializationStartFacts,
        effects: &E,
    ) -> Result<Option<Type<'db>>, E::Error> {
        effects.checkpoint(MappingWork::RootAdmission).await?;
        if let Type::NominalInstance(instance) = ty
            && !effects.nominal_is_definition_generic(db, instance).await?
        {
            return Ok(Some(ty));
        }
        if let Type::TypeVar(typevar) = ty
            && !effects.typevar_is_paramspec(db, typevar).await?
        {
            effects.checkpoint(MappingWork::TypeVarLookup).await?;
            match effects.lookup_typevar(db, specialization, typevar).await? {
                Some(mapped) if facts.no_materialization(effects.materialization_kind(db, specialization).await?) => {
                    return Ok(Some(mapped));
                }
                None if !specialize_self_domain || !effects.typevar_is_self(db, typevar).await? => {
                    return Ok(Some(ty));
                }
                _ => {}
            }
        }
        if facts.terminal(ty) {
            return Ok(Some(ty));
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::task::Poll;

    use ruff_python_ast::name::Name;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::types::{GenericContext, TypeVarVariance};

    struct Observed<'db> {
        steps: RefCell<Vec<&'static str>>,
        refuse: Option<&'static str>,
        nominal_generic: bool,
        paramspec: bool,
        mapped: Option<Type<'db>>,
        materialization: Option<MaterializationKind>,
        self_variable: bool,
    }

    impl Observed<'_> {
        fn record(&self, step: &'static str) -> Result<(), &'static str> {
            self.steps.borrow_mut().push(step);
            if self.refuse == Some(step) {
                Err(step)
            } else {
                Ok(())
            }
        }
    }

    impl<'db> SynchronousSpecializationStartEffects<'db> for Observed<'db> {
        type Error = &'static str;
        fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error> {
            self.record(match work {
                MappingWork::RootAdmission => "root",
                MappingWork::TypeVarLookup => "lookup checkpoint",
                _ => "unexpected checkpoint",
            })
        }
        fn nominal_is_definition_generic(
            &self,
            _db: &'db dyn Db,
            _instance: NominalInstanceType<'db>,
        ) -> Result<bool, Self::Error> {
            self.record("nominal")?;
            Ok(self.nominal_generic)
        }
        fn typevar_is_paramspec(
            &self,
            _db: &'db dyn Db,
            _variable: BoundTypeVarInstance<'db>,
        ) -> Result<bool, Self::Error> {
            self.record("paramspec")?;
            Ok(self.paramspec)
        }
        fn lookup_typevar(
            &self,
            _db: &'db dyn Db,
            _specialization: Specialization<'db>,
            _variable: BoundTypeVarInstance<'db>,
        ) -> Result<Option<Type<'db>>, Self::Error> {
            self.record("lookup")?;
            Ok(self.mapped)
        }
        fn materialization_kind(
            &self,
            _db: &'db dyn Db,
            _specialization: Specialization<'db>,
        ) -> Result<Option<MaterializationKind>, Self::Error> {
            self.record("materialization")?;
            Ok(self.materialization)
        }
        fn typevar_is_self(
            &self,
            _db: &'db dyn Db,
            _variable: BoundTypeVarInstance<'db>,
        ) -> Result<bool, Self::Error> {
            self.record("self")?;
            Ok(self.self_variable)
        }
    }

    impl<'db> SpecializationStartEffects<'db> for Observed<'db> {
        type Error = &'static str;
        async fn checkpoint(&self, work: MappingWork) -> Result<(), Self::Error> {
            SynchronousSpecializationStartEffects::checkpoint(self, work)
        }
        async fn nominal_is_definition_generic(
            &self,
            db: &'db dyn Db,
            instance: NominalInstanceType<'db>,
        ) -> Result<bool, Self::Error> {
            SynchronousSpecializationStartEffects::nominal_is_definition_generic(self, db, instance)
        }
        async fn typevar_is_paramspec(
            &self,
            db: &'db dyn Db,
            variable: BoundTypeVarInstance<'db>,
        ) -> Result<bool, Self::Error> {
            SynchronousSpecializationStartEffects::typevar_is_paramspec(self, db, variable)
        }
        async fn lookup_typevar(
            &self,
            db: &'db dyn Db,
            specialization: Specialization<'db>,
            variable: BoundTypeVarInstance<'db>,
        ) -> Result<Option<Type<'db>>, Self::Error> {
            SynchronousSpecializationStartEffects::lookup_typevar(
                self,
                db,
                specialization,
                variable,
            )
        }
        async fn materialization_kind(
            &self,
            db: &'db dyn Db,
            specialization: Specialization<'db>,
        ) -> Result<Option<MaterializationKind>, Self::Error> {
            SynchronousSpecializationStartEffects::materialization_kind(self, db, specialization)
        }
        async fn typevar_is_self(
            &self,
            db: &'db dyn Db,
            variable: BoundTypeVarInstance<'db>,
        ) -> Result<bool, Self::Error> {
            SynchronousSpecializationStartEffects::typevar_is_self(self, db, variable)
        }
    }

    #[test]
    fn specialization_prefix_preserves_lookup_guards_and_refusal_order() {
        let db = setup_db();
        let env = db.program_environment();
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let ty = Type::TypeVar(variable);
        let specialization =
            GenericContext::from_typevar_instances(&db, &env, []).default_specialization(&db, None);
        for (paramspec, mapped, materialization, self_domain, self_variable, expected, steps) in [
            (
                true,
                None,
                None,
                false,
                false,
                None,
                vec!["root", "paramspec"],
            ),
            (
                false,
                Some(Type::Never),
                None,
                true,
                true,
                Some(Type::Never),
                vec![
                    "root",
                    "paramspec",
                    "lookup checkpoint",
                    "lookup",
                    "materialization",
                ],
            ),
            (
                false,
                Some(Type::Never),
                Some(MaterializationKind::Top),
                true,
                true,
                None,
                vec![
                    "root",
                    "paramspec",
                    "lookup checkpoint",
                    "lookup",
                    "materialization",
                ],
            ),
            (
                false,
                None,
                None,
                false,
                true,
                Some(ty),
                vec!["root", "paramspec", "lookup checkpoint", "lookup"],
            ),
            (
                false,
                None,
                None,
                true,
                false,
                Some(ty),
                vec!["root", "paramspec", "lookup checkpoint", "lookup", "self"],
            ),
            (
                false,
                None,
                None,
                true,
                true,
                None,
                vec!["root", "paramspec", "lookup checkpoint", "lookup", "self"],
            ),
        ] {
            let effects = Observed {
                steps: RefCell::default(),
                refuse: None,
                nominal_generic: false,
                paramspec,
                mapped,
                materialization,
                self_variable,
            };
            assert_eq!(
                specialization_start_sync(
                    &db,
                    ty,
                    specialization,
                    self_domain,
                    SpecializationStartFacts,
                    &effects
                ),
                Ok(expected)
            );
            assert_eq!(*effects.steps.borrow(), steps);
            effects.steps.borrow_mut().clear();
            assert_eq!(
                try_poll_immediate(specialization_start_with(
                    &db,
                    ty,
                    specialization,
                    self_domain,
                    SpecializationStartFacts,
                    &effects
                )),
                Poll::Ready(Ok(expected))
            );
            assert_eq!(*effects.steps.borrow(), steps);
            for (index, refused) in steps.iter().enumerate() {
                let effects = Observed {
                    steps: RefCell::default(),
                    refuse: Some(*refused),
                    nominal_generic: false,
                    paramspec,
                    mapped,
                    materialization,
                    self_variable,
                };
                assert_eq!(
                    try_poll_immediate(specialization_start_with(
                        &db,
                        ty,
                        specialization,
                        self_domain,
                        SpecializationStartFacts,
                        &effects
                    )),
                    Poll::Ready(Err(*refused))
                );
                assert_eq!(*effects.steps.borrow(), steps[..=index]);
            }
        }
    }

    #[test]
    fn specialization_prefix_admits_terminals_before_returning() {
        let db = setup_db();
        let env = db.program_environment();
        let specialization =
            GenericContext::from_typevar_instances(&db, &env, []).default_specialization(&db, None);
        for (ty, generic, expected, steps) in [
            (Type::Never, false, Some(Type::Never), vec!["root"]),
            (
                Type::object(),
                false,
                Some(Type::object()),
                vec!["root", "nominal"],
            ),
            (Type::object(), true, None, vec!["root", "nominal"]),
        ] {
            let effects = Observed {
                steps: RefCell::default(),
                refuse: None,
                nominal_generic: generic,
                paramspec: false,
                mapped: None,
                materialization: None,
                self_variable: false,
            };
            assert_eq!(
                try_poll_immediate(specialization_start_with(
                    &db,
                    ty,
                    specialization,
                    false,
                    SpecializationStartFacts,
                    &effects
                )),
                Poll::Ready(Ok(expected))
            );
            assert_eq!(*effects.steps.borrow(), steps);
        }
    }
}
