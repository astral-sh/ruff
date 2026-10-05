use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::convert::Infallible;
use std::panic::{AssertUnwindSafe, catch_unwind};

use ruff_db::files::system_path_to_file;
use ruff_python_ast::{PythonVersion, name::Name};
use salsa::Database as _;
use salsa::attempt_probe::AttemptOutcome;
use salsa::execution_probe::{ExecutionLimits, RegistryBuilder, try_with_execution_budget};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use ty_python_core::ProgramFile;

use super::{
    DefaultSpecializationEffects, DefaultSpecializationWork, Unrestricted,
    default_specialization_with, fill_in_defaults_with, register_specialization_values,
    specialize_partial_with,
};
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::global_symbol;
use crate::types::callable::CallableTypeKind;
use crate::types::generics::{ApplySpecialization, GenericContext, Specialization};
use crate::types::tuple::TupleType;
use crate::types::typevar::{
    TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance, TypeVarNonce,
};
use crate::types::{
    BindingContext, BoundTypeVarInstance, ClassLiteral, KnownClass, MaterializationKind,
    Parameters, StaticClassLiteral, Type, TypeContext, TypeMapping, TypeVarKind,
};
use crate::{Db, ProgramEnvironment};

#[test]
fn specialization_schema_preserves_all_fields_without_queries() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let empty = GenericContext::from_typevar_instances(&db, &env, []);
    let context = GenericContext::from_typevar_instances(
        &db,
        &env,
        [variable(&db, 0, TypeVarKind::LegacyTypeVar)],
    );
    let tuple = TupleType::homogeneous(&db, &env, Type::bool_literal(true));
    db.clone().clear_salsa_events();
    for fields in [
        (empty, Box::<[Type<'_>]>::default(), None, None),
        (
            context,
            Box::from([Type::bool_literal(false)]),
            Some(MaterializationKind::Top),
            Some(tuple),
        ),
        (
            context,
            Box::from([Type::unknown()]),
            Some(MaterializationKind::Bottom),
            None,
        ),
    ] {
        assert!(Specialization::field_work(&fields).is_some());
        let expected_fields = fields.clone();
        let outcome = try_with_execution_budget(
            &db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 1_000_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let values = register_specialization_values(&db, &mut registry)?;
                registry
                    .seal()?
                    .run(|endpoint| async move { Ok(endpoint.intern_value(&values, fields).await) })
            },
        );
        let Ok(AttemptOutcome::Complete(Ok(actual))) = outcome else {
            anyhow::bail!("specialization interning did not complete: {outcome:?}");
        };
        let expected = Specialization::new(
            &db,
            expected_fields.0,
            expected_fields.1,
            expected_fields.2,
            expected_fields.3,
        );
        assert_eq!(actual, expected);
    }
    assert!(
        db.clone()
            .take_salsa_events()
            .iter()
            .all(|event| { !matches!(event.kind, salsa::EventKind::WillExecute { .. }) })
    );
    Ok(())
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

const SOURCE: &str = r#"
from typing import Generic, TypeVar
class Anchor: ...
class Lazy[T = Anchor, U = T]: ...
class Missing[T, U = T]: ...
class Recursive[T = T]: ...
T = TypeVar("T", default=Anchor)
U = TypeVar("U", default=T)
class Legacy(Generic[T, U]): ...
"#;

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/defaults.py", SOURCE)
        .build()
}

fn class<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<StaticClassLiteral<'db>> {
    let env = db.program_environment();
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/defaults.py")?,
        env.program(db),
    );
    let ty = global_symbol(db, file, name)
        .place
        .ignore_possibly_undefined();
    match ty {
        Some(Type::ClassLiteral(ClassLiteral::Static(class))) => Ok(class),
        _ => anyhow::bail!("missing class {name}"),
    }
}

fn context<'db>(db: &'db TestDb, name: &str) -> anyhow::Result<GenericContext<'db>> {
    class(db, name)?
        .generic_context(db)
        .ok_or_else(|| anyhow::anyhow!("missing context {name}"))
}

