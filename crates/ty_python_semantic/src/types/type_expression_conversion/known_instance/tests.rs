use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion, name::Name};

use super::*;
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::place::global_symbol;
use crate::types::class::NamedTupleSpec;
use crate::types::constraints::{OwnedConstraintSet, TypeVarSolution};
use crate::types::dedicated::pydantic::ConfigBoolean;
use crate::types::known_instance::{
    DeprecatedInstance, FieldInstance, FunctoolsPartialInstance, InternedConstraintSet,
    InternedConstraintSetSolution, MethodWrapper, MethodWrapperKind, SentinelInstance,
};
use crate::types::newtype::NewType;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{TypeVarIdentity, TypeVarNonce};
use crate::types::{BindingContext, CallableType, GenericContext};

struct Fixture<'db> {
    file: ProgramFile<'db>,
    scope: ScopeId<'db>,
    definition: Definition<'db>,
}

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            "/src/conversion.py",
            "class Owner: pass\ntype Alias = int\n",
        )
        .build()
}

fn fixture(db: &TestDb) -> anyhow::Result<Fixture<'_>> {
    let file = db.program_file(system_path_to_file(db, "/src/conversion.py")?);
    let module = parsed_module(db, file.python_file(db)).load(db);
    let ast::Stmt::ClassDef(class) = &module.suite()[0] else {
        anyhow::bail!("expected class definition");
    };
    let index = semantic_index(db, file);
    Ok(Fixture {
        file,
        scope: index.scope_id(FileScopeId::global()),
        definition: index.expect_single_definition(class),
    })
}

fn variable(db: &TestDb, kind: TypeVarKind) -> TypeVarInstance<'_> {
    TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static("T"), None, kind),
        None,
        None,
        None,
    )
}

struct Trace<'db> {
    db: &'db TestDb,
    file: ProgramFile<'db>,
    events: RefCell<Vec<&'static str>>,
    refuse: Option<usize>,
    bound: Option<BoundTypeVarInstance<'db>>,
    bindings: RefCell<Vec<(FileScopeId, Option<Definition<'db>>, TypeVarInstance<'db>)>>,
}

impl<'db> Trace<'db> {
    fn new(db: &'db TestDb, file: ProgramFile<'db>) -> Self {
        Self {
            db,
            file,
            events: RefCell::default(),
            refuse: None,
            bound: None,
            bindings: RefCell::default(),
        }
    }

    fn record(&self, event: &'static str) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        events.push(event);
        if self.refuse == Some(events.len() - 1) {
            Err(event)
        } else {
            Ok(())
        }
    }
}

