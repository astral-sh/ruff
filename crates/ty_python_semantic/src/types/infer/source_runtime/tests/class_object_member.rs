//! Observes canonical class-member lookup and class-object provider admissions on cold source input.
//! Runtime observations cover query publication and child ordering that Python mdtests cannot inspect.

mod entry;
mod metaclass_instance_relation;
mod public_promotion;

use std::cell::RefCell;

use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::IngredientImpl;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::class::{ClassMetaclass, KnownClassArgument};
use crate::types::mapping::specialization::SpecializationConfiguration;
use crate::types::member_lookup::class_dispatch::{
    ClassMemberDispatchFacts, OrdinaryClassMemberDispatch, class_member_dispatch_sync,
};
use crate::types::{
    BindingContext, BoundTypeVarInstance, MemberEntryEffects, Specialization, SubclassOfInner,
    SubclassOfType, apply_specialization_ingredient, class_member_lookup_ingredient,
};

const IMPLICIT_META: &str = "class Product: pass\n";
const EXPLICIT_META: &str =
    "class Meta(type):\n    value = True\nclass Product(metaclass=Meta): pass\n";
const INHERITED_META: &str = "class Meta(type):\n    value = True\nclass Base(metaclass=Meta): pass\nclass Product(Base): pass\n";
const DECLARED_VALUE: &str = "class Product:\n    value: bool\n";

/// Identifies real provider boundaries without changing their admission policy or returned values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum Stage {
    InstanceStorage,
    Transfer,
}

/// Counts requested storage children and successful final transfers in one controlled run.
#[derive(Clone, Copy, Debug, Default)]
struct Journal {
    storage_requested: usize,
    storage_completed: usize,
    transfer_requested: usize,
    transfer_completed: usize,
}

thread_local! {
    static JOURNAL: RefCell<Option<Journal>> = const { RefCell::new(None) };
}

/// Records a child request or arrival before the provider's actual final-transfer admission.
pub(in crate::types::infer) fn observe_before(stage: Stage) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            match stage {
                Stage::InstanceStorage => journal.storage_requested += 1,
                Stage::Transfer => journal.transfer_requested += 1,
            }
        }
    });
}

/// Records a completed storage child or a transfer inside its admitted local operation.
pub(in crate::types::infer) fn observe_after(stage: Stage) {
    JOURNAL.with_borrow_mut(|journal| {
        if let Some(journal) = journal {
            match stage {
                Stage::InstanceStorage => journal.storage_completed += 1,
                Stage::Transfer => journal.transfer_completed += 1,
            }
        }
    });
}

/// Restricts passive provider observations to one request and its cleanup.
#[derive(Debug)]
struct Recording;

impl Recording {
    fn start() -> Self {
        JOURNAL.with_borrow_mut(|journal| {
            assert!(journal.is_none());
            *journal = Some(Journal::default());
        });
        observations::reset(None);
        Self
    }

    fn journal(&self) -> Journal {
        JOURNAL.with_borrow(|journal| journal.unwrap())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        JOURNAL.with_borrow_mut(|journal| *journal = None);
    }
}

/// Selects the canonical metaclass-member query or the class object's own namespace provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Route {
    Canonical,
    Provider,
}

/// Resolves a class from real controlled definition inference before requesting its member.
#[derive(Clone, Copy, Debug)]
struct Request<'name, 'db> {
    definition: Definition<'db>,
    name: &'name Name,
    route: Route,
}

/// Retains the actual input identity alongside every field of the resolved member record.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Resolved<'db> {
    receiver: Type<'db>,
    member: PlaceAndQualifiers<'db>,
}

