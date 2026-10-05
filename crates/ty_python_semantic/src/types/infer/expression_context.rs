//! Finite contextual expression identities and their complete passive memo schema.

use std::convert::Infallible;

use ruff_db::diagnostic::Diagnostic;
use salsa::execution_probe::{
    FixedQueryKeyProfile as CopyMemoProfile, PassiveMemoProfile, RegistryBuilder, RunResult,
};
use salsa::plumbing::function::Configuration;
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{
    ExpressionInference, ExpressionInferenceExtra, ExpressionWithContext,
    infer_expression_type_impl, infer_expression_types_impl,
};
use crate::Db;
use crate::types::Type;
use crate::types::constraints::control::hash_slots;

impl FiniteInternedConfiguration for ExpressionWithContext<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // The expression identity, context tag and optional Type have fixed scalar fields.
        // Only Type's inline payload can add hashing or equality work; its handles stay opaque.
        3usize.checked_add(fields.1.annotation.map_or(0, Type::inline_payload_bytes))
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types::infer) struct ExpressionInferenceProfile;

impl<C> PassiveMemoProfile<C> for ExpressionInferenceProfile
where
    C: for<'db> Configuration<Output<'db> = ExpressionInference<'db>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        expression_retirement_work(output)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        expression_retirement_work_with(output, &mut |units| fuel.consume(units))
    }
}

fn expression_retirement_work(inference: &ExpressionInference<'_>) -> Option<usize> {
    expression_retirement_work_with(inference, &mut |_| Ok(())).ok()
}

fn expression_retirement_work_with(
    inference: &ExpressionInference<'_>,
    consume: &mut impl FnMut(usize) -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    consume(1)?;
    let ExpressionInference {
        expressions,
        extra,
        #[cfg(debug_assertions)]
            scope: _,
    } = inference;
    let mut work = 3usize
        .checked_add(expressions.iter().len())
        .ok_or(QuoteError::Overflow)?;
    let Some(extra) = extra.as_deref() else {
        return Ok(work);
    };
    let ExpressionInferenceExtra {
        implicit_aliases,
        string_annotations,
        expected_types,
        type_expression_flags,
        comparison_truthiness,
        collection_use_constraints,
        bindings,
        diagnostics,
        called_functions,
        cycle_recovery: _,
    } = extra;
    // Frozen maps and boxed slices own dense scalar entries. The fixed work includes the
    // extra box and empty containers, including the box in a provisional cycle seed.
    work = [
        implicit_aliases.len(),
        string_annotations.iter().len(),
        expected_types.iter().len(),
        type_expression_flags.iter().len(),
        comparison_truthiness.iter().len(),
        bindings.len(),
        called_functions.len(),
    ]
    .into_iter()
    .try_fold(
        work.checked_add(12).ok_or(QuoteError::Overflow)?,
        |work, length| work.checked_add(length).ok_or(QuoteError::Overflow),
    )?;

    // These private tables only insert, extend and shrink. Their capacity bounds the outer
    // sparse traversal; each inner IndexSet owns dense Copy Types and two backing containers.
    let collection_slots =
        table_retirement_work(collection_use_constraints.capacity()).ok_or(QuoteError::Overflow)?;
    work = work
        .checked_add(collection_slots)
        .ok_or(QuoteError::Overflow)?;
    consume(collection_slots)?;
    #[expect(
        clippy::iter_over_hash_type,
        reason = "retirement sums independent collection payloads"
    )]
    for types in collection_use_constraints.values() {
        work = work
            .checked_add(types.len())
            .and_then(|work| work.checked_add(2))
            .ok_or(QuoteError::Overflow)?;
    }

    let (length, _, used, used_capacity) = diagnostics.storage();
    work = work
        .checked_add(length)
        .and_then(|work| work.checked_add(used))
        .and_then(|work| work.checked_add(table_retirement_work(used_capacity)?))
        .ok_or(QuoteError::Overflow)?;
    consume(length)?;
    for diagnostic in diagnostics {
        work = work
            .checked_add(diagnostic_retirement_work_with(diagnostic, consume)?)
            .ok_or(QuoteError::Overflow)?;
    }
    Ok(work)
}