fn executions(db: &TestDb) -> Vec<String> {
    db.clone()
        .take_salsa_events()
        .into_iter()
        .filter_map(|event| {
            let salsa::EventKind::WillExecute { database_key } = event.kind else {
                return None;
            };
            Some(
                db.ingredient_debug_name(database_key.ingredient_index())
                    .into_owned(),
            )
        })
        .collect()
}

// Keep the original filling decisions independent of the shared operation under test.
fn original_fill<'db>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    types: &[Option<Type<'db>>],
) -> Box<[Type<'db>]> {
    let env = ProgramEnvironment::from_program(context.program(db));
    let types = types.iter().copied();
    let variables = context.variables(db);
    assert_eq!(context.len(db), types.len());
    let mut expanded = Vec::with_capacity(types.len());
    for (ty, variable) in types.zip(variables) {
        let ty = if let Some(ty) = ty {
            ty
        } else if let Some(default) = variable.default_type(db) {
            default.apply_type_mapping(
                db,
                &env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                    generic_context: context,
                    types: (&expanded[..]).into(),
                    skip: None,
                }),
                TypeContext::default(),
            )
        } else {
            match variable.kind(db) {
                TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => {
                    Type::homogeneous_tuple(db, &env, Type::unknown())
                }
                TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => {
                    Type::paramspec_value_callable(db, Parameters::unknown())
                }
                _ => Type::unknown(),
            }
        };
        expanded.push(ty);
    }
    expanded.into_boxed_slice()
}

fn original_partial<'db>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    types: &[Option<Type<'db>>],
) -> Specialization<'db> {
    Specialization::new(db, context, original_fill(db, context, types), None, None)
}

fn original_default<'db>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    known: Option<KnownClass>,
) -> Specialization<'db> {
    let partial = original_partial(db, context, &vec![None; context.len(db)]);
    if known == Some(KnownClass::Tuple) {
        let env = ProgramEnvironment::from_program(context.program(db));
        Specialization::new(
            db,
            context,
            partial.types(db),
            None,
            Some(TupleType::homogeneous(db, &env, Type::unknown())),
        )
    } else {
        partial
    }
}

fn variable(db: &TestDb, index: usize, kind: TypeVarKind) -> BoundTypeVarInstance<'_> {
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            TypeVarIdentity::new(db, Name::new(format!("V{index}")), None, kind),
            None,
            None,
            None,
        ),
        BindingContext::Synthetic(db.program_environment().program(db)),
        None,
        TypeVarNonce::NONE,
    )
}

fn eager_default<'db>(
    db: &'db TestDb,
    variable: BoundTypeVarInstance<'db>,
    default: Type<'db>,
) -> BoundTypeVarInstance<'db> {
    let typevar = variable.typevar(db);
    BoundTypeVarInstance::new(
        db,
        TypeVarInstance::new(
            db,
            typevar.identity(db),
            None,
            typevar.explicit_variance(db),
            Some(TypeVarDefaultEvaluation::Eager(default)),
        ),
        variable.binding_context(db),
        variable.paramspec_attr(db),
        variable.freshness(db),
    )
}