macro_rules! trace_effects {
    ($trait:ident $(, $async:ident)?) => {
        impl<'db> $trait<'db> for Trace<'db> {
            type Error = &'static str;

            $($async)? fn dispatch(&self) -> Result<(), Self::Error> {
                self.record("dispatch")
            }

            $($async)? fn kind(&self, variable: TypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error> {
                self.record("kind")?;
                Ok(variable.kind(self.db))
            }

            $($async)? fn scope_program_file(&self, scope: ScopeId<'db>) -> Result<ProgramFile<'db>, Self::Error> {
                self.record("program file")?;
                Ok(scope.program_file(self.db))
            }

            $($async)? fn semantic_index(&self, file: ProgramFile<'db>) -> Result<&'db SemanticIndex<'db>, Self::Error> {
                self.record("semantic index")?;
                assert_eq!(file, self.file);
                Ok(semantic_index(self.db, file))
            }

            $($async)? fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> Result<FileScopeId, Self::Error> {
                self.record("file scope")?;
                Ok(scope.file_scope_id(self.db))
            }

            $($async)? fn bind_typevar(
                &self,
                index: &SemanticIndex<'db>,
                scope: FileScopeId,
                binding: Option<Definition<'db>>,
                variable: TypeVarInstance<'db>,
            ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
                self.record("binding")?;
                assert!(std::ptr::eq(index, semantic_index(self.db, self.file)));
                self.bindings.borrow_mut().push((scope, binding, variable));
                Ok(self.bound)
            }

            $($async)? fn interned_inner(&self, inner: InternedType<'db>) -> Result<Type<'db>, Self::Error> {
                self.record("inner")?;
                Ok(inner.inner(self.db))
            }

            $($async)? fn union_result(&self, union: UnionTypeInstance<'db>) -> Result<Result<Type<'db>, InvalidTypeExpressionError<'db>>, Self::Error> {
                self.record("union result")?;
                Ok(union.union_type(self.db).clone())
            }

            $($async)? fn to_meta_type(&self, ty: Type<'db>, env: &ProgramEnvironment<'db>) -> Result<Type<'db>, Self::Error> {
                self.record("meta type")?;
                assert_eq!(env.program(self.db), self.file.program(self.db));
                Ok(ty.to_meta_type(self.db, env))
            }
        }
    };
}

trace_effects!(SynchronousKnownInstanceConversionEffects);
trace_effects!(KnownInstanceConversionEffects, async);

fn invalid(invalid: InvalidTypeExpression<'_>) -> Result<Type<'_>, InvalidTypeExpressionError<'_>> {
    Err(InvalidTypeExpressionError {
        invalid_expressions: smallvec_inline![invalid],
        fallback_type: Type::unknown(),
    })
}

#[test]
fn complete_known_instance_dispatch_preserves_values_errors_and_effects() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    let env = db.program_environment();
    let context = GenericContext::from_typevar_instances(&db, &env, []);
    let callable = CallableType::unknown(&db);
    let inner = InternedType::new(&db, Type::Never);
    let partial = FunctoolsPartialInstance::new(&db, inner, callable);
    let sentinel = SentinelInstance::new(&db, Name::new_static("sentinel"), fixture.definition);
    let newtype = NewType::new(&db, Name::new_static("New"), fixture.definition, None);
    let Some(Type::KnownInstance(KnownInstanceType::TypeAliasType(alias))) =
        global_symbol(&db, fixture.file, "Alias")
            .place
            .ignore_possibly_undefined()
    else {
        anyhow::bail!("expected type alias");
    };
    let typevar = variable(&db, TypeVarKind::LegacyTypeVar);
    let stored_error = InvalidTypeExpressionError {
        invalid_expressions: [
            InvalidTypeExpression::Deprecated,
            InvalidTypeExpression::Generic,
            InvalidTypeExpression::Field,
        ]
        .into_iter()
        .collect(),
        fallback_type: Type::AlwaysTruthy,
    };
    let mut cases = vec![
        (
            KnownInstanceType::TypeAliasType(alias),
            Ok(Type::TypeAlias(alias)),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::NewType(newtype),
            Ok(Type::NewTypeInstance(newtype)),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::Callable(callable),
            Ok(Type::Callable(callable)),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::Sentinel(sentinel),
            Ok(Type::KnownInstance(KnownInstanceType::Sentinel(sentinel))),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::TypeVar(typevar),
            Ok(Type::KnownInstance(KnownInstanceType::TypeVar(typevar))),
            vec![
                "dispatch",
                "kind",
                "kind",
                "program file",
                "semantic index",
                "file scope",
                "binding",
            ],
        ),
        (
            KnownInstanceType::Literal(inner),
            Ok(Type::Never),
            vec!["dispatch", "inner"],
        ),
        (
            KnownInstanceType::Annotated(inner),
            Ok(Type::Never),
            vec!["dispatch", "inner"],
        ),
        (
            KnownInstanceType::LiteralStringAlias(inner),
            Ok(Type::Never),
            vec!["dispatch", "inner"],
        ),
        (
            KnownInstanceType::TypeGenericAlias(inner),
            Ok(Type::Never),
            vec!["dispatch", "inner", "meta type"],
        ),
        (
            KnownInstanceType::UnionType(UnionTypeInstance::new(&db, None, Ok(Type::Never))),
            Ok(Type::Never),
            vec!["dispatch", "union result"],
        ),
        (
            KnownInstanceType::UnionType(UnionTypeInstance::new(
                &db,
                None,
                Err(stored_error.clone()),
            )),
            Err(stored_error),
            vec!["dispatch", "union result"],
        ),
        (
            KnownInstanceType::Deprecated(DeprecatedInstance { message: None }),
            invalid(InvalidTypeExpression::Deprecated),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::Field(FieldInstance::new(
                &db,
                None,
                false,
                None,
                None,
                None,
                ConfigBoolean::default(),
            )),
            invalid(InvalidTypeExpression::Field),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::ConstraintSet(InternedConstraintSet::new(
                &db,
                OwnedConstraintSet::always(),
            )),
            invalid(InvalidTypeExpression::ConstraintSet),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::ConstraintSetSolution(InternedConstraintSetSolution::new(
                &db,
                Box::<[TypeVarSolution<'_>]>::default(),
            )),
            invalid(InvalidTypeExpression::ConstraintSetSolution),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::GenericContext(context),
            invalid(InvalidTypeExpression::GenericContext),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::Specialization(context.default_specialization(&db, None)),
            invalid(InvalidTypeExpression::Specialization),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::SubscriptedProtocol(context),
            invalid(InvalidTypeExpression::Protocol),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::SubscriptedGeneric(context),
            invalid(InvalidTypeExpression::Generic),
            vec!["dispatch"],
        ),
        (
            KnownInstanceType::NamedTupleSpec(NamedTupleSpec::unknown(&db)),
            invalid(InvalidTypeExpression::NamedTupleSpec),
            vec!["dispatch"],
        ),
    ];
    for known in [
        KnownInstanceType::FunctoolsPartial(partial),
        KnownInstanceType::FunctoolsPartialCall(partial),
        KnownInstanceType::MethodWrapper(MethodWrapper::new(
            &db,
            Type::Never,
            MethodWrapperKind::Staticmethod,
        )),
        KnownInstanceType::Range {
            is_non_empty: false,
        },
        KnownInstanceType::Range { is_non_empty: true },
    ] {
        cases.push((
            known,
            invalid(InvalidTypeExpression::InvalidType(
                Type::KnownInstance(known),
                fixture.scope,
            )),
            vec!["dispatch"],
        ));
    }
    for (known, expected, events) in cases {
        let ordinary = in_type_expression_known_instance_sync(
            known,
            fixture.scope,
            None,
            InferenceFlags::empty(),
            KnownInstanceConversionFacts,
            &InlineConversion { db: &db },
        );
        assert_eq!(ordinary, Ok(expected.clone()), "{known:?}");
        for refusal in std::iter::once(None).chain((0..events.len()).map(Some)) {
            let effects = Trace {
                refuse: refusal,
                ..Trace::new(&db, fixture.file)
            };
            let asynchronous = try_poll_immediate(in_type_expression_known_instance_with(
                known,
                fixture.scope,
                None,
                InferenceFlags::empty(),
                KnownInstanceConversionFacts,
                &effects,
            ));
            let expected_result =
                refusal.map_or_else(|| Ok(expected.clone()), |index| Err(events[index]));
            let expected_events = &events[..refusal.map_or(events.len(), |index| index + 1)];
            assert_eq!(
                asynchronous,
                Poll::Ready(expected_result.clone()),
                "{known:?}"
            );
            assert_eq!(*effects.events.borrow(), expected_events, "{known:?}");
            effects.events.borrow_mut().clear();
            let synchronous = in_type_expression_known_instance_sync(
                known,
                fixture.scope,
                None,
                InferenceFlags::empty(),
                KnownInstanceConversionFacts,
                &effects,
            );
            assert_eq!(synchronous, expected_result, "{known:?}");
            assert_eq!(*effects.events.borrow(), expected_events, "{known:?}");
        }
    }
    Ok(())
}

#[test]
fn kind_guards_short_circuit_before_binding_dependencies() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    for (kind, flags, expected_kind_reads, error) in [
        (
            TypeVarKind::LegacyParamSpec,
            InferenceFlags::empty(),
            1,
            Some(true),
        ),
        (
            TypeVarKind::LegacyParamSpec,
            InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
            1,
            Some(true),
        ),
        (
            TypeVarKind::LegacyParamSpec,
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
            1,
            None,
        ),
        (
            TypeVarKind::Pep695ParamSpec,
            InferenceFlags::empty(),
            1,
            Some(true),
        ),
        (
            TypeVarKind::LegacyTypeVarTuple,
            InferenceFlags::empty(),
            2,
            Some(false),
        ),
        (
            TypeVarKind::LegacyTypeVarTuple,
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR,
            1,
            Some(false),
        ),
        (
            TypeVarKind::LegacyTypeVarTuple,
            InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
            1,
            None,
        ),
        (
            TypeVarKind::Pep695TypeVarTuple,
            InferenceFlags::empty(),
            2,
            Some(false),
        ),
        (
            TypeVarKind::LegacyTypeVar,
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR | InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
            0,
            None,
        ),
        (
            TypeVarKind::LegacyParamSpec,
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR | InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
            0,
            None,
        ),
        (
            TypeVarKind::LegacyTypeVarTuple,
            InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR | InferenceFlags::IN_UNPACK_TYPE_ARGUMENT,
            0,
            None,
        ),
    ] {
        let variable = variable(&db, kind);
        let known = KnownInstanceType::TypeVar(variable);
        let expected = match error {
            Some(true) => invalid(InvalidTypeExpression::InvalidBareParamSpec(variable)),
            Some(false) => invalid(InvalidTypeExpression::InvalidBareTypeVarTuple(variable)),
            None => Ok(Type::KnownInstance(known)),
        };
        let effects = Trace::new(&db, fixture.file);
        let mut events = vec!["dispatch"];
        events.extend(std::iter::repeat_n("kind", expected_kind_reads));
        if error.is_none() {
            events.extend(["program file", "semantic index", "file scope", "binding"]);
        }
        let asynchronous = try_poll_immediate(in_type_expression_known_instance_with(
            known,
            fixture.scope,
            None,
            flags,
            KnownInstanceConversionFacts,
            &effects,
        ));
        assert_eq!(asynchronous, Poll::Ready(Ok(expected.clone())));
        assert_eq!(*effects.events.borrow(), events);
        effects.events.borrow_mut().clear();
        assert_eq!(
            in_type_expression_known_instance_sync(
                known,
                fixture.scope,
                None,
                flags,
                KnownInstanceConversionFacts,
                &effects,
            ),
            Ok(expected.clone())
        );
        assert_eq!(*effects.events.borrow(), events);
        assert_eq!(
            in_type_expression_known_instance_sync(
                known,
                fixture.scope,
                None,
                flags,
                KnownInstanceConversionFacts,
                &InlineConversion { db: &db },
            ),
            Ok(expected)
        );
    }
    Ok(())
}