impl<'db> MemberOperation<'db> for Request<'_, 'db> {
    type Output = Resolved<'db>;

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
        let receiver = endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                inference
                    .original_class_type(self.definition)
                    .map(Type::ClassLiteral)
                    .ok_or(RunError::Contract("fixture definition is not a class"))
            })
            .await;
        let member = match self.route {
            Route::Canonical => {
                access
                    .class_member_lookup(receiver, self.name, MemberLookupPolicy::default())
                    .await?
            }
            Route::Provider => {
                SourceEffects::new(access, program)
                    .class_object_member_value(receiver, self.name, MemberLookupPolicy::default())
                    .await?
            }
        };
        Ok(Resolved { receiver, member })
    }
}

/// Creates an independent database with no semantic inference memos for the fixture.
fn database(source: &str) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    db
}

/// Finds Product's definition in prepared syntax without requesting its inferred class type.
fn request<'name, 'db>(
    prepared: &PreparedAnalysisFile<'db>,
    name: &'name Name,
    route: Route,
) -> Request<'name, 'db> {
    let class = prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == "Product")
        .unwrap();
    Request {
        definition: prepared.semantic_index().expect_single_definition(class),
        name,
        route,
    }
}

/// Runs ordinary resolution after controlled completion, bypassing the canonical parent memo.
fn ordinary<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    receiver: Type<'db>,
    name: &Name,
    route: Route,
) -> PlaceAndQualifiers<'db> {
    let env = ProgramEnvironment::from_file(prepared.program_file());
    match route {
        Route::Canonical => match class_member_dispatch_sync(
            receiver,
            name,
            MemberLookupPolicy::default(),
            ClassMemberDispatchFacts,
            &OrdinaryClassMemberDispatch { db, env: &env },
        ) {
            Ok(member) => member,
            Err(never) => match never {},
        },
        Route::Provider => {
            receiver.class_object_member(db, &env, name, MemberLookupPolicy::default())
        }
    }
}

/// Confirms that source builders and the analysis attempt retire after success or refusal.
fn assert_cleanup() {
    assert_eq!(observations::counts().0, 0);
    assert_no_active_attempt();
}

/// Cold implicit, explicit, and inherited metaclasses publish the real class-member query result.
/// Ordinary dispatch is run afterward without fetching that parent memo, while a repeated
/// controlled query must reuse the original canonical memo address.
#[test_case::test_case(IMPLICIT_META, "__call__"; "implicit metaclass")]
#[test_case::test_case(EXPLICIT_META, "value"; "explicit metaclass")]
#[test_case::test_case(INHERITED_META, "value"; "inherited metaclass")]
fn cold_canonical_member_completes_and_reuses_its_memo(source: &str, name: &'static str) {
    let db = database(source);
    let prepared = prepare(&db);
    let name = Name::new_static(name);
    let request = request(&prepared, &name, Route::Canonical);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            definition_inference_ingredient(&db),
            request.definition.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let recording = Recording::start();
    let cold = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    let journal = recording.journal();
    drop(recording);
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(resolved)) = cold.value else {
        panic!("cold class-member lookup: {:?}", cold.value);
    };
    assert!(journal.transfer_completed > 0, "{journal:?}");
    assert!(!resolved.member.place.is_undefined());
    assert_eq!(
        resolved.member,
        ordinary(&db, &prepared, resolved.receiver, &name, Route::Canonical),
    );
    let key = MemberLookupKey::new(
        &db,
        prepared.program_file().program(&db),
        resolved.receiver,
        name.as_str(),
        MemberLookupPolicy::default(),
    );
    let ingredient = class_member_lookup_ingredient(&db);
    let database_key = ingredient.database_key_index(key.as_id());
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key.as_id()).is_ok());
    let first = cold
        .reads
        .iter()
        .find(|read| read.key == database_key)
        .unwrap();
    let warm = capture(&db, || {
        controlled_member_operation(&prepared, request, &funded())
    })
    .unwrap();
    warm.check_root_reads().unwrap();
    assert_eq!(warm.value, cold.value);
    assert!(
        warm.reads
            .iter()
            .any(|read| { read.key == database_key && read.memo_address == first.memo_address })
    );
    assert_cleanup();
}

