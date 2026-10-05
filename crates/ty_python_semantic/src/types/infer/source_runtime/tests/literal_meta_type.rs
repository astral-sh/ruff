//! Literal meta-types use canonical class queries or admitted enum fields and preserve exact classes.

use std::panic::AssertUnwindSafe;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::class::{KnownClassArgument, known_class_to_class_literal_ingredient};
use crate::types::{KnownClass, MemberEntryEffects};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Interruption {
    None,
    Work,
    Bytes,
    Cancel,
}

/// Converts a member receiver to its meta-type, optionally interrupting its caller afterward.
#[derive(Clone, Copy, Debug)]
struct Request<'a, 'db> {
    ty: Type<'db>,
    remaining: Option<&'a Cell<Option<usize>>>,
    interruption: Interruption,
}

impl<'a, 'db> Request<'a, 'db> {
    const fn new(ty: Type<'db>) -> Self {
        Self {
            ty,
            remaining: None,
            interruption: Interruption::None,
        }
    }
}

impl<'db> MemberOperation<'db> for Request<'_, 'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let meta_type = MemberEntryEffects::meta_type(&effects, self.ty).await?;
        if let Some(remaining) = self.remaining {
            remaining.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                access.db(),
            ));
        }
        if self.interruption != Interruption::None {
            let endpoint = access.endpoint();
            endpoint
                .local_call(|| {
                    match self.interruption {
                        Interruption::None => {}
                        Interruption::Work => endpoint.admit_work(funded().semantic_work_limit)?,
                        Interruption::Bytes => endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: funded().requested_bytes_limit,
                        })?,
                        Interruption::Cancel => access.db().cancellation_token().cancel(),
                    }
                    endpoint.check_completion()
                })
                .await;
        }
        Ok(meta_type)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scalar {
    Bool,
    Int,
    Bytes,
    String,
    LiteralString,
}

impl Scalar {
    const fn class(self) -> KnownClass {
        match self {
            Self::Bool => KnownClass::Bool,
            Self::Int => KnownClass::Int,
            Self::Bytes => KnownClass::Bytes,
            Self::String | Self::LiteralString => KnownClass::Str,
        }
    }

    fn literal(self, db: &TestDb) -> Type<'_> {
        match self {
            Self::Bool => Type::bool_literal(true),
            Self::Int => Type::int_literal(17),
            Self::Bytes => Type::bytes_literal(db, b"value"),
            Self::String => Type::string_literal(db, "value"),
            Self::LiteralString => Type::literal_string(),
        }
    }
}

