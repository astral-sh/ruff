//! Constructor Self mapping through cold source queries and the retained mapping driver.

use std::future::{Future, poll_fn};
use std::panic::AssertUnwindSafe;

use ruff_python_ast::name::Name;
use salsa::plumbing::AsId;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::call::preparation::known_class::KnownClassBindingEffects;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::instance::{NominalInstanceClass, NominalVisitorKind};
use crate::types::mapping::source::MappingSourceEffects;
use crate::types::mapping::source::observations::{
    self as mapping_observations, BindingContextSnapshot, OwnedMappingSnapshot,
};
use crate::types::signatures::constructor_preparation::{
    ConstructorSignatureEffects, InlineConstructorSignatureEffects,
};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::tuple::TupleSpec;
use crate::types::typevar::{
    BindingContext, TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity,
};
use crate::types::{
    BoundTypeVarInstance, GenericContext, MappingOperation, MaterializationOperation,
    TypeVarBoundOrConstraints, TypeVarKind, TypeVarVariance, legacy_inline,
};

const SOURCE: &str = "class Base: pass\nclass Product(Base): pass\nclass Other: pass\ndef anchor(value, *, pair): pass\n";

/// Holds syntax-derived definitions without requesting their inferred types.
#[derive(Clone, Copy, Debug)]
struct Definitions<'db> {
    base: Definition<'db>,
    product: Definition<'db>,
    other: Definition<'db>,
    parameter: Definition<'db>,
}

impl<'db> Definitions<'db> {
    fn new(prepared: &PreparedAnalysisFile<'db>) -> Self {
        let class = |name| {
            let class = prepared
                .parsed_module()
                .syntax()
                .body
                .iter()
                .filter_map(Stmt::as_class_def_stmt)
                .find(|class| class.name.as_str() == name)
                .expect("fixture class");
            prepared.semantic_index().expect_single_definition(class)
        };
        let function = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .find_map(Stmt::as_function_def_stmt)
            .expect("fixture parameter owner");
        Self {
            base: class("Base"),
            product: class("Product"),
            other: class("Other"),
            parameter: prepared
                .semantic_index()
                .expect_single_definition(&function.parameters.args[0].parameter),
        }
    }
}

/// Gets an instance through controlled definition inference and instance construction.
async fn instance<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    program: Program<'db>,
    env: &ProgramEnvironment<'db>,
    definition: Definition<'db>,
) -> RunResult<Type<'db>> {
    let inference = access.definition(definition).await?;
    let effects = SourceEffects::new(access, program);
    let class = effects
        .local_with_fixed_transfers(3, 0, || {
            inference
                .original_class_type(definition)
                .ok_or(RunError::Contract("Self mapping fixture is not a class"))
        })
        .await??;
    Type::instance_with(access.db(), env, &effects, ClassType::NonGeneric(class)).await
}

/// Interns an invariant `Self` with the given upper bound and binds it to the supplied definition.
/// The fixture uses the controlled type-variable interners so setup obeys the active read boundary.
async fn self_variable<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    access: &A,
    effects: &SourceEffects<'_, 'run, 'db, A>,
    upper_bound: Type<'db>,
    definition: Definition<'db>,
) -> RunResult<BoundTypeVarInstance<'db>> {
    let name = effects
        .local_with_fixed_transfers(1, 0, || Name::new_static("Self"))
        .await?;
    effects
        .local_with_fixed_transfers(
            3,
            size_of::<(Name, Option<Definition<'db>>, TypeVarKind)>(),
            || (),
        )
        .await?;
    let identity = access
        .intern_typevar_identity(&name, None, TypeVarKind::TypingSelf)
        .await?;
    let bounds = effects
        .local_with_fixed_transfers(2, 0, || {
            TypeVarBoundOrConstraintsEvaluation::from(TypeVarBoundOrConstraints::UpperBound(
                upper_bound,
            ))
        })
        .await?;
    effects
        .local_with_fixed_transfers(
            4,
            size_of::<(
                TypeVarIdentity<'db>,
                Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
                Option<TypeVarVariance>,
                Option<TypeVarDefaultEvaluation<'db>>,
            )>(),
            || (),
        )
        .await?;
    let variable = access
        .intern_typevar_instance(
            identity,
            Some(bounds),
            Some(TypeVarVariance::Invariant),
            None,
        )
        .await?;
    TypeVarBindingEffects::bind(effects, variable, definition).await
}