/// An always-defined own declaration completes the cold provider without requesting metaclass
/// instance storage; the ordinary provider preserves the same complete member record.
#[test]
fn own_declaration_returns_before_instance_storage() {
    let db = database(DECLARED_VALUE);
    let prepared = prepare(&db);
    let name = Name::new_static("value");
    let recording = Recording::start();
    let cold = capture(&db, || {
        controlled_member_operation(
            &prepared,
            request(&prepared, &name, Route::Provider),
            &funded(),
        )
    })
    .unwrap();
    let journal = recording.journal();
    drop(recording);
    cold.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(resolved)) = cold.value else {
        panic!("cold declared class-object member: {:?}", cold.value);
    };
    assert_eq!(journal.storage_requested, 0, "{journal:?}");
    assert_eq!(journal.storage_completed, 0, "{journal:?}");
    assert_eq!(journal.transfer_completed, 1, "{journal:?}");
    assert!(!resolved.member.place.is_undefined());
    assert_eq!(
        resolved.member,
        ordinary(&db, &prepared, resolved.receiver, &name, Route::Provider),
    );
    assert_cleanup();
}

/// Derives a stored generic alias from the real source expression before requesting its meta-type.
#[derive(Clone, Copy, Debug)]
struct AliasMetaRequest<'db> {
    expression: Expression<'db>,
    key: ExpressionNodeKey,
}

impl<'db> MemberOperation<'db> for AliasMetaRequest<'db> {
    type Output = (Type<'db>, Type<'db>);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let inference = access
            .expression(self.expression, TypeContext::default())
            .await?;
        let endpoint = access.endpoint();
        let alias = endpoint
            .local_call(|| {
                endpoint.admit_work(2)?;
                endpoint.check_completion()?;
                Ok(inference.expression_type(self.key))
            })
            .await;
        let meta =
            MemberEntryEffects::meta_type(&SourceEffects::new(access, program), alias).await?;
        Ok((alias, meta))
    }
}

/// Finds a canonical specialization key without interning or executing the queried operation.
fn specialization_key<'db, C: SpecializationConfiguration>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    fields: &(Type<'db>, Specialization<'db>, bool),
) -> Option<salsa::Id> {
    let mut entries = C::argument_ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| entry.value().fields() == fields);
    let id = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    id
}

/// A cold source alias carries its stored specialization into selected-metaclass conversion.
/// The canonical specialization read distinguishes this from dropping the mapping even when
/// the selected metaclass's unchanged result would otherwise hide that mistake.
#[test]
fn cold_alias_metatype_uses_its_stored_specialization() {
    let db = database(
        "from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Meta(type): pass\nclass Product(Generic[T], metaclass=Meta): pass\nleft = right = Product[bool]\n",
    );
    let prepared = prepare(&db);
    let key = expression_key(&prepared);
    let expression = prepared.semantic_index().expression(key);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            expression.as_id()
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let mut events_db = db.clone();
    events_db.take_salsa_events();
    observations::reset(None);
    let result = capture(&db, || {
        controlled_member_operation(&prepared, AliasMetaRequest { expression, key }, &funded())
    })
    .unwrap();
    result.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete((receiver @ Type::GenericAlias(alias), meta))) = result.value
    else {
        panic!("cold stored-alias meta-type: {:?}", result.value);
    };
    let stored = alias.specialization(&db);
    let ingredient = apply_specialization_ingredient(&db);
    let id = specialization_key(&db, ingredient, &(meta, stored, false))
        .expect("selected meta-type did not use the alias's stored specialization");
    let database_key = ingredient.database_key_index(id);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, id).is_ok());
    assert!(result.reads.iter().any(|read| read.key == database_key));
    assert!(events_db.take_salsa_events().iter().any(|event| {
        matches!(event.kind, salsa::EventKind::WillExecute { database_key: actual } if actual == database_key)
    }));
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(meta, receiver.to_meta_type(&db, &env));
    let Type::ClassLiteral(ClassLiteral::Static(meta)) = meta else {
        panic!("the selected metaclass is not the fixture's static class");
    };
    assert_eq!(meta.name(&db), "Meta");
    assert_cleanup();
}

