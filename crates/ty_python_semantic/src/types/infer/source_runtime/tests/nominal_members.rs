use std::cell::RefCell;
use std::panic::AssertUnwindSafe;

use salsa::execution_probe::{FinalSourceError, FinalSourceMemo};
use salsa::prepared_source_probe::Read;
use ty_python_core::definition::Definition;
use ty_python_core::scope::{NodeWithScopeRef, ScopeId};

use super::*;
use crate::place::Place;
use crate::place::implicit_globals::module_type_body_scope_ingredient;
use crate::place::implicit_symbol::ModuleGlobalSymbolEffects;
use crate::types::class::member_source::{
    InlineMemberSourceEffects, SynchronousImplicitAttributeEffects,
    SynchronousStaticCodeGeneratorEffects,
};
use crate::types::class::{
    CodeGeneratorKind, KnownClassArgument, KnownClassInstanceEffects,
    code_generator_of_static_class_ingredient, implicit_attribute_names_ingredient,
    interpret_class_literal_lookup,
};
use crate::types::infer::SourceDefinitionEffect;
use crate::types::member_lookup::normalization::MemberNormalizationEffects;
use crate::types::{BindingContext, ParamSpecAttrKind};

#[test]
fn callable_member_dispatch_reads_kinds_through_the_canonical_provider() {
    for (kind, name, operation) in [
        (CallableTypeKind::Regular, "__call__", None),
        (CallableTypeKind::DunderParamSpec, "__call__", None),
        (CallableTypeKind::ParamSpecValue, "__call__", None),
        (CallableTypeKind::ClassMethodLike, "__call__", None),
        (
            CallableTypeKind::FunctionLike,
            "__get__",
            Some(GeneralMemberOperation::FunctionDunderGet),
        ),
        (
            CallableTypeKind::StaticMethodLike,
            "__get__",
            Some(GeneralMemberOperation::FunctionDunderGet),
        ),
        (
            CallableTypeKind::ClassMethodLike,
            "__get__",
            Some(GeneralMemberOperation::FunctionDunderGet),
        ),
        (
            CallableTypeKind::FunctionLike,
            "__call__",
            Some(GeneralMemberOperation::DunderCall),
        ),
        (
            CallableTypeKind::StaticMethodLike,
            "__call__",
            Some(GeneralMemberOperation::DunderCall),
        ),
        (
            CallableTypeKind::StaticMethodLike,
            "__func__",
            Some(GeneralMemberOperation::UnderlyingFunction),
        ),
        (
            CallableTypeKind::ClassMethodLike,
            "__func__",
            Some(GeneralMemberOperation::UnderlyingFunction),
        ),
        (
            CallableTypeKind::FunctionLike,
            "attribute",
            Some(GeneralMemberOperation::CallableRuntimeClass),
        ),
        (
            CallableTypeKind::Regular,
            "attribute",
            Some(GeneralMemberOperation::ObjectFallback),
        ),
    ] {
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let ty = Type::Callable(CallableType::new(
            &db,
            CallableSignature::single(crate::types::signatures::Signature::unknown()),
            kind,
        ));
        observations::reset(None);
        let cold = capture(&db, || {
            controlled_member(
                &prepared,
                MemberReceiver::Type(ty),
                &Name::new_static(name),
                MemberLookupPolicy::default(),
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
            )
        })
        .unwrap();
        if let Some(operation) = operation {
            assert_eq!(
                cold.value,
                Ok(unavailable(OperationId::MemberLookup(operation))),
                "{kind:?}.{name}"
            );
        } else {
            cold.check_root_reads().unwrap();
            let Ok(AnalysisOutcome::Complete((receiver, result))) = cold.value else {
                panic!("{kind:?}.{name}: {:?}", cold.value);
            };
            assert_eq!(receiver, ty);
            assert_eq!(result.unwrap().member(&db).place, Place::bound(ty));
            let env = ProgramEnvironment::from_file(prepared.program_file());
            assert_eq!(
                result.unwrap().member(&db),
                ty.member_lookup_with_policy(&db, &env, name, MemberLookupPolicy::default(),)
            );
        }
        assert_no_active_attempt();
    }
}

#[test]
fn paramspec_member_dispatch_preserves_kind_and_existing_attribute() {
    for (kind, attribute, name, operation) in [
        (
            TypeVarKind::LegacyParamSpec,
            None,
            "args",
            GeneralMemberOperation::ParamSpec,
        ),
        (
            TypeVarKind::Pep695ParamSpec,
            None,
            "kwargs",
            GeneralMemberOperation::ParamSpec,
        ),
        (
            TypeVarKind::LegacyParamSpec,
            Some(ParamSpecAttrKind::Args),
            "kwargs",
            GeneralMemberOperation::ParamSpec,
        ),
        (
            TypeVarKind::Pep695ParamSpec,
            Some(ParamSpecAttrKind::Kwargs),
            "args",
            GeneralMemberOperation::ParamSpec,
        ),
        (
            TypeVarKind::LegacyTypeVar,
            None,
            "args",
            GeneralMemberOperation::TypeVar,
        ),
        (
            TypeVarKind::LegacyParamSpec,
            None,
            "attribute",
            GeneralMemberOperation::TypeVar,
        ),
    ] {
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let raw = TypeVarInstance::new(
            &db,
            TypeVarIdentity::new(&db, Name::new_static("P"), None, kind),
            None,
            None,
            None,
        );
        let variable = BoundTypeVarInstance::new(
            &db,
            raw,
            BindingContext::Synthetic(program),
            attribute,
            crate::types::typevar::TypeVarNonce::NONE,
        );
        observations::reset(None);
        let cold = capture(&db, || {
            controlled_member(
                &prepared,
                MemberReceiver::Type(Type::TypeVar(variable)),
                &Name::new_static(name),
                MemberLookupPolicy::default(),
                &funded(),
                NO_EXTRA_MODULE_CHARGE,
            )
        })
        .unwrap();
        assert_eq!(
            cold.value,
            Ok(unavailable(OperationId::MemberLookup(operation)))
        );
        assert_no_active_attempt();
    }
}

#[derive(Clone, Debug, Default)]
struct Recovery {
    initial: Vec<salsa::Id>,
    entered: Vec<(salsa::Id, u32)>,
    completed: Vec<salsa::Id>,
    first_remaining: Option<usize>,
    cancellation_requested: bool,
}

thread_local! {
    static RECORDING: Cell<bool> = const { Cell::new(false) };
    static CANCEL_RECOVERY: Cell<bool> = const { Cell::new(false) };
    static RECOVERY: RefCell<Recovery> = RefCell::new(Recovery::default());
}

struct Recording;

impl Recording {
    fn start(cancel: bool) -> Self {
        assert!(!RECORDING.replace(true));
        CANCEL_RECOVERY.set(cancel);
        RECOVERY.with_borrow_mut(|recovery| *recovery = Recovery::default());
        observations::reset(None);
        Self
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.set(false);
        CANCEL_RECOVERY.set(false);
    }
}

pub(in crate::types::infer::source_runtime) fn observe_initial(id: salsa::Id) {
    if RECORDING.get() {
        RECOVERY.with_borrow_mut(|recovery| recovery.initial.push(id));
    }
}

pub(in crate::types::infer::source_runtime) fn observe_recovery(
    db: &dyn Db,
    id: salsa::Id,
    iteration: u32,
) {
    if RECORDING.get() {
        RECOVERY.with_borrow_mut(|recovery| {
            recovery.entered.push((id, iteration));
            if recovery.first_remaining.is_none() {
                recovery.first_remaining =
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
            }
        });
        if CANCEL_RECOVERY.replace(false) {
            RECOVERY.with_borrow_mut(|recovery| recovery.cancellation_requested = true);
            db.cancellation_token().cancel();
        }
    }
}

pub(in crate::types::infer::source_runtime) fn observe_recovered(id: salsa::Id) {
    if RECORDING.get() {
        RECOVERY.with_borrow_mut(|recovery| recovery.completed.push(id));
    }
}

fn cyclic_member_fixture() -> TestDb {
    TestDbBuilder::new()
        .with_file("src/main.py", "import main\nvalue = main.value or True\n")
        .build()
        .unwrap()
}

