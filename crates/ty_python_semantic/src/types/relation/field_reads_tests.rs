use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion};
use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RegistryBuilder, RunResult};
use salsa::{DatabaseKeyIndex, Event, EventKind};
use ty_python_core::definition::Definition;
use ty_python_core::semantic_index;

use super::field_reads::RelationFieldReads;
use crate::Db;
use crate::db::tests::TestDbBuilder;
use crate::types::generics::Specialization;
use crate::types::infer::infer_definition_types;
use crate::types::{ClassLiteral, GenericAlias, StaticClassLiteral, Type};

type AliasFields<'db> = (
    StaticClassLiteral<'db>,
    Specialization<'db>,
    &'db [Type<'db>],
);

fn read_alias_fields<'db>(
    fields: &RelationFieldReads<'db>,
    alias: GenericAlias<'db>,
    _definition: Definition<'db>,
) -> AliasFields<'db> {
    let origin = fields.alias_origin(alias);
    let specialization = fields.alias_specialization(alias);
    (
        origin,
        specialization,
        fields.specialization_types(specialization),
    )
}

struct Admit;

impl ExecutionAdmission for Admit {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn executed(events: &[Event]) -> Vec<DatabaseKeyIndex> {
    events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::WillExecute { database_key } => Some(database_key),
            _ => None,
        })
        .collect()
}

#[test]
fn existing_alias_fields_keep_the_database_lifetime_in_ordinary_and_runtime_reads()
-> anyhow::Result<()> {
    let db = TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file("/src/fields.py", "class C[T, U]:\n    pass\n")
        .build()?;
    let file = system_path_to_file(&db, "/src/fields.py")?;
    let program_file = db.program_file(file);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let [ast::Stmt::ClassDef(class_node)] = module.suite().as_slice() else {
        anyhow::bail!("the fixture declares one class");
    };
    let definition = semantic_index(&db, program_file).expect_single_definition(class_node);
    let inference = infer_definition_types(&db, definition);
    let Type::ClassLiteral(ClassLiteral::Static(origin)) = inference.binding_type(definition)
    else {
        anyhow::bail!("the declaration produces a static class");
    };
    let context = origin
        .generic_context(&db)
        .expect("C has two type parameters");
    let arguments = [Type::int_literal(7), Type::bool_literal(true)];
    let specialization = context.specialize(&db, arguments.as_slice());
    let alias = GenericAlias::new(&db, origin, specialization);
    let expected = (
        alias.origin(&db),
        alias.specialization(&db),
        specialization.types(&db),
    );
    assert_eq!(expected.0, origin);
    assert_eq!(expected.1, specialization);
    assert_eq!(expected.2, arguments);
    let mut event_reader = db.clone();
    let preparation = executed(&event_reader.take_salsa_events());
    assert!(!preparation.is_empty());

    // The returned slice borrows the real database, not the short-lived capability.
    let ordinary = {
        let fields = RelationFieldReads::new(&db as &dyn salsa::Database);
        read_alias_fields(&fields, alias, definition)
    };
    assert_eq!(ordinary, expected);
    assert!(std::ptr::eq(ordinary.2, expected.2));
    assert!(executed(&event_reader.take_salsa_events()).is_empty());

    let fields = RelationFieldReads::new(&db as &dyn salsa::Database);
    let outcome = try_with_attempt(&db, 100_000, || {
        RegistryBuilder::new(&db, &Admit)?
            .seal()?
            .run(move |endpoint| async move {
                let selected = endpoint
                    .local_call(|| Ok(read_alias_fields(&fields, alias, definition)))
                    .await;
                endpoint.checkpoint()?.await?;
                Ok(selected)
            })
    });
    let Ok(AttemptOutcome::Complete(Ok(controlled))) = outcome else {
        panic!("field-only root failed: {outcome:?}");
    };
    assert_eq!(controlled, ordinary);
    assert!(std::ptr::eq(controlled.2, expected.2));
    let runtime_queries = executed(&event_reader.take_salsa_events());
    assert!(
        runtime_queries.is_empty(),
        "field-only root executed {runtime_queries:?}"
    );
    Ok(())
}
