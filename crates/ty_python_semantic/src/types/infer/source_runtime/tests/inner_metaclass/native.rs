use salsa::execution_probe::NativeValueOperation;

use super::*;
use crate::types::class::static_literal::inner_metaclass::error_for_native_test;
use crate::types::function::DataclassTransformerFlags;
#[cfg(debug_assertions)]
use crate::types::todo_type;
use crate::types::{
    DataclassTransformerParams, MetaclassCandidate, MetaclassTransformInfo, SubclassOfType,
};

/// Runs the production native quotation and admits actual equality in the same execution budget.
fn compare<'db, C: InnerMetaclassConfiguration>(
    prepared: &PreparedAnalysisFile<'db>,
    _ingredient: &IngredientImpl<C>,
    left: &MetaclassSelectionResult<'db>,
    right: &MetaclassSelectionResult<'db>,
    policy: &AnalysisPolicy,
    entered: &Cell<bool>,
    compared: &Cell<bool>,
    remaining: &Cell<Option<usize>>,
) -> Result<AnalysisOutcome<bool>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        RegistryBuilder::with_budget(session.db(), session.budget())?
            .seal()?
            .run(|endpoint| async move {
                entered.set(true);
                remaining.set(salsa::attempt_probe::remaining_allowance_for_diagnostics(
                    session.db(),
                ));
                let quote = super::super::super::inner_metaclass::quote::<C>(
                    endpoint.clone(),
                    NativeValueOperation::Comparison { left, right },
                )
                .await?;
                Ok(endpoint
                    .local_call(|| {
                        endpoint.admit_work(quote.work)?;
                        endpoint.admit(ExecutionWork::Resource {
                            requested_bytes: quote.requested_bytes,
                        })?;
                        endpoint.check_completion()?;
                        compared.set(true);
                        Ok(left == right)
                    })
                    .await)
            })
    })
}