fn member<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<(Type<'db>, MemberLookupResult<'db>)>, AnalysisFailure> {
    controlled_member(
        prepared,
        MemberReceiver::Module(&ModuleName::new_static("main").unwrap()),
        &Name::new_static("value"),
        MemberLookupPolicy::default(),
        policy,
        NO_EXTRA_MODULE_CHARGE,
    )
}

#[test]
fn natural_member_cycle_publishes_the_canonical_result_and_reuses_it() {
    let db = cyclic_member_fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start(false);
    let cold = capture(&db, || member(&prepared, &funded())).unwrap();
    drop(recording);
    let Ok(AnalysisOutcome::Complete((receiver, result))) = cold.value else {
        panic!("natural member cycle: {:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let key = MemberLookupKey::new(
        &db,
        prepared.program_file().program(&db),
        receiver,
        "value",
        MemberLookupPolicy::default(),
    );
    let recovery = RECOVERY.with_borrow(Clone::clone);
    assert!(recovery.initial.contains(&key.as_id()), "{recovery:?}");
    assert!(
        recovery.entered.iter().any(|(id, _)| *id == key.as_id()),
        "{recovery:?}"
    );
    assert_eq!(
        recovery.completed,
        recovery
            .entered
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, member_lookup_ingredient(&db), key.as_id())
            .is_ok()
    );
    assert_eq!(
        result.unwrap().member(&db).place.expect_type(),
        Type::bool_literal(true)
    );
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();

    let ordinary_db = cyclic_member_fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_file = ordinary_prepared.program_file();
    let ordinary_program = ordinary_file.program(&ordinary_db);
    let ordinary_module = resolve_module_confident(
        &ordinary_db,
        ordinary_program.resolver_environment(&ordinary_db),
        &ModuleName::new_static("main").unwrap(),
    )
    .unwrap();
    let ordinary_key = MemberLookupKey::new(
        &ordinary_db,
        ordinary_program,
        Type::module_literal(&ordinary_db, ordinary_file, ordinary_module),
        "value",
        MemberLookupPolicy::default(),
    );
    let expected = member_lookup_with_policy_impl(&ordinary_db, ordinary_key, None, None)
        .unwrap()
        .member(&ordinary_db);
    let actual = result.unwrap().member(&db);
    assert_eq!(actual.place.expect_type(), expected.place.expect_type());
    assert_eq!(actual.qualifiers, expected.qualifiers);

    let mut reader = db.clone();
    reader.take_salsa_events();
    assert_eq!(member_lookup_with_policy_impl(&db, key, None, None), result);
    let recording = Recording::start(false);
    assert_eq!(member(&prepared, &funded()), cold.value);
    drop(recording);
    assert!(RECOVERY.with_borrow(|recovery| recovery.entered.is_empty()));
    let events = reader.take_salsa_events();
    for query in ["member_lookup_with_policy_inner", "infer_definition_types"] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn interrupted_member_recovery_cleans_up_and_retries_in_the_same_revision() {
    let measured = cyclic_member_fixture();
    let prepared = prepare(&measured);
    let recording = Recording::start(false);
    assert!(matches!(
        member(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    drop(recording);
    let recovery_work = funded().semantic_work_limit
        - RECOVERY.with_borrow(|recovery| recovery.first_remaining.unwrap());

    for cancel in [false, true] {
        let db = cyclic_member_fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let recording = Recording::start(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: recovery_work,
                ..funded()
            }
        };
        let interrupted = salsa::Cancelled::catch(AssertUnwindSafe(|| member(&prepared, &policy)));
        drop(recording);
        let recovery = RECOVERY.with_borrow(Clone::clone);
        assert!(!recovery.entered.is_empty(), "{recovery:?}");
        assert_eq!(recovery.cancellation_requested, cancel);
        if cancel {
            assert!(
                matches!(interrupted, Err(salsa::Cancelled::Local)),
                "{interrupted:?}"
            );
            assert_eq!(recovery.completed.len(), recovery.entered.len());
        } else {
            assert_eq!(
                interrupted.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                })
            );
            assert!(recovery.completed.is_empty(), "{recovery:?}");
        }
        let id = recovery.entered[0].0;
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, member_lookup_ingredient(&db), id).is_ok(),
            cancel
        );
        assert_eq!(observations::counts().0, 0);
        assert_no_active_attempt();

        let recording = Recording::start(false);
        let retry = capture(&db, || member(&prepared, &funded())).unwrap();
        drop(recording);
        let Ok(AnalysisOutcome::Complete((_, result))) = retry.value else {
            panic!("member recovery retry: {:?}", retry.value);
        };
        retry.check_root_reads().unwrap();
        let recovery = RECOVERY.with_borrow(Clone::clone);
        if cancel {
            assert!(recovery.entered.is_empty(), "{recovery:?}");
        } else {
            assert!(
                recovery.entered.iter().any(|(key, _)| *key == id),
                "{recovery:?}"
            );
            assert_eq!(recovery.completed.len(), recovery.entered.len());
        }
        assert_eq!(
            result.unwrap().member(&db).place.expect_type(),
            Type::bool_literal(true)
        );
        assert!(
            FinalSourceMemo::certify(&db as &dyn Db, member_lookup_ingredient(&db), id).is_ok()
        );
        assert_eq!(observations::counts().0, 0);
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

pub(in crate::types::infer) trait MemberOperation<'db>: Sized {
    type Output;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run;
}

impl<'db> MemberOperation<'db> for DescriptorRequest<'db> {
    type Output = DescriptorResult<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.descriptor_get(self).await
    }
}

struct MemberPartsRequest<'db>(LookupParts<'db>);

struct DescriptorEntryRequest<'db>(DescriptorRequest<'db>);

impl<'db> MemberOperation<'db> for DescriptorEntryRequest<'db> {
    type Output = DescriptorResult<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let endpoint = access.endpoint();
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(ProgramEnvironment::from_program(program))
            })
            .await;
        crate::types::descriptor::effects::DescriptorEffects::descriptor(
            &SourceEffects::new(access, program),
            access.db(),
            &env,
            self.0,
        )
        .await
    }
}

impl<'db> MemberOperation<'db> for MemberPartsRequest<'db> {
    type Output = (MemberLookupResult<'db>, LookupParts<'db>);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let result = access.member_result(self.0).await?;
        let parts =
            MemberNormalizationEffects::parts(&SourceEffects::new(access, program), result).await?;
        Ok((result, parts))
    }
}

fn controlled_descriptor<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    request: DescriptorRequest<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<DescriptorResult<'db>>, AnalysisFailure> {
    controlled_member_operation(prepared, request, policy)
}

pub(in crate::types::infer) fn controlled_member_operation<'db, O: MemberOperation<'db>>(
    prepared: &PreparedAnalysisFile<'db>,
    operation: O,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<O::Output>, AnalysisFailure> {
    with_analysis_session(prepared, policy, |session| {
        let environments = StableStorage::new();
        let builders = StableStorage::new();
        let owners = StableStorage::new();
        let default_arguments = StableStorage::new();
        let return_callables = crate::types::relation::source::resources::ReturnCallableMappingStorage::new();
        let mapping = StableStorage::new();
        let checkers = CheckerStorage::new();
        let resources = SourceResources::new(
            &environments,
            &builders,
            &owners,
            &mapping,
            &checkers,
            &default_arguments,
            &return_callables,
        );
        let mut registry = RegistryBuilder::with_budget(session.db(), session.budget())?;
        let (function, overload) = register_function_values(session.db(), &mut registry)?;
        let callable = register_callable_values(session.db(), &mut registry)?;
        let bound_method = register_bound_method_values(session.db(), &mut registry)?;
        let descriptor_get_call_context =
            register_descriptor_get_call_context_values(session.db(), &mut registry)?;
        let descriptor_dispatch = register_descriptor_dispatch_values(session.db(), &mut registry)?;
        let descriptor_dispatches = register_descriptor_dispatches_values(session.db(), &mut registry)?;
        let property = register_property_values(session.db(), &mut registry)?;
        let tuple = register_tuple_values(session.db(), &mut registry)?;
        let string_literal = registry.finite_interned_values_with_memos(
            StringLiteralType::ingredient(session.db().zalsa()),
            (),
        )?;
        let union = register_union_values(session.db(), &mut registry)?;
        let intersection = register_intersection_values(session.db(), &mut registry)?;
        let module = register_module_values(session.db(), &mut registry)?;
        let class = register_class_values(session.db(), &mut registry)?;
        let known_class = register_known_class_values(session.db(), &mut registry)?;
        let member = register_member_lookup_values(session.db(), &mut registry)?;
        let type_pair = register_source_type_pair_values(session.db(), &mut registry)?;
        let expression_context = register_expression_context_values(session.db(), &mut registry)?;
        let values = SourceValues {
            type_pair,
            expression_context,
            function,
            overload,
            callable,
            bound_method,
            descriptor_get_call_context,
            descriptor_dispatch,
            descriptor_dispatches,
            property,
            tuple,
            string_literal,
            union,
            intersection,
            module,
            class,
            known_class,
            member,
        };
        let (run, routes) = register(session, prepared, registry, &values, resources)?;
        let values = &values;
        let weak = Rc::downgrade(&routes);
        let query_routes = Rc::clone(&routes);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes: query_routes,
                    values,
                };
                operation.run(&access, session.program()).await
            })
        }));
        crate::types::relation::source::receiver_constraint_observations::observe_run_drained();
        assert_eq!(observations::counts().0, 0);
        let normalization = crate::types::normalization::source::observations::snapshot();
        assert_eq!(normalization.children, normalization.dropped);
        assert_eq!(normalization.buffers, normalization.dropped_buffers);
        assert_eq!(normalization.live_buffers, 0);
        assert_eq!(Rc::strong_count(&routes), 1);
        drop(routes);
        assert!(weak.upgrade().is_none());
        match result {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

#[derive(Clone, Copy)]
struct ImplicitModuleNameRequest<'db> {
    file: ProgramFile<'db>,
    name: &'static str,
}

