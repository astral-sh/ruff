//! Controls for canonical exact-tuple class conversion and publication in the source runtime.
//!
//! These controls need Rust access to canonical memo identity and the retained tuple spec;
//! tuple typing behavior is also covered by the tuple mdtests.

use test_case::test_case;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::BindingContext;
use crate::types::tuple::{
    TupleSpec, TupleType, VariableLengthTuple, VariableSegment, to_class_type_ingredient,
};
use crate::types::typevar::TypeVarNonce;

/// Chooses an exact tuple input without evaluating its class-conversion query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Shape {
    Empty,
    Fixed,
    Homogeneous,
    Mixed,
    Variadic,
}

impl Shape {
    /// Interns a tuple fixture while leaving its canonical class memo cold.
    fn tuple<'db>(self, db: &'db TestDb, program: Program<'db>) -> TupleType<'db> {
        let spec = match self {
            Self::Empty => TupleSpec::heterogeneous([]),
            Self::Fixed => {
                TupleSpec::heterogeneous([Type::bool_literal(true), Type::bool_literal(false)])
            }
            Self::Homogeneous => TupleSpec::homogeneous(Type::any()),
            Self::Mixed => VariableLengthTuple::mixed(
                [Type::bool_literal(true)],
                VariableSegment::Homogeneous(Type::bool_literal(false)),
                [Type::bool_literal(true)],
            ),
            Self::Variadic => {
                let raw = TypeVarInstance::new(
                    db,
                    TypeVarIdentity::new(
                        db,
                        Name::new_static("Ts"),
                        None,
                        TypeVarKind::Pep695TypeVarTuple,
                    ),
                    None,
                    None,
                    None,
                );
                let variable = BoundTypeVarInstance::new(
                    db,
                    raw,
                    BindingContext::Synthetic(program),
                    None,
                    TypeVarNonce::NONE,
                );
                VariableLengthTuple::mixed([], VariableSegment::TypeVarTuple(variable), [])
            }
        };
        TupleType::new(db, &ProgramEnvironment::from_program(program), &spec)
    }
}

/// Invokes the registered canonical tuple-class query for an exact tuple.
#[derive(Clone, Copy, Debug)]
struct Request<'db>(TupleType<'db>);

impl<'db> MemberOperation<'db> for Request<'db> {
    type Output = ClassType<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.tuple_class(self.0).await
    }
}

/// Creates a Python 3.13 database using the real vendored builtins.
fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("src/main.py", "left = right = True\n")
        .build()
}

/// Checks that conversion produces the ordinary class argument and retains the exact tuple,
/// then verifies native and controlled memo reuse.
#[test_case(Shape::Empty; "empty tuple")]
#[test_case(Shape::Fixed; "fixed tuple")]
#[test_case(Shape::Homogeneous; "homogeneous tuple")]
#[test_case(Shape::Mixed; "prefix variable suffix")]
#[test_case(Shape::Variadic; "type variable tuple")]
fn cold_class_conversion_preserves_tuple_and_reuses_canonical_memo(
    shape: Shape,
) -> anyhow::Result<()> {
    let db = database()?;
    let prepared = prepare(&db);
    let tuple = shape.tuple(&db, prepared.program_file().program(&db));
    let ingredient = to_class_type_ingredient(&db);
    let key = ingredient.database_key_index(tuple.as_id());
    let query_name = db.ingredient_debug_name(key.ingredient_index());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, tuple.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let revision = salsa::plumbing::current_revision(&db);
    let mut events = db.clone();
    events.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, Request(tuple), &funded())
    })
    .map_err(|error| anyhow::anyhow!("cold tuple-class capture failed: {error:?}"))?;
    let Ok(AnalysisOutcome::Complete(class @ ClassType::Generic(alias))) = cold.value else {
        anyhow::bail!("tuple class conversion did not complete: {:?}", cold.value);
    };
    assert_eq!(cold.check_root_reads(), Ok(()));
    let read = cold
        .reads
        .iter()
        .find(|read| read.key == key)
        .ok_or_else(|| anyhow::anyhow!("tuple class read missing"))?;
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, tuple.as_id()).is_ok());
    assert!(
        find_will_execute_event_by_name(
            &db,
            &query_name,
            Some(tuple.as_id()),
            &events.take_salsa_events()
        )
        .is_some()
    );
    let specialization = alias.specialization(&db);
    let retained = specialization
        .tuple(&db)
        .ok_or_else(|| anyhow::anyhow!("tuple specialization lost its exact tuple"))?;
    assert!(std::ptr::eq(retained, tuple.tuple(&db)));
    let [element] = specialization.types(&db) else {
        anyhow::bail!("builtin tuple specialization should have one type argument");
    };
    if shape == Shape::Variadic {
        let Some(Type::TypeVar(variable)) = retained.class_elements().variable else {
            anyhow::bail!("fixture should retain its TypeVarTuple segment");
        };
        assert_eq!(specialization.types(&db), &[Type::TypeVar(variable)]);
    }

    let ordinary_db = database()?;
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_tuple = shape.tuple(
        &ordinary_db,
        ordinary_prepared.program_file().program(&ordinary_db),
    );
    let ordinary = ordinary_tuple.to_class_type(&ordinary_db);
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ClassType::Generic(ordinary_alias) = ordinary else {
        anyhow::bail!("ordinary builtin tuple should be generic");
    };
    let [ordinary_element] = ordinary_alias
        .specialization(&ordinary_db)
        .types(&ordinary_db)
    else {
        anyhow::bail!("ordinary builtin tuple should have one type argument");
    };
    assert_eq!(
        element.display(&db, &env).to_string(),
        ordinary_element
            .display(&ordinary_db, &ordinary_env)
            .to_string()
    );
    assert_eq!(
        Type::from(class).display(&db, &env).to_string(),
        Type::from(ordinary)
            .display(&ordinary_db, &ordinary_env)
            .to_string()
    );

    let native = capture(&db, || tuple.to_class_type(&db))
        .map_err(|error| anyhow::anyhow!("native tuple-class capture failed: {error:?}"))?;
    assert_eq!(native.check_root_reads(), Ok(()));
    assert_eq!(native.value, class);
    assert!(
        native
            .reads
            .iter()
            .any(|native_read| native_read.key == key
                && native_read.memo_address == read.memo_address)
    );
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, Request(tuple), &funded())
    })
    .map_err(|error| anyhow::anyhow!("warm tuple-class capture failed: {error:?}"))?;
    assert_eq!(warm.check_root_reads(), Ok(()));
    assert_eq!(warm.value, cold.value);
    assert!(
        warm.reads
            .iter()
            .any(|warm_read| warm_read.key == key && warm_read.memo_address == read.memo_address)
    );
    assert_function_query_was_not_run_by_name(
        &db,
        &query_name,
        Some(tuple.as_id()),
        &events.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
    Ok(())
}