/// Requests the finite lookup conversion for explicit metaclass provenance input.
#[derive(Clone, Copy, Debug)]
struct LookupTargetRequest<'db>(ClassMetaclass<'db>);

impl<'db> MemberOperation<'db> for LookupTargetRequest<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        SourceEffects::new(access, program)
            .metaclass_lookup_value(self.0)
            .await
    }
}

/// Explicit ProtocolFallback input resolves through the cold canonical ABCMeta class lookup.
/// This tests finite provider conversion only; inferring a protocol's metaclass remains covered
/// by `inner_metaclass::unsupported_inner_metaclass_child_stays_unpublished`.
#[test]
fn protocol_fallback_input_uses_canonical_abcmeta() {
    let db = database(IMPLICIT_META);
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let argument = KnownClassArgument::new(&db, KnownClass::ABCMeta, program);
    let ingredient = known_class_to_class_literal_ingredient(&db);
    let key = ingredient.database_key_index(argument.as_id());
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    observations::reset(None);
    let result = capture(&db, || {
        controlled_member_operation(
            &prepared,
            LookupTargetRequest(ClassMetaclass::ProtocolFallback),
            &funded(),
        )
    })
    .unwrap();
    result.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(meta)) = result.value else {
        panic!("protocol lookup fallback conversion: {:?}", result.value);
    };
    let read = result.reads.iter().find(|read| read.key == key).unwrap();
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, argument.as_id()).is_ok());
    let env = ProgramEnvironment::from_program(program);
    let ordinary = capture(&db, || KnownClass::ABCMeta.to_class_literal(&db, &env)).unwrap();
    assert_eq!(ordinary.value, meta);
    assert!(ordinary.reads.iter().any(|ordinary_read| {
        ordinary_read.key == key && ordinary_read.memo_address == read.memo_address
    }));
    assert_ne!(meta, KnownClass::Type.to_class_literal(&db, &env));
    assert_cleanup();
}

/// Calls the real class-object provider with an explicitly constructed class-like input.
#[derive(Clone, Copy, Debug)]
struct ProviderRequest<'name, 'db> {
    receiver: Type<'db>,
    name: &'name Name,
}

impl<'db> MemberOperation<'db> for ProviderRequest<'_, 'db> {
    type Output = PlaceAndQualifiers<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        SourceEffects::new(access, program)
            .class_object_member_value(self.receiver, self.name, MemberLookupPolicy::default())
            .await
    }
}

/// A preconstructed TypeVar subclass input refuses precisely at its unsupported MRO child.
/// It cannot return an undefined fallback, request instance storage, or transfer a result.
#[test]
fn typevar_subclass_provider_preserves_the_demanded_child_refusal() {
    let db = database(IMPLICIT_META);
    let prepared = prepare(&db);
    let program = prepared.program_file().program(&db);
    let env = ProgramEnvironment::from_program(program);
    let raw = TypeVarInstance::new(
        &db,
        TypeVarIdentity::new(&db, Name::new_static("T"), None, TypeVarKind::LegacyTypeVar),
        None,
        None,
        None,
    );
    let variable = BoundTypeVarInstance::new(
        &db,
        raw,
        BindingContext::Synthetic(program),
        None,
        crate::types::typevar::TypeVarNonce::NONE,
    );
    let receiver = SubclassOfType::from(&db, &env, SubclassOfInner::TypeVar(variable));
    let name = Name::new_static("value");
    let recording = Recording::start();
    let result = controlled_member_operation(
        &prepared,
        ProviderRequest {
            receiver,
            name: &name,
        },
        &funded(),
    );
    let journal = recording.journal();
    drop(recording);
    assert_eq!(
        result,
        Ok(unavailable(OperationId::MemberLookup(
            GeneralMemberOperation::SubclassMro
        )))
    );
    assert_eq!(journal.storage_requested, 0, "{journal:?}");
    assert_eq!(journal.transfer_requested, 0, "{journal:?}");
    assert_cleanup();
}

