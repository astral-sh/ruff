use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ruff_db::system::DbWithWritableSystem;
use ruff_python_ast::Stmt;
use ty_python_core::{Truthiness, semantic_index};

use super::*;
use crate::db::tests::setup_db;
use crate::types::infer::{TypeContext, infer_expression_types};
use crate::types::signatures::effects::legacy_inline;

#[test]
fn native_expression_payload_keeps_binding_ownership_and_replaces_truthiness() {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = (chosen := True)\n")
        .unwrap();
    let file = system_path_to_file(&db, "src/main.py").unwrap();
    let program_file = db.program_file(file);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let Stmt::Assign(assignment) = &module.syntax().body[0] else {
        panic!("fixture assignment")
    };
    let index = semantic_index(&db, program_file);
    let expression = index.expression(assignment.value.as_ref());
    let inference = infer_expression_types(&db, expression, TypeContext::default());
    assert!(!inference.extra.as_ref().unwrap().bindings.is_empty());
    let env = ProgramEnvironment::from_file(program_file);
    let key = assignment.value.as_ref().into();

    for (include_bindings, scope_region) in [(true, false), (false, false), (true, true)] {
        let region = if scope_region {
            InferenceRegion::Scope(expression.scope(&db), TypeContext::default())
        } else {
            InferenceRegion::Expression(expression, TypeContext::default())
        };
        let mut builder =
            TypeInferenceBuilder::new(&db, &env, region, file, program_file, index, &module);
        builder.context.defuse();
        // An overwritten value cannot keep a truthiness override from an earlier inference.
        builder
            .comparison_truthiness
            .insert(key, Truthiness::AlwaysFalse);
        legacy_inline(builder.extend_expression_unchecked_with(
            inference,
            include_bindings,
            &LegacyExpressionMergeEffects,
        ));
        assert!(!builder.comparison_truthiness.contains_key(&key));
        if include_bindings && !scope_region {
            assert_eq!(&builder.into_expression_inference(), inference);
        } else {
            assert!(builder.bindings.is_empty());
            assert_eq!(
                builder.expression_type(assignment.value.as_ref()),
                inference.expression_type(assignment.value.as_ref()),
            );
        }
    }
}

struct RefuseUnion;

impl<'db> ExpressionMergeEffects<'db> for RefuseUnion {
    type Error = ();

    async fn prefix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
    ) -> Result<(), Self::Error> {
        builder.extend_expression_prefix(inference);
        Ok(())
    }

    async fn union(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _existing: Type<'db>,
        _incoming: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Err(())
    }

    async fn suffix(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        inference: &ExpressionInference<'db>,
        resolved_cycle: Option<Type<'db>>,
        include_bindings: bool,
    ) -> Result<(), Self::Error> {
        builder.extend_expression_suffix(inference, resolved_cycle, include_bindings);
        Ok(())
    }
}

#[test]
fn cycle_merge_copies_one_value_and_requires_union_for_two() {
    let mut db = setup_db();
    db.write_file("src/main.py", "left = right = True\n")
        .unwrap();
    let file = system_path_to_file(&db, "src/main.py").unwrap();
    let program_file = db.program_file(file);
    let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
    let Stmt::Assign(assignment) = &module.syntax().body[0] else {
        panic!("fixture assignment")
    };
    let index = semantic_index(&db, program_file);
    let expression = index.expression(assignment.value.as_ref());
    let env = ProgramEnvironment::from_file(program_file);
    let make_builder = || {
        TypeInferenceBuilder::new(
            &db,
            &env,
            InferenceRegion::Expression(expression, TypeContext::default()),
            file,
            program_file,
            index,
            &module,
        )
    };
    let mut source = make_builder();
    source.cycle_recovery = Some(Type::bool_literal(true));
    let inference = source.into_expression_inference();

    // These are private builder states; no synthetic result is installed in a query cache.
    for previous in [None, Some(Type::bool_literal(false))] {
        let mut builder = make_builder();
        builder.context.defuse();
        builder.cycle_recovery = previous;
        let result = crate::types::signatures::effects::try_poll_immediate(
            builder.extend_expression_with(&inference, &RefuseUnion),
        );
        if previous.is_none() {
            assert_eq!(result, Poll::Ready(Ok(())));
            assert_eq!(builder.cycle_recovery, Some(Type::bool_literal(true)));
        } else {
            assert_eq!(result, Poll::Ready(Err(())));
            assert_eq!(builder.cycle_recovery, previous);
        }
    }
}