impl<'db> MemberOperation<'db> for ImplicitModuleNameRequest<'db> {
    type Output = bool;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<bool>
    where
        'db: 'run,
    {
        ModuleGlobalSymbolEffects::is_module_global(
            &SourceEffects::new(access, program),
            access.db(),
            self.file,
            self.name,
        )
        .await
    }
}

struct ModuleTypeScopeEvents {
    db: TestDb,
    key: salsa::DatabaseKeyIndex,
    cancellation: Option<salsa::CancellationToken>,
    entries: usize,
    first_remaining: Option<usize>,
}

thread_local! {
    static MODULE_TYPE_SCOPE_EVENTS: RefCell<Option<ModuleTypeScopeEvents>> = const { RefCell::new(None) };
}

struct ModuleTypeScopeRecording;

impl ModuleTypeScopeRecording {
    fn start(db: &TestDb, program: Program<'_>, cancel: bool) -> Self {
        MODULE_TYPE_SCOPE_EVENTS.with_borrow_mut(|events| {
            assert!(events.is_none());
            *events = Some(ModuleTypeScopeEvents {
                db: db.clone(),
                key: module_type_body_scope_ingredient(db).database_key_index(program.as_id()),
                cancellation: cancel.then(|| db.cancellation_token()),
                entries: 0,
                first_remaining: None,
            });
        });
        observations::reset(None);
        Self
    }

    fn snapshot(&self) -> (usize, Option<usize>) {
        MODULE_TYPE_SCOPE_EVENTS.with_borrow(|events| {
            let events = events.as_ref().unwrap();
            (events.entries, events.first_remaining)
        })
    }
}

impl Drop for ModuleTypeScopeRecording {
    fn drop(&mut self) {
        MODULE_TYPE_SCOPE_EVENTS.with_borrow_mut(|events| *events = None);
    }
}

fn module_type_scope_event(event: &salsa::EventKind) {
    if let salsa::EventKind::WillExecute { database_key } = event {
        MODULE_TYPE_SCOPE_EVENTS.with_borrow_mut(|events| {
            if let Some(events) = events
                && *database_key == events.key
            {
                events.entries += 1;
                if events.first_remaining.is_none() {
                    events.first_remaining =
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(&events.db);
                }
                if let Some(cancellation) = events.cancellation.take() {
                    cancellation.cancel();
                }
            }
        });
    }
}

fn module_type_scope_fixture(custom_typeshed: bool) -> TestDb {
    let mut builder = TestDbBuilder::new()
        .with_salsa_event_callback(module_type_scope_event)
        .with_file("src/main.py", "pass\n");
    if custom_typeshed {
        builder = builder
            .with_custom_typeshed("/typeshed".into())
            .with_file("/typeshed/stdlib/VERSIONS", "types: 3.0-\n")
            .with_file("/typeshed/stdlib/types.pyi", "class ModuleType: ...\n");
    }
    builder.build().unwrap()
}

/// Distinct implicit names reuse one canonical ModuleType scope memo while retaining independent
/// membership results: `__name__` is an implicit global, whereas `__dict__` is only a module member.
#[test]
fn implicit_module_names_share_the_canonical_scope_memo() {
    let db = module_type_scope_fixture(false);
    let prepared = prepare(&db);
    let file = prepared.program_file();
    let program = file.program(&db);
    let ingredient = module_type_body_scope_ingredient(&db);
    let key = ingredient.database_key_index(program.as_id());
    let revision = salsa::plumbing::current_revision(&db);
    let mut reader = db.clone();
    reader.take_salsa_events();
    observations::reset(None);

    let first = capture(&db, || {
        controlled_member_operation(
            &prepared,
            ImplicitModuleNameRequest {
                file,
                name: "__name__",
            },
            &funded(),
        )
    })
    .unwrap();
    assert_eq!(first.value, Ok(AnalysisOutcome::Complete(true)));
    first.check_root_reads().unwrap();
    let first_read = first.reads.iter().find(|read| read.key == key).unwrap();
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, program.as_id()).is_ok());
    assert!(
        find_will_execute_event_by_name(
            &db,
            "module_type_body_scope_inner",
            Some(program.as_id()),
            &reader.take_salsa_events(),
        )
        .is_some()
    );

    let second = capture(&db, || {
        controlled_member_operation(
            &prepared,
            ImplicitModuleNameRequest {
                file,
                name: "__dict__",
            },
            &funded(),
        )
    })
    .unwrap();
    assert_eq!(second.value, Ok(AnalysisOutcome::Complete(false)));
    second.check_root_reads().unwrap();
    let second_read = second.reads.iter().find(|read| read.key == key).unwrap();
    assert_eq!(
        second_read.status,
        salsa::prepared_source_probe::Status::Final
    );
    assert_eq!(second_read.memo_address, first_read.memo_address);
    assert_eq!(second_read.stamp, first_read.stamp);
    assert_function_query_was_not_run_by_name(
        &db,
        "module_type_body_scope_inner",
        Some(program.as_id()),
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Work refusal and local cancellation at scope-query entry leave its memo unpublished. The route
/// harness verifies owner cleanup before a funded retry completes in the same database revision.
#[test]
fn interrupted_module_type_scope_releases_owners_and_retries_in_the_same_revision() {
    let measured = module_type_scope_fixture(false);
    let measured_prepared = prepare(&measured);
    let measured_program = measured_prepared.program_file().program(&measured);
    let recording = ModuleTypeScopeRecording::start(&measured, measured_program, false);
    assert_eq!(
        controlled_member_operation(
            &measured_prepared,
            ImplicitModuleNameRequest {
                file: measured_prepared.program_file(),
                name: "__name__",
            },
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete(true)),
    );
    let (entries, remaining) = recording.snapshot();
    assert!(entries > 0);
    let entry_work = funded().semantic_work_limit - remaining.unwrap();
    drop(recording);

    for cancel in [false, true] {
        let db = module_type_scope_fixture(false);
        let prepared = prepare(&db);
        let program = prepared.program_file().program(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let request = ImplicitModuleNameRequest {
            file: prepared.program_file(),
            name: "__name__",
        };
        let recording = ModuleTypeScopeRecording::start(&db, program, cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: entry_work,
                ..funded()
            }
        };
        let stopped = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_member_operation(&prepared, request, &policy)
        }));
        let (entries, remaining) = recording.snapshot();
        assert!(entries > 0);
        drop(recording);
        if cancel {
            assert!(
                matches!(stopped, Err(salsa::Cancelled::Local)),
                "{stopped:?}"
            );
        } else {
            assert_eq!(remaining, Some(0));
            assert_eq!(
                stopped.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            );
        }
        let ingredient = module_type_body_scope_ingredient(&db);
        assert_eq!(
            FinalSourceMemo::certify(&db as &dyn Db, ingredient, program.as_id()).map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        assert_no_active_attempt();
        observations::reset(None);
        assert_eq!(
            controlled_member_operation(&prepared, request, &funded()),
            Ok(AnalysisOutcome::Complete(true)),
        );
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, program.as_id()).is_ok());
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

