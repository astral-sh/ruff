use super::*;
use crate::db::tests::TestDbBuilder;
use crate::place::explicit_global_symbol;
use crate::types::KnownInstanceType;
use ruff_db::files::system_path_to_file;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::PythonVersion;
use salsa::prepared_source_probe as probe;
use ty_python_core::ProgramFile;

#[test]
fn stored_input_scan_queues_deep_shared_payloads_and_keeps_aliases_opaque() -> anyhow::Result<()> {
    let mut db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .build()?;
    db.write_file(
        "/src/scan.py",
        r#"
class Box[T]: ...
class Pair[L, R]: ...
type Recursive[T] = tuple[T, Recursive[list[T]]]
"#,
    )?;
    let env = db.program_environment();
    let file = system_path_to_file(&db, "/src/scan.py")?;
    let file = ProgramFile::new(&db, file, env.program(&db));
    let prepared = PreparedDeclarations::prepare(&db, file).unwrap();
    let class = |name: &str| {
        prepared.globals[name]
            .value
            .place
            .expect_type()
            .as_class_literal()
            .unwrap()
            .as_static()
            .unwrap()
    };
    let boxed = class("Box");
    let pair = class("Pair");
    let box_context = prepared.context(boxed).unwrap().unwrap();
    let pair_context = prepared.context(pair).unwrap().unwrap();
    let variable = box_context.variables(&db).next().unwrap();

    let leaf = probe::capture(&db, || {
        StoredInputScanner::collect(
            &db,
            &env,
            Type::TypeVar(variable),
            &TypeCollector::default(),
        )
    })
    .unwrap();
    assert!(leaf.reads.is_empty());
    assert_eq!(
        leaf.value.variables.into_iter().collect::<Vec<_>>(),
        [variable]
    );

    let mut nested = Type::TypeVar(variable);
    for _ in 0..4096 {
        nested = Type::GenericAlias(GenericAlias::new(
            &db,
            boxed,
            box_context.specialize(&db, [nested].as_slice()),
        ));
    }
    let shared = Type::GenericAlias(GenericAlias::new(
        &db,
        pair,
        pair_context.specialize(&db, [nested, nested].as_slice()),
    ));
    let visited = TypeCollector::default();
    let captured = probe::capture(&db, || {
        StoredInputScanner::collect(&db, &env, shared, &visited)
    })
    .unwrap();
    assert!(captured.reads.is_empty());
    assert_eq!(captured.value.origins.len(), 2);
    assert!(
        captured
            .value
            .origins
            .contains(&ClassLiteral::Static(boxed))
    );
    assert!(captured.value.origins.contains(&ClassLiteral::Static(pair)));
    assert_eq!(captured.value.variables.len(), 3);
    assert!(captured.value.variables.contains(&variable));
    for variable in pair_context.variables(&db) {
        assert!(captured.value.variables.contains(&variable));
    }
    let repeated = probe::capture(&db, || {
        StoredInputScanner::collect(&db, &env, shared, &visited)
    })
    .unwrap();
    assert!(repeated.reads.is_empty());
    assert!(repeated.value.variables.is_empty());
    assert!(repeated.value.origins.is_empty());

    let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
        explicit_global_symbol(&db, file, "Recursive")
            .place
            .expect_type()
    else {
        panic!("expected the runtime object for a type alias");
    };
    let opaque = probe::capture(&db, || {
        StoredInputScanner::collect(&db, &env, Type::TypeAlias(alias), &TypeCollector::default())
    })
    .unwrap();
    assert!(opaque.reads.is_empty());
    assert!(opaque.value.variables.is_empty());
    assert!(opaque.value.origins.is_empty());
    Ok(())
}