fn table_retirement_work(capacity: usize) -> Option<usize> {
    if capacity == 0 {
        Some(0)
    } else {
        hash_slots::<Infallible>(capacity).ok()
    }
}

#[cfg(test)]
fn diagnostic_retirement_work(diagnostic: &Diagnostic) -> Option<usize> {
    diagnostic_retirement_work_with(diagnostic, &mut |_| Ok(())).ok()
}

fn diagnostic_retirement_work_with(
    diagnostic: &Diagnostic,
    consume: &mut impl FnMut(usize) -> Result<(), QuoteError>,
) -> Result<usize, QuoteError> {
    consume(1)?;
    // An Arc can become its payload's final owner after quotation. Count the full diagnostic
    // even while it is shared. Messages and source-file handles have passive destruction;
    // each annotation's bound includes its message, tags and possible source/line-index owners.
    let annotations = diagnostic
        .annotations()
        .len()
        .checked_mul(8)
        .ok_or(QuoteError::Overflow)?;
    let edits = diagnostic
        .fix()
        .map_or(0, |fix| fix.edits().len())
        .checked_mul(4)
        .ok_or(QuoteError::Overflow)?;
    let mut work = 16usize
        .checked_add(annotations)
        .and_then(|work| work.checked_add(edits))
        .ok_or(QuoteError::Overflow)?;
    // Subdiagnostics own a box, a message and an annotation vector, with no recursive subs.
    consume(diagnostic.sub_diagnostics().len())?;
    for sub in diagnostic.sub_diagnostics() {
        work = work
            .checked_add(4)
            .and_then(|work| work.checked_add(sub.annotations().len().checked_mul(8)?))
            .ok_or(QuoteError::Overflow)?;
    }
    Ok(work)
}

pub(in crate::types::infer) type ExpressionContextMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::infer::ExpressionWithContext<'static>,
        crate::types::infer::InferExpressionTypesImplConfiguration,
        crate::types::infer::expression_context::ExpressionInferenceProfile,
    >,
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::infer::ExpressionWithContext<'static>,
        crate::types::infer::InferExpressionTypeImplConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
);

pub(super) fn register_expression_context_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::infer::ExpressionWithContext<'static>,
        crate::types::infer::expression_context::ExpressionContextMemoSchema<'db>,
    >,