#[test]
fn binding_preserves_scope_definition_and_original_variable() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    let variable = variable(&db, TypeVarKind::LegacyTypeVar);
    let bound = BoundTypeVarInstance::new(
        &db,
        variable,
        BindingContext::Definition(fixture.definition),
        None,
        TypeVarNonce::NONE,
    );
    for result in [None, Some(bound)] {
        let effects = Trace {
            bound: result,
            ..Trace::new(&db, fixture.file)
        };
        let known = KnownInstanceType::TypeVar(variable);
        let expected = result
            .map(Type::TypeVar)
            .unwrap_or(Type::KnownInstance(known));
        let actual = try_poll_immediate(in_type_expression_known_instance_with(
            known,
            fixture.scope,
            Some(fixture.definition),
            InferenceFlags::empty(),
            KnownInstanceConversionFacts,
            &effects,
        ));
        assert_eq!(actual, Poll::Ready(Ok(Ok(expected))));
        assert_eq!(
            *effects.bindings.borrow(),
            [(FileScopeId::global(), Some(fixture.definition), variable)]
        );
        assert_eq!(
            *effects.events.borrow(),
            [
                "dispatch",
                "kind",
                "kind",
                "program file",
                "semantic index",
                "file scope",
                "binding"
            ]
        );
    }
    assert_eq!(
        in_type_expression_known_instance_sync(
            KnownInstanceType::TypeVar(variable),
            fixture.scope,
            Some(fixture.definition),
            InferenceFlags::empty(),
            KnownInstanceConversionFacts,
            &InlineConversion { db: &db },
        ),
        Ok(Ok(Type::TypeVar(bound)))
    );
    Ok(())
}