/// Selects one resource limit while leaving the other resource at the established funded ceiling.
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

/// Counts completed class-object provider transfers during a fresh cold request.
/// Nested lookups transfer first; the requested member's provider transfers last.
fn completed_transfers(policy: &AnalysisPolicy) -> usize {
    let db = database(IMPLICIT_META);
    let prepared = prepare(&db);
    let name = Name::new_static("__call__");
    let recording = Recording::start();
    let _result = controlled_member_operation(
        &prepared,
        request(&prepared, &name, Route::Canonical),
        policy,
    );
    let journal = recording.journal();
    drop(recording);
    assert_cleanup();
    journal.transfer_completed
}

/// Work and byte exhaustion refuse the provider's final transfer before the canonical class-member
/// query publishes. Cold probes find the lowest budget that completes all nested and outer transfers;
/// the preceding budget refuses the outer transfer. The same database then completes under the
/// established funded limits in the same revision. Passive hooks observe real admission.
#[test_case::test_case(Resource::Work; "semantic work")]
#[test_case::test_case(Resource::Bytes; "requested bytes")]
fn transfer_refusal_keeps_the_parent_unpublished_and_retries(resource: Resource) {
    let mut low = 0;
    let mut high = resource.limit();
    let expected_transfers = completed_transfers(&resource.policy(high));
    assert!(expected_transfers > 0);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if completed_transfers(&resource.policy(middle)) == expected_transfers {
            high = middle;
        } else {
            low = middle;
        }
    }
    let db = database(IMPLICIT_META);
    let prepared = prepare(&db);
    let name = Name::new_static("__call__");
    let request = request(&prepared, &name, Route::Canonical);
    let revision = salsa::plumbing::current_revision(&db);
    let recording = Recording::start();
    let result = controlled_member_operation(&prepared, request, &resource.policy(low));
    let journal = recording.journal();
    drop(recording);
    assert_eq!(
        result,
        Ok(AnalysisOutcome::Incomplete {
            reason: resource.reason(),
            completed: ()
        })
    );
    assert_eq!(journal.transfer_requested, expected_transfers, "{journal:?}");
    assert_eq!(journal.transfer_completed, expected_transfers - 1, "{journal:?}");
    assert_cleanup();
    let program = prepared.program_file().program(&db);
    let mut keys = MemberLookupKey::ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| {
            let (key_program, ty, key_name, policy) = entry.value().fields();
            *key_program == program
                && *key_name == name
                && *policy == MemberLookupPolicy::default()
                && matches!(ty, Type::ClassLiteral(ClassLiteral::Static(class)) if class.name(&db) == "Product")
        });
    let id = keys
        .next()
        .expect("the refused class-member request did not intern its key")
        .key()
        .key_index();
    assert!(keys.next().is_none());
    let ingredient = class_member_lookup_ingredient(&db);
    let database_key = ingredient.database_key_index(id);
    assert_eq!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, database_key.key_index()).map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    let recording = Recording::start();
    let retry = controlled_member_operation(&prepared, request, &funded());
    let journal = recording.journal();
    drop(recording);
    let Ok(AnalysisOutcome::Complete(resolved)) = retry else {
        panic!("same-revision class-member retry: {retry:?}");
    };
    assert!(journal.transfer_completed > 0, "{journal:?}");
    let key = MemberLookupKey::new(
        &db,
        prepared.program_file().program(&db),
        resolved.receiver,
        name.as_str(),
        MemberLookupPolicy::default(),
    );
    assert_eq!(database_key, ingredient.database_key_index(key.as_id()));
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, key.as_id()).is_ok());
    assert_eq!(
        resolved.member,
        ordinary(&db, &prepared, resolved.receiver, &name, Route::Canonical),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_cleanup();
}