> {
    let owner = ExpressionWithContext::ingredient(db.zalsa());
    let inference = registry.passive_memo::<_, _, ExpressionInferenceProfile>(
        owner,
        infer_expression_types_impl::fn_ingredient_(db, db.zalsa()),
    )?;
    let ty = registry.passive_memo::<_, _, CopyMemoProfile>(
        owner,
        infer_expression_type_impl::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.finite_interned_values_with_memos(owner, (inference, ty))
}

#[cfg(test)]
mod tests {
    use ruff_db::diagnostic::{
        Annotation, DiagnosticId, DiagnosticTag, Severity, SubDiagnostic, SubDiagnosticSeverity,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_diagnostics::{Edit, Fix};
    use ruff_python_ast::Stmt;
    use ruff_source_file::SourceFileBuilder;
    use ruff_text_size::TextRange;
    use rustc_hash::FxHashSet;
    use salsa::attempt_probe::{AttemptOutcome, try_with_attempt};
    use salsa::execution_probe::{ExecutionAdmission, ExecutionWork, RunError};
    use salsa::plumbing::{AsId, ZalsaDatabase};
    use ty_python_core::definition::Definition;
    use ty_python_core::expression::Expression;
    use ty_python_core::{ExpressionNodeKey, semantic_index};

    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::types::infer::{
        FrozenMap, FrozenSet, InferExpression, TypeContext, TypeExpressionFlags,
        infer_definition_types, infer_expression_types,
    };
    use crate::types::{Truthiness, TypeCheckDiagnostics, todo_type};

    fn fixture() -> anyhow::Result<TestDb> {
        let mut db = setup_db();
        db.write_file("src/main.py", "def f():\n    return 1\nleft = right = f\n")?;
        Ok(db)
    }

    fn fixture_parts(db: &TestDb) -> anyhow::Result<(Expression<'_>, Definition<'_>)> {
        let file = system_path_to_file(db, "src/main.py")?;
        let program_file = db.program_file(file);
        let module = parsed_module(db, program_file.python_file(db)).load(db);
        let Some(Stmt::FunctionDef(function)) = module.syntax().body.first() else {
            anyhow::bail!("fixture function");
        };
        let Some(Stmt::Assign(assignment)) = module.syntax().body.last() else {
            anyhow::bail!("fixture assignment");
        };
        let index = semantic_index(db, program_file);
        Ok((
            index.expression(assignment.value.as_ref()),
            index.expect_single_definition(function),
        ))
    }

    struct Admission;

    impl ExecutionAdmission for Admission {
        fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
            Ok(())
        }
    }

    #[test]
    fn complete_schema_preserves_contextual_identity() -> anyhow::Result<()> {
        let db = fixture()?;
        let mut reader = db.clone();
        let (expression, _) = fixture_parts(&db)?;
        let owner = ExpressionWithContext::ingredient(db.zalsa());
        let inference = infer_expression_types_impl::fn_ingredient_(&db, db.zalsa());
        let _scalar = infer_expression_type_impl::fn_ingredient_(&db, db.zalsa());
        let context = TypeContext::new(Some(Type::unknown()));
        let other_context = TypeContext::new(Some(Type::Never));
        let admission = Admission;
        reader.clear_salsa_events();
        let outcome = try_with_attempt(&db, 1_000_000, || {
            let mut registry = RegistryBuilder::new(&db, &admission)?;
            let incomplete =
                registry.passive_memo::<_, _, ExpressionInferenceProfile>(owner, inference)?;
            assert!(matches!(
                registry.finite_interned_values_with_memos(owner, (incomplete,)),
                Err(RunError::Contract(
                    "finite interned value memo mapping is unsupported"
                ))
            ));
            let first =
                registry.passive_memo::<_, _, ExpressionInferenceProfile>(owner, inference)?;
            let duplicate =
                registry.passive_memo::<_, _, ExpressionInferenceProfile>(owner, inference)?;
            assert!(matches!(
                registry.finite_interned_values_with_memos(owner, (first, duplicate)),
                Err(RunError::Contract(
                    "finite interned value memo mapping is unsupported"
                ))
            ));
            let values = register_expression_context_values(&db, &mut registry)?;
            registry.seal()?.run(|endpoint| async move {
                let first = endpoint.intern_value(&values, (expression, context)).await;
                let equal = endpoint.intern_value(&values, (expression, context)).await;
                let different = endpoint
                    .intern_value(&values, (expression, other_context))
                    .await;
                Ok((first, equal, different))
            })
        });
        let Ok(AttemptOutcome::Complete(Ok((first, equal, different)))) = outcome else {
            anyhow::bail!("contextual interning did not complete: {outcome:?}");
        };
        assert_eq!(first, equal);
        assert_ne!(first, different);
        assert!(
            reader
                .take_salsa_events()
                .iter()
                .all(|event| { !matches!(event.kind, salsa::EventKind::WillExecute { .. }) })
        );
        assert_eq!(
            InferExpression::WithContext(first),
            InferExpression::new(&db, expression, context)
        );
        assert_eq!(
            InferExpression::WithContext(different),
            InferExpression::new(&db, expression, other_context)
        );
        assert_eq!(
            InferExpression::new(&db, expression, TypeContext::default()),
            InferExpression::Bare(expression)
        );
        let payload = todo_type!("contextual annotation payload");
        assert_eq!(
            ExpressionWithContext::field_work(&(expression, TypeContext::new(Some(payload)))),
            3usize.checked_add(payload.inline_payload_bytes())
        );
        Ok(())
    }

    #[test]
    fn retirement_covers_every_owned_expression_field() -> anyhow::Result<()> {
        let db = fixture()?;
        let (expression, definition) = fixture_parts(&db)?;
        let key: ExpressionNodeKey = expression.node_ref(&db).into();
        let scope = expression.scope(&db);
        let function = infer_definition_types(&db, definition)
            .binding_type(definition)
            .as_function_literal()
            .ok_or_else(|| anyhow::anyhow!("fixture function type"))?;
        let mut inference =
            ExpressionInference::cycle_initial(scope, Type::divergent(expression.as_id()));
        let seed = expression_retirement_work(&inference);
        assert!(seed.is_some());
        inference.expressions = FrozenMap::from_iter([(key, Type::unknown())]);
        assert!(expression_retirement_work(&inference) > seed);
        inference.expressions = FrozenMap::default();
        let mut diagnostics = TypeCheckDiagnostics::default();
        diagnostics.push(Diagnostic::new(
            DiagnosticId::RevealedType,
            Severity::Info,
            "type",
        ));
        let extras = [
            ExpressionInferenceExtra {
                implicit_aliases: Box::new([definition]),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                string_annotations: FrozenSet::from(FxHashSet::from_iter([key])),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                expected_types: FrozenMap::from_iter([(key, Type::unknown())]),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                type_expression_flags: FrozenMap::from_iter([(key, TypeExpressionFlags::UNPACK)]),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                comparison_truthiness: FrozenMap::from_iter([(key, Truthiness::AlwaysFalse)]),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                collection_use_constraints: [(
                    definition,
                    [Type::unknown(), Type::Never].into_iter().collect(),
                )]
                .into_iter()
                .collect(),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                bindings: Box::new([(definition, Type::unknown())]),
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                diagnostics,
                ..ExpressionInferenceExtra::default()
            },
            ExpressionInferenceExtra {
                called_functions: Box::new([function]),
                ..ExpressionInferenceExtra::default()
            },
        ];
        let mut reader = db.clone();
        reader.clear_salsa_events();
        for extra in extras {
            inference.extra = Some(Box::new(extra));
            assert!(expression_retirement_work(&inference) > seed);
        }
        assert!(reader.take_salsa_events().is_empty());
        assert_eq!(table_retirement_work(usize::MAX), None);
        Ok(())
    }

    #[test]
    fn diagnostics_quote_possible_final_shared_ownership() {
        let file = SourceFileBuilder::new("sample.py", "x\n").finish();
        file.index();
        let mut annotation = Annotation::primary(file.into());
        annotation.set_message("annotation");
        annotation.push_tag(DiagnosticTag::Unnecessary);
        let mut diagnostic = Diagnostic::new(DiagnosticId::RevealedType, Severity::Info, "type");
        let empty = diagnostic_retirement_work(&diagnostic);
        diagnostic.annotate(annotation.clone());
        let annotated = diagnostic_retirement_work(&diagnostic);
        assert!(annotated > empty);
        let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, "detail");
        sub.annotate(annotation);
        diagnostic.sub(sub);
        let with_sub = diagnostic_retirement_work(&diagnostic);
        assert!(with_sub > annotated);
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            "y".to_owned(),
            TextRange::default(),
        )));
        let with_fix = diagnostic_retirement_work(&diagnostic);
        assert!(with_fix > with_sub);
        let shared = diagnostic.clone();
        assert_eq!(diagnostic_retirement_work(&shared), with_fix);
        drop(diagnostic);
        assert_eq!(diagnostic_retirement_work(&shared), with_fix);
    }

    #[test]
    fn ordinary_suppressed_payload_has_retirement_metadata() -> anyhow::Result<()> {
        let mut db = fixture()?;
        db.write_file(
            "src/main.py",
            "def f():\n    return 1\nleft = right = missing  # ty: ignore[unresolved-reference]\n",
        )?;
        let (expression, _) = fixture_parts(&db)?;
        let inference =
            infer_expression_types(&db, expression, TypeContext::new(Some(Type::unknown())));
        let diagnostics = &inference
            .extra
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("used suppression"))?
            .diagnostics;
        assert!(diagnostics.used_len() > 0);
        let mut reader = db.clone();
        reader.clear_salsa_events();
        assert!(expression_retirement_work(inference).is_some());
        assert!(reader.take_salsa_events().is_empty());
        Ok(())
    }
}