/// The direct scope selector only accepts vendored definitions. A custom typeshed instead requires
/// the unsupported `ImplicitModuleClass` fallback; repeating the lookup reports that refusal without
/// publishing a scope memo.
#[test]
fn custom_module_type_scope_refuses_without_publishing_a_memo() {
    let db = module_type_scope_fixture(true);
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let revision = salsa::plumbing::current_revision(&db);
    for _ in 0..2 {
        observations::reset(None);
        assert_eq!(
            controlled_member_operation(
                &prepared,
                ImplicitModuleNameRequest {
                    file: prepared.program_file(),
                    name: "__name__",
                },
                &funded(),
            ),
            Ok(unavailable(OperationId::DefinitionBody(
                SourceDefinitionEffect::ImplicitModuleClass,
            ))),
        );
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                module_type_body_scope_ingredient(&db),
                program.as_id(),
            )
            .map(|_| ()),
            Err(FinalSourceError::MissingMemo),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn member_result_fields_preserve_errors_and_metadata_with_explicit_reads() {
    for has_error in [false, true] {
        for has_metadata in [false, true] {
            let db = boolean_fixture(true);
            let prepared = prepare(&db);
            let revision = salsa::plumbing::current_revision(&db);
            let parts = LookupParts {
                member: Place::bound(Type::bool_literal(true)).into(),
                error: has_error.then_some(crate::types::MemberLookupErrorKind::GetAttr {
                    receiver: Type::object(),
                    name: Type::unknown(),
                }),
                properties: None,
                descriptor: DescriptorOrigin {
                    incomplete: has_metadata,
                    ..DescriptorOrigin::default()
                },
            };
            observations::reset(None);
            let cold = capture(&db, || {
                controlled_member_operation(&prepared, MemberPartsRequest(parts), &funded())
            })
            .unwrap();
            assert!(cold.reads.is_empty());
            let Ok(AnalysisOutcome::Complete((result, actual))) = cold.value else {
                panic!("member result fields did not complete");
            };
            assert_eq!(actual.member, parts.member);
            assert_eq!(actual.error, parts.error);
            assert_eq!(actual.properties, parts.properties);
            assert_eq!(actual.descriptor, parts.descriptor);
            assert_eq!(
                result,
                crate::types::member_lookup_result_with_origin(
                    &db,
                    parts.member,
                    parts.error,
                    parts.properties,
                    parts.descriptor,
                )
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[test]
fn native_slot_descriptors_use_explicit_fields_and_preserve_class_access() {
    for instance in [None, Some(Type::object())] {
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let slot = crate::types::SlotDescriptorType::new(&db, Type::bool_literal(true));
        let request = DescriptorRequest {
            ty: Type::SlotDescriptor(slot),
            instance,
            owner: Type::object(),
        };
        observations::reset(None);
        let cold = capture(&db, || {
            controlled_member_operation(&prepared, DescriptorEntryRequest(request), &funded())
        })
        .unwrap();
        assert!(cold.reads.is_empty());
        let Ok(AnalysisOutcome::Complete(result)) = cold.value else {
            panic!("slot descriptor did not complete");
        };
        let env = ProgramEnvironment::from_file(prepared.program_file());
        assert_eq!(
            result,
            crate::types::descriptor::evaluate_entry(&db, &env, request, None)
        );
        let actual = result.unwrap().unwrap();
        assert_eq!(actual.kind, crate::types::AttributeKind::DataDescriptor);
        assert_eq!(
            actual.return_type,
            if instance.is_some() {
                Type::bool_literal(true)
            } else {
                request.ty
            }
        );
        assert_no_active_attempt();
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DescriptorObservation {
    entries: usize,
    first_remaining: Option<usize>,
    cancellation_requested: bool,
}

thread_local! {
    static DESCRIPTOR_RECORDING: Cell<bool> = const { Cell::new(false) };
    static DESCRIPTOR_CANCEL: Cell<bool> = const { Cell::new(false) };
    static DESCRIPTOR_OBSERVATION: Cell<DescriptorObservation> = Cell::new(DescriptorObservation::default());
}

struct DescriptorRecording;

impl DescriptorRecording {
    fn start(cancel: bool) -> Self {
        assert!(!DESCRIPTOR_RECORDING.replace(true));
        DESCRIPTOR_CANCEL.set(cancel);
        DESCRIPTOR_OBSERVATION.set(DescriptorObservation::default());
        observations::reset(None);
        Self
    }
}

impl Drop for DescriptorRecording {
    fn drop(&mut self) {
        DESCRIPTOR_RECORDING.set(false);
        DESCRIPTOR_CANCEL.set(false);
    }
}

pub(in crate::types::infer::source_runtime) fn observe_descriptor_body(db: &dyn Db) {
    if DESCRIPTOR_RECORDING.get() {
        let mut observed = DESCRIPTOR_OBSERVATION.get();
        observed.entries += 1;
        if observed.first_remaining.is_none() {
            observed.first_remaining =
                salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
        }
        if DESCRIPTOR_CANCEL.replace(false) {
            observed.cancellation_requested = true;
            db.cancellation_token().cancel();
        }
        DESCRIPTOR_OBSERVATION.set(observed);
    }
}

fn descriptor_request() -> DescriptorRequest<'static> {
    DescriptorRequest {
        ty: Type::unknown(),
        instance: Some(Type::object()),
        owner: Type::object(),
    }
}

#[test]
fn canonical_descriptor_protocol_preserves_results_and_reuses_the_memo() {
    let db = boolean_fixture(true);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let recording = DescriptorRecording::start(false);
    let cold = capture(&db, || {
        controlled_descriptor(&prepared, descriptor_request(), &funded())
    })
    .unwrap();
    drop(recording);
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = cold.value else {
        panic!("cold descriptor: {:?}", cold.value);
    };
    assert_eq!(DESCRIPTOR_OBSERVATION.get().entries, 1);
    let events = events_db.take_salsa_events();
    let event =
        find_will_execute_event_by_name(&db, "try_call_dunder_get_inner", None, &events).unwrap();
    let salsa::EventKind::WillExecute { database_key } = event.kind else {
        panic!("expected descriptor execution");
    };
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            descriptor_get_ingredient(&db),
            database_key.key_index()
        )
        .is_ok()
    );
    let descriptor = result.unwrap().unwrap();
    assert_eq!(descriptor.return_type, Type::unknown());
    assert_eq!(descriptor.origin, DescriptorOrigin::default());
    assert!(descriptor.kind.is_data());

    let ordinary_db = boolean_fixture(true);
    let ordinary_prepared = prepare(&ordinary_db);
    let env =
        ProgramEnvironment::from_program(ordinary_prepared.program_file().program(&ordinary_db));
    let request = descriptor_request();
    assert_eq!(
        result,
        request
            .ty
            .try_call_dunder_get(&ordinary_db, &env, request.instance, request.owner)
    );

    let env = ProgramEnvironment::from_program(prepared.program_file().program(&db));
    assert_eq!(
        result,
        request
            .ty
            .try_call_dunder_get(&db, &env, request.instance, request.owner)
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "try_call_dunder_get_inner",
        None,
        &events_db.take_salsa_events(),
    );
    let recording = DescriptorRecording::start(false);
    assert_eq!(
        controlled_descriptor(&prepared, request, &funded()),
        Ok(AnalysisOutcome::Complete(result))
    );
    drop(recording);
    assert_eq!(DESCRIPTOR_OBSERVATION.get().entries, 0);
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn interrupted_descriptor_protocol_cleans_up_and_retries_in_the_same_revision() {
    let measured = boolean_fixture(true);
    let measured_prepared = prepare(&measured);
    let recording = DescriptorRecording::start(false);
    assert!(matches!(
        controlled_descriptor(&measured_prepared, descriptor_request(), &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    drop(recording);
    let entry_work =
        funded().semantic_work_limit - DESCRIPTOR_OBSERVATION.get().first_remaining.unwrap();

    for cancel in [false, true] {
        let db = boolean_fixture(true);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let recording = DescriptorRecording::start(cancel);
        let policy = if cancel {
            funded()
        } else {
            AnalysisPolicy {
                semantic_work_limit: entry_work,
                ..funded()
            }
        };
        let interrupted = salsa::Cancelled::catch(AssertUnwindSafe(|| {
            controlled_descriptor(&prepared, descriptor_request(), &policy)
        }));
        drop(recording);
        assert_eq!(DESCRIPTOR_OBSERVATION.get().entries, 1);
        assert_eq!(DESCRIPTOR_OBSERVATION.get().cancellation_requested, cancel);
        if cancel {
            assert!(
                matches!(interrupted, Err(salsa::Cancelled::Local)),
                "{interrupted:?}"
            );
        } else {
            assert_eq!(
                interrupted.unwrap(),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
        }
        let events = events_db.take_salsa_events();
        let event =
            find_will_execute_event_by_name(&db, "try_call_dunder_get_inner", None, &events)
                .unwrap();
        let salsa::EventKind::WillExecute { database_key } = event.kind else {
            panic!("expected descriptor execution");
        };
        assert_eq!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                descriptor_get_ingredient(&db),
                database_key.key_index()
            )
            .is_ok(),
            cancel,
            "cancel={cancel}"
        );
        assert_no_active_attempt();

        let recording = DescriptorRecording::start(false);
        let retry = capture(&db, || {
            controlled_descriptor(&prepared, descriptor_request(), &funded())
        })
        .unwrap();
        drop(recording);
        retry.check_root_reads().unwrap();
        let Ok(AnalysisOutcome::Complete(Ok(Some(result)))) = retry.value else {
            panic!("descriptor retry: {:?}", retry.value);
        };
        assert_eq!(result.return_type, Type::unknown());
        assert_eq!(DESCRIPTOR_OBSERVATION.get().entries, usize::from(!cancel));
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                descriptor_get_ingredient(&db),
                database_key.key_index()
            )
            .is_ok()
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[derive(Clone, Copy)]
enum CodeGeneratorRequest<'db> {
    Definition(Definition<'db>),
    Known(KnownClass),
}

impl<'db> CodeGeneratorRequest<'db> {
    fn for_fixture(prepared: &PreparedAnalysisFile<'db>, known: Option<KnownClass>) -> Self {
        if let Some(known) = known {
            return Self::Known(known);
        }
        let class = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .find_map(Stmt::as_class_def_stmt)
            .unwrap();
        Self::Definition(prepared.semantic_index().expect_single_definition(class))
    }

    fn ordinary_class(self, db: &'db TestDb, program: Program<'db>) -> StaticClassLiteral<'db> {
        match self {
            Self::Definition(definition) => {
                let Some(ClassLiteral::Static(class)) =
                    infer_definition_types(db, definition).original_class_type(definition)
                else {
                    panic!("fixture definition is not a static class");
                };
                class
            }
            Self::Known(known) => known
                .try_to_class_literal(db, &ProgramEnvironment::from_program(program))
                .unwrap(),
        }
    }
}

impl<'db> MemberOperation<'db> for CodeGeneratorRequest<'db> {
    type Output = (StaticClassLiteral<'db>, Option<CodeGeneratorKind<'db>>);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let endpoint = access.endpoint();
        let class = match self {
            Self::Definition(definition) => {
                let inference = access.definition(definition).await?;
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(2)?;
                        endpoint.check_completion()?;
                        let Some(ClassLiteral::Static(class)) =
                            inference.original_class_type(definition)
                        else {
                            return Err(RunError::Contract(
                                "fixture definition is not a static class",
                            ));
                        };
                        Ok(class)
                    })
                    .await
            }
            Self::Known(known) => {
                let lookup = access.known_class_lookup(program, known).await?;
                endpoint
                    .local_call(|| {
                        endpoint.admit_work(2)?;
                        endpoint.check_completion()?;
                        interpret_class_literal_lookup(lookup)
                            .ok_or(RunError::Contract("fixture known class is unavailable"))
                    })
                    .await
            }
        };
        if CODE_GENERATOR_RECORDING.get() {
            CODE_GENERATOR_OBSERVATION.with(|observed| {
                observed.set(CodeGeneratorObservation {
                    key: Some(class.as_id()),
                    ..observed.get()
                });
            });
        }
        Ok((class, access.code_generator(class).await?))
    }
}

fn ordinary_code_generator<'db>(
    db: &'db TestDb,
    class: StaticClassLiteral<'db>,
) -> Option<CodeGeneratorKind<'db>> {
    let Ok(generator) = SynchronousStaticCodeGeneratorEffects::code_generator_query(
        &InlineMemberSourceEffects::new(db),
        class,
    );
    generator
}

#[derive(Clone, Copy, Default)]
struct CodeGeneratorObservation {
    key: Option<salsa::Id>,
    entries: usize,
    first_remaining: Option<usize>,
}

thread_local! {
    static CODE_GENERATOR_RECORDING: Cell<bool> = const { Cell::new(false) };
    static CODE_GENERATOR_OBSERVATION: Cell<CodeGeneratorObservation> = Cell::new(CodeGeneratorObservation::default());
}

struct CodeGeneratorRecording;

impl CodeGeneratorRecording {
    fn start() -> Self {
        assert!(!CODE_GENERATOR_RECORDING.replace(true));
        CODE_GENERATOR_OBSERVATION.set(CodeGeneratorObservation::default());
        observations::reset(None);
        Self
    }
}

impl Drop for CodeGeneratorRecording {
    fn drop(&mut self) {
        CODE_GENERATOR_RECORDING.set(false);
    }
}

pub(in crate::types::infer::source_runtime) fn observe_code_generator_body(
    db: &dyn Db,
    class: StaticClassLiteral<'_>,
) {
    if CODE_GENERATOR_RECORDING.get() {
        CODE_GENERATOR_OBSERVATION.with(|observed| {
            let mut value = observed.get();
            if value.key == Some(class.as_id()) {
                value.entries += 1;
                if value.first_remaining.is_none() {
                    value.first_remaining =
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(db);
                }
                observed.set(value);
            }
        });
    }
}

#[test]
fn cold_code_generator_queries_match_ordinary_inference_and_reuse_the_original_memos() {
    for known in [None, Some(KnownClass::Object), Some(KnownClass::ModuleType)] {
        let db = class_fixture();
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let request = CodeGeneratorRequest::for_fixture(&prepared, known);
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let recording = CodeGeneratorRecording::start();
        let cold = capture(&db, || {
            controlled_member_operation(&prepared, request, &funded())
        })
        .unwrap();
        drop(recording);
        cold.check_root_reads().unwrap();
        let Ok(AnalysisOutcome::Complete((class, generator))) = cold.value else {
            panic!("{known:?}: {:?}", cold.value);
        };
        assert_eq!(generator, None, "{known:?}");
        assert_eq!(CODE_GENERATOR_OBSERVATION.get().entries, 1);
        let ingredient = code_generator_of_static_class_ingredient(&db);
        let key = ingredient.database_key_index(class.as_id());
        assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, class.as_id()).is_ok());
        let cold_read = cold.reads.iter().find(|read| read.key == key).unwrap();
        let events = events_db.take_salsa_events();
        assert!(
            find_will_execute_event_by_name(
                &db,
                "code_generator_of_static_class",
                Some(class.as_id()),
                &events,
            )
            .is_some()
        );

        let ordinary_db = class_fixture();
        let ordinary_prepared = prepare(&ordinary_db);
        let ordinary_class = CodeGeneratorRequest::for_fixture(&ordinary_prepared, known)
            .ordinary_class(
                &ordinary_db,
                ordinary_prepared.program_file().program(&ordinary_db),
            );
        assert_eq!(
            ordinary_code_generator(&ordinary_db, ordinary_class),
            generator
        );

        let native = capture(&db, || ordinary_code_generator(&db, class)).unwrap();
        assert_eq!(native.value, generator);
        assert!(native.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        }));
        let recording = CodeGeneratorRecording::start();
        let warm = capture(&db, || {
            controlled_member_operation(&prepared, request, &funded())
        })
        .unwrap();
        drop(recording);
        warm.check_root_reads().unwrap();
        assert_eq!(warm.value, cold.value);
        assert_eq!(CODE_GENERATOR_OBSERVATION.get().entries, 0);
        assert!(warm.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        }));
        assert_function_query_was_not_run_by_name(
            &db,
            "code_generator_of_static_class",
            Some(class.as_id()),
            &events_db.take_salsa_events(),
        );
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn interrupted_code_generator_query_releases_owners_and_retries_in_the_same_revision() {
    let measured = class_fixture();
    let measured_prepared = prepare(&measured);
    let recording = CodeGeneratorRecording::start();
    assert!(matches!(
        controlled_member_operation(
            &measured_prepared,
            CodeGeneratorRequest::for_fixture(&measured_prepared, None),
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete((_, None)))
    ));
    drop(recording);
    let entry_work =
        funded().semantic_work_limit - CODE_GENERATOR_OBSERVATION.get().first_remaining.unwrap();

    let db = class_fixture();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let request = CodeGeneratorRequest::for_fixture(&prepared, None);
    let recording = CodeGeneratorRecording::start();
    assert_eq!(
        controlled_member_operation(
            &prepared,
            request,
            &AnalysisPolicy {
                semantic_work_limit: entry_work,
                ..funded()
            },
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: (),
        })
    );
    drop(recording);
    let observed = CODE_GENERATOR_OBSERVATION.get();
    assert_eq!(observed.entries, 1);
    assert_eq!(observed.first_remaining, Some(0));
    let key = observed.key.unwrap();
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            code_generator_of_static_class_ingredient(&db),
            key,
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    assert_no_active_attempt();

    let recording = CodeGeneratorRecording::start();
    let retry = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    drop(recording);
    retry.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete((class, None))) = retry.value else {
        panic!("code generator retry: {:?}", retry.value);
    };
    assert_eq!(class.as_id(), key);
    assert_eq!(CODE_GENERATOR_OBSERVATION.get().entries, 1);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            code_generator_of_static_class_ingredient(&db),
            key,
        )
        .is_ok()
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn code_generator_query_supports_explicit_object_metaclass() {
    let mut db = setup_db();
    db.write_file("src/main.py", "class Product(object):\n    pass\n")
        .unwrap();
    let prepared = prepare(&db);
    let recording = CodeGeneratorRecording::start();
    // An explicit object base selects type as its metaclass and supplies no code generator.
    assert!(matches!(
        controlled_member_operation(
            &prepared,
            CodeGeneratorRequest::for_fixture(&prepared, None),
            &funded(),
        ),
        Ok(AnalysisOutcome::Complete((_, None))),
    ));
    drop(recording);
    let observed = CODE_GENERATOR_OBSERVATION.get();
    assert_eq!(observed.entries, 1);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            code_generator_of_static_class_ingredient(&db),
            observed.key.unwrap(),
        )
        .is_ok(),
    );
    assert_no_active_attempt();
}

