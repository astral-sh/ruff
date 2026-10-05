use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion, name::Name};
use smallvec::smallvec_inline;
use ty_python_core::{FileScopeId, ProgramFile, semantic_index};

use super::*;
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder};
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::special_form::{LegacyStdlibAlias, TypeQualifier};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance, TypeVarNonce};
use crate::types::{BindingContext, BoundTypeVarInstance, TypeVarKind, TypingModule};

struct Fixture<'db> {
    file: ProgramFile<'db>,
    scope: ScopeId<'db>,
    definition: Definition<'db>,
}

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/conversion.py", "class Owner: pass\n")
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event<'db> {
    Dispatch,
    KnownClass(KnownClass),
    Tuple(Type<'db>),
    TypeForm(Type<'db>),
    Intersection(Type<'db>, Type<'db>),
    Callable,
    TypingSelf(ScopeId<'db>, Option<Definition<'db>>, InferenceFlags),
}

struct Trace<'db> {
    db: &'db TestDb,
    file: ProgramFile<'db>,
    events: RefCell<Vec<Event<'db>>>,
    refuse: Option<usize>,
    self_result: Result<Type<'db>, InvalidTypeExpression<'db>>,
}

impl<'db> Trace<'db> {
    fn record(&self, event: Event<'db>) -> Result<(), Event<'db>> {
        let mut events = self.events.borrow_mut();
        events.push(event);
        if self.refuse == Some(events.len() - 1) {
            Err(event)
        } else {
            Ok(())
        }
    }

    fn check_environment(&self, env: &ProgramEnvironment<'db>) {
        assert_eq!(env.program(self.db), self.file.program(self.db));
    }
}

macro_rules! trace_effects {
    ($trait:ident $(, $async:ident)?) => {
        impl<'db> $trait<'db> for Trace<'db> {
            type Error = Event<'db>;

            $($async)? fn dispatch(&self) -> Result<(), Self::Error> {
                self.record(Event::Dispatch)
            }

            $($async)? fn known_class_instance(
                &self,
                env: &ProgramEnvironment<'db>,
                class: KnownClass,
            ) -> Result<Type<'db>, Self::Error> {
                self.record(Event::KnownClass(class))?;
                self.check_environment(env);
                Ok(class.to_instance(self.db, env))
            }

            $($async)? fn homogeneous_tuple(
                &self,
                env: &ProgramEnvironment<'db>,
                element: Type<'db>,
            ) -> Result<Type<'db>, Self::Error> {
                self.record(Event::Tuple(element))?;
                self.check_environment(env);
                Ok(Type::homogeneous_tuple(self.db, env, element))
            }

            $($async)? fn type_form(&self, argument: Type<'db>) -> Result<Type<'db>, Self::Error> {
                self.record(Event::TypeForm(argument))?;
                Ok(TypeFormType::from_type_expression(self.db, argument))
            }

            $($async)? fn intersection(
                &self,
                env: &ProgramEnvironment<'db>,
                left: Type<'db>,
                right: Type<'db>,
            ) -> Result<Type<'db>, Self::Error> {
                self.record(Event::Intersection(left, right))?;
                self.check_environment(env);
                Ok(IntersectionType::from_two_elements(self.db, env, left, right))
            }

            $($async)? fn unknown_callable(&self) -> Result<Type<'db>, Self::Error> {
                self.record(Event::Callable)?;
                Ok(Type::Callable(CallableType::unknown(self.db)))
            }

            $($async)? fn typing_self(
                &self,
                scope: ScopeId<'db>,
                binding: Option<Definition<'db>>,
                flags: InferenceFlags,
            ) -> Result<Result<Type<'db>, InvalidTypeExpression<'db>>, Self::Error> {
                self.record(Event::TypingSelf(scope, binding, flags))?;
                Ok(self.self_result)
            }
        }
    };
}

trace_effects!(SynchronousSpecialFormConversionEffects);
trace_effects!(SpecialFormConversionEffects, async);

struct Case<'db> {
    special_form: SpecialFormType,
    flags: InferenceFlags,
    expected: Result<Type<'db>, InvalidTypeExpression<'db>>,
    fallback: Type<'db>,
    events: Vec<Event<'db>>,
}

impl<'db> Case<'db> {
    fn valid(special_form: SpecialFormType, expected: Type<'db>) -> Self {
        Self {
            special_form,
            flags: InferenceFlags::empty(),
            expected: Ok(expected),
            fallback: Type::unknown(),
            events: vec![Event::Dispatch],
        }
    }

