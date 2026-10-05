//! The original specialization query derives its mapping before creating a fresh visitor.

use std::convert::Infallible;

#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::execution_probe::{PassiveMemoProfile, QueryKeyProfile};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::OwnedTypeMapping;
use crate::types::{GenericContext, MaterializationKind, Specialization, Type, TypeContext};
use crate::{Db, Program, ProgramEnvironment};

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) trait SpecializationConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<
        Fields<'a> = (Type<'a>, Specialization<'a>, bool),
    > + for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>
{
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<C> SpecializationConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (Type<'a>, Specialization<'a>, bool),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = Type<'a>>
{
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct SpecializationKeyProfile;

#[cfg(any(test, feature = "experimental-analysis"))]
impl<C: SpecializationConfiguration> QueryKeyProfile<C> for SpecializationKeyProfile {
    fn input_work<'db>(
        fields: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        3usize.checked_add(fields.0.inline_payload_bytes())
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<C: SpecializationConfiguration> PassiveMemoProfile<C> for SpecializationKeyProfile {
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        Some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousSpecializationEffects)]
    pub(in crate::types) trait SpecializationEffects<'db> {
        type Error;
        type Environment;

        #[operation(source)]
        async fn generic_context(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<GenericContext<'db>, Self::Error>;
        #[operation(source)]
        async fn program(&self, db: &'db dyn Db, context: GenericContext<'db>) -> Result<Program<'db>, Self::Error>;
        #[operation(local)]
        async fn environment(&self, program: Program<'db>) -> Result<Self::Environment, Self::Error>;
        #[operation(source)]
        async fn materialization_kind(&self, db: &'db dyn Db, specialization: Specialization<'db>) -> Result<Option<MaterializationKind>, Self::Error>;
        #[operation(local)]
        async fn mapping(&self, specialization: Specialization<'db>, specialize_self_domain: bool, kind: Option<MaterializationKind>) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error>;
        #[operation(child)]
        async fn map_fresh(&self, db: &'db dyn Db, ty: Type<'db>, env: Self::Environment, mapping: OwnedTypeMapping<'db, 'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(shared_specialization_sync)]
    #[capabilities(effects = SpecializationEffects)]
    #[passive_values()]
    pub(in crate::types) async fn shared_specialization_with<'db, E: SpecializationEffects<'db>>(
        db: &'db dyn Db,
        ty: Type<'db>,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let context = effects.generic_context(db, specialization).await?;
        let program = effects.program(db, context).await?;
        let env = effects.environment(program).await?;
        let kind = effects.materialization_kind(db, specialization).await?;
        let mapping = effects.mapping(specialization, specialize_self_domain, kind).await?;
        effects.map_fresh(db, ty, env, mapping).await
    }
}

pub(in crate::types) struct InlineSpecializationEffects;

impl<'db> SynchronousSpecializationEffects<'db> for InlineSpecializationEffects {
    type Error = Infallible;
    type Environment = ProgramEnvironment<'db>;

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

    fn mapping(
        &self,
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
        kind: Option<MaterializationKind>,
    ) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error> {
        Ok(OwnedTypeMapping::Specialization {
            specialization,
            specialize_self_domain,
            materialization_kind: kind,
        })
    }

    fn map_fresh(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        env: Self::Environment,
        mapping: OwnedTypeMapping<'db, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.apply_type_mapping(db, &env, &mapping.into_mapping(), TypeContext::default()))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::task::Poll;

    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::name::Name;
    use salsa::plumbing::AsId;
    use ty_python_core::ProgramFile;

    use super::*;
    use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
    use crate::place::global_symbol;
    use crate::types::mapping::source::observations::{
        BindingContextSnapshot, OwnedMappingSnapshot,
    };
    use crate::types::signatures::effects::try_poll_immediate;
    use crate::types::typevar::{
        ParamSpecAttrKind, TypeVarIdentity, TypeVarInstance, TypeVarNonce,
    };
    use crate::types::{
        ApplySpecialization, BindingContext, BoundTypeVarInstance, ClassLiteral, TypeMapping,
        TypeVarKind, TypeVarVariance,
    };

    struct Observed<'db> {
        specialization: Specialization<'db>,
        program: Program<'db>,
        specialize_self_domain: bool,
        kind: Option<MaterializationKind>,
        steps: RefCell<Vec<&'static str>>,
        refuse: Option<&'static str>,
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

    impl<'db> SpecializationEffects<'db> for Observed<'db> {
        type Error = &'static str;
        type Environment = Program<'db>;

        async fn generic_context(
            &self,
            db: &'db dyn Db,
            specialization: Specialization<'db>,
        ) -> Result<GenericContext<'db>, Self::Error> {
            self.record("generic context")?;
            assert_eq!(specialization, self.specialization);
            Ok(specialization.generic_context(db))
        }

        async fn program(
            &self,
            db: &'db dyn Db,
            context: GenericContext<'db>,
        ) -> Result<Program<'db>, Self::Error> {
            self.record("program")?;
            assert_eq!(context, self.specialization.generic_context(db));
            Ok(context.program(db))
        }

        async fn environment(
            &self,
            program: Program<'db>,
        ) -> Result<Self::Environment, Self::Error> {
            self.record("environment")?;
            assert_eq!(program, self.program);
            Ok(program)
        }

        async fn materialization_kind(
            &self,
            db: &'db dyn Db,
            specialization: Specialization<'db>,
        ) -> Result<Option<MaterializationKind>, Self::Error> {
            self.record("materialization kind")?;
            assert_eq!(specialization, self.specialization);
            Ok(specialization.materialization_kind(db))
        }

        async fn mapping(
            &self,
            specialization: Specialization<'db>,
            specialize_self_domain: bool,
            kind: Option<MaterializationKind>,
        ) -> Result<OwnedTypeMapping<'db, 'db>, Self::Error> {
            self.record("mapping")?;
            assert_eq!(specialization, self.specialization);
            assert_eq!(specialize_self_domain, self.specialize_self_domain);
            assert_eq!(kind, self.kind);
            Ok(OwnedTypeMapping::Specialization {
                specialization,
                specialize_self_domain,
                materialization_kind: kind,
            })
        }

        async fn map_fresh(
            &self,
            _db: &'db dyn Db,
            ty: Type<'db>,
            env: Self::Environment,
            mapping: OwnedTypeMapping<'db, 'db>,
        ) -> Result<Type<'db>, Self::Error> {
            self.record("fresh visitor")?;
            assert_eq!(env, self.program);
            assert_eq!(
                mapping,
                OwnedTypeMapping::Specialization {
                    specialization: self.specialization,
                    specialize_self_domain: self.specialize_self_domain,
                    materialization_kind: self.kind,
                }
            );
            assert_eq!(ty, Type::bool_literal(true));
            Ok(Type::bool_literal(false))
        }
    }

    #[test]
    fn shared_specialization_orders_derived_inputs_and_stops_at_each_refusal() {
        let db = setup_db();
        let env = db.program_environment();
        let variable = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let context = GenericContext::from_typevar_instances(&db, &env, [variable]);
        let steps = [
            "generic context",
            "program",
            "environment",
            "materialization kind",
            "mapping",
            "fresh visitor",
        ];
        for kind in [
            None,
            Some(MaterializationKind::Top),
            Some(MaterializationKind::Bottom),
        ] {
            let specialization = Specialization::new(&db, context, &[Type::any()][..], kind, None);
            for specialize_self_domain in [false, true] {
                for refuse_at in 0..=steps.len() {
                    let refuse = steps.get(refuse_at).copied();
                    let effects = Observed {
                        specialization,
                        program: env.program(&db),
                        specialize_self_domain,
                        kind,
                        steps: RefCell::default(),
                        refuse,
                    };
                    let result = try_poll_immediate(shared_specialization_with(
                        &db,
                        Type::bool_literal(true),
                        specialization,
                        specialize_self_domain,
                        &effects,
                    ));
                    assert_eq!(
                        result,
                        Poll::Ready(match refuse {
                            Some(step) => Err(step),
                            None => Ok(Type::bool_literal(false)),
                        })
                    );
                    let completed_steps = (refuse_at + 1).min(steps.len());
                    assert_eq!(*effects.steps.borrow(), steps[..completed_steps]);
                }
            }
        }
    }

    #[test]
    fn owned_mapping_retains_specialization_modes() {
        let db = setup_db();
        let env = db.program_environment();
        let specialization =
            GenericContext::from_typevar_instances(&db, &env, []).default_specialization(&db, None);
        for specialize_self_domain in [false, true] {
            for materialization_kind in [
                None,
                Some(MaterializationKind::Top),
                Some(MaterializationKind::Bottom),
            ] {
                let request = OwnedTypeMapping::Specialization {
                    specialization,
                    specialize_self_domain,
                    materialization_kind,
                };
                let mapping = request.into_mapping();
                assert_eq!(OwnedTypeMapping::from_mapping(&mapping), Some(request));
                let expected = ApplySpecialization::Specialization {
                    specialization,
                    specialize_self_domain,
                };
                assert_eq!(
                    mapping,
                    match materialization_kind {
                        None => TypeMapping::ApplySpecialization(expected),
                        Some(materialization_kind) => {
                            TypeMapping::ApplySpecializationWithMaterialization {
                                specialization: expected,
                                materialization_kind,
                            }
                        }
                    }
                );
            }
        }
        for kind in [MaterializationKind::Top, MaterializationKind::Bottom] {
            let request = OwnedTypeMapping::Materialize(kind);
            assert_eq!(
                OwnedTypeMapping::from_mapping(&request.into_mapping()),
                Some(request)
            );
        }
    }

    /// Selects which handle differs when a Single substitution is recaptured.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SingleChange {
        Neither,
        Variable,
        Replacement,
        Both,
    }

    /// Single conversion preserves both handles exactly. Recapture accepts changed handles because
    /// this descriptor retains no borrowed storage whose owner would need to be authenticated.
    #[test_case::test_case(SingleChange::Neither; "same Single handles")]
    #[test_case::test_case(SingleChange::Variable; "changed Single variable")]
    #[test_case::test_case(SingleChange::Replacement; "changed Single replacement")]
    #[test_case::test_case(SingleChange::Both; "changed Single handles")]
    fn owned_single_round_trips_and_recaptures_handles(change: SingleChange) {
        let db = setup_db();
        let env = db.program_environment();
        let first = BoundTypeVarInstance::synthetic(
            &db, &env, Name::new_static("T"), TypeVarVariance::Invariant,
        );
        let second = BoundTypeVarInstance::synthetic(
            &db, &env, Name::new_static("U"), TypeVarVariance::Invariant,
        );
        let request = OwnedTypeMapping::Single {
            variable: first,
            replacement: Type::bool_literal(true),
        };
        let mapping = request.into_mapping();
        assert_eq!(
            mapping,
            TypeMapping::ApplySpecialization(ApplySpecialization::Single(
                first, Type::bool_literal(true),
            )),
        );
        assert_eq!(OwnedTypeMapping::from_mapping(&mapping), Some(request));
        assert_eq!(OwnedMappingSnapshot::from(request), OwnedMappingSnapshot::Single {
            variable: first.as_id(),
        });
        let variable = match change {
            SingleChange::Neither | SingleChange::Replacement => first,
            SingleChange::Variable | SingleChange::Both => second,
        };
        let replacement = match change {
            SingleChange::Neither | SingleChange::Variable => Type::bool_literal(true),
            SingleChange::Replacement | SingleChange::Both => Type::TypeVar(first),
        };
        let changed = OwnedTypeMapping::Single { variable, replacement };
        assert_eq!(OwnedTypeMapping::from_mapping(&changed.into_mapping()), Some(changed));
        assert_eq!(request.recapture(&changed.into_mapping()), Some(changed));
        assert_eq!(
            OwnedTypeMapping::Materialize(MaterializationKind::Top)
                .recapture(&changed.into_mapping()),
            Some(changed),
        );
    }

    /// Materializing a Single substitution cannot be captured as an ordinary Single descriptor.
    #[test_case::test_case(MaterializationKind::Top; "materializing Single top")]
    #[test_case::test_case(MaterializationKind::Bottom; "materializing Single bottom")]
    fn owned_single_rejects_materialization(materialization_kind: MaterializationKind) {
        let db = setup_db();
        let env = db.program_environment();
        let variable = BoundTypeVarInstance::synthetic(
            &db, &env, Name::new_static("T"), TypeVarVariance::Invariant,
        );
        let replacement = Type::bool_literal(true);
        let request = OwnedTypeMapping::Single { variable, replacement };
        let mapping = TypeMapping::ApplySpecializationWithMaterialization {
            specialization: ApplySpecialization::Single(variable, replacement),
            materialization_kind,
        };
        assert_eq!(OwnedTypeMapping::from_mapping(&mapping), None);
        assert_eq!(request.recapture(&mapping), None);
    }

    fn binding_definition(db: &TestDb) -> anyhow::Result<BindingContext<'_>> {
        let file = ProgramFile::new(
            db,
            system_path_to_file(db, "/src/binding.py")?,
            db.program_environment().program(db),
        );
        let Some(Type::ClassLiteral(ClassLiteral::Static(class))) =
            global_symbol(db, file, "Marker")
                .place
                .ignore_possibly_undefined()
        else {
            anyhow::bail!("missing Marker class");
        };
        Ok(BindingContext::Definition(class.definition(db)))
    }

    #[test]
    fn owned_mapping_retains_legacy_binding_contexts() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/binding.py", "class Marker: ...\n")
            .build()?;
        for context in [
            binding_definition(&db)?,
            BindingContext::Synthetic(db.program_environment().program(&db)),
        ] {
            let request = OwnedTypeMapping::BindLegacyTypevars(context);
            let mapping = request.into_mapping();
            assert_eq!(mapping, TypeMapping::BindLegacyTypevars(context));
            assert_eq!(OwnedTypeMapping::from_mapping(&mapping), Some(request));
            let expected = match context {
                BindingContext::Definition(definition) => {
                    BindingContextSnapshot::Definition(definition.as_id())
                }
                BindingContext::Synthetic(program) => {
                    BindingContextSnapshot::Synthetic(program.as_id())
                }
            };
            assert_eq!(
                OwnedMappingSnapshot::from(request),
                OwnedMappingSnapshot::BindLegacyTypevars(expected)
            );
        }
        Ok(())
    }

    #[test]
    fn legacy_binding_preserves_already_bound_identity() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file("/src/binding.py", "class Marker: ...\n")
            .build()?;
        let env = db.program_environment();
        let definition = binding_definition(&db)?;
        let synthetic = BindingContext::Synthetic(env.program(&db));
        let variable = TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(
                &db,
                Name::new_static("P"),
                None,
                TypeVarKind::LegacyParamSpec,
            ),
            None,
            None,
            None,
        );
        for (original_context, incoming_context) in
            [(definition, synthetic), (synthetic, definition)]
        {
            for attribute in [
                None,
                Some(ParamSpecAttrKind::Args),
                Some(ParamSpecAttrKind::Kwargs),
            ] {
                let bound = BoundTypeVarInstance::new(
                    &db,
                    variable,
                    original_context,
                    attribute,
                    TypeVarNonce::NONE.increment(),
                );
                assert_eq!(bound.bind_legacy_typevars(), Type::TypeVar(bound));
                assert_eq!(
                    Type::TypeVar(bound).apply_type_mapping(
                        &db,
                        &env,
                        &TypeMapping::BindLegacyTypevars(incoming_context),
                        TypeContext::default(),
                    ),
                    Type::TypeVar(bound),
                );
            }
        }
        Ok(())
    }

    /// Type-alias and borrowed substitutions remain unsupported by conversion and Single recapture.
    #[test]
    fn owned_mapping_rejects_other_substitution_modes() {
        let db = setup_db();
        let env = db.program_environment();
        let context = GenericContext::from_typevar_instances(&db, &env, []);
        let specialization = context.default_specialization(&db, None);
        let ordinary = ApplySpecialization::Specialization {
            specialization,
            specialize_self_domain: false,
        };
        let single = OwnedTypeMapping::Single {
            variable: BoundTypeVarInstance::synthetic(
                &db, &env, Name::new_static("T"), TypeVarVariance::Invariant,
            ),
            replacement: Type::bool_literal(true),
        };
        for unsupported in [
            ApplySpecialization::TypeAlias(specialization),
            ApplySpecialization::Partial {
                generic_context: context,
                types: (&[][..]).into(),
                skip: None,
            },
            ApplySpecialization::WithBindings {
                specialization: &ordinary,
                bindings: &[],
            },
        ] {
            assert_eq!(
                OwnedTypeMapping::from_mapping(&TypeMapping::ApplySpecialization(unsupported)),
                None
            );
            assert_eq!(single.recapture(&TypeMapping::ApplySpecialization(unsupported)), None);
            assert_eq!(
                OwnedTypeMapping::from_mapping(
                    &TypeMapping::ApplySpecializationWithMaterialization {
                        specialization: unsupported,
                        materialization_kind: MaterializationKind::Top,
                    }
                ),
                None
            );
            assert_eq!(
                single.recapture(&TypeMapping::ApplySpecializationWithMaterialization {
                    specialization: unsupported,
                    materialization_kind: MaterializationKind::Top,
                }),
                None,
            );
        }
        assert_eq!(
            OwnedTypeMapping::from_mapping(&TypeMapping::ReplaceParameterDefaults),
            None
        );
    }
}