/// Every scalar literal returns its exact class, and ordinary conversion reuses the cold child memo.
/// In particular, an integer literal returns the `int` class rather than a type including subclasses.
#[test]
fn scalar_meta_types_complete_and_reuse_canonical_class_memos() {
    for scalar in [
        Scalar::Bool,
        Scalar::Int,
        Scalar::Bytes,
        Scalar::String,
        Scalar::LiteralString,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let ty = scalar.literal(&db);
        let argument = KnownClassArgument::new(&db, scalar.class(), program);
        let ingredient = known_class_to_class_literal_ingredient(&db);
        let key = ingredient.database_key_index(argument.as_id());
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        let revision = salsa::plumbing::current_revision(&db);
        let mut events = db.clone();
        events.take_salsa_events();
        observations::reset(None);
        let cold = capture(&db, || {
            controlled_member_operation(&prepared, Request::new(ty), &funded())
        })
        .unwrap();
        let Ok(AnalysisOutcome::Complete(meta_type @ Type::ClassLiteral(_))) = cold.value else {
            panic!("{scalar:?}: {:?}", cold.value);
        };
        cold.check_root_reads().unwrap();
        let read = cold.reads.iter().find(|read| read.key == key).unwrap();
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
        assert!(events.take_salsa_events().iter().any(|event| {
            matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)
        }));

        let env = ProgramEnvironment::from_program(program);
        let ordinary = capture(&db, || ty.to_meta_type(&db, &env)).unwrap();
        assert_eq!(ordinary.value, meta_type);
        assert_eq!(meta_type, scalar.class().to_class_literal(&db, &env));
        assert!(ordinary.reads.iter().any(|ordinary_read| {
            ordinary_read.key == key && ordinary_read.memo_address == read.memo_address
        }));
        let warm = capture(&db, || {
            controlled_member_operation(&prepared, Request::new(ty), &funded())
        })
        .unwrap();
        assert_eq!(warm.value, cold.value);
        assert!(warm.reads.iter().any(|warm_read| {
            warm_read.key == key && warm_read.memo_address == read.memo_address
        }));
        assert_function_query_was_not_run_by_name(
            &db,
            "known_class_to_class_literal",
            Some(argument.as_id()),
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

fn enum_fixture() -> TestDb {
    TestDbBuilder::new()
        .with_file(
            "src/main.py",
            "from enum import Enum\nfrom typing import Literal\n\
             class Choice(Enum):\n    FIRST = 1\n    SECOND = 2\n\
             selected: Literal[Choice.FIRST] = Choice.FIRST\n",
        )
        .build()
        .unwrap()
}

/// Obtains an enum value as input; its meta-type conversion still runs through controlled field reads.
fn enum_literal<'db>(db: &'db TestDb, prepared: &PreparedAnalysisFile<'db>) -> Type<'db> {
    let ty = crate::place::global_symbol(db, prepared.program_file(), "selected")
        .place
        .expect_type();
    assert!(ty.as_enum_literal().is_some());
    ty
}

/// Enum conversion reads the stored class identity and returns that exact class without a child query.
#[test]
fn enum_meta_type_preserves_the_exact_class() {
    let db = enum_fixture();
    let prepared = prepare(&db);
    let ty = enum_literal(&db, &prepared);
    observations::reset(None);
    let controlled = capture(&db, || {
        controlled_member_operation(&prepared, Request::new(ty), &funded())
    })
    .unwrap();
    let class = ty.as_enum_literal().unwrap().enum_class(&db);
    assert_eq!(
        controlled.value,
        Ok(AnalysisOutcome::Complete(Type::ClassLiteral(class)))
    );
    assert!(controlled.reads.is_empty());
    assert_eq!(
        ty.to_meta_type(&db, &ProgramEnvironment::from_file(prepared.program_file())),
        Type::ClassLiteral(class),
    );
    assert_no_active_attempt();
}

/// Enum conversion obeys separate work and byte limits and retries with the same handles and revision.
#[test]
fn enum_meta_type_refusal_retries_in_the_same_revision() {
    let db = enum_fixture();
    let prepared = prepare(&db);
    let ty = enum_literal(&db, &prepared);
    let expected = Type::ClassLiteral(ty.as_enum_literal().unwrap().enum_class(&db));
    let revision = salsa::plumbing::current_revision(&db);
    let remaining = Cell::new(None);
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Request {
                remaining: Some(&remaining),
                ..Request::new(ty)
            },
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    let work = funded().semantic_work_limit - remaining.get().unwrap();
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        let outcome = controlled_member_operation(
            &prepared,
            Request::new(ty),
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
        );
        match outcome {
            Ok(AnalysisOutcome::Complete(actual)) => {
                assert_eq!(actual, expected);
                upper = middle;
            }
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::RequestedAllocationLimit,
                ..
            }) => {
                lower = middle + 1;
            }
            other => panic!("unexpected enum conversion outcome: {other:?}"),
        }
        assert_no_active_attempt();
    }
    assert!(lower > 0);
    assert_eq!(
        controlled_member_operation(
            &prepared,
            Request::new(ty),
            &AnalysisPolicy {
                requested_bytes_limit: lower,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Complete(expected)),
    );
    for (policy, reason) in [
        (
            AnalysisPolicy {
                semantic_work_limit: work - 1,
                ..funded()
            },
            AnalysisIncomplete::WorkLimit,
        ),
        (
            AnalysisPolicy {
                requested_bytes_limit: lower - 1,
                ..funded()
            },
            AnalysisIncomplete::RequestedAllocationLimit,
        ),
    ] {
        assert_eq!(
            controlled_member_operation(&prepared, Request::new(ty), &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason,
                completed: ()
            }),
        );
        assert_no_active_attempt();
        assert_eq!(
            controlled_member_operation(&prepared, Request::new(ty), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// A completed known-class child survives caller refusal or cancellation and is reused on retry.
#[test]
fn completed_literal_class_child_survives_caller_interruption() {
    for interruption in [
        Interruption::Work,
        Interruption::Bytes,
        Interruption::Cancel,
    ] {
        let db = fixture();
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let ty = Type::bool_literal(true);
        let argument = KnownClassArgument::new(&db, KnownClass::Bool, program);
        let ingredient = known_class_to_class_literal_ingredient(&db);
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        let revision = salsa::plumbing::current_revision(&db);
        let remaining = Cell::new(None);
        observations::reset(None);
        let result = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_member_operation(
                &prepared,
                Request {
                    ty,
                    remaining: Some(&remaining),
                    interruption,
                },
                &funded(),
            )
        }));
        match (interruption, result) {
            (Interruption::Cancel, Err(salsa::Cancelled::Local)) => {}
            (
                Interruption::Work,
                Ok(Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    ..
                })),
            ) => {}
            (
                Interruption::Bytes,
                Ok(Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    ..
                })),
            ) => {}
            other => panic!("unexpected caller interruption: {other:?}"),
        }
        assert!(remaining.get().is_some());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
        assert_no_active_attempt();

        let mut events = db.clone();
        events.take_salsa_events();
        let expected =
            KnownClass::Bool.to_class_literal(&db, &ProgramEnvironment::from_program(program));
        assert_eq!(
            controlled_member_operation(&prepared, Request::new(ty), &funded()),
            Ok(AnalysisOutcome::Complete(expected)),
        );
        assert_function_query_was_not_run_by_name(
            &db,
            "known_class_to_class_literal",
            Some(argument.as_id()),
            &events.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// Converting `Never` to its meta-type preserves `Never`.
#[test]
fn never_meta_type_remains_never() {
    let db = fixture();
    let prepared = prepare(&db);
    observations::reset(None);
    assert_eq!(
        controlled_member_operation(&prepared, Request::new(Type::Never), &funded()),
        Ok(AnalysisOutcome::Complete(Type::Never)),
    );
    assert_no_active_attempt();
}