fn implicit_names_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file(
        "src/main.py",
        r#"
class Records[T]:
    class_attribute = 0

    def first(self, other):
        self.retained_heap_attribute = 1
        self.repeated = 2
        self.repeated = 3
        other.unrelated = 4
        [None for self.eager_list in ()]
        {None for self.eager_set in ()}
        {0: None for self.eager_dict in ()}
        [[None for self.eager_nested in ()] for _ in ()]
        (None for self.lazy_generator in ())

        def nested(self):
            self.lazy_function = 5
            [None for self.lazy_nested_eager in ()]

        lazy = lambda self: [None for self.lazy_lambda in ()]

    def generic[U](self, value: U):
        self.alpha = value
        self.repeated = value
        self.retained_heap_attribute = value
"#,
    )
    .unwrap();
    db
}

fn implicit_names_scope<'db>(prepared: &PreparedAnalysisFile<'db>) -> ScopeId<'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .find_map(Stmt::as_class_def_stmt)
        .unwrap();
    let index = prepared.semantic_index();
    index.scope_id(index.node_scope(NodeWithScopeRef::Class(class)))
}

const EXPECTED_IMPLICIT_NAMES: &[&str] = &[
    "alpha",
    "eager_dict",
    "eager_list",
    "eager_nested",
    "eager_set",
    "lazy_generator",
    "repeated",
    "retained_heap_attribute",
];