/// Creates a callable with one positional-only parameter.
async fn single_parameter_callable<'run, 'db: 'run, A: SourceAccess<'run, 'db>>(
    effects: &SourceEffects<'_, 'run, 'db, A>,
    context: Option<GenericContext<'db>>,
    parameter: Type<'db>,
    return_type: Type<'db>,
) -> RunResult<Type<'db>> {
    let parameter = effects
        .local_with_fixed_transfers(3, 0, || {
            [Parameter::positional_only(None).with_annotated_type(parameter)]
        })
        .await?;
    let parameters = KnownClassBindingEffects::standard_parameters(effects, parameter).await?;
    let signature = effects
        .local_with_fixed_transfers(3, 0, || {
            Signature::new_generic(context, parameters, return_type)
        })
        .await?;
    KnownClassBindingEffects::single_callable(effects, signature).await
}

/// Distinguishes the early exits and recursive callable traversal of Self mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    SelfFree,
    SameContext,
    Inherited,
    Unmatched,
    NestedCallable,
    PreservedGenericContext,
    RemovedGenericContext,
}

/// Retains inputs for comparison with `InlineConstructorSignatureEffects` after cold execution.
#[derive(Debug)]
struct Mapped<'db> {
    input: Type<'db>,
    replacement: Type<'db>,
    context: Option<BindingContext<'db>>,
    result: Type<'db>,
}

/// Maps one input through the controlled constructor callback and retains it for parity checks.
#[derive(Clone, Copy, Debug)]
struct MappingRequest<'db> {
    definitions: Definitions<'db>,
    case: Case,
}

impl<'db> MemberOperation<'db> for MappingRequest<'db> {
    type Output = Mapped<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let db = access.db();
        let env = ProgramEnvironment::from_program(program);
        let effects = SourceEffects::new(access, program);
        let owner_definition = if self.case == Case::Unmatched {
            self.definitions.other
        } else {
            self.definitions.base
        };
        let owner = instance(access, program, &env, owner_definition).await?;
        let replacement = instance(access, program, &env, self.definitions.product).await?;
        let binding_context = BindingContext::Definition(owner_definition);
        let variable = self_variable(access, &effects, owner, owner_definition).await?;
        let context = match self.case {
            Case::SameContext | Case::RemovedGenericContext => Some(binding_context),
            Case::SelfFree
            | Case::Inherited
            | Case::Unmatched
            | Case::NestedCallable
            | Case::PreservedGenericContext => None,
        };
        let input = match self.case {
            Case::SelfFree => owner,
            Case::SameContext | Case::Inherited | Case::Unmatched => Type::TypeVar(variable),
            Case::NestedCallable => {
                let inner = single_parameter_callable(
                    &effects,
                    None,
                    Type::TypeVar(variable),
                    Type::TypeVar(variable),
                )
                .await?;
                single_parameter_callable(&effects, None, inner, Type::TypeVar(variable)).await?
            }
            Case::PreservedGenericContext | Case::RemovedGenericContext => {
                let generic_context =
                    KnownClassBindingEffects::generic_context(&effects, [variable]).await?;
                single_parameter_callable(
                    &effects,
                    Some(generic_context),
                    Type::TypeVar(variable),
                    Type::TypeVar(variable),
                )
                .await?
            }
        };
        mapping_observations::reset(None);
        let result = ConstructorSignatureEffects::bind_self_type(
            &effects,
            db,
            &env,
            input,
            replacement,
            context,
        )
        .await?;
        Ok(Mapped {
            input,
            replacement,
            context,
            result,
        })
    }
}

