use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_python_ast::name::Name;
use ty_python_core::semantic_index;

use super::*;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::types::TypeVarKind;
use crate::types::signatures::effects::try_poll_immediate;
use crate::types::typevar::{TypeVarDefaultEvaluation, TypeVarIdentity};

struct Trace<'db> {
    ordinary: OrdinaryLazyDefaultEffects<'db>,
    events: RefCell<Vec<&'static str>>,
    callee: Option<KnownClass>,
    refuse: Option<usize>,
}

impl<'db> Trace<'db> {
    fn new(db: &'db dyn Db, callee: Option<KnownClass>, refuse: Option<usize>) -> Self {
        Self {
            ordinary: OrdinaryLazyDefaultEffects { db },
            events: RefCell::default(),
            callee,
            refuse,
        }
    }

    fn record(&self, event: &'static str) -> Result<(), &'static str> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event);
        if self.refuse == Some(index) {
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

impl<'db> SynchronousLazyDefaultEffects<'db> for Trace<'db> {
    type Error = &'static str;
    type Source = LazyDefaultSource<'db>;

    fn definition(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        self.record("definition")?;
        Ok(infallible(self.ordinary.definition(variable)))
    }

    fn source(&self, definition: Definition<'db>) -> Result<Self::Source, Self::Error> {
        self.record("source")?;
        Ok(infallible(self.ordinary.source(definition)))
    }

    fn select<'source>(
        &self,
        source: &'source Self::Source,
    ) -> Result<LazyDefaultExpression<'source>, Self::Error> {
        self.record("select")?;
        Ok(infallible(self.ordinary.select(source)))
    }

    fn expression_type(
        &self,
        _definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        if matches!(expression, ast::Expr::Name(name) if name.id == "factory") {
            self.record("callee_expression")?;
            Ok(Type::int_literal(17))
        } else {
            self.record("default_expression")?;
            Ok(Type::int_literal(23))
        }
    }

    fn known_class(&self, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error> {
        self.record("known_class")?;
        assert_eq!(ty, Type::int_literal(17));
        Ok(self.callee)
    }

    fn keyword_cursor<'source>(
        &self,
        call: &'source ast::ExprCall,
    ) -> Result<LazyDefaultKeywordCursor<'source>, Self::Error> {
        self.record("keyword_cursor")?;
        Ok(infallible(self.ordinary.keyword_cursor(call)))
    }

    fn next_keyword<'source>(
        &self,
        cursor: &mut LazyDefaultKeywordCursor<'source>,
    ) -> Result<Option<&'source ast::Keyword>, Self::Error> {
        self.record("next_keyword")?;
        Ok(infallible(self.ordinary.next_keyword(cursor)))
    }

    fn paramspec_value(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.record("paramspec_value")?;
        assert_eq!(ty, Type::int_literal(23));
        Ok(Type::int_literal(29))
    }

    fn recovery_file(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<ProgramFile<'db>, Self::Error> {
        self.record("recovery_file")?;
        Ok(infallible(self.ordinary.recovery_file(variable)))
    }

    fn cycle_normalize(
        &self,
        default: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        _cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Self::Error> {
        self.record("cycle_normalize")?;
        assert_eq!(previous, Type::int_literal(31));
        assert_eq!(default, Type::int_literal(37));
        Ok(Type::int_literal(41))
    }

    fn recursive_normalize(
        &self,
        default: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        _cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Self::Error> {
        self.record("recursive_normalize")?;
        assert_eq!(default, Type::int_literal(37));
        Ok(Type::int_literal(43))
    }
}

impl<'db> LazyDefaultEffects<'db> for Trace<'db> {
    type Error = &'static str;
    type Source = LazyDefaultSource<'db>;

    async fn definition(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<Definition<'db>>, Self::Error> {
        SynchronousLazyDefaultEffects::definition(self, variable)
    }

    async fn source(&self, definition: Definition<'db>) -> Result<Self::Source, Self::Error> {
        SynchronousLazyDefaultEffects::source(self, definition)
    }

    async fn select<'source>(
        &self,
        source: &'source Self::Source,
    ) -> Result<LazyDefaultExpression<'source>, Self::Error> {
        SynchronousLazyDefaultEffects::select(self, source)
    }

    async fn expression_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        SynchronousLazyDefaultEffects::expression_type(self, definition, expression)
    }

    async fn known_class(&self, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error> {
        SynchronousLazyDefaultEffects::known_class(self, ty)
    }

    async fn keyword_cursor<'source>(
        &self,
        call: &'source ast::ExprCall,
    ) -> Result<LazyDefaultKeywordCursor<'source>, Self::Error> {
        SynchronousLazyDefaultEffects::keyword_cursor(self, call)
    }

    async fn next_keyword<'source>(
        &self,
        cursor: &mut LazyDefaultKeywordCursor<'source>,
    ) -> Result<Option<&'source ast::Keyword>, Self::Error> {
        SynchronousLazyDefaultEffects::next_keyword(self, cursor)
    }

    async fn paramspec_value(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        SynchronousLazyDefaultEffects::paramspec_value(self, ty)
    }

    async fn recovery_file(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> Result<ProgramFile<'db>, Self::Error> {
        SynchronousLazyDefaultEffects::recovery_file(self, variable)
    }

    async fn cycle_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Self::Error> {
        SynchronousLazyDefaultEffects::cycle_normalize(self, default, env, previous, cycle)
    }

    async fn recursive_normalize(
        &self,
        default: Type<'db>,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle<'_>,
    ) -> Result<Type<'db>, Self::Error> {
        SynchronousLazyDefaultEffects::recursive_normalize(self, default, env, cycle)
    }
}