fn assert_implicit_names(names: &[Name]) {
    assert_eq!(
        names.iter().map(Name::as_str).collect::<Vec<_>>(),
        EXPECTED_IMPLICIT_NAMES,
    );
}

#[derive(Clone, Copy)]
struct ImplicitNamesRequest<'db>(ScopeId<'db>);

impl<'db> MemberOperation<'db> for ImplicitNamesRequest<'db> {
    type Output = &'db [Name];

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        access.implicit_attribute_names(self.0).await
    }
}

fn ordinary_implicit_names<'db>(db: &'db TestDb, scope: ScopeId<'db>) -> &'db [Name] {
    let Ok(names) =
        SynchronousImplicitAttributeEffects::names(&InlineMemberSourceEffects::new(db), scope);
    names
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImplicitNamesPhase {
    Scan,
    Comparison,
}

#[derive(Clone, Copy, Debug)]
struct ImplicitNamesPhaseObservation {
    remaining: usize,
    retained: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct ImplicitNamesObservation {
    key: Option<salsa::Id>,
    live_buffers: usize,
    created: usize,
    dropped: usize,
    retained: usize,
    scan: Option<ImplicitNamesPhaseObservation>,
    comparison: Option<ImplicitNamesPhaseObservation>,
    cancellation_requested: bool,
}

impl ImplicitNamesObservation {
    fn phase(self, phase: ImplicitNamesPhase) -> Option<ImplicitNamesPhaseObservation> {
        match phase {
            ImplicitNamesPhase::Scan => self.scan,
            ImplicitNamesPhase::Comparison => self.comparison,
        }
    }
}

thread_local! {
    static IMPLICIT_NAMES_RECORDING: Cell<bool> = const { Cell::new(false) };
    static IMPLICIT_NAMES_CANCEL: Cell<Option<ImplicitNamesPhase>> = const { Cell::new(None) };
    static IMPLICIT_NAMES_OBSERVATION: Cell<ImplicitNamesObservation> = Cell::new(ImplicitNamesObservation::default());
}

fn update_implicit_names_observation(
    update: impl FnOnce(ImplicitNamesObservation) -> ImplicitNamesObservation,
) {
    IMPLICIT_NAMES_OBSERVATION.with(|observed| observed.set(update(observed.get())));
}

struct ImplicitNamesRecording;

impl ImplicitNamesRecording {
    fn start(scope: ScopeId<'_>, cancel: Option<ImplicitNamesPhase>) -> Self {
        assert_eq!(IMPLICIT_NAMES_OBSERVATION.get().live_buffers, 0);
        assert!(!IMPLICIT_NAMES_RECORDING.replace(true));
        IMPLICIT_NAMES_CANCEL.set(cancel);
        IMPLICIT_NAMES_OBSERVATION.set(ImplicitNamesObservation {
            key: Some(scope.as_id()),
            ..ImplicitNamesObservation::default()
        });
        observations::reset(None);
        Self
    }
}

impl Drop for ImplicitNamesRecording {
    fn drop(&mut self) {
        IMPLICIT_NAMES_RECORDING.set(false);
        IMPLICIT_NAMES_CANCEL.set(None);
    }
}

pub(in crate::types::infer) struct ImplicitNamesLifetime(bool);

impl ImplicitNamesLifetime {
    pub(in crate::types::infer) fn new(scope: ScopeId<'_>) -> Self {
        let recording = IMPLICIT_NAMES_RECORDING.get()
            && IMPLICIT_NAMES_OBSERVATION.get().key == Some(scope.as_id());
        if recording {
            update_implicit_names_observation(|mut observed| {
                observed.live_buffers += 1;
                observed.created += 1;
                observed
            });
        }
        Self(recording)
    }
}

impl Drop for ImplicitNamesLifetime {
    fn drop(&mut self) {
        if self.0 {
            update_implicit_names_observation(|mut observed| {
                observed.live_buffers -= 1;
                observed.dropped += 1;
                observed
            });
        }
    }
}

fn observe_implicit_names_phase(db: &dyn Db, scope: ScopeId<'_>, phase: ImplicitNamesPhase) {
    if !IMPLICIT_NAMES_RECORDING.get()
        || IMPLICIT_NAMES_OBSERVATION.get().key != Some(scope.as_id())
    {
        return;
    }
    let remaining = salsa::attempt_probe::remaining_allowance_for_diagnostics(db).unwrap();
    update_implicit_names_observation(|mut observed| {
        let value = ImplicitNamesPhaseObservation {
            remaining,
            retained: observed.retained,
        };
        match phase {
            ImplicitNamesPhase::Scan => {
                observed.scan.get_or_insert(value);
            }
            ImplicitNamesPhase::Comparison => {
                observed.comparison.get_or_insert(value);
            }
        }
        observed
    });
    if IMPLICIT_NAMES_CANCEL.get() == Some(phase) {
        IMPLICIT_NAMES_CANCEL.set(None);
        update_implicit_names_observation(|mut observed| {
            observed.cancellation_requested = true;
            observed
        });
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn implicit_name_inserted(
    db: &dyn Db,
    scope: ScopeId<'_>,
    retained: usize,
) {
    if IMPLICIT_NAMES_RECORDING.get() && IMPLICIT_NAMES_OBSERVATION.get().key == Some(scope.as_id())
    {
        update_implicit_names_observation(|mut observed| {
            observed.retained = retained;
            observed
        });
        observe_implicit_names_phase(db, scope, ImplicitNamesPhase::Scan);
    }
}

pub(in crate::types::infer) fn implicit_names_sort_comparison(db: &dyn Db, scope: ScopeId<'_>) {
    observe_implicit_names_phase(db, scope, ImplicitNamesPhase::Comparison);
}

fn assert_implicit_names_buffers_released() {
    let observed = IMPLICIT_NAMES_OBSERVATION.get();
    assert_eq!(observed.live_buffers, 0, "{observed:?}");
    assert_eq!(observed.created, observed.dropped, "{observed:?}");
}

/// Ordinary and generic methods contribute sorted, unique attribute names, as do eager
/// descendants of a method. This includes generator-expression scopes, which the semantic index
/// classifies as eager. Class attributes, other receivers and nested functions or lambdas do not.
/// A cold controlled query agrees with ordinary inference and publishes a canonical memo
/// reused by ordinary and warm controlled queries.
#[test]
fn cold_implicit_names_preserve_scope_selection_and_reuse_the_canonical_memo() {
    let db = implicit_names_fixture();
    let prepared = prepare(&db);
    let scope = implicit_names_scope(&prepared);
    let revision = salsa::plumbing::current_revision(&db);
    let ingredient = implicit_attribute_names_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, scope.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    let recording = ImplicitNamesRecording::start(scope, None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, ImplicitNamesRequest(scope), &funded())
    })
    .unwrap();
    drop(recording);
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(names)) = cold.value else {
        panic!("implicit names: {:?}", cold.value);
    };
    assert_implicit_names(names);
    assert_implicit_names_buffers_released();
    let observed = IMPLICIT_NAMES_OBSERVATION.get();
    assert_eq!(observed.created, 1);
    assert!(observed.comparison.unwrap().retained > names.len());
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, scope.as_id()).is_ok());
    let key = ingredient.database_key_index(scope.as_id());
    let cold_read = cold.reads.iter().find(|read| read.key == key).unwrap();
    assert!(
        find_will_execute_event_by_name(
            &db,
            "implicit_attribute_names",
            Some(scope.as_id()),
            &events_db.take_salsa_events(),
        )
        .is_some()
    );

    let ordinary_db = implicit_names_fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    assert_eq!(
        ordinary_implicit_names(&ordinary_db, implicit_names_scope(&ordinary_prepared)),
        names,
    );
    let native = capture(&db, || ordinary_implicit_names(&db, scope)).unwrap();
    native.check_root_reads().unwrap();
    assert!(std::ptr::eq(native.value, names));
    assert!(
        native.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    let recording = ImplicitNamesRecording::start(scope, None);
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, ImplicitNamesRequest(scope), &funded())
    })
    .unwrap();
    drop(recording);
    warm.check_root_reads().unwrap();
    assert_eq!(warm.value, cold.value);
    assert_eq!(IMPLICIT_NAMES_OBSERVATION.get().created, 0);
    assert_implicit_names_buffers_released();
    assert!(
        warm.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    assert_function_query_was_not_run_by_name(
        &db,
        "implicit_attribute_names",
        Some(scope.as_id()),
        &events_db.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// Work refusal and native cancellation during traversal or a sort comparison release the
/// retained name buffer. Neither publishes a partial memo; a same-revision retry returns all names.
#[test]
fn interrupted_implicit_names_release_the_buffer_and_retry_in_the_same_revision() {
    let measured = implicit_names_fixture();
    let prepared = prepare(&measured);
    let scope = implicit_names_scope(&prepared);
    let recording = ImplicitNamesRecording::start(scope, None);
    let Ok(AnalysisOutcome::Complete(names)) =
        controlled_member_operation(&prepared, ImplicitNamesRequest(scope), &funded())
    else {
        panic!("implicit names calibration failed");
    };
    assert_implicit_names(names);
    drop(recording);
    let measured_observation = IMPLICIT_NAMES_OBSERVATION.get();
    assert_implicit_names_buffers_released();

    for phase in [ImplicitNamesPhase::Scan, ImplicitNamesPhase::Comparison] {
        let measured_phase = measured_observation.phase(phase).unwrap();
        assert!(measured_phase.retained > 0);
        let phase_work = funded().semantic_work_limit - measured_phase.remaining;
        for cancel in [false, true] {
            let db = implicit_names_fixture();
            let prepared = prepare(&db);
            let scope = implicit_names_scope(&prepared);
            let revision = salsa::plumbing::current_revision(&db);
            let recording = ImplicitNamesRecording::start(scope, cancel.then_some(phase));
            let policy = if cancel {
                funded()
            } else {
                AnalysisPolicy {
                    semantic_work_limit: phase_work,
                    ..funded()
                }
            };
            let interrupted = salsa::Cancelled::catch(AssertUnwindSafe(|| {
                controlled_member_operation(&prepared, ImplicitNamesRequest(scope), &policy)
            }));
            drop(recording);
            let observed = IMPLICIT_NAMES_OBSERVATION.get();
            let interrupted_phase = observed.phase(phase).unwrap();
            assert!(interrupted_phase.retained > 0, "{phase:?}: {observed:?}");
            assert_eq!(observed.created, 1, "{phase:?}: {observed:?}");
            assert_eq!(observed.cancellation_requested, cancel);
            assert_implicit_names_buffers_released();
            if cancel {
                assert!(
                    matches!(interrupted, Err(salsa::Cancelled::Local)),
                    "{phase:?}: {interrupted:?}"
                );
            } else {
                assert_eq!(interrupted_phase.remaining, 0, "{phase:?}: {observed:?}");
                assert_eq!(
                    interrupted.unwrap(),
                    Ok(AnalysisOutcome::Incomplete {
                        reason: AnalysisIncomplete::WorkLimit,
                        completed: (),
                    }),
                );
            }
            assert_eq!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    implicit_attribute_names_ingredient(&db),
                    scope.as_id(),
                )
                .map(|_| ()),
                Err(FinalSourceError::MissingMemo),
            );
            assert_no_active_attempt();

            let recording = ImplicitNamesRecording::start(scope, None);
            let retry = capture(&db, || {
                controlled_member_operation(&prepared, ImplicitNamesRequest(scope), &funded())
            })
            .unwrap();
            drop(recording);
            retry.check_root_reads().unwrap();
            let Ok(AnalysisOutcome::Complete(names)) = retry.value else {
                panic!("{phase:?} implicit names retry: {:?}", retry.value);
            };
            assert_implicit_names(names);
            assert_eq!(IMPLICIT_NAMES_OBSERVATION.get().created, 1);
            assert_implicit_names_buffers_released();
            assert!(
                FinalSourceMemo::certify(
                    &db as &dyn Db,
                    implicit_attribute_names_ingredient(&db),
                    scope.as_id(),
                )
                .is_ok()
            );
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}

#[derive(Clone, Copy)]
struct PlainInstanceMemberRequest<'name, 'db> {
    definition: Definition<'db>,
    name: &'name Name,
}

impl<'db> MemberOperation<'db> for PlainInstanceMemberRequest<'_, 'db> {
    type Output = (Type<'db>, MemberLookupResult<'db>);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let inference = access.definition(self.definition).await?;
        let endpoint = access.endpoint();
        let class_ty = endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                let Some(ClassLiteral::Static(class)) =
                    inference.original_class_type(self.definition)
                else {
                    return Err(RunError::Contract(
                        "fixture definition is not a static class",
                    ));
                };
                Ok(Type::ClassLiteral(ClassLiteral::Static(class)))
            })
            .await;
        let effects = SourceEffects::new(access, program);
        let Some(class) = KnownClassInstanceEffects::to_class_type(&effects, class_ty).await?
        else {
            return Err(RunError::Contract("fixture class has no class type"));
        };
        let instance = KnownClassInstanceEffects::instance(&effects, class).await?;
        let result = access
            .member_lookup(instance, self.name, MemberLookupPolicy::default())
            .await?;
        Ok((instance, result))
    }
}