/// Tracks actual callback suspension and completion without changing its poll result.
#[derive(Debug, Default)]
struct Progress {
    pending: Cell<usize>,
    completed: Cell<bool>,
}

/// Retains both signature inputs so parity includes parameter metadata and eager defaults.
#[derive(Debug)]
struct MappedSignature<'db> {
    original: Parameters<'db>,
    parameters: Parameters<'db>,
    original_return: Type<'db>,
    return_type: Type<'db>,
    replacement: Type<'db>,
    context: BindingContext<'db>,
}

/// Maps a signature through the controlled constructor callback.
///
/// Fixture setup infers classes and constructs the class instances and tuple input before mapping
/// observations are reset. The completion and suspension observations cover the callback itself.
///
/// The positional-only `value` has a `Self` annotation and an eager default typed as `Self`.
/// The keyword-only `pair` and the return type are both `tuple[Self, Self]`. The five parameter
/// child entries visit `value`'s annotation, its default, `pair`'s annotation, and then its two
/// tuple elements, in that order. The return root visits its two tuple elements separately.
#[derive(Clone, Copy, Debug)]
struct SignatureRequest<'a, 'db> {
    definitions: Definitions<'db>,
    progress: &'a Progress,
    cancel_at: Option<usize>,
}

impl<'db> MemberOperation<'db> for SignatureRequest<'_, 'db> {
    type Output = MappedSignature<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let db = access.db();
        let env = ProgramEnvironment::from_program(program);
        let effects = SourceEffects::new(access, program);
        let owner = instance(access, program, &env, self.definitions.base).await?;
        let replacement = instance(access, program, &env, self.definitions.product).await?;
        let context = BindingContext::Definition(self.definitions.base);
        let variable = self_variable(access, &effects, owner, self.definitions.base).await?;
        let self_type = Type::TypeVar(variable);
        let tuple = Type::tuple(
            MappingSourceEffects::construct_tuple(
                &effects,
                &env,
                &TupleSpec::heterogeneous([self_type, self_type]),
            )
            .await?,
        );
        let original = KnownClassBindingEffects::standard_parameters(
            &effects,
            [
                Parameter::positional_only(Some(Name::new_static("value")))
                    .with_annotated_type(self_type)
                    .with_default_type(self_type)
                    .with_definition(Some(self.definitions.parameter)),
                Parameter::keyword_only(Name::new_static("pair")).with_annotated_type(tuple),
            ],
        )
        .await?;
        let mut parameters = original.clone();
        let mut return_type = tuple;
        mapping_observations::reset(self.cancel_at);
        {
            let mapping = ConstructorSignatureEffects::bind_self_signature_types(
                &effects,
                db,
                &env,
                &mut parameters,
                &mut return_type,
                replacement,
                Some(context),
            );
            let mut mapping = std::pin::pin!(mapping);
            poll_fn(|cx| {
                let result = mapping.as_mut().poll(cx);
                if result.is_pending() {
                    self.progress.pending.set(self.progress.pending.get() + 1);
                }
                result
            })
            .await?;
        }
        self.progress.completed.set(true);
        Ok(MappedSignature {
            original,
            parameters,
            original_return: tuple,
            return_type,
            replacement,
            context,
        })
    }
}

/// Creates a fresh database containing the class and parameter definitions for these controls.
fn database() -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", SOURCE).unwrap();
    db
}

/// Checks driver drainage and retirement of observed tuple and set buffers.
fn assert_drained() {
    assert_eq!(observations::counts().0, 0);
    assert_eq!(mapping_observations::tuple_snapshot().live, 0);
    assert_eq!(mapping_observations::set_snapshot().live, 0);
    assert_no_active_attempt();
}