fn database(source: &str) -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(ast::PythonVersion::PY313)
        .with_file("/src/lazy_default.py", source)
        .build()
}

fn variable<'db>(
    db: &'db dyn Db,
    definition: Option<Definition<'db>>,
    kind: TypeVarKind,
) -> TypeVarInstance<'db> {
    TypeVarInstance::new(
        db,
        TypeVarIdentity::new(db, Name::new_static("T"), definition, kind),
        None,
        None,
        Some(TypeVarDefaultEvaluation::Lazy),
    )
}

fn source_variable(db: &TestDb) -> anyhow::Result<TypeVarInstance<'_>> {
    let file = ProgramFile::new(
        db,
        system_path_to_file(db, "/src/lazy_default.py")?,
        db.program_environment().program(db),
    );
    let module = parsed_module(db, file.python_file(db)).load(db);
    let index = semantic_index(db, file);
    let (definitions, kind) = match module.suite().first() {
        Some(ast::Stmt::ClassDef(class)) => {
            match class.type_params.as_ref().and_then(|params| params.first()) {
                Some(ast::TypeParam::TypeVar(parameter)) => {
                    (index.definitions(parameter), TypeVarKind::Pep695TypeVar)
                }
                Some(ast::TypeParam::ParamSpec(parameter)) => {
                    (index.definitions(parameter), TypeVarKind::Pep695ParamSpec)
                }
                Some(ast::TypeParam::TypeVarTuple(parameter)) => (
                    index.definitions(parameter),
                    TypeVarKind::Pep695TypeVarTuple,
                ),
                None => (index.definitions(class), TypeVarKind::LegacyTypeVar),
            }
        }
        Some(ast::Stmt::Assign(assignment)) => {
            let Some(ast::Expr::Name(name)) = assignment.targets.first() else {
                anyhow::bail!("fixture assignment must have a named target");
            };
            (index.definitions(name), TypeVarKind::LegacyTypeVar)
        }
        _ => anyhow::bail!("fixture must define a class or assign a name"),
    };
    let [definition] = definitions else {
        anyhow::bail!("fixture must have exactly one definition");
    };
    Ok(variable(db, Some(*definition), kind))
}

fn assert_paths<'db>(
    db: &'db dyn Db,
    variable: TypeVarInstance<'db>,
    callee: Option<KnownClass>,
    expected: Option<Type<'db>>,
    steps: &[&'static str],
) {
    for asynchronous in [false, true] {
        for refuse in std::iter::once(None).chain((0..steps.len()).map(Some)) {
            let effects = Trace::new(db, callee, refuse);
            let actual = if asynchronous {
                try_poll_immediate(lazy_default_with(variable, LazyDefaultFacts, &effects))
            } else {
                Poll::Ready(lazy_default_sync(variable, LazyDefaultFacts, &effects))
            };
            assert_eq!(
                actual,
                Poll::Ready(refuse.map_or(Ok(expected), |index| Err(steps[index])))
            );
            assert_eq!(
                effects.events.borrow().as_slice(),
                &steps[..refuse.map_or(steps.len(), |index| index + 1)]
            );
        }
    }
}

#[test]
fn missing_definition_or_default_short_circuits() -> anyhow::Result<()> {
    let db = setup_db();
    assert_paths(
        &db,
        variable(&db, None, TypeVarKind::LegacyTypeVar),
        None,
        None,
        &["definition"],
    );
    for source in [
        "class A[T]: ...\n",
        "class A[**P]: ...\n",
        "class A[*Ts]: ...\n",
        "class A: ...\n",
        "T = 1\n",
    ] {
        let db = database(source)?;
        assert_paths(
            &db,
            source_variable(&db)?,
            None,
            None,
            &["definition", "source", "select"],
        );
    }
    Ok(())
}