    fn invalid(special_form: SpecialFormType, expected: InvalidTypeExpression<'db>) -> Self {
        Self {
            special_form,
            flags: InferenceFlags::empty(),
            expected: Err(expected),
            fallback: Type::unknown(),
            events: vec![Event::Dispatch],
        }
    }

    fn wrapped(&self) -> Result<Type<'db>, InvalidTypeExpressionError<'db>> {
        self.expected.map_err(|error| InvalidTypeExpressionError {
            invalid_expressions: smallvec_inline![error],
            fallback_type: self.fallback,
        })
    }
}

fn assert_case<'db>(
    db: &'db TestDb,
    fixture: &Fixture<'db>,
    binding: Option<Definition<'db>>,
    case: &Case<'db>,
) {
    for refusal in std::iter::once(None).chain((0..case.events.len()).map(Some)) {
        let effects = Trace {
            db,
            file: fixture.file,
            events: RefCell::default(),
            refuse: refusal,
            self_result: case.expected,
        };
        let expected_raw = refusal.map_or(Ok(case.expected), |index| Err(case.events[index]));
        let expected_wrapped =
            refusal.map_or_else(|| Ok(case.wrapped()), |index| Err(case.events[index]));
        let expected_events = &case.events[..refusal.map_or(case.events.len(), |index| index + 1)];

        assert_eq!(
            try_poll_immediate(special_form_type_expression_with(
                case.special_form,
                fixture.scope,
                binding,
                case.flags,
                SpecialFormConversionFacts,
                &effects,
            )),
            Poll::Ready(expected_raw),
            "raw async: {:?}, refusal {refusal:?}",
            case.special_form,
        );
        assert_eq!(*effects.events.borrow(), expected_events);
        effects.events.borrow_mut().clear();

        assert_eq!(
            special_form_type_expression_sync(
                case.special_form,
                fixture.scope,
                binding,
                case.flags,
                SpecialFormConversionFacts,
                &effects,
            ),
            expected_raw,
            "raw sync: {:?}, refusal {refusal:?}",
            case.special_form,
        );
        assert_eq!(*effects.events.borrow(), expected_events);
        effects.events.borrow_mut().clear();

        assert_eq!(
            try_poll_immediate(in_type_expression_special_form_with(
                case.special_form,
                fixture.scope,
                binding,
                case.flags,
                SpecialFormConversionFacts,
                &effects,
            )),
            Poll::Ready(expected_wrapped.clone()),
            "wrapped async: {:?}, refusal {refusal:?}",
            case.special_form,
        );
        assert_eq!(*effects.events.borrow(), expected_events);
        effects.events.borrow_mut().clear();

        assert_eq!(
            in_type_expression_special_form_sync(
                case.special_form,
                fixture.scope,
                binding,
                case.flags,
                SpecialFormConversionFacts,
                &effects,
            ),
            expected_wrapped,
            "wrapped sync: {:?}, refusal {refusal:?}",
            case.special_form,
        );
        assert_eq!(*effects.events.borrow(), expected_events);
    }
}

fn assert_ordinary_case<'db>(db: &'db TestDb, fixture: &Fixture<'db>, case: &Case<'db>) {
    assert_eq!(
        special_form_type_expression_sync(
            case.special_form,
            fixture.scope,
            Some(fixture.definition),
            case.flags,
            SpecialFormConversionFacts,
            &InlineConversion { db },
        ),
        Ok(case.expected),
        "ordinary raw: {:?}",
        case.special_form,
    );
    assert_eq!(
        in_type_expression_special_form_sync(
            case.special_form,
            fixture.scope,
            Some(fixture.definition),
            case.flags,
            SpecialFormConversionFacts,
            &InlineConversion { db },
        ),
        Ok(case.wrapped()),
        "ordinary wrapped: {:?}",
        case.special_form,
    );
}