#[test]
fn provided_missing_and_mixed_defaults_match_original_and_keep_owner() -> anyhow::Result<()> {
    let db = database()?;
    for name in ["Lazy", "Missing", "Legacy"] {
        let context = context(&db, name)?;
        let variables = context.variables(&db).collect::<Vec<_>>();
        assert_eq!(variables.len(), 2);
        assert_eq!(
            variables[0].binding_context(&db),
            BindingContext::Definition(class(&db, name)?.definition(&db))
        );
        for inputs in [
            [None, None],
            [Some(Type::int_literal(7)), None],
            [None, Some(Type::int_literal(9))],
            [Some(Type::int_literal(7)), Some(Type::int_literal(9))],
        ] {
            let actual = infallible(specialize_partial_with(&db, context, inputs, &Unrestricted));
            assert_eq!(actual, original_partial(&db, context, &inputs), "{name}");
            assert_eq!(actual.generic_context(&db), context);
            assert_eq!(actual.materialization_kind(&db), None);
            assert_eq!(actual.tuple_inner(&db), None);
        }
        let supplied = infallible(specialize_partial_with(
            &db,
            context,
            [Some(Type::int_literal(7)), None],
            &Unrestricted,
        ));
        assert_eq!(
            supplied.types(&db),
            [Type::int_literal(7), Type::int_literal(7)]
        );
        assert_eq!(
            variables[1].default_type(&db),
            Some(Type::TypeVar(variables[0]))
        );
        let missing = infallible(default_specialization_with(
            &db,
            context,
            None,
            &Unrestricted,
        ));
        assert_eq!(missing.types(&db)[0], missing.types(&db)[1]);
        assert_eq!(missing.types(&db)[0] == Type::unknown(), name == "Missing");
    }
    let lazy = context(&db, "Lazy")?;
    let variables = lazy.variables(&db).collect::<Vec<_>>();
    let first = eager_default(&db, variables[0], Type::int_literal(3));
    let second = eager_default(&db, variables[1], Type::TypeVar(first));
    let eager =
        GenericContext::from_typevar_instances(&db, &db.program_environment(), [first, second]);
    let actual = infallible(default_specialization_with(&db, eager, None, &Unrestricted));
    assert_eq!(actual, original_default(&db, eager, None));
    assert_eq!(
        actual.types(&db),
        [Type::int_literal(3), Type::int_literal(3)]
    );
    Ok(())
}

#[test]
fn lazy_default_source_reads_match_before_validation_reads() -> anyhow::Result<()> {
    for name in ["Lazy", "Legacy"] {
        for provided in [false, true] {
            let run = |shared| -> anyhow::Result<Vec<String>> {
                let db = database()?;
                let context = context(&db, name)?;
                let inputs = [provided.then_some(Type::int_literal(7)), None];
                db.clone().clear_salsa_events();
                let actual = if shared {
                    infallible(specialize_partial_with(&db, context, inputs, &Unrestricted))
                } else {
                    original_partial(&db, context, &inputs)
                };
                let reads = executions(&db);
                assert!(
                    reads
                        .iter()
                        .any(|name| name.contains("bound_typevar_default_type"))
                );
                assert!(
                    reads
                        .iter()
                        .any(|name| name.contains("lazy_default_unchecked"))
                );
                assert_eq!(actual.types(&db)[0], actual.types(&db)[1]);
                Ok(reads)
            };
            assert_eq!(run(false)?, run(true)?, "{name}, provided={provided}");
        }
    }
    Ok(())
}

#[test]
fn supplied_arguments_bypass_even_recursive_defaults_and_remain_partial() -> anyhow::Result<()> {
    let db = database()?;
    let recursive = context(&db, "Recursive")?;
    let context = context(&db, "Lazy")?;
    let variables = context.variables(&db).collect::<Vec<_>>();
    db.clone().clear_salsa_events();
    let supplied = infallible(specialize_partial_with(
        &db,
        recursive,
        [Some(Type::int_literal(4))],
        &Unrestricted,
    ));
    let inputs = [
        Some(Type::TypeVar(variables[1])),
        Some(Type::int_literal(7)),
    ];
    let partial = infallible(specialize_partial_with(&db, context, inputs, &Unrestricted));
    let reads = executions(&db);
    assert!(reads.is_empty(), "supplied values read defaults: {reads:?}");
    assert_eq!(supplied.types(&db), [Type::int_literal(4)]);
    assert_eq!(partial, original_partial(&db, context, &inputs));
    assert_eq!(
        partial.types(&db),
        [Type::TypeVar(variables[1]), Type::int_literal(7)]
    );
    let recursive = context.specialize_recursive(&db, inputs);
    assert_eq!(
        recursive.types(&db),
        [Type::int_literal(7), Type::int_literal(7)]
    );
    assert_ne!(partial, recursive);
    Ok(())
}