#[test]
fn pep695_defaults_select_expression_and_paramspec_conversion() -> anyhow::Result<()> {
    for (source, converted) in [
        ("class A[T = marker]: ...\n", false),
        ("class A[**P = marker]: ...\n", true),
        ("class A[*Ts = *tuple[int]]: ...\n", false),
    ] {
        let db = database(source)?;
        let mut steps = vec!["definition", "source", "select", "default_expression"];
        let expected = if converted {
            steps.push("paramspec_value");
            Type::int_literal(29)
        } else {
            Type::int_literal(23)
        };
        assert_paths(&db, source_variable(&db)?, None, Some(expected), &steps);
    }
    Ok(())
}

#[test]
fn legacy_defaults_infer_callee_before_searching_keywords() -> anyhow::Result<()> {
    let db = database("T = factory(other=0, **options, default=1, after=2)\n")?;
    let variable = source_variable(&db)?;
    for (callee, converted) in [
        (None, false),
        (Some(KnownClass::TypeVar), false),
        (Some(KnownClass::ParamSpec), true),
        (Some(KnownClass::ExtensionsParamSpec), true),
    ] {
        let mut steps = vec![
            "definition",
            "source",
            "select",
            "callee_expression",
            "known_class",
            "keyword_cursor",
            "next_keyword",
            "next_keyword",
            "next_keyword",
            "default_expression",
        ];
        let expected = if converted {
            steps.push("paramspec_value");
            Type::int_literal(29)
        } else {
            Type::int_literal(23)
        };
        assert_paths(&db, variable, callee, Some(expected), &steps);
    }

    let db = database("T = factory(other=0)\n")?;
    assert_paths(
        &db,
        source_variable(&db)?,
        Some(KnownClass::ParamSpec),
        None,
        &[
            "definition",
            "source",
            "select",
            "callee_expression",
            "known_class",
            "keyword_cursor",
            "next_keyword",
            "next_keyword",
        ],
    );
    Ok(())
}

#[salsa::interned]
struct RecoveryInput<'db> {
    #[returns(copy)]
    variable: TypeVarInstance<'db>,
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
    let variable = input.variable(db);
    for asynchronous in [false, true] {
        let effects = Trace::new(db, None, Some(0));
        let missing = if asynchronous {
            try_poll_immediate(lazy_default_recover_with(
                cycle,
                Some(Type::int_literal(31)),
                None,
                variable,
                LazyDefaultFacts,
                &effects,
            ))
        } else {
            Poll::Ready(lazy_default_recover_sync(
                cycle,
                Some(Type::int_literal(31)),
                None,
                variable,
                LazyDefaultFacts,
                &effects,
            ))
        };
        assert_eq!(missing, Poll::Ready(Ok(None)));
        assert!(effects.events.borrow().is_empty());
        for (previous, event, expected) in [
            (
                Some(Type::int_literal(31)),
                "cycle_normalize",
                Type::int_literal(41),
            ),
            (None, "recursive_normalize", Type::int_literal(43)),
        ] {
            let steps = ["recovery_file", event];
            for refuse in [None, Some(0), Some(1)] {
                let effects = Trace::new(db, None, refuse);
                let actual = if asynchronous {
                    try_poll_immediate(lazy_default_recover_with(
                        cycle,
                        previous,
                        Some(Type::int_literal(37)),
                        variable,
                        LazyDefaultFacts,
                        &effects,
                    ))
                } else {
                    Poll::Ready(lazy_default_recover_sync(
                        cycle,
                        previous,
                        Some(Type::int_literal(37)),
                        variable,
                        LazyDefaultFacts,
                        &effects,
                    ))
                };
                assert_eq!(
                    actual,
                    Poll::Ready(refuse.map_or(Ok(Some(expected)), |index| Err(steps[index])))
                );
                assert_eq!(
                    effects.events.borrow().as_slice(),
                    &steps[..refuse.map_or(steps.len(), |index| index + 1)]
                );
            }
        }
    }
    value
}

#[test]
fn recovery_short_circuits_missing_values_and_selects_normalization() -> anyhow::Result<()> {
    let db = database("class A[T = marker]: ...\n")?;
    assert!(recovery_contract(
        &db,
        RecoveryInput::new(&db, source_variable(&db)?)
    ));
    Ok(())
}