#[derive(Clone, Copy)]
struct ImportedMissingMemberRequest<'name, 'db> {
    file: ProgramFile<'db>,
    name: &'name str,
}

impl<'db> MemberOperation<'db> for ImportedMissingMemberRequest<'_, 'db> {
    type Output = PlaceAndQualifiers<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let endpoint = access.endpoint();
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(ProgramEnvironment::from_file(self.file))
            })
            .await;
        crate::place::imported_symbol_with(
            access.db(),
            &env,
            &SourceEffects::new(access, program),
            Some(self.file),
            self.name,
            None,
        )
        .await
    }
}

fn assert_member_memo_and_class_dependency<'trace>(
    db: &TestDb,
    reads: &'trace [Read],
    key: MemberLookupKey<'_>,
) -> &'trace Read {
    let member_ingredient = member_lookup_ingredient(db);
    let member_key = member_ingredient.database_key_index(key.as_id());
    let member_read = reads
        .iter()
        .find(|read| read.key == member_key && read.parent.is_none())
        .unwrap();
    assert!(FinalSourceMemo::certify(db as &dyn Db, member_ingredient, key.as_id()).is_ok());
    let class_ingredient = crate::types::class_member_lookup_ingredient(db);
    let class_key = class_ingredient.database_key_index(key.as_id());
    assert!(
        reads
            .iter()
            .any(|read| read.key == class_key && read.parent == Some(member_key)),
        "missing canonical class-member dependency: {reads:?}",
    );
    assert!(FinalSourceMemo::certify(db as &dyn Db, class_ingredient, key.as_id()).is_ok());
    member_read
}