#[test]
fn finite_dispatch_preserves_values_errors_and_fallbacks() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    let mut cases = vec![
        Case::valid(SpecialFormType::Never, Type::Never),
        Case::valid(SpecialFormType::NoReturn, Type::Never),
        Case::valid(SpecialFormType::LiteralString, Type::literal_string()),
        Case::valid(SpecialFormType::Any, Type::any()),
        Case::valid(SpecialFormType::Unknown, Type::unknown()),
        Case::valid(SpecialFormType::AlwaysTruthy, Type::AlwaysTruthy),
        Case::valid(SpecialFormType::AlwaysFalsy, Type::AlwaysFalsy),
        Case::invalid(SpecialFormType::TypeAlias, InvalidTypeExpression::TypeAlias),
        Case::invalid(SpecialFormType::Protocol, InvalidTypeExpression::Protocol),
        Case::invalid(SpecialFormType::Generic, InvalidTypeExpression::Generic),
        Case::invalid(
            SpecialFormType::Annotated,
            InvalidTypeExpression::RequiresTwoArguments(SpecialFormType::Annotated),
        ),
        Case {
            fallback: Type::Dynamic(DynamicType::InvalidConcatenateUnknown),
            ..Case::invalid(
                SpecialFormType::Concatenate,
                InvalidTypeExpression::Concatenate,
            )
        },
        Case {
            flags: InferenceFlags::IN_VALID_CONCATENATE_CONTEXT,
            fallback: Type::Dynamic(DynamicType::InvalidConcatenateUnknown),
            ..Case::invalid(
                SpecialFormType::Concatenate,
                InvalidTypeExpression::RequiresTwoArguments(SpecialFormType::Concatenate),
            )
        },
    ];
    for special_form in [SpecialFormType::Divergent, SpecialFormType::Todo] {
        cases.push(Case::invalid(
            special_form,
            InvalidTypeExpression::InvalidType(Type::SpecialForm(special_form), fixture.scope),
        ));
    }
    for module in [TypingModule::Typing, TypingModule::TypingExtensions] {
        cases.push(Case::invalid(
            SpecialFormType::TypedDict(module),
            InvalidTypeExpression::TypedDict,
        ));
    }
    for special_form in [
        SpecialFormType::Literal,
        SpecialFormType::Union,
        SpecialFormType::Intersection,
    ] {
        cases.push(Case::invalid(
            special_form,
            InvalidTypeExpression::RequiresArguments(special_form),
        ));
    }
    for special_form in [
        SpecialFormType::Optional,
        SpecialFormType::Not,
        SpecialFormType::Top,
        SpecialFormType::Bottom,
        SpecialFormType::TypeOf,
        SpecialFormType::TypeIs,
        SpecialFormType::TypeGuard,
        SpecialFormType::Unpack,
        SpecialFormType::CallableTypeOf,
        SpecialFormType::RegularCallableTypeOf,
    ] {
        cases.push(Case::invalid(
            special_form,
            InvalidTypeExpression::RequiresOneArgument(special_form),
        ));
    }
    for qualifier in [
        TypeQualifier::ReadOnly,
        TypeQualifier::Final,
        TypeQualifier::ClassVar,
        TypeQualifier::Required,
        TypeQualifier::NotRequired,
        TypeQualifier::InitVar,
    ] {
        cases.push(Case::invalid(
            SpecialFormType::TypeQualifier(qualifier),
            InvalidTypeExpression::TypeQualifier(qualifier),
        ));
    }
    for case in cases {
        assert_case(&db, &fixture, Some(fixture.definition), &case);
        assert_ordinary_case(&db, &fixture, &case);
    }
    Ok(())
}