#[test]
fn packed_fallbacks_and_tuple_override_preserve_metadata() -> anyhow::Result<()> {
    let db = setup_db();
    let env = db.program_environment();
    let kinds = [
        TypeVarKind::LegacyTypeVar,
        TypeVarKind::Pep695TypeVar,
        TypeVarKind::TypingSelf,
        TypeVarKind::Pep613Alias,
        TypeVarKind::LegacyTypeVarTuple,
        TypeVarKind::Pep695TypeVarTuple,
        TypeVarKind::LegacyParamSpec,
        TypeVarKind::Pep695ParamSpec,
    ];
    let context = GenericContext::from_typevar_instances(
        &db,
        &env,
        kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| variable(&db, index, kind)),
    );
    let actual = infallible(default_specialization_with(
        &db,
        context,
        None,
        &Unrestricted,
    ));
    assert_eq!(actual, original_default(&db, context, None));
    assert_eq!(actual.types(&db).len(), kinds.len());
    assert_eq!(&actual.types(&db)[..4], &[Type::unknown(); 4]);
    let tuple = Type::homogeneous_tuple(&db, &env, Type::unknown());
    assert_eq!(&actual.types(&db)[4..6], &[tuple; 2]);
    for ty in &actual.types(&db)[6..] {
        let Type::Callable(callable) = ty else {
            anyhow::bail!("ParamSpec was not packed");
        };
        assert_eq!(callable.kind(&db), CallableTypeKind::ParamSpecValue);
        let [signature] = callable.signatures(&db).overloads.as_slice() else {
            anyhow::bail!("unexpected overloads");
        };
        assert_eq!(signature.parameters().iter().count(), 2);
        assert!(
            signature
                .parameters()
                .iter()
                .all(|parameter| parameter.annotated_type() == Type::unknown())
        );
        assert_ne!(
            *ty,
            Type::paramspec_value_callable(&db, Parameters::gradual_form())
        );
    }
    let supplied = RecordingEffects::default();
    let retained = specialize_partial_with(
        &db,
        context,
        actual.types(&db).iter().copied().map(Some),
        &supplied,
    )
    .map_err(|()| anyhow::anyhow!("unexpected supplied-value refusal"))?;
    assert_eq!(retained, actual);
    assert!(!supplied.events.borrow().iter().any(|event| matches!(
        event,
        Event::Default(_) | Event::Map { .. } | Event::Tuple | Event::ParamSpec
    )));
    let tuple_context = GenericContext::from_typevar_instances(
        &db,
        &env,
        [variable(&db, 20, TypeVarKind::LegacyTypeVar)],
    );
    let regular = infallible(default_specialization_with(
        &db,
        tuple_context,
        None,
        &Unrestricted,
    ));
    let tuple = infallible(default_specialization_with(
        &db,
        tuple_context,
        Some(KnownClass::Tuple),
        &Unrestricted,
    ));
    assert_eq!(
        tuple,
        original_default(&db, tuple_context, Some(KnownClass::Tuple))
    );
    assert_eq!(tuple.types(&db), regular.types(&db));
    assert_eq!(tuple.generic_context(&db), tuple_context);
    assert_eq!(tuple.materialization_kind(&db), None);
    assert_eq!(regular.tuple_inner(&db), None);
    assert_eq!(
        tuple.tuple_inner(&db),
        Some(TupleType::homogeneous(&db, &env, Type::unknown()))
    );
    Ok(())
}