fn plain_instance_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "class Plain:\n    pass\n")
        .unwrap();
    db
}

/// An absent attribute on a plain instance stays undefined under the default lookup policy.
/// The receiver comes from cold class-definition inference and the shared instance constructor; lookup
/// publishes canonical member and class-member memos, matching ordinary inference.
#[test]
fn cold_plain_instance_missing_member_completes_and_reuses_canonical_memos() {
    let db = plain_instance_fixture();
    let prepared = prepare(&db);
    let CodeGeneratorRequest::Definition(definition) =
        CodeGeneratorRequest::for_fixture(&prepared, None)
    else {
        panic!("expected the fixture's class definition");
    };
    let name = Name::new_static("missing");
    let request = PlainInstanceMemberRequest {
        definition,
        name: &name,
    };
    let revision = salsa::plumbing::current_revision(&db);
    let definition_ingredient = definition_inference_ingredient(&db);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, definition_ingredient, definition.as_id())
            .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete((receiver, result))) = cold.value else {
        panic!("plain instance missing attribute: {:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    assert!(matches!(receiver, Type::NominalInstance(_)));
    assert_eq!(result, Place::Undefined.into());
    let program = prepared.program_file().program(&db);
    let key = MemberLookupKey::new(
        &db,
        program,
        receiver,
        name.as_str(),
        MemberLookupPolicy::default(),
    );
    let cold_read = assert_member_memo_and_class_dependency(&db, &cold.reads, key);
    assert!(
        cold.reads.iter().any(|read| {
            read.key == definition_ingredient.database_key_index(definition.as_id())
        })
    );
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, definition_ingredient, definition.as_id()).is_ok()
    );
    let events = events_db.take_salsa_events();
    assert!(
        find_will_execute_event_by_name(
            &db,
            "infer_definition_types",
            Some(definition.as_id()),
            &events
        )
        .is_some()
    );
    assert!(
        find_will_execute_event_by_name(
            &db,
            "member_lookup_with_policy_inner",
            Some(key.as_id()),
            &events
        )
        .is_some()
    );

    let ordinary_db = plain_instance_fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    let ordinary_class = CodeGeneratorRequest::for_fixture(&ordinary_prepared, None)
        .ordinary_class(
            &ordinary_db,
            ordinary_prepared.program_file().program(&ordinary_db),
        );
    let ordinary_class = Type::ClassLiteral(ClassLiteral::Static(ordinary_class))
        .to_class_type(&ordinary_db)
        .unwrap();
    let ordinary_receiver = Type::instance(&ordinary_db, &ordinary_env, ordinary_class);
    assert_eq!(
        ordinary_receiver.member_lookup_with_policy(
            &ordinary_db,
            &ordinary_env,
            name.as_str(),
            MemberLookupPolicy::default(),
        ),
        result.unwrap().member(&db),
    );
    let native = capture(&db, || {
        crate::types::member_lookup_with_policy_inner(&db, key)
    })
    .unwrap();
    native.check_root_reads().unwrap();
    assert_eq!(native.value, result);
    assert!(
        native.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    observations::reset(None);
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    warm.check_root_reads().unwrap();
    assert_eq!(warm.value, cold.value);
    assert!(
        warm.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    let events = events_db.take_salsa_events();
    for query in [
        "infer_definition_types",
        "member_lookup_with_policy_inner",
        "class_member_with_policy_inner",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

fn missing_module_member_fixture() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", "pass\n").unwrap();
    db
}

/// A name absent from a module stays undefined after looking on `ModuleType` and its bases.
/// The fallback suppresses typeshed's synthetic `__getattr__`, preserving an absent import
/// instead of producing a dynamic attribute. Its canonical member memos remain reusable.
#[test]
fn cold_module_missing_member_completes_through_no_getattr_fallback() {
    let db = missing_module_member_fixture();
    let prepared = prepare(&db);
    let file = prepared.program_file();
    let request = ImportedMissingMemberRequest {
        file,
        name: "missing",
    };
    let revision = salsa::plumbing::current_revision(&db);
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let Ok(AnalysisOutcome::Complete(actual)) = cold.value else {
        panic!("module missing-name fallback: {:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    assert_eq!(actual, Place::Undefined.into());
    let env = ProgramEnvironment::from_file(file);
    let module_type = KnownClass::ModuleType.to_instance(&db, &env);
    let key = MemberLookupKey::new(
        &db,
        file.program(&db),
        module_type,
        "missing",
        MemberLookupPolicy::NO_GETATTR_LOOKUP,
    );
    let cold_read = assert_member_memo_and_class_dependency(&db, &cold.reads, key);
    let module_argument = KnownClassArgument::new(&db, KnownClass::ModuleType, file.program(&db));
    let instance_ingredient = known_class_to_instance_ingredient(&db);
    let instance_key = instance_ingredient.database_key_index(module_argument.as_id());
    assert!(cold.reads.iter().any(|read| read.key == instance_key));
    assert!(
        FinalSourceMemo::certify(&db as &dyn Db, instance_ingredient, module_argument.as_id())
            .is_ok()
    );
    assert!(
        find_will_execute_event_by_name(
            &db,
            "member_lookup_with_policy_inner",
            Some(key.as_id()),
            &events_db.take_salsa_events(),
        )
        .is_some()
    );

    let ordinary_db = missing_module_member_fixture();
    let ordinary_prepared = prepare(&ordinary_db);
    let ordinary_env = ProgramEnvironment::from_file(ordinary_prepared.program_file());
    assert_eq!(
        crate::place::imported_symbol(
            &ordinary_db,
            &ordinary_env,
            Some(ordinary_prepared.program_file()),
            "missing",
            None,
        ),
        actual,
    );
    let native = capture(&db, || {
        crate::types::member_lookup_with_policy_inner(&db, key)
    })
    .unwrap();
    native.check_root_reads().unwrap();
    assert_eq!(native.value, Place::Undefined.into());
    assert!(
        native.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    observations::reset(None);
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    warm.check_root_reads().unwrap();
    assert_eq!(warm.value, cold.value);
    assert!(
        warm.reads.iter().any(|read| {
            read.key == cold_read.key && read.memo_address == cold_read.memo_address
        })
    );
    let events = events_db.take_salsa_events();
    for query in [
        "member_lookup_with_policy_inner",
        "class_member_with_policy_inner",
    ] {
        assert_function_query_was_not_run_by_name(&db, query, None, &events);
    }
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}