/// Checks the replacement's plain class, its prepared class literal, and the binding context.
fn assert_binding_snapshot(
    snapshot: OwnedMappingSnapshot,
    replacement: Type<'_>,
    context: Option<BindingContext<'_>>,
) {
    let Type::NominalInstance(instance) = replacement else {
        panic!("fixture replacement is not a nominal instance");
    };
    let NominalVisitorKind::Class(NominalInstanceClass::Plain(class)) = instance.visitor_kind()
    else {
        panic!("fixture replacement is not a plain class instance");
    };
    let ClassType::NonGeneric(literal) = class else {
        panic!("fixture replacement has a generic class");
    };
    assert_eq!(
        snapshot,
        OwnedMappingSnapshot::BindSelf {
            replacement_plain_class: Some(class.as_id()),
            replacement_class: Some(literal.as_id()),
            binding_context: context.map(BindingContextSnapshot::from),
        }
    );
}

/// Self-free input is preserved, same-context matching avoids MRO lookup, and hierarchy matching
/// binds inherited owners while preserving unrelated Self variables. A nested callable visits
/// its parameter and return types through the same retained mapping visitor. With no binding
/// context, a callable's generic context is preserved. Each result matches
/// `InlineConstructorSignatureEffects` when run after the cold execution.
#[test_case::test_case(Case::SelfFree; "Self-free explicit annotation")]
#[test_case::test_case(Case::SameContext; "same binding context")]
#[test_case::test_case(Case::Inherited; "inherited owner")]
#[test_case::test_case(Case::Unmatched; "unrelated owner")]
#[test_case::test_case(Case::NestedCallable; "nested callable")]
#[test_case::test_case(Case::PreservedGenericContext; "preserved generic context")]
fn cold_mapping_matches_ordinary(case: Case) {
    let db = database();
    let prepared = prepare(&db);
    let definitions = Definitions::new(&prepared);
    let mut event_db = db.clone();
    event_db.take_salsa_events();
    observations::reset(None);
    mapping_observations::reset(None);
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, MappingRequest { definitions, case }, &funded())
    })
    .unwrap();
    assert_eq!(captured.check_root_reads(), Ok(()));
    let Ok(AnalysisOutcome::Complete(mapped)) = captured.value else {
        panic!("cold Self mapping {case:?}: {:?}", captured.value);
    };
    let events = event_db.take_salsa_events();
    match case {
        Case::SelfFree | Case::SameContext | Case::RemovedGenericContext => {
            assert_function_query_was_not_run_by_name(&db, "class_mro_literals", None, &events);
        }
        Case::Inherited
        | Case::Unmatched
        | Case::NestedCallable
        | Case::PreservedGenericContext => {
            assert!(
                find_will_execute_event_by_name(&db, "class_mro_literals", None, &events).is_some()
            );
        }
    }
    match case {
        Case::SelfFree | Case::Unmatched => assert_eq!(mapped.result, mapped.input),
        Case::SameContext | Case::Inherited => assert_eq!(mapped.result, mapped.replacement),
        Case::NestedCallable | Case::PreservedGenericContext | Case::RemovedGenericContext => {
            assert_ne!(mapped.result, mapped.input)
        }
    }
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 1);
    let root = snapshot.roots[0].unwrap();
    assert_binding_snapshot(root.mapping, mapped.replacement, mapped.context);
    if case == Case::NestedCallable {
        assert!(snapshot.child_count >= 4);
        assert!(
            snapshot.children[..snapshot.child_count]
                .iter()
                .flatten()
                .all(|child| child.visitor == root.visitor && child.mapping == root.mapping)
        );
    }
    assert_drained();
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let ordinary = legacy_inline(ConstructorSignatureEffects::bind_self_type(
        &InlineConstructorSignatureEffects,
        &db,
        &env,
        mapped.input,
        mapped.replacement,
        mapped.context,
    ));
    assert_eq!(mapped.result, ordinary);
}