#[test]
fn zero_and_mismatched_arity_retain_the_original_invariant() {
    let db = setup_db();
    let env = db.program_environment();
    let empty = GenericContext::from_typevar_instances(&db, &env, []);
    let one = GenericContext::from_typevar_instances(
        &db,
        &env,
        [variable(&db, 0, TypeVarKind::LegacyTypeVar)],
    );
    let values = [Some(Type::int_literal(1)), None];
    for context in [empty, one] {
        for len in 0..=values.len() {
            let original = catch_unwind(AssertUnwindSafe(|| {
                original_fill(&db, context, &values[..len])
            }));
            let shared = catch_unwind(AssertUnwindSafe(|| {
                infallible(fill_in_defaults_with(
                    &db,
                    context,
                    values[..len].iter().copied(),
                    &Unrestricted,
                ))
            }));
            assert_eq!(shared.is_err(), len != context.len(&db));
            assert_eq!(shared.is_err(), original.is_err());
            if let (Ok(original), Ok(shared)) = (original, shared) {
                assert_eq!(shared, original);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Dependency {
    Default,
    Map,
    Tuple,
    ParamSpec,
    Intern,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event<'db> {
    Work(DefaultSpecializationWork),
    Default(BoundTypeVarInstance<'db>),
    Map {
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: Vec<Type<'db>>,
    },
    Tuple,
    ParamSpec,
    Intern {
        context: GenericContext<'db>,
        types: Vec<Type<'db>>,
        borrowed: bool,
        tuple: Option<TupleType<'db>>,
    },
}

#[derive(Default)]
struct RecordingEffects<'db> {
    events: RefCell<Vec<Event<'db>>>,
    refuse_at: Option<usize>,
    stop_after: Option<Dependency>,
    stopped: Cell<bool>,
}

impl<'db> RecordingEffects<'db> {
    fn event(&self, event: Event<'db>) -> Result<(), ()> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.stopped.get() || self.refuse_at == Some(index) {
            Err(())
        } else {
            Ok(())
        }
    }

    fn returned(&self, dependency: Dependency) {
        if self.stop_after == Some(dependency) {
            self.stopped.set(true);
        }
    }
}

impl<'db> DefaultSpecializationEffects<'db> for RecordingEffects<'db> {
    type Error = ();

    fn checkpoint(&self, work: DefaultSpecializationWork) -> Result<(), ()> {
        self.event(Event::Work(work))
    }

    fn default_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Option<Type<'db>>, ()> {
        self.event(Event::Default(variable))?;
        let result = infallible(Unrestricted.default_type(db, env, variable));
        self.returned(Dependency::Default);
        Ok(result)
    }

    fn map_default(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        context: GenericContext<'db>,
        prefix: &[Type<'db>],
    ) -> Result<Type<'db>, ()> {
        self.event(Event::Map {
            default,
            context,
            prefix: prefix.to_vec(),
        })?;
        let result = infallible(Unrestricted.map_default(db, env, default, context, prefix));
        self.returned(Dependency::Map);
        Ok(result)
    }

    fn unknown_tuple(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<TupleType<'db>, ()> {
        self.event(Event::Tuple)?;
        let result = infallible(Unrestricted.unknown_tuple(db, env));
        self.returned(Dependency::Tuple);
        Ok(result)
    }

    fn unknown_paramspec(&self, db: &'db dyn Db) -> Result<Type<'db>, ()> {
        self.event(Event::ParamSpec)?;
        let result = infallible(Unrestricted.unknown_paramspec(db));
        self.returned(Dependency::ParamSpec);
        Ok(result)
    }

    fn intern_specialization(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        types: Cow<'_, [Type<'db>]>,
        tuple: Option<TupleType<'db>>,
    ) -> Result<Specialization<'db>, ()> {
        self.event(Event::Intern {
            context,
            types: types.to_vec(),
            borrowed: matches!(types, Cow::Borrowed(_)),
            tuple,
        })?;
        let result = infallible(Unrestricted.intern_specialization(db, context, types, tuple));
        self.returned(Dependency::Intern);
        Ok(result)
    }
}