/// Native equality preserves each result variant and refuses before comparing an unfunded inline payload.
/// Todo strings remain borrowed; conflict candidates and conflicting bases also contribute their payloads.
/// The same small scalar allowance admits an interned transform-parameter handle with 128 field
/// specifiers, since equality compares the handle without traversing its stored field-specifier slice.
#[test]
fn inner_metaclass_native_equality_admits_payloads_and_retries() {
    let db = database("class Product: ...\n");
    let prepared = prepare(&db);
    let class = cold_class(&db, &prepared);
    let class_type = ClassType::NonGeneric(ClassLiteral::Static(class));
    let plain = Ok((
        ClassMetaclass::Selected(Type::ClassLiteral(ClassLiteral::Static(class))),
        None,
    ));
    let params = DataclassTransformerParams::new(
        &db,
        DataclassTransformerFlags::default(),
        vec![Type::unknown(); 128].into_boxed_slice(),
    );
    let with_metadata = Ok((
        ClassMetaclass::ProtocolFallback,
        Some(MetaclassTransformInfo {
            params,
            from_explicit_metaclass: true,
        }),
    ));
    let values = [
        plain.clone(),
        with_metadata.clone(),
        Ok((ClassMetaclass::ProtocolFallback, None)),
        Err(error_for_native_test(MetaclassErrorKind::Cycle)),
        Err(error_for_native_test(MetaclassErrorKind::GenericMetaclass)),
        Err(error_for_native_test(MetaclassErrorKind::NotCallable(
            Type::unknown(),
        ))),
        Err(error_for_native_test(
            MetaclassErrorKind::PartlyNotCallable(SubclassOfType::subclass_of_unknown()),
        )),
        Err(error_for_native_test(MetaclassErrorKind::Conflict {
            candidate: MetaclassCandidate {
                metaclass: class_type,
                base: Some(ClassBase::Protocol),
            },
            base_metaclass: class_type,
            base: ClassBase::Class(class_type),
            explicit_metaclass: Some(class_type),
        })),
    ];
    let entered = Cell::new(false);
    let compared = Cell::new(false);
    let remaining = Cell::new(None);
    for left in &values {
        for right in &values {
            assert_eq!(
                compare(
                    &prepared,
                    try_metaclass_inner_ingredient(&db),
                    left,
                    right,
                    &funded(),
                    &entered,
                    &compared,
                    &remaining
                ),
                Ok(AnalysisOutcome::Complete(left == right))
            );
            assert!(compared.replace(false));
            assert_no_active_attempt();
        }
    }
    let before = funded().semantic_work_limit - remaining.get().unwrap();
    let scalar_policy = AnalysisPolicy {
        semantic_work_limit: before + 96,
        ..funded()
    };
    assert_eq!(
        compare(
            &prepared,
            try_metaclass_inner_ingredient(&db),
            &plain,
            &plain,
            &scalar_policy,
            &entered,
            &compared,
            &remaining
        ),
        Ok(AnalysisOutcome::Complete(true))
    );
    assert!(compared.replace(false));
    assert_eq!(
        compare(
            &prepared,
            try_metaclass_inner_ingredient(&db),
            &with_metadata,
            &with_metadata,
            &scalar_policy,
            &entered,
            &compared,
            &remaining
        ),
        Ok(AnalysisOutcome::Complete(true))
    );
    assert!(compared.replace(false));

    #[cfg(debug_assertions)]
    {
        let payload = todo_type!(
            "metaclass equality payload retains enough characters to exceed the scalar work budget without allocating the borrowed text during comparison"
        );
        let different = todo_type!(
            "metaclass equality payload retains enough characters to exceed the scalar work budget without allocating the borrowed text during comparisons"
        );
        let Type::Dynamic(dynamic) = payload else {
            panic!("todo payload");
        };
        let compound = Err(error_for_native_test(MetaclassErrorKind::Conflict {
            candidate: MetaclassCandidate {
                metaclass: class_type,
                base: Some(ClassBase::Dynamic(dynamic)),
            },
            base_metaclass: class_type,
            base: ClassBase::Dynamic(dynamic),
            explicit_metaclass: None,
        }));
        let payload_results = [
            Ok((ClassMetaclass::Selected(payload), None)),
            Ok((
                ClassMetaclass::Selected(SubclassOfType::from(
                    &db,
                    &ProgramEnvironment::from_file(prepared.program_file()),
                    dynamic,
                )),
                None,
            )),
            Err(error_for_native_test(MetaclassErrorKind::NotCallable(
                payload,
            ))),
            Err(error_for_native_test(
                MetaclassErrorKind::PartlyNotCallable(payload),
            )),
            compound,
        ];
        for value in &payload_results {
            assert_eq!(
                compare(
                    &prepared,
                    try_metaclass_inner_ingredient(&db),
                    value,
                    value,
                    &scalar_policy,
                    &entered,
                    &compared,
                    &remaining
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
            assert!(!compared.get());
            assert_eq!(
                compare(
                    &prepared,
                    try_metaclass_inner_ingredient(&db),
                    value,
                    value,
                    &funded(),
                    &entered,
                    &compared,
                    &remaining
                ),
                Ok(AnalysisOutcome::Complete(true))
            );
            assert!(compared.replace(false));
            assert_no_active_attempt();
        }
        let left = Ok((ClassMetaclass::Selected(payload), None));
        let right = Ok((ClassMetaclass::Selected(different), None));
        assert_eq!(
            compare(
                &prepared,
                try_metaclass_inner_ingredient(&db),
                &left,
                &right,
                &funded(),
                &entered,
                &compared,
                &remaining
            ),
            Ok(AnalysisOutcome::Complete(false))
        );
        assert!(compared.replace(false));
    }

    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        entered.set(false);
        let result = compare(
            &prepared,
            try_metaclass_inner_ingredient(&db),
            &plain,
            &plain,
            &AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            },
            &entered,
            &compared,
            &remaining,
        );
        assert!(
            matches!(
                result,
                Ok(AnalysisOutcome::Complete(_))
                    | Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::RequestedAllocationLimit,
                        ..
                    })
            ),
            "{result:?}"
        );
        compared.set(false);
        if entered.get() {
            upper = middle;
        } else {
            lower = middle + 1;
        }
        assert_no_active_attempt();
    }
    assert_eq!(
        compare(
            &prepared,
            try_metaclass_inner_ingredient(&db),
            &plain,
            &plain,
            &AnalysisPolicy {
                requested_bytes_limit: lower,
                ..funded()
            },
            &entered,
            &compared,
            &remaining
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        })
    );
    assert!(entered.get());
    assert!(!compared.get());
    assert_eq!(
        compare(
            &prepared,
            try_metaclass_inner_ingredient(&db),
            &plain,
            &plain,
            &funded(),
            &entered,
            &compared,
            &remaining
        ),
        Ok(AnalysisOutcome::Complete(true))
    );
    assert!(compared.get());
    assert_missing(&db, class);
    assert_no_active_attempt();
}