#[test]
fn constructors_preserve_arguments_order_and_canonical_results() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    let env = db.program_environment();
    let tuple = Type::homogeneous_tuple(&db, &env, Type::object());
    let named_tuple = KnownClass::NamedTupleLike.to_instance(&db, &env);
    let mut cases = vec![
        Case {
            events: vec![Event::Dispatch, Event::KnownClass(KnownClass::Type)],
            ..Case::valid(
                SpecialFormType::Type,
                KnownClass::Type.to_instance(&db, &env),
            )
        },
        Case {
            events: vec![Event::Dispatch, Event::TypeForm(Type::any())],
            ..Case::valid(
                SpecialFormType::TypeForm,
                TypeFormType::from_type_expression(&db, Type::any()),
            )
        },
        Case {
            events: vec![Event::Dispatch, Event::Tuple(Type::unknown())],
            ..Case::valid(
                SpecialFormType::Tuple,
                Type::homogeneous_tuple(&db, &env, Type::unknown()),
            )
        },
        Case {
            events: vec![
                Event::Dispatch,
                Event::Tuple(Type::object()),
                Event::KnownClass(KnownClass::NamedTupleLike),
                Event::Intersection(tuple, named_tuple),
            ],
            ..Case::valid(
                SpecialFormType::NamedTuple,
                IntersectionType::from_two_elements(&db, &env, tuple, named_tuple),
            )
        },
    ];
    for (alias, class) in [
        (LegacyStdlibAlias::List, KnownClass::List),
        (LegacyStdlibAlias::Dict, KnownClass::Dict),
        (LegacyStdlibAlias::Set, KnownClass::Set),
        (LegacyStdlibAlias::FrozenSet, KnownClass::FrozenSet),
        (LegacyStdlibAlias::ChainMap, KnownClass::ChainMap),
        (LegacyStdlibAlias::Counter, KnownClass::Counter),
        (LegacyStdlibAlias::DefaultDict, KnownClass::DefaultDict),
        (LegacyStdlibAlias::Deque, KnownClass::Deque),
        (LegacyStdlibAlias::OrderedDict, KnownClass::OrderedDict),
    ] {
        cases.push(Case {
            events: vec![Event::Dispatch, Event::KnownClass(class)],
            ..Case::valid(
                SpecialFormType::LegacyStdlibAlias(alias),
                class.to_instance(&db, &env),
            )
        });
    }
    for special_form in [
        SpecialFormType::TypingCallable,
        SpecialFormType::CollectionsAbcCallable,
    ] {
        cases.push(Case {
            events: vec![Event::Dispatch, Event::Callable],
            ..Case::valid(special_form, Type::Callable(CallableType::unknown(&db)))
        });
    }
    for case in cases {
        assert_case(&db, &fixture, Some(fixture.definition), &case);
        assert_ordinary_case(&db, &fixture, &case);
    }
    Ok(())
}

#[test]
fn self_in_type_alias_stops_before_context_dependent_effects() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    for flags in [
        InferenceFlags::IN_TYPE_ALIAS,
        InferenceFlags::IN_TYPE_ALIAS
            | InferenceFlags::IN_RETURN_TYPE
            | InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER,
    ] {
        let case = Case {
            flags,
            ..Case::invalid(
                SpecialFormType::TypingSelf,
                InvalidTypeExpression::TypingSelfInTypeAlias,
            )
        };
        assert_case(&db, &fixture, Some(fixture.definition), &case);
        assert_ordinary_case(&db, &fixture, &case);
    }
    Ok(())
}

#[test]
fn self_child_preserves_context_results_and_incompatible_receiver_fallback() -> anyhow::Result<()> {
    let db = database()?;
    let fixture = fixture(&db)?;
    let variable = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(&db, Name::new_static("Self"), None, TypeVarKind::TypingSelf),
        None,
        None,
        None,
    );
    let bound = BoundTypeVarInstance::new(
        &db,
        variable,
        BindingContext::Definition(fixture.definition),
        None,
        TypeVarNonce::NONE,
    );
    let cases = [
        Case::valid(
            SpecialFormType::TypingSelf,
            Type::SpecialForm(SpecialFormType::TypingSelf),
        ),
        Case::valid(SpecialFormType::TypingSelf, Type::TypeVar(bound)),
        Case::invalid(
            SpecialFormType::TypingSelf,
            InvalidTypeExpression::InvalidType(
                Type::SpecialForm(SpecialFormType::TypingSelf),
                fixture.scope,
            ),
        ),
        Case::invalid(
            SpecialFormType::TypingSelf,
            InvalidTypeExpression::TypingSelfInStaticMethod,
        ),
        Case::invalid(
            SpecialFormType::TypingSelf,
            InvalidTypeExpression::TypingSelfInMetaclass,
        ),
        Case {
            flags: InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER | InferenceFlags::IN_RETURN_TYPE,
            fallback: Type::TypeVar(bound),
            ..Case::invalid(
                SpecialFormType::TypingSelf,
                InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(bound),
            )
        },
        Case {
            flags: InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER
                | InferenceFlags::IN_PARAMETER_ANNOTATION,
            fallback: Type::TypeVar(bound),
            ..Case::invalid(
                SpecialFormType::TypingSelf,
                InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(bound),
            )
        },
    ];
    for mut case in cases {
        for binding in [None, Some(fixture.definition)] {
            case.events = vec![
                Event::Dispatch,
                Event::TypingSelf(fixture.scope, binding, case.flags),
            ];
            assert_case(&db, &fixture, binding, &case);
        }
    }
    Ok(())
}