#[test]
fn recording_effects_preserve_prefixes_and_every_refusal_stops() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let lazy = context(&db, "Lazy")?;
    let source_variables = lazy.variables(&db).collect::<Vec<_>>();
    let packed = GenericContext::from_typevar_instances(
        &db,
        &env,
        [
            variable(&db, 0, TypeVarKind::LegacyTypeVarTuple),
            variable(&db, 1, TypeVarKind::LegacyParamSpec),
            variable(&db, 2, TypeVarKind::LegacyTypeVar),
        ],
    );
    for (context, known) in [
        (lazy, None),
        (packed, None),
        (lazy, Some(KnownClass::Tuple)),
    ] {
        let complete = RecordingEffects::default();
        let expected = default_specialization_with(&db, context, known, &complete)
            .map_err(|()| anyhow::anyhow!("unexpected complete refusal"))?;
        assert_eq!(expected, original_default(&db, context, known));
        let trace = complete.events.into_inner();
        assert_eq!(
            trace.last(),
            Some(&Event::Work(DefaultSpecializationWork::Publish))
        );
        if context == lazy {
            let mappings = trace
                .iter()
                .filter_map(|event| match event {
                    Event::Map {
                        default,
                        context,
                        prefix,
                    } => Some((*default, *context, prefix.as_slice())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(mappings.len(), 2);
            assert_eq!(mappings[0].1, context);
            assert!(mappings[0].2.is_empty());
            assert_eq!(
                mappings[1],
                (
                    Type::TypeVar(source_variables[0]),
                    context,
                    &expected.types(&db)[..1]
                )
            );
        }
        let internings = trace
            .iter()
            .filter_map(|event| match event {
                Event::Intern {
                    borrowed, tuple, ..
                } => Some((*borrowed, *tuple)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(internings[0], (false, None));
        assert_eq!(
            internings.len(),
            if known == Some(KnownClass::Tuple) {
                2
            } else {
                1
            }
        );
        if known == Some(KnownClass::Tuple) {
            assert_eq!(internings[1], (true, expected.tuple_inner(&db)));
        }
        for index in 0..trace.len() {
            let effects = RecordingEffects {
                refuse_at: Some(index),
                ..RecordingEffects::default()
            };
            assert_eq!(
                default_specialization_with(&db, context, known, &effects),
                Err(())
            );
            assert_eq!(*effects.events.borrow(), trace[..=index]);
        }
        for _ in 0..2 {
            let retry = RecordingEffects::default();
            assert_eq!(
                default_specialization_with(&db, context, known, &retry),
                Ok(expected)
            );
            assert_eq!(*retry.events.borrow(), trace);
        }
    }
    let effects = RecordingEffects::default();
    let inputs = [Some(Type::int_literal(7)), None];
    let result = specialize_partial_with(&db, lazy, inputs, &effects)
        .map_err(|()| anyhow::anyhow!("unexpected partial refusal"))?;
    assert_eq!(
        result.types(&db),
        [Type::int_literal(7), Type::int_literal(7)]
    );
    assert_eq!(
        effects
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                Event::Default(variable) => Some(*variable),
                _ => None,
            })
            .collect::<Vec<_>>(),
        [source_variables[1]]
    );
    assert!(effects.events.borrow().contains(&Event::Map {
        default: Type::TypeVar(source_variables[0]),
        context: lazy,
        prefix: vec![Type::int_literal(7)],
    }));
    Ok(())
}

#[test]
fn marked_dependency_results_are_rejected_before_fallback_or_append() -> anyhow::Result<()> {
    let db = database()?;
    let env = db.program_environment();
    let lazy = context(&db, "Lazy")?;
    let tuple = GenericContext::from_typevar_instances(
        &db,
        &env,
        [variable(&db, 0, TypeVarKind::Pep695TypeVarTuple)],
    );
    let paramspec = GenericContext::from_typevar_instances(
        &db,
        &env,
        [variable(&db, 1, TypeVarKind::Pep695ParamSpec)],
    );
    for (context, known, stop_after) in [
        (lazy, None, Dependency::Default),
        (tuple, None, Dependency::Default),
        (lazy, None, Dependency::Map),
        (tuple, None, Dependency::Tuple),
        (paramspec, None, Dependency::ParamSpec),
        (lazy, Some(KnownClass::Tuple), Dependency::Intern),
    ] {
        let effects = RecordingEffects {
            stop_after: Some(stop_after),
            ..RecordingEffects::default()
        };
        assert_eq!(
            default_specialization_with(&db, context, known, &effects),
            Err(())
        );
        assert!(effects.stopped.get());
        let events = effects.events.borrow();
        assert_eq!(
            events.last(),
            Some(&Event::Work(DefaultSpecializationWork::Resume))
        );
        let preceding = &events[events.len() - 2];
        assert!(matches!(
            (stop_after, preceding),
            (Dependency::Default, Event::Default(_))
                | (Dependency::Map, Event::Map { .. })
                | (Dependency::Tuple, Event::Tuple)
                | (Dependency::ParamSpec, Event::ParamSpec)
                | (Dependency::Intern, Event::Intern { .. })
        ));
    }
    Ok(())
}