/// Compares the controlled result's complete parameters and return type with
/// `InlineConstructorSignatureEffects::bind_self_signature_types`.
fn assert_signature_parity<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    mapped: MappedSignature<'db>,
) {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    let mut ordinary_parameters = mapped.original.clone();
    let mut ordinary_return = mapped.original_return;
    legacy_inline(ConstructorSignatureEffects::bind_self_signature_types(
        &InlineConstructorSignatureEffects,
        db,
        &env,
        &mut ordinary_parameters,
        &mut ordinary_return,
        mapped.replacement,
        Some(mapped.context),
    ));
    assert_eq!(mapped.parameters, ordinary_parameters);
    assert_eq!(mapped.return_type, ordinary_return);
    let first = mapped.parameters.iter().next().unwrap();
    assert_eq!(first.annotated_type(), mapped.replacement);
    assert_eq!(first.eager_default_type(), Some(mapped.replacement));
    assert_eq!(
        first.definition(),
        mapped.original.iter().next().unwrap().definition()
    );
}

/// Parameters and their eager defaults share one visitor; the return root uses a fresh visitor.
/// Equality with `InlineConstructorSignatureEffects` checks the return type and complete
/// parameters, including names, order, kinds, definitions and annotation flags.
#[test]
fn signature_parameters_and_return_use_separate_retained_roots() {
    let db = database();
    let prepared = prepare(&db);
    let progress = Progress::default();
    observations::reset(None);
    mapping_observations::reset(None);
    let result = controlled_member_operation(
        &prepared,
        SignatureRequest {
            definitions: Definitions::new(&prepared),
            progress: &progress,
            cancel_at: None,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(mapped)) = result else {
        panic!("signature Self mapping: {result:?}");
    };
    assert!(progress.completed.get());
    let snapshot = mapping_observations::mapping_snapshot();
    assert_eq!(snapshot.root_count, 2);
    let first = snapshot.roots[0].unwrap();
    let second = snapshot.roots[1].unwrap();
    assert_ne!(first.visitor, second.visitor);
    assert_eq!(first.mapping, second.mapping);
    assert_binding_snapshot(first.mapping, mapped.replacement, Some(mapped.context));
    assert_eq!(snapshot.child_count, 7);
    assert!(
        snapshot.children[..5]
            .iter()
            .flatten()
            .all(|child| child.visitor == first.visitor && child.mapping == first.mapping)
    );
    assert!(
        snapshot.children[5..7]
            .iter()
            .flatten()
            .all(|child| child.visitor == second.visitor && child.mapping == second.mapping)
    );
    assert_drained();
    assert_signature_parity(&db, &prepared, mapped);
}

/// Mapping a callable whose generic context contains Self returns `Incomplete` with
/// `GenericContextSelfRemoval` when removing Self from that context is required.
#[test]
fn generic_context_refuses_its_specific_unmigrated_child() {
    let db = database();
    let prepared = prepare(&db);
    observations::reset(None);
    mapping_observations::reset(None);
    let result = controlled_member_operation(
        &prepared,
        MappingRequest {
            definitions: Definitions::new(&prepared),
            case: Case::RemovedGenericContext,
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("generic context Self removal: {result:?}");
    };
    assert_eq!(
        reason,
        AnalysisIncomplete::UnavailableOperation(OperationId::SelfMapping(
            MaterializationOperation::Leaf(MappingOperation::GenericContextSelfRemoval),
        ))
    );
    assert_drained();
}

/// Selects one independent budget while leaving the other fully funded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resource {
    Work,
    Bytes,
}

impl Resource {
    fn policy(self, limit: usize) -> AnalysisPolicy {
        match self {
            Self::Work => AnalysisPolicy {
                semantic_work_limit: limit,
                ..funded()
            },
            Self::Bytes => AnalysisPolicy {
                requested_bytes_limit: limit,
                ..funded()
            },
        }
    }

    fn limit(self) -> usize {
        match self {
            Self::Work => funded().semantic_work_limit,
            Self::Bytes => funded().requested_bytes_limit,
        }
    }

    const fn reason(self) -> AnalysisIncomplete {
        match self {
            Self::Work => AnalysisIncomplete::WorkLimit,
            Self::Bytes => AnalysisIncomplete::RequestedAllocationLimit,
        }
    }
}

/// Reports signature-mapping callback completion under the selected budget on a fresh database.
/// No probe reuses memos from an earlier probe.
fn completes(resource: Resource, limit: usize) -> bool {
    let db = database();
    let prepared = prepare(&db);
    let progress = Progress::default();
    observations::reset(None);
    mapping_observations::reset(None);
    let _result = controlled_member_operation(
        &prepared,
        SignatureRequest {
            definitions: Definitions::new(&prepared),
            progress: &progress,
            cancel_at: None,
        },
        &resource.policy(limit),
    );
    assert_drained();
    progress.completed.get()
}

/// Exhausting either budget prevents completion, drains owners, and permits a funded retry in
/// the same revision. Each search probe is cold, so its cutoff cannot borrow a warmed query.
#[test_case::test_case(Resource::Work; "work")]
#[test_case::test_case(Resource::Bytes; "bytes")]
fn refusal_drains_before_same_revision_retry(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    assert!(completes(resource, high));
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if completes(resource, middle) {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    let request = SignatureRequest {
        definitions: Definitions::new(&prepared),
        progress: &progress,
        cancel_at: None,
    };
    observations::reset(None);
    mapping_observations::reset(None);
    let result = controlled_member_operation(&prepared, request, &resource.policy(low));
    let Ok(AnalysisOutcome::Incomplete {
        reason,
        completed: (),
    }) = result
    else {
        panic!("Self mapping {resource:?} refusal: {result:?}");
    };
    assert_eq!(reason, resource.reason());
    assert!(!progress.completed.get());
    assert_drained();
    observations::reset(None);
    mapping_observations::reset(None);
    let retry = controlled_member_operation(&prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(mapped)) = retry else {
        panic!("funded Self mapping retry: {retry:?}");
    };
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
    assert_signature_parity(&db, &prepared, mapped);
}

/// Cancelling the fifth child (the second tuple element) leaves one mapped element and an earlier output
/// parameter retained. The callback suspends, the populated tuple buffer retires, and a
/// funded retry completes in the same revision. The cancelled callback returns no mapped result.
#[test]
fn cancellation_with_a_populated_tuple_child_drains_before_retry() {
    let db = database();
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let progress = Progress::default();
    let request = SignatureRequest {
        definitions: Definitions::new(&prepared),
        progress: &progress,
        cancel_at: Some(5),
    };
    observations::reset(None);
    mapping_observations::reset(None);
    let cancelled = salsa::Cancelled::catch(AssertUnwindSafe(|| {
        controlled_member_operation(&prepared, request, &funded())
    }));
    assert!(
        matches!(cancelled, Err(salsa::Cancelled::Local)),
        "{cancelled:?}"
    );
    assert!(progress.pending.get() > 0);
    assert!(!progress.completed.get());
    let tuples = mapping_observations::tuple_snapshot();
    assert_eq!(tuples.created, 1);
    assert_eq!(tuples.dropped, 1);
    assert_eq!(tuples.partial_drops, 1);
    assert_eq!(tuples.push_count, 1);
    assert_eq!(tuples.pushes[0].unwrap().len, 1);
    assert_eq!(tuples.last_dropped_len, Some(1));
    assert_eq!(mapping_observations::mapping_snapshot().root_count, 1);
    assert_drained();
    observations::reset(None);
    mapping_observations::reset(None);
    let retry = controlled_member_operation(
        &prepared,
        SignatureRequest {
            cancel_at: None,
            ..request
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(mapped)) = retry else {
        panic!("Self mapping cancellation retry: {retry:?}");
    };
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_drained();
    assert_signature_parity(&db, &prepared, mapped);
}
