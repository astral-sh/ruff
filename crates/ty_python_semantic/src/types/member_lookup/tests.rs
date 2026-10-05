use super::runtime::{
    ClassMemberProvider, DescriptorGetProvider, DescriptorLookupKeyProfile, InstanceMemberProvider,
    MemberDependencies, MemberEffects, MemberQueries, MemberQueryAccess, MemberSourcesAccess,
    NoMemberQueries, PlaceLookupKeyProfile, PlaceProvider,
};
use crate::Program;
use crate::place::{ConsideredDefinitions, RequiresExplicitReExport, place_by_id_ingredient};
use crate::types::class::static_literal::class_decorators_ingredient;
use crate::types::class::{
    CodeGeneratorKind, code_generator_of_static_class_ingredient, explicit_bases_ingredient,
    implicit_attribute_names_ingredient, known_class_to_class_literal_ingredient,
    known_class_to_class_literal_key, known_class_to_instance_ingredient,
    pep695_generic_context_ingredient, static_class_generic_context_ingredient,
    try_mro_unspecialized_ingredient,
};
use crate::types::descriptor::effects::descriptor_get_ingredient;
use crate::types::enums::{enum_class_literal_ingredient, enum_metadata, enum_metadata_ingredient};
use crate::types::infer::definition_inference_ingredient;
use crate::types::protocol_class::interface_build::runtime::{
    LookupDeclarationSources, ProtocolInterfaceProvider, ProtocolMroProvider, ProtocolSources,
};
use crate::types::protocol_class::{ProtocolInterface, protocol_interface_memo_ingredient};
use crate::types::relation::runtime::protocol::{
    ProtocolObjectProvider, ProtocolQueries, ProtocolQueryAccess, ProtocolRuntimeObservations,
    check_protocol_presence_for_test,
};
use crate::types::relation::runtime_resources::{
    CallBuilders, CallEnvironments, CallRelationOwners, CallResourceCapacity,
};
use crate::types::typevar::TypeVarSet;
use ruff_db::system::DbWithWritableSystem;
use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::num::NonZeroUsize;
use ty_python_core::finalized_sources::{place_table_ingredient, use_def_map_ingredient};
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;

use compact_str::CompactString;
use ruff_db::files::system_path_to_file;
use ruff_python_ast::PythonVersion;
use salsa::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
use salsa::execution_probe::FixedQueryKeyProfile as CopyMemoProfile;
use salsa::execution_probe::{
    CallableRoute, CallableRouteProvider, Demand, ExecutionAdmission, ExecutionWork,
    FinalSourceError, FinalSourceMemo, NativeValueOperation, NativeValueQuote, RegistryBuilder,
    RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::{Configuration, IngredientImpl, InternedQueryConfiguration};
use salsa::plumbing::{AsId, Ingredient, ZalsaDatabase};
use salsa::prepared_source_probe;
use ty_python_core::definition::{Definition, DefinitionState};
use ty_python_core::{global_scope, place_table, use_def_map};

use super::*;
use crate::Db;
use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
use crate::place::{
    DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy, TypeOrigin,
    global_symbol,
};
use crate::types::infer::infer_definition_types;
use crate::types::instance::protocol_object_equivalence_ingredient;
use crate::types::literal::{StringLiteralType, intern_member_name_literal};
use crate::types::protocol_class::protocol_interface_ingredient;
use crate::types::{
    ClassLiteral, ClassType, KnownClass, MemberLookupResult, ProtocolInstanceType, ResolvedMember,
    StaticClassLiteral, TypeQualifiers, class_member_lookup_ingredient, member_lookup_ingredient,
};

const PATH: &str = "/src/members.py";

fn database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            PATH,
            "from typing import ClassVar\nclass C:\n    value: ClassVar[int] = 1\n",
        )
        .build()
}

fn instance(db: &TestDb) -> anyhow::Result<Type<'_>> {
    let file = db.program_file(system_path_to_file(db, PATH)?);
    let Type::ClassLiteral(class) = global_symbol(db, file, "C").place.expect_type() else {
        panic!("the fixture defines a class literal");
    };
    Ok(Type::instance(
        db,
        &db.program_environment(),
        ClassType::NonGeneric(class),
    ))
}

struct Admission;

impl ExecutionAdmission for Admission {
    fn admit(&self, _work: ExecutionWork) -> RunResult<()> {
        Ok(())
    }
}

fn assert_no_execution(reader: &mut TestDb) {
    assert!(
        reader
            .take_salsa_events()
            .iter()
            .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
}

#[test]
fn member_lookup_key_schema_preserves_both_query_configs_and_policies() -> anyhow::Result<()> {
    let db = database()?;
    let mut reader = db.clone();
    let ty = instance(&db)?;
    let program = db.program_environment().program(&db);
    let owner = MemberLookupKey::ingredient(db.zalsa());
    let class = class_member_lookup_ingredient(&db);
    let member = member_lookup_ingredient(&db);
    let names = [
        Name::new_static("unqueried"),
        Name::new_static("unqueried"),
        Name::new_static("unqueried"),
    ];
    let admission = Admission;
    reader.clear_salsa_events();
    let result = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let singleton = registry.passive_memo::<_, _, CopyMemoProfile>(owner, class)?;
        assert!(matches!(
            registry.finite_interned_values_with_memos(owner, (singleton,)),
            Err(RunError::Contract(
                "finite interned value memo mapping is unsupported"
            ))
        ));
        let class = registry.passive_memo::<_, _, CopyMemoProfile>(owner, class)?;
        let member = registry.passive_memo::<_, _, CopyMemoProfile>(owner, member)?;
        let values = registry.finite_interned_values_with_memos(owner, (class, member))?;
        registry.seal()?.run(|endpoint| async move {
            let [first, equal, restricted] = names;
            let first = intern_member_lookup_key(
                &endpoint,
                &values,
                program,
                ty,
                first,
                MemberLookupPolicy::default(),
            )
            .await;
            let equal = intern_member_lookup_key(
                &endpoint,
                &values,
                program,
                ty,
                equal,
                MemberLookupPolicy::default(),
            )
            .await;
            let restricted = intern_member_lookup_key(
                &endpoint,
                &values,
                program,
                ty,
                restricted,
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .await;
            Ok((first, equal, restricted))
        })
    });
    let Ok(AttemptOutcome::Complete(Ok((first, equal, restricted)))) = result else {
        panic!("actual two-slot member keys did not complete: {result:?}");
    };
    assert_eq!(first, equal);
    assert_ne!(first, restricted);
    for (key, policy) in [
        (first, MemberLookupPolicy::default()),
        (restricted, MemberLookupPolicy::NO_INSTANCE_FALLBACK),
    ] {
        assert_eq!(key.program(&db), program);
        assert_eq!(key.ty(&db), ty);
        assert_eq!(key.name(&db).as_str(), "unqueried");
        assert_eq!(key.policy(&db), policy);
        assert!(matches!(
            FinalSourceMemo::certify(&db as &dyn Db, class, key.as_id()),
            Err(FinalSourceError::MissingMemo)
        ));
        assert!(matches!(
            FinalSourceMemo::certify(&db as &dyn Db, member, key.as_id()),
            Err(FinalSourceError::MissingMemo)
        ));
    }
    assert_no_execution(&mut reader);
    Ok(())
}

#[test]
fn member_lookup_accessors_select_the_ordinary_memos() -> anyhow::Result<()> {
    let db = database()?;
    let mut reader = db.clone();
    let ty = instance(&db)?;
    let env = db.program_environment();
    let policy = MemberLookupPolicy::default();
    let expected_class = ty.class_member_with_policy(&db, &env, "value", policy);
    let expected_member =
        ty.member_lookup_with_policy_and_receiver(&db, &env, "value", policy, None);
    let resolved = expected_member.expect("the declared class variable resolves");
    for result in [expected_class, resolved.member(&db)] {
        assert!(result.qualifiers.contains(TypeQualifiers::CLASS_VAR));
        assert!(matches!(
            result.place,
            Place::Defined(DefinedPlace {
                origin: TypeOrigin::Declared,
                definedness: Definedness::AlwaysDefined,
                provenance: Provenance::SingleDefinition(_),
                ..
            })
        ));
    }
    let key = MemberLookupKey::new(&db, env.program(&db), ty, "value", policy);
    let class = class_member_lookup_ingredient(&db);
    let member = member_lookup_ingredient(&db);
    let class_memo = FinalSourceMemo::certify(&db as &dyn Db, class, key.as_id()).unwrap();
    let member_memo = FinalSourceMemo::certify(&db as &dyn Db, member, key.as_id()).unwrap();
    assert_ne!(class_memo.database_key(), member_memo.database_key());
    reader.clear_salsa_events();
    salsa::attach(&db, || {
        assert_eq!(
            *class.fetch(&db as &dyn Db, db.zalsa(), db.zalsa_local(), key.as_id()),
            expected_class,
        );
        assert_eq!(
            *member.fetch(&db as &dyn Db, db.zalsa(), db.zalsa_local(), key.as_id()),
            expected_member,
        );
    });
    assert_no_execution(&mut reader);
    Ok(())
}

#[test]
fn member_lookup_owned_key_interning_reuses_populated_two_slot_key() -> anyhow::Result<()> {
    let db = database()?;
    let mut reader = db.clone();
    let ty = instance(&db)?;
    let env = db.program_environment();
    let program = env.program(&db);
    let policy = MemberLookupPolicy::default();
    let expected_class = ty.class_member_with_policy(&db, &env, "value", policy);
    let expected_member =
        ty.member_lookup_with_policy_and_receiver(&db, &env, "value", policy, None);
    let original = MemberLookupKey::new(&db, program, ty, "value", policy);
    let owner = MemberLookupKey::ingredient(db.zalsa());
    let class = class_member_lookup_ingredient(&db);
    let member = member_lookup_ingredient(&db);
    let _class_memo = FinalSourceMemo::certify(&db as &dyn Db, class, original.as_id()).unwrap();
    let _member_memo = FinalSourceMemo::certify(&db as &dyn Db, member, original.as_id()).unwrap();
    let name = Name::new_static("value");
    let admission = Admission;
    reader.clear_salsa_events();
    let result = try_with_attempt(&db, 100_000, || {
        let mut registry = RegistryBuilder::new(&db, &admission)?;
        let class = registry.passive_memo::<_, _, CopyMemoProfile>(owner, class)?;
        let member = registry.passive_memo::<_, _, CopyMemoProfile>(owner, member)?;
        let values = registry.finite_interned_values_with_memos(owner, (class, member))?;
        registry.seal()?.run(|endpoint| async move {
            Ok(intern_member_lookup_key(&endpoint, &values, program, ty, name, policy).await)
        })
    });
    assert_eq!(result, Ok(AttemptOutcome::Complete(Ok(original))));
    let _class_memo = FinalSourceMemo::certify(&db as &dyn Db, class, original.as_id()).unwrap();
    let _member_memo = FinalSourceMemo::certify(&db as &dyn Db, member, original.as_id()).unwrap();
    salsa::attach(&db, || {
        assert_eq!(
            *class.fetch(
                &db as &dyn Db,
                db.zalsa(),
                db.zalsa_local(),
                original.as_id()
            ),
            expected_class,
        );
        assert_eq!(
            *member.fetch(
                &db as &dyn Db,
                db.zalsa(),
                db.zalsa_local(),
                original.as_id()
            ),
            expected_member,
        );
    });
    assert_no_execution(&mut reader);
    Ok(())
}

#[derive(Clone, Copy)]
enum TextRequest {
    Key,
    Literal,
}

#[derive(Debug, PartialEq, Eq)]
enum TextOutput<'db> {
    Key(MemberLookupKey<'db>),
    Literal(Type<'db>),
}

struct TextAdmission {
    reader: RefCell<TestDb>,
    entered: Cell<bool>,
    copy_resource_seen: Cell<bool>,
    refuse_resource: bool,
    work: RefCell<Vec<ExecutionWork>>,
    copy_events: RefCell<Vec<salsa::Event>>,
}

impl TextAdmission {
    fn begin(&self) {
        self.reader.borrow_mut().clear_salsa_events();
        self.entered.set(true);
    }
}

impl ExecutionAdmission for TextAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        if self.entered.get() {
            self.work.borrow_mut().push(work);
            if matches!(work, ExecutionWork::Resource { .. })
                && !self.copy_resource_seen.replace(true)
            {
                self.copy_events
                    .borrow_mut()
                    .extend(self.reader.borrow_mut().take_salsa_events());
                if self.refuse_resource {
                    return Err(RunError::Refused(Incomplete::Interrupted));
                }
            }
        }
        Ok(())
    }
}

struct TextRun<'db> {
    outcome: AttemptOutcome<RunResult<TextOutput<'db>>>,
    entered: bool,
    work: Vec<ExecutionWork>,
    copy_events: Vec<salsa::Event>,
    events: Vec<salsa::Event>,
}

fn run_text<'db>(
    db: &'db TestDb,
    ty: Type<'db>,
    name: &str,
    request: TextRequest,
    allowance: usize,
    refuse_resource: bool,
) -> TextRun<'db> {
    let program = db.program_environment().program(db);
    let owner = MemberLookupKey::ingredient(db.zalsa());
    let class = class_member_lookup_ingredient(db);
    let member = member_lookup_ingredient(db);
    let strings = StringLiteralType::ingredient(db.zalsa());
    let admission = TextAdmission {
        reader: RefCell::new(db.clone()),
        entered: Cell::new(false),
        copy_resource_seen: Cell::new(false),
        refuse_resource,
        work: RefCell::new(Vec::new()),
        copy_events: RefCell::new(Vec::new()),
    };
    let outcome = try_with_attempt(db, allowance, || {
        let mut registry = RegistryBuilder::new(db, &admission)?;
        let class = registry.passive_memo::<_, _, CopyMemoProfile>(owner, class)?;
        let member = registry.passive_memo::<_, _, CopyMemoProfile>(owner, member)?;
        let keys = registry.finite_interned_values_with_memos(owner, (class, member))?;
        let strings = registry.finite_interned_values_with_memos(strings, ())?;
        let admission = &admission;
        registry.seal()?.run(|endpoint| async move {
            admission.begin();
            Ok(match request {
                TextRequest::Key => TextOutput::Key(
                    intern_member_lookup_key_from_str(
                        &endpoint,
                        &keys,
                        program,
                        ty,
                        name,
                        MemberLookupPolicy::default(),
                    )
                    .await,
                ),
                TextRequest::Literal => {
                    TextOutput::Literal(intern_member_name_literal(&endpoint, &strings, name).await)
                }
            })
        })
    })
    .expect("the text request starts outside a query");
    TextRun {
        outcome,
        entered: admission.entered.get(),
        work: admission.work.into_inner(),
        copy_events: admission.copy_events.into_inner(),
        events: admission.reader.into_inner().take_salsa_events(),
    }
}

fn text_cases() -> [String; 4] {
    [
        String::new(),
        "unqueried_text".to_owned(),
        "méthode_λ".to_owned(),
        "long_unqueried_member_".repeat(16),
    ]
}

fn assert_no_semantic_events(events: &[salsa::Event], allow_intern: bool) {
    for event in events {
        assert!(!matches!(
            event.kind,
            salsa::EventKind::WillExecute { .. }
                | salsa::EventKind::WillDiscardStaleOutput { .. }
                | salsa::EventKind::DidDiscard { .. }
                | salsa::EventKind::DidDiscardAccumulated { .. }
        ));
        if !allow_intern {
            assert!(!matches!(
                event.kind,
                salsa::EventKind::DidInternValue { .. }
                    | salsa::EventKind::DidReuseInternedValue { .. }
            ));
        }
    }
}

fn assert_text_work(run: &TextRun<'_>, name: &str, handle_bytes: usize, field_units: usize) {
    assert!(run.entered);
    let work: Vec<_> = run
        .work
        .iter()
        .copied()
        .filter(|work| !matches!(work, ExecutionWork::Poll))
        .collect();
    assert_eq!(
        work,
        [
            ExecutionWork::Work {
                units: 1 + name.len()
            },
            ExecutionWork::Resource {
                requested_bytes: handle_bytes + name.len()
            },
            ExecutionWork::Work { units: 1 },
            ExecutionWork::Work { units: field_units },
            ExecutionWork::Work { units: 1 },
            ExecutionWork::Resource { requested_bytes: 0 },
        ],
    );
    assert_no_semantic_events(&run.copy_events, false);
    assert_no_semantic_events(&run.events, true);
}

fn intern_events(run: &TextRun<'_>) -> usize {
    run.events
        .iter()
        .filter(|event| matches!(event.kind, salsa::EventKind::DidInternValue { .. }))
        .count()
}

#[test]
fn borrowed_member_names_use_admitted_owned_fields() -> anyhow::Result<()> {
    let db = database()?;
    let ty = instance(&db)?;
    let program = db.program_environment().program(&db);
    let owner = MemberLookupKey::ingredient(db.zalsa());
    let class = class_member_lookup_ingredient(&db);
    let member = member_lookup_ingredient(&db);
    let mut created = 0;
    for name in text_cases() {
        let mut first = None;
        for _ in 0..2 {
            let before = owner.entries(db.zalsa()).count();
            let run = run_text(&db, ty, &name, TextRequest::Key, 100_000, false);
            let added = owner.entries(db.zalsa()).count() - before;
            assert_eq!(intern_events(&run), added);
            assert_eq!(added, usize::from(first.is_none()));
            created += added;
            assert_text_work(
                &run,
                &name,
                size_of::<Name>(),
                4 + name.len() + ty.inline_payload_bytes(),
            );
            let AttemptOutcome::Complete(Ok(TextOutput::Key(key))) = run.outcome else {
                panic!("borrowed member key did not complete: {:?}", run.outcome);
            };
            assert_eq!(key.program(&db), program);
            assert_eq!(key.ty(&db), ty);
            assert_eq!(key.name(&db).as_str(), name);
            assert_eq!(key.policy(&db), MemberLookupPolicy::default());
            assert_eq!(
                key,
                MemberLookupKey::new(
                    &db,
                    program,
                    ty,
                    name.as_str(),
                    MemberLookupPolicy::default()
                ),
            );
            assert_eq!(*first.get_or_insert(key), key);
            assert!(matches!(
                FinalSourceMemo::certify(&db as &dyn Db, class, key.as_id()),
                Err(FinalSourceError::MissingMemo)
            ));
            assert!(matches!(
                FinalSourceMemo::certify(&db as &dyn Db, member, key.as_id()),
                Err(FinalSourceError::MissingMemo)
            ));
        }
    }
    assert!(created > 0);
    Ok(())
}

#[test]
fn member_name_literals_use_the_actual_empty_memo_schema() -> anyhow::Result<()> {
    let db = database()?;
    let ty = instance(&db)?;
    let strings = StringLiteralType::ingredient(db.zalsa());
    assert_eq!(strings.memo_table_types().len(), 0);
    let mut created = 0;
    for name in text_cases() {
        let mut first = None;
        for _ in 0..2 {
            let before = strings.entries(db.zalsa()).count();
            let run = run_text(&db, ty, &name, TextRequest::Literal, 100_000, false);
            let added = strings.entries(db.zalsa()).count() - before;
            assert_eq!(intern_events(&run), added);
            assert!(added <= usize::from(first.is_none()));
            created += added;
            assert_text_work(&run, &name, size_of::<CompactString>(), 1 + name.len());
            let AttemptOutcome::Complete(Ok(TextOutput::Literal(ty))) = run.outcome else {
                panic!("member literal did not complete: {:?}", run.outcome);
            };
            let Type::LiteralValue(literal) = ty else {
                panic!("the text request returns a literal");
            };
            assert!(literal.is_promotable());
            assert_eq!(literal.as_string().unwrap().value(&db), name);
            assert_eq!(ty, Type::string_literal(&db, name.as_str()));
            assert_eq!(*first.get_or_insert(ty), ty);
        }
    }
    assert!(created > 0);

    let untouched = setup_db();
    let owner = MemberLookupKey::ingredient(untouched.zalsa());
    let _class = class_member_lookup_ingredient(&untouched);
    let _member = member_lookup_ingredient(&untouched);
    assert_eq!(owner.memo_table_types().len(), 2);
    assert_eq!(owner.entries(untouched.zalsa()).count(), 0);
    let admission = Admission;
    let rejected = try_with_attempt(&untouched, 100_000, || {
        let mut registry = RegistryBuilder::new(&untouched, &admission)?;
        match registry.finite_interned_values_with_memos(owner, ()) {
            Err(error) => Err(error),
            Ok(_) => Ok(()),
        }
    });
    assert_eq!(
        rejected,
        Ok(AttemptOutcome::Complete(Err(RunError::Contract(
            "finite interned value memo mapping is unsupported",
        )))),
    );
    Ok(())
}

#[test]
fn member_text_refusal_precedes_interning() -> anyhow::Result<()> {
    let db = database()?;
    let ty = instance(&db)?;
    let keys = MemberLookupKey::ingredient(db.zalsa());
    let strings = StringLiteralType::ingredient(db.zalsa());
    for request in [TextRequest::Key, TextRequest::Literal] {
        for (allowance, refuse_resource, reason, name) in [
            (
                0,
                false,
                Incomplete::Allowance,
                "allowance_refused_member_text",
            ),
            (
                100_000,
                true,
                Incomplete::Interrupted,
                "resource_refused_member_text",
            ),
        ] {
            let before = (
                keys.entries(db.zalsa()).count(),
                strings.entries(db.zalsa()).count(),
            );
            let stamp = db.zalsa().current_revision();
            let run = run_text(&db, ty, name, request, allowance, refuse_resource);
            assert!(run.entered);
            assert_eq!(run.outcome, AttemptOutcome::Incomplete(reason));
            assert_no_semantic_events(&run.copy_events, false);
            assert_no_semantic_events(&run.events, false);
            let work: Vec<_> = run
                .work
                .iter()
                .filter(|work| !matches!(work, ExecutionWork::Poll))
                .collect();
            if refuse_resource {
                let handle_bytes = match request {
                    TextRequest::Key => size_of::<Name>(),
                    TextRequest::Literal => size_of::<CompactString>(),
                };
                assert_eq!(
                    work,
                    [
                        &ExecutionWork::Work {
                            units: 1 + name.len()
                        },
                        &ExecutionWork::Resource {
                            requested_bytes: handle_bytes + name.len()
                        },
                    ],
                );
            } else {
                assert!(work.is_empty());
            }
            assert_eq!(
                (
                    keys.entries(db.zalsa()).count(),
                    strings.entries(db.zalsa()).count(),
                ),
                before,
            );
            let retry = run_text(&db, ty, name, request, 100_000, false);
            let expected = match request {
                TextRequest::Key => TextOutput::Key(MemberLookupKey::new(
                    &db,
                    db.program_environment().program(&db),
                    ty,
                    name,
                    MemberLookupPolicy::default(),
                )),
                TextRequest::Literal => TextOutput::Literal(Type::string_literal(&db, name)),
            };
            assert_eq!(retry.outcome, AttemptOutcome::Complete(Ok(expected)));
            assert_eq!(db.zalsa().current_revision(), stamp);
        }
    }
    Ok(())
}

fn print_member_inventory_reads(
    label: &str,
    preparation: &[salsa::Event],
    reads: &[prepared_source_probe::Read],
    events: &[salsa::Event],
) {
    eprintln!("LOOKUP_IDENTITIES {label}: canonical query IDs; arguments are not decoded");
    for (index, event) in preparation.iter().enumerate() {
        eprintln!("LOOKUP_PREPARATION_EVENT {label} {index}: {event:?}");
    }
    for (index, read) in reads.iter().enumerate() {
        eprintln!("LOOKUP_READ {label} {index}: {read:?}");
    }
    for (index, event) in events.iter().enumerate() {
        eprintln!("LOOKUP_EVENT {label} {index}: {event:?}");
    }
}

fn capture_lookup<'db>(
    db: &'db TestDb,
    label: &str,
    receiver: Type<'db>,
    name: &str,
    policy: MemberLookupPolicy,
) -> MemberLookupResult<'db> {
    let env = db.program_environment();
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        receiver.member_lookup_with_policy_and_receiver(db, &env, name, policy, None)
    })
    .expect("ordinary lookup capture starts outside a query");
    let events = reader.take_salsa_events();
    assert_eq!(captured.check_root_reads(), Ok(()));
    salsa::attach(db, || {
        eprintln!(
            "LOOKUP_ROOT {label}: receiver={receiver:?}, program={:?}, name={name:?}, policy={policy:?}, result={:?}",
            env.program(db),
            captured.value,
        );
        print_member_inventory_reads(label, &preparation, &captured.reads, &events);
    });
    captured.value
}

fn capture_object_lookup(db: &TestDb, label: &str, name: &str, policy: MemberLookupPolicy) {
    let env = db.program_environment();
    let object = KnownClass::Object.to_instance(db, &env);
    assert!(
        capture_lookup(db, label, object, name, policy)
            .expect("object has no failing descriptor")
            .member(db)
            .is_undefined()
    );
}

#[test]
#[ignore = "temporary ordinary lookup dependency inventory"]
fn ordinary_object_member_lookup_read_inventory() {
    let roots = [
        (
            "restricted_value",
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        ),
        ("default_value", "value", MemberLookupPolicy::default()),
        (
            "restricted_getattr",
            "__getattr__",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        ),
    ];
    let db = setup_db();
    for (label, name, policy) in roots {
        capture_object_lookup(&db, &format!("sequential_{label}"), name, policy);
    }
    for (label, name, policy) in roots {
        let db = setup_db();
        capture_object_lookup(&db, &format!("fresh_{label}"), name, policy);
    }
}

const DATA_MEMBER_PATH: &str = "/src/data_members.py";

fn data_member_database() -> anyhow::Result<TestDb> {
    TestDbBuilder::new()
        .with_python_version(PythonVersion::PY313)
        .with_file(
            DATA_MEMBER_PATH,
            "from typing import Protocol\n\nclass C:\n    value: int\n\nclass P(Protocol):\n    value: int\n\nc: C\np: P\n",
        )
        .build()
}

fn data_member_operands<'db>(
    db: &'db TestDb,
) -> anyhow::Result<(Type<'db>, ProtocolInstanceType<'db>)> {
    let file = db.program_file(system_path_to_file(db, DATA_MEMBER_PATH)?);
    let receiver = global_symbol(db, file, "c").place.expect_type();
    let target = global_symbol(db, file, "p").place.expect_type();
    let Some(protocol) = target.as_protocol_instance() else {
        anyhow::bail!("the data-member fixture declares p as a protocol instance");
    };
    Ok((receiver, protocol))
}

fn capture_data_member_presence<'db>(
    db: &'db TestDb,
    label: &str,
    receiver: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
) -> bool {
    let env = db.program_environment();
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        crate::types::protocol_class::has_all_protocol_members_defined(db, &env, receiver, protocol)
    })
    .expect("ordinary protocol presence capture starts outside a query");
    let events = reader.take_salsa_events();
    assert_eq!(captured.check_root_reads(), Ok(()));
    salsa::attach(db, || {
        eprintln!(
            "PRESENCE_ROOT {label}: receiver={receiver:?}, protocol={protocol:?}, program={:?}, result={:?}",
            env.program(db),
            captured.value,
        );
        print_member_inventory_reads(label, &preparation, &captured.reads, &events);
    });
    captured.value
}

fn assert_declared_int_lookup<'db>(db: &'db TestDb, result: MemberLookupResult<'db>) {
    let member = result
        .expect("the declared data member has no failing descriptor")
        .member(db);
    assert_eq!(member.qualifiers, TypeQualifiers::empty());
    assert!(matches!(
        member.place,
        Place::Defined(DefinedPlace {
            origin: TypeOrigin::Declared,
            definedness: Definedness::AlwaysDefined,
            public_type_policy: PublicTypePolicy::Raw,
            provenance: Provenance::SingleDefinition(_),
            ..
        })
    ));
    assert_eq!(
        member.place.expect_type(),
        KnownClass::Int.to_instance(db, &db.program_environment()),
    );
}

#[test]
#[ignore = "temporary ordinary lookup dependency inventory"]
fn ordinary_data_member_lookup_read_inventory() -> anyhow::Result<()> {
    let policies = [
        ("restricted_value", MemberLookupPolicy::NO_INSTANCE_FALLBACK),
        ("default_value", MemberLookupPolicy::default()),
    ];
    let db = data_member_database()?;
    let (receiver, protocol) = data_member_operands(&db)?;
    let results = policies.map(|(label, policy)| {
        capture_lookup(
            &db,
            &format!("data_sequential_{label}"),
            receiver,
            "value",
            policy,
        )
    });
    let present = capture_data_member_presence(&db, "data_sequential_presence", receiver, protocol);
    for result in results {
        assert_declared_int_lookup(&db, result);
    }
    assert!(present);

    for (label, policy) in policies {
        let db = data_member_database()?;
        let (receiver, _) = data_member_operands(&db)?;
        let result = capture_lookup(
            &db,
            &format!("data_fresh_{label}"),
            receiver,
            "value",
            policy,
        );
        assert_declared_int_lookup(&db, result);
    }
    let db = data_member_database()?;
    let (receiver, protocol) = data_member_operands(&db)?;
    assert!(capture_data_member_presence(
        &db,
        "data_fresh_presence",
        receiver,
        protocol,
    ));
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct NativeDataMemberInputs<'db> {
    classes: [StaticClassLiteral<'db>; 2],
    protocol_class: ClassType<'db>,
    receiver: Type<'db>,
    protocol: ProtocolInstanceType<'db>,
}

fn data_member_definition<'db>(
    states: impl Iterator<Item = DefinitionState<'db>>,
) -> anyhow::Result<Definition<'db>> {
    let mut definitions = states.filter_map(|state| match state {
        DefinitionState::Defined(definition) => Some(definition),
        _ => None,
    });
    let Some(definition) = definitions.next() else {
        anyhow::bail!("the data-member fixture must have a source definition");
    };
    anyhow::ensure!(
        definitions.next().is_none(),
        "the definition must be unique"
    );
    Ok(definition)
}

fn native_data_member_class<'db>(
    db: &'db TestDb,
    name: &str,
) -> anyhow::Result<StaticClassLiteral<'db>> {
    let file = db.program_file(system_path_to_file(db, DATA_MEMBER_PATH)?);
    let scope = global_scope(db, file);
    let places = place_table(db, scope);
    let uses = use_def_map(db, scope);
    let Some(symbol) = places.symbol_id(name) else {
        anyhow::bail!("the data-member fixture must define {name}");
    };
    let definition = data_member_definition(
        uses.end_of_scope_symbol_bindings(symbol)
            .map(|binding| binding.binding),
    )?;
    let Type::ClassLiteral(ClassLiteral::Static(class)) =
        infer_definition_types(db, definition).binding_type(definition)
    else {
        anyhow::bail!("{name} must have a real static class declaration output");
    };
    Ok(class)
}

fn native_data_member_inputs(db: &TestDb) -> anyhow::Result<NativeDataMemberInputs<'_>> {
    let classes = [
        native_data_member_class(db, "C")?,
        native_data_member_class(db, "P")?,
    ];
    let env = db.program_environment();
    let receiver = Type::instance(db, &env, classes[0].identity_specialization(db));
    let protocol_class = classes[1].identity_specialization(db);
    let target = Type::instance(db, &env, protocol_class);
    anyhow::ensure!(matches!(receiver, Type::NominalInstance(_)));
    let Some(protocol) = target.as_protocol_instance() else {
        anyhow::bail!("the native instance constructor must recognize P as a protocol");
    };
    Ok(NativeDataMemberInputs {
        classes,
        protocol_class,
        receiver,
        protocol,
    })
}

fn native_data_member_declaration<'db>(
    db: &'db TestDb,
    class: StaticClassLiteral<'db>,
) -> anyhow::Result<Definition<'db>> {
    let scope = class.body_scope(db);
    let places = place_table(db, scope);
    let uses = use_def_map(db, scope);
    let Some(symbol) = places.symbol_id("value") else {
        anyhow::bail!("each native fixture class must declare value");
    };
    let definition = data_member_definition(
        uses.end_of_scope_symbol_declarations(symbol)
            .map(|declaration| declaration.declaration),
    )?;
    let _ = infer_definition_types(db, definition);
    Ok(definition)
}

fn native_object_key<'db, C>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    protocol: ProtocolInstanceType<'db>,
) -> Option<salsa::Id>
where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<
            Fields<'a> = (ProtocolInstanceType<'a>, ()),
        > + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>,
{
    let mut entries = C::argument_ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| *entry.value().fields() == (protocol, ()));
    let id = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    id
}

fn assert_native_member_targets_missing<'db>(
    db: &'db TestDb,
    inputs: NativeDataMemberInputs<'db>,
    stage: &str,
) {
    assert!(matches!(
        FinalSourceMemo::certify(
            db as &dyn Db,
            protocol_interface_ingredient(db),
            inputs.protocol_class.as_id(),
        ),
        Err(FinalSourceError::MissingMemo)
    ));
    let object = protocol_object_equivalence_ingredient(db);
    let object_id = native_object_key(db, object, inputs.protocol);
    if let Some(id) = object_id {
        assert!(matches!(
            FinalSourceMemo::certify(db as &dyn Db, object, id),
            Err(FinalSourceError::MissingMemo)
        ));
    }
    let program = db.program_environment().program(db);
    let keys = MemberLookupKey::ingredient(db.zalsa());
    for policy in [
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        MemberLookupPolicy::default(),
    ] {
        let mut entries = keys.entries(db.zalsa()).filter(|entry| {
            let (key_program, receiver, name, key_policy) = entry.value().fields();
            *key_program == program
                && *receiver == inputs.receiver
                && name.as_str() == "value"
                && *key_policy == policy
        });
        let id = entries.next().map(|entry| entry.key().key_index());
        assert!(entries.next().is_none());
        if let Some(id) = id {
            assert!(matches!(
                FinalSourceMemo::certify(db as &dyn Db, class_member_lookup_ingredient(db), id),
                Err(FinalSourceError::MissingMemo)
            ));
            assert!(matches!(
                FinalSourceMemo::certify(db as &dyn Db, member_lookup_ingredient(db), id),
                Err(FinalSourceError::MissingMemo)
            ));
        }
        eprintln!(
            "NATIVE_TARGETS {stage}: policy={policy:?}, member_key={id:?}, CL=missing, ML=missing"
        );
    }
    eprintln!("NATIVE_TARGETS {stage}: CI=missing, CO_key={object_id:?}, CO=missing");
    eprintln!(
        "NATIVE_TARGETS {stage}: PP/DG absence is not asserted without configuration accessors; the getter-CL request is not derived"
    );
}

fn capture_native_member_outcomes<'db>(
    db: &'db TestDb,
    label: &str,
    inputs: NativeDataMemberInputs<'db>,
    definition: Definition<'db>,
) {
    let present = capture_data_member_presence(
        db,
        &format!("{label}_presence"),
        inputs.receiver,
        inputs.protocol,
    );
    let results = [
        ("restricted_value", MemberLookupPolicy::NO_INSTANCE_FALLBACK),
        ("default_value", MemberLookupPolicy::default()),
    ]
    .map(|(root, policy)| {
        capture_lookup(
            db,
            &format!("{label}_{root}"),
            inputs.receiver,
            "value",
            policy,
        )
    });
    assert!(present);
    for result in results {
        assert_declared_int_lookup(db, result);
        assert!(matches!(
            result,
            Ok(ResolvedMember::Plain(PlaceAndQualifiers {
                place: Place::Defined(DefinedPlace {
                    provenance: Provenance::SingleDefinition(actual),
                    ..
                }),
                ..
            })) if actual == definition
        ));
    }
}

#[test]
#[ignore = "temporary native class-input dependency inventory"]
fn native_data_member_preparation_read_inventory() -> anyhow::Result<()> {
    let db = data_member_database()?;
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let captured = prepared_source_probe::capture(&db, || native_data_member_inputs(&db))
        .expect("native input construction starts outside a query");
    let events = reader.take_salsa_events();
    assert_eq!(captured.check_root_reads(), Ok(()));
    salsa::attach(&db, || {
        eprintln!(
            "NATIVE_INPUT_ROOT native_inputs: result={:?}",
            captured.value
        );
        print_member_inventory_reads("native_inputs", &preparation, &captured.reads, &events);
    });
    let inputs = captured.value?;
    assert_native_member_targets_missing(&db, inputs, "after_inputs");

    let preparation = reader.take_salsa_events();
    let captured = prepared_source_probe::capture(&db, || {
        Ok::<_, anyhow::Error>([
            native_data_member_declaration(&db, inputs.classes[0])?,
            native_data_member_declaration(&db, inputs.classes[1])?,
        ])
    })
    .expect("native member preparation starts outside a query");
    let events = reader.take_salsa_events();
    assert_eq!(captured.check_root_reads(), Ok(()));
    salsa::attach(&db, || {
        eprintln!(
            "NATIVE_SOURCE_ROOT native_declarations: result={:?}",
            captured.value
        );
        print_member_inventory_reads(
            "native_declarations",
            &preparation,
            &captured.reads,
            &events,
        );
    });
    let definitions = captured.value?;
    assert_native_member_targets_missing(&db, inputs, "after_member_declarations");
    capture_native_member_outcomes(&db, "native", inputs, definitions[0]);

    let oracle = data_member_database()?;
    let native_oracle = native_data_member_inputs(&oracle)?;
    let (ordinary_receiver, ordinary_protocol) = data_member_operands(&oracle)?;
    assert_eq!(native_oracle.receiver, ordinary_receiver);
    assert_eq!(native_oracle.protocol, ordinary_protocol);
    let definition = native_data_member_declaration(&oracle, native_oracle.classes[0])?;
    capture_native_member_outcomes(&oracle, "native_oracle", native_oracle, definition);
    Ok(())
}

struct PreparedMemberEndpoint<'db> {
    inputs: NativeDataMemberInputs<'db>,
    classes: Vec<StaticClassLiteral<'db>>,
    enum_classes: Vec<ClassLiteral<'db>>,
    definitions: Vec<Definition<'db>>,
    declaration: Option<Definition<'db>>,
    expected: Type<'db>,
    owner: Type<'db>,
    known_keys: Vec<(KnownClass, Program<'db>, salsa::Id)>,
    type_key: salsa::Id,
    object_key: salsa::Id,
}

fn prepare_member_endpoint(db: &TestDb, fresh: bool) -> anyhow::Result<PreparedMemberEndpoint<'_>> {
    let inputs = native_data_member_inputs(db)?;
    let env = db.program_environment();
    let program = env.program(db);
    let declaration = if place_table(db, inputs.classes[0].body_scope(db))
        .symbol_id("value")
        .is_some()
    {
        Some(native_data_member_declaration(db, inputs.classes[0])?)
    } else {
        None
    };
    let protocol_declaration = native_data_member_declaration(db, inputs.classes[1])?;
    let definitions = declaration
        .into_iter()
        .chain([protocol_declaration])
        .collect::<Vec<_>>();
    let annotation = declaration.unwrap_or(protocol_declaration);
    let expected = infer_definition_types(db, annotation)
        .inferred_declaration(annotation)
        .declared()
        .expect("the fixture annotation is declared")
        .inner_type();
    let declared_class = expected
        .nominal_class(db, &env)
        .unwrap()
        .class_literal(db)
        .as_static()
        .unwrap();
    let mut classes = inputs.classes.to_vec();
    let mut known_keys = Vec::new();
    for known in [
        KnownClass::Object,
        KnownClass::Type,
        KnownClass::Int,
        KnownClass::Enum,
        KnownClass::EnumType,
    ] {
        let class = known.try_to_class_literal(db, &env).unwrap();
        let key = known_class_to_class_literal_key(db, known, program);
        known_keys.push((known, program, key));
        if matches!(known, KnownClass::Object | KnownClass::Type) {
            classes.push(class);
        }
    }
    classes.push(declared_class);
    classes.sort_by_key(|class| class.as_id());
    classes.dedup();
    let type_key = known_keys
        .iter()
        .find(|(known, _, _)| *known == KnownClass::Type)
        .unwrap()
        .2;
    let object_key = known_keys
        .iter()
        .find(|(known, _, _)| *known == KnownClass::Object)
        .unwrap()
        .2;
    let _ = KnownClass::Type.to_instance(db, &env);
    let enum_classes = classes
        .iter()
        .filter(|class| **class != inputs.classes[1])
        .map(|class| ClassLiteral::Static(*class))
        .collect::<Vec<_>>();
    for class in &classes {
        let scope = class.body_scope(db);
        let _ = place_table(db, scope);
        let _ = use_def_map(db, scope);
        let _ = class.generic_context(db);
        let _ = class.explicit_bases(db);
        let _ = class.known_function_decorators(db).count();
        let _ = CodeGeneratorKind::from_class(db, (*class).into());
        let _ = implicit_attribute_names_ingredient(db).fetch(
            db,
            db.zalsa(),
            db.zalsa_local(),
            scope.as_id(),
        );
    }
    for class in &enum_classes {
        let _ = enum_metadata(db, *class);
        let _ = class.into_enum_class(db);
    }
    let owner = inputs.receiver.to_meta_type(db, &env);
    if fresh {
        assert_native_member_targets_missing(db, inputs, "final_member_source_bundle");
    }
    let prepared = PreparedMemberEndpoint {
        inputs,
        classes,
        enum_classes,
        definitions,
        declaration,
        expected,
        owner,
        known_keys,
        type_key,
        object_key,
    };
    if fresh {
        assert_member_endpoint_cold(db, &prepared);
    }
    Ok(prepared)
}

fn existing_native_query_key<'db, C>(
    db: &'db TestDb,
    _ingredient: &IngredientImpl<C>,
    fields: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
) -> Option<salsa::Id>
where
    C: InternedQueryConfiguration,
    <C as salsa::plumbing::interned::Configuration>::Fields<'db>: PartialEq,
{
    let mut entries = C::argument_ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| entry.value().fields() == fields);
    let result = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    result
}

fn assert_member_endpoint_cold(db: &TestDb, prepared: &PreparedMemberEndpoint<'_>) {
    let pp = place_by_id_ingredient(db);
    let scope = prepared.inputs.classes[0].body_scope(db);
    if let Some(symbol) = place_table(db, scope).symbol_id("value")
        && let Some(id) = existing_native_query_key(
            db,
            pp,
            &(
                scope,
                symbol.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            ),
        )
    {
        assert!(
            matches!(
                FinalSourceMemo::certify(db as &dyn Db, pp, id),
                Err(FinalSourceError::MissingMemo)
            ),
            "C.value PP must remain cold after preparation"
        );
    }
    let dg = descriptor_get_ingredient(db);
    let program = db.program_environment().program(db);
    if let Some(id) = existing_native_query_key(
        db,
        dg,
        &(
            program,
            prepared.expected,
            Some(prepared.inputs.receiver),
            prepared.owner,
        ),
    ) {
        assert!(
            matches!(
                FinalSourceMemo::certify(db as &dyn Db, dg, id),
                Err(FinalSourceError::MissingMemo)
            ),
            "the actual C.value descriptor request must remain cold"
        );
    }
    let owner = MemberLookupKey::ingredient(db.zalsa());
    for entry in owner.entries(db.zalsa()) {
        let (key_program, ty, name, policy) = entry.value().fields();
        if *key_program == program
            && *ty == prepared.expected
            && name.as_str() == "__get__"
            && *policy == MemberLookupPolicy::REQUIRE_CONCRETE
        {
            assert!(
                matches!(
                    FinalSourceMemo::certify(
                        db as &dyn Db,
                        class_member_lookup_ingredient(db),
                        entry.key().key_index()
                    ),
                    Err(FinalSourceError::MissingMemo)
                ),
                "the getter CL must remain cold"
            );
        }
    }
}

// Every selected PP/DG/getter key above is derived from native typed inputs, without interning.
fn member_sources<'db, C>(
    db: &'db C::DbView,
    ingredient: &'db IngredientImpl<C>,
    ids: impl IntoIterator<Item = salsa::Id>,
) -> Vec<FinalSourceMemo<'db, C>>
where
    C: Configuration,
{
    let mut memos = ids
        .into_iter()
        .map(|id| FinalSourceMemo::certify(db, ingredient, id).unwrap())
        .collect::<Vec<_>>();
    memos.sort_by_key(|memo| memo.database_key().key_index());
    memos.dedup_by_key(|memo| memo.database_key().key_index());
    assert!(
        memos
            .windows(2)
            .all(|pair| pair[0].database_key().key_index() < pair[1].database_key().key_index())
    );
    memos
}

#[derive(Debug)]
struct MemberEndpointOutput<'db> {
    restricted: MemberLookupResult<'db>,
    presence: Option<bool>,
    missing: Option<MemberLookupResult<'db>>,
    object_equivalence: Option<bool>,
    required_default: Option<MemberLookupResult<'db>>,
}
struct MemberEndpointRecord<'db> {
    outcome: AttemptOutcome<RunResult<MemberEndpointOutput<'db>>>,
    execution_error: Option<RunError>,
    reads: Vec<prepared_source_probe::Read>,
    stamp: prepared_source_probe::Stamp,
    preparation: Vec<salsa::Event>,
    events: Vec<salsa::Event>,
    work: Vec<ExecutionWork>,
    unsupported: Vec<&'static str>,
}
#[derive(Default)]
struct MemberEndpointAdmission {
    work: RefCell<Vec<ExecutionWork>>,
    refuse_next: Cell<bool>,
}
impl ExecutionAdmission for MemberEndpointAdmission {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.work.borrow_mut().push(work);
        if self.refuse_next.replace(false) {
            Err(RunError::Refused(Incomplete::Allowance))
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
struct MemberRecoveryJournal {
    armed: Cell<bool>,
    initial: Cell<usize>,
    body: Cell<usize>,
    recovery: Cell<usize>,
    admissions: Cell<usize>,
    owner_live: Cell<bool>,
    child_queued: Cell<bool>,
    child_started: Cell<bool>,
    cycle: Cell<Option<(salsa::Id, u32)>>,
    drops: RefCell<Vec<&'static str>>,
}

impl MemberRecoveryJournal {
    fn assert_finished(&self) {
        assert_eq!(self.initial.get(), 1);
        assert_eq!(self.body.get(), 1);
        assert_eq!(self.recovery.get(), 1);
        assert_eq!(self.admissions.get(), 1);
        assert!(!self.armed.get());
        assert!(!self.owner_live.get());
        assert!(self.child_queued.get());
        assert!(!self.child_started.get());
        assert_eq!(&*self.drops.borrow(), &["child", "recovery-owner"]);
    }
}

struct MemberRecoveryChild<'a>(&'a MemberRecoveryJournal);
impl Drop for MemberRecoveryChild<'_> {
    fn drop(&mut self) {
        assert!(self.0.owner_live.get());
        self.0.drops.borrow_mut().push("child");
    }
}

struct MemberRecoveryAdmission<'run, 'db: 'run> {
    base: &'run MemberEndpointAdmission,
    journal: &'run MemberRecoveryJournal,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'run RefCell<Option<Demand<()>>>,
}

impl ExecutionAdmission for MemberRecoveryAdmission<'_, '_> {
    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.base.admit(work)?;
        if !matches!(work, ExecutionWork::Work { units: 1 }) || !self.journal.armed.replace(false) {
            return Ok(());
        }
        self.journal
            .admissions
            .set(self.journal.admissions.get() + 1);
        let endpoint = self
            .endpoint
            .borrow()
            .as_ref()
            .cloned()
            .expect("recovery installed its endpoint");
        let child = MemberRecoveryChild(self.journal);
        let pending = endpoint.demand(move || {
            child.0.child_started.set(true);
            std::future::poll_fn(move |_| -> std::task::Poll<RunResult<()>> {
                let _child = &child;
                panic!("the recovery refusal must drain the child without executing it");
            })
        })?;
        assert!(!self.journal.child_queued.replace(true));
        assert!(self.pending.borrow_mut().replace(pending).is_none());
        // The actual provider's recovery callback supplies the refusal after this admission.
        Ok(())
    }
}

struct MemberRecoverySlots<'a, 'run, 'db: 'run> {
    endpoint: &'a RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
    pending: &'a RefCell<Option<Demand<()>>>,
}
impl Drop for MemberRecoverySlots<'_, '_, '_> {
    fn drop(&mut self) {
        drop(self.pending.borrow_mut().take());
        drop(self.endpoint.borrow_mut().take());
    }
}

// Inputs and outputs are Copy. This guard observes the recovery frame and its borrowed cycle/last value.
struct MemberRecoveryOwner<'a, I: Copy + PartialEq, V: Copy + PartialEq> {
    journal: &'a MemberRecoveryJournal,
    cycle: &'a salsa::Cycle<'a>,
    last: &'a V,
    input: I,
    value: V,
    expected_input: I,
    expected_value: V,
    expected_last: V,
}
impl<I: Copy + PartialEq, V: Copy + PartialEq> Drop for MemberRecoveryOwner<'_, I, V> {
    fn drop(&mut self) {
        assert_eq!(
            self.journal.cycle.get(),
            Some((self.cycle.id(), self.cycle.iteration()))
        );
        assert!(self.cycle.head_ids().any(|id| id == self.cycle.id()));
        assert!(self.input == self.expected_input);
        assert!(self.value == self.expected_value);
        assert!(*self.last == self.expected_last);
        assert!(self.journal.owner_live.replace(false));
        self.journal.drops.borrow_mut().push("recovery-owner");
    }
}

struct MemberRecoveryValues<I, V> {
    selected: Cell<Option<(salsa::Id, I)>>,
    initial: Cell<Option<V>>,
    computed: Cell<Option<V>>,
}

impl<I, V> MemberRecoveryValues<I, V> {
    fn new() -> Self {
        Self {
            selected: Cell::new(None),
            initial: Cell::new(None),
            computed: Cell::new(None),
        }
    }
}

fn assert_declared_recovery_values<I: Copy>(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    values: &MemberRecoveryValues<I, PlaceAndQualifiers<'_>>,
) -> salsa::Id {
    let (id, _) = values.selected.get().unwrap();
    let seed: PlaceAndQualifiers<'_> = Place::bound(Type::divergent(id)).into();
    assert_eq!(values.initial.get(), Some(seed));
    let value = values.computed.get().unwrap();
    assert_restricted_member_result(db, prepared, Ok(ResolvedMember::Plain(value)));
    assert_ne!(value, seed);
    id
}

struct MemberRecoveryProvider<'run, 'db: 'run, C: Configuration, P> {
    inner: P,
    route: CallableRoute<'run, 'db, C>,
    values: &'run MemberRecoveryValues<C::Input<'db>, C::Output<'db>>,
    journal: &'run MemberRecoveryJournal,
    endpoint: &'run RefCell<ManuallyDrop<Option<TaskEndpoint<'run, 'db>>>>,
}

impl<'run, 'db: 'run, C, P> CallableRouteProvider<'run, 'db, C>
    for MemberRecoveryProvider<'run, 'db, C, P>
where
    C: Configuration,
    C::Input<'db>: Copy + PartialEq,
    C::Output<'db>: Copy + PartialEq + std::fmt::Debug,
    P: CallableRouteProvider<'run, 'db, C>,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        self.inner.native_value(endpoint, db, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        input: C::Input<'db>,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        let selected = self
            .values
            .selected
            .get()
            .filter(|(_, selected)| *selected == input);
        if let Some((id, _)) = selected {
            assert!(matches!(
                C::CYCLE_STRATEGY,
                salsa::plumbing::CycleRecoveryStrategy::Fixpoint
            ));
            // This deliberate dependency edge exercises recovery; the C/P source itself is acyclic.
            let seed = endpoint
                .child_call(|| async { Ok(*endpoint.fetch_ref(&self.route, id)?.await?) })
                .await;
            assert_eq!(Some(seed), self.values.initial.get());
        }
        let value = self.inner.body(endpoint, db, input).await?;
        if selected.is_some() {
            self.journal.body.set(self.journal.body.get() + 1);
            self.values.computed.set(Some(value));
        }
        Ok(value)
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        let value = self.inner.initial(endpoint, db, id, input).await?;
        if self
            .values
            .selected
            .get()
            .is_some_and(|selected| selected == (id, input))
        {
            self.journal.initial.set(self.journal.initial.get() + 1);
            self.values.initial.set(Some(value));
        }
        Ok(value)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
    ) -> RunResult<C::Output<'db>>
    where
        'run: 'call,
    {
        let Some((id, expected_input)) = self
            .values
            .selected
            .get()
            .filter(|(_, selected)| *selected == input)
        else {
            return self
                .inner
                .recover(endpoint, db, cycle, last, value, input)
                .await;
        };
        assert_eq!(cycle.id(), id);
        assert_eq!(cycle.head_ids().collect::<Vec<_>>(), [id]);
        assert!(input == expected_input);
        let expected_last = self
            .values
            .initial
            .get()
            .expect("the actual seed completed");
        let expected_value = self
            .values
            .computed
            .get()
            .expect("the actual member body completed");
        assert_eq!(*last, expected_last);
        assert_eq!(value, expected_value);
        self.journal.recovery.set(self.journal.recovery.get() + 1);
        self.journal.cycle.set(Some((id, cycle.iteration())));
        assert!(!self.journal.owner_live.replace(true));
        let _owner = MemberRecoveryOwner {
            journal: self.journal,
            cycle,
            last,
            input,
            value,
            expected_input,
            expected_value,
            expected_last,
        };
        assert!(
            self.endpoint
                .borrow_mut()
                .replace(endpoint.clone())
                .is_none()
        );
        assert!(!self.journal.armed.replace(true));
        let returned = self
            .inner
            .recover(endpoint, db, cycle, last, value, input)
            .await;
        panic!("the actual member recovery refusal returned to its caller: {returned:?}")
    }
}

fn run_member_endpoint<'db>(
    db: &'db TestDb,
    prepared: &PreparedMemberEndpoint<'db>,
    allowance: usize,
    full: bool,
    definitions_available: bool,
) -> MemberEndpointRecord<'db> {
    run_member_endpoint_inner(
        db,
        prepared,
        allowance,
        full,
        definitions_available,
        MemberEndpointControl::None,
    )
}

fn run_member_endpoint_inner<'db>(
    db: &'db TestDb,
    prepared: &PreparedMemberEndpoint<'db>,
    allowance: usize,
    full: bool,
    definitions_available: bool,
    control: MemberEndpointControl,
) -> MemberEndpointRecord<'db> {
    let journal = MemberOwnershipJournal::default();
    run_member_endpoint_observed(
        db,
        prepared,
        allowance,
        full,
        definitions_available,
        control,
        &journal,
    )
}

fn run_member_endpoint_observed<'db>(
    db: &'db TestDb,
    prepared: &PreparedMemberEndpoint<'db>,
    allowance: usize,
    full: bool,
    definitions_available: bool,
    control: MemberEndpointControl,
    journal: &MemberOwnershipJournal,
) -> MemberEndpointRecord<'db> {
    let pt = place_table_ingredient(db);
    let ud = use_def_map_ingredient(db);
    let di = definition_inference_ingredient(db);
    let gc = static_class_generic_context_ingredient(db);
    let eb = explicit_bases_ingredient(db);
    let pc = pep695_generic_context_ingredient(db);
    let kc = known_class_to_class_literal_ingredient(db);
    let ki = known_class_to_instance_ingredient(db);
    let cm = try_mro_unspecialized_ingredient(db);
    let ci = protocol_interface_ingredient(db);
    let co = protocol_object_equivalence_ingredient(db);
    let cl = class_member_lookup_ingredient(db);
    let ml = member_lookup_ingredient(db);
    let pp = place_by_id_ingredient(db);
    let dg = descriptor_get_ingredient(db);
    let em = enum_metadata_ingredient(db);
    let ec = enum_class_literal_ingredient(db);
    let de = class_decorators_ingredient(db);
    let cg = code_generator_of_static_class_ingredient(db);
    let ia = implicit_attribute_names_ingredient(db);
    let program = db.program_environment().program(db);
    let places = member_sources(
        db as &dyn ty_python_core::Db,
        pt,
        prepared
            .classes
            .iter()
            .map(|class| class.body_scope(db).as_id()),
    );
    let uses = member_sources(
        db as &dyn ty_python_core::Db,
        ud,
        prepared
            .classes
            .iter()
            .map(|class| class.body_scope(db).as_id()),
    );
    let definitions = member_sources(
        db as &dyn Db,
        di,
        prepared
            .definitions
            .iter()
            .map(|definition| definition.as_id()),
    );
    let contexts = member_sources(
        db as &dyn Db,
        gc,
        prepared.classes.iter().map(|class| class.as_id()),
    );
    let bases = member_sources(
        db as &dyn Db,
        eb,
        prepared
            .classes
            .iter()
            .filter(|class| class.has_explicit_bases(db))
            .map(|class| class.as_id()),
    );
    let known = member_sources(
        db as &dyn Db,
        kc,
        prepared.known_keys.iter().map(|(_, _, id)| *id),
    );
    let instances = member_sources(db as &dyn Db, ki, [prepared.type_key]);
    let metadata = member_sources(
        db as &dyn Db,
        em,
        prepared.enum_classes.iter().map(|class| class.as_id()),
    );
    let enum_classes = member_sources(
        db as &dyn Db,
        ec,
        prepared.enum_classes.iter().map(|class| class.as_id()),
    );
    let mut decorator_memos = prepared
        .classes
        .iter()
        .filter_map(
            |class| match FinalSourceMemo::certify(db as &dyn Db, de, class.as_id()) {
                Ok(memo) => Some(memo),
                Err(FinalSourceError::MissingMemo) => None,
                other => panic!("invalid decorator source: {other:?}"),
            },
        )
        .collect::<Vec<_>>();
    let mut generator_memos = prepared
        .classes
        .iter()
        .filter_map(
            |class| match FinalSourceMemo::certify(db as &dyn Db, cg, class.as_id()) {
                Ok(memo) => Some(memo),
                Err(FinalSourceError::MissingMemo) => None,
                other => panic!("invalid generator source: {other:?}"),
            },
        )
        .collect::<Vec<_>>();
    decorator_memos.sort_by_key(|memo| memo.database_key().key_index());
    decorator_memos.dedup_by_key(|memo| memo.database_key().key_index());
    generator_memos.sort_by_key(|memo| memo.database_key().key_index());
    generator_memos.dedup_by_key(|memo| memo.database_key().key_index());
    let names = member_sources(
        db as &dyn Db,
        ia,
        prepared
            .classes
            .iter()
            .map(|class| class.body_scope(db).as_id()),
    );
    let admission = MemberEndpointAdmission::default();
    let recovery = MemberRecoveryJournal::default();
    let member_recovery = MemberRecoveryValues::new();
    let class_recovery = MemberRecoveryValues::new();
    let place_recovery = MemberRecoveryValues::new();
    let descriptor_recovery = MemberRecoveryValues::new();
    let target = control.recovery_target();
    let place_input = if target == Some(MemberRecoveryTarget::Place) {
        let scope = prepared.inputs.classes[0].body_scope(db);
        let symbol = place_table(db, scope).symbol_id("value").unwrap();
        Some((
            scope,
            symbol.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        ))
    } else {
        None
    };
    let descriptor_input = (
        program,
        prepared.expected,
        Some(prepared.inputs.receiver),
        prepared.owner,
    );
    if target.is_some() {
        assert_native_member_targets_missing(db, prepared.inputs, "recovery_key_inputs");
        assert_member_endpoint_cold(db, prepared);
    }
    let execution_error = Cell::new(None);
    let unsupported = RefCell::new(Vec::new());
    let observations = ProtocolRuntimeObservations::default();
    let mut reader = db.clone();
    let preparation = reader.take_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        try_with_attempt(db, allowance, || {
            let result = (|| {
                let base;
                let sources;
                let values;
                let strings;
                let place_keys;
                let descriptor_keys;
                let object_keys;
                let recovery_admission;
                let endpoint_slot = RefCell::new(ManuallyDrop::new(None));
                let pending = RefCell::new(None);
                recovery_admission = MemberRecoveryAdmission {
                    base: &admission,
                    journal: &recovery,
                    endpoint: &endpoint_slot,
                    pending: &pending,
                };
                let environments = CallEnvironments::with_capacity(CallResourceCapacity {
                    calls: NonZeroUsize::new(2).unwrap(),
                });
                let builders = CallBuilders::with_capacity(CallResourceCapacity {
                    calls: NonZeroUsize::new(2).unwrap(),
                });
                let owners = CallRelationOwners::with_capacity(CallResourceCapacity {
                    calls: NonZeroUsize::new(2).unwrap(),
                });
                // Clear the endpoint and demand before the admission's borrowed pools retire.
                let _reset = MemberRecoverySlots {
                    endpoint: &endpoint_slot,
                    pending: &pending,
                };
                let mut registry = RegistryBuilder::new(db, &recovery_admission)?;
                base = ProtocolSources {
                    places: registry.register_final_source(
                        db as &dyn ty_python_core::Db,
                        pt,
                        &places,
                    )?,
                    uses: registry.register_final_source(
                        db as &dyn ty_python_core::Db,
                        ud,
                        &uses,
                    )?,
                    definitions: if definitions_available {
                        Some(registry.register_final_source(db as &dyn Db, di, &definitions)?)
                    } else {
                        None
                    },
                    contexts: registry.register_final_source(db as &dyn Db, gc, &contexts)?,
                    bases: registry.register_final_source(db as &dyn Db, eb, &bases)?,
                    pep695: absent_member_source(pc),
                    object: registry.register_final_source(db as &dyn Db, kc, &known)?,
                    object_program: program,
                    object_id: prepared.object_key,
                };
                let mro = registry.reserve_callable(db as &dyn Db, cm)?;
                let interface = registry.reserve_callable(db as &dyn Db, ci)?;
                let object = registry.reserve_callable(db as &dyn Db, co)?;
                let class = registry.reserve_callable(db as &dyn Db, cl)?;
                let member = registry.reserve_callable(db as &dyn Db, ml)?;
                let place = registry.reserve_callable(db as &dyn Db, pp)?;
                let descriptor = registry.reserve_callable(db as &dyn Db, dg)?;
                object_keys = registry.fixed_callable_query_keys(&object)?;
                place_keys = registry.callable_query_keys::<_, PlaceLookupKeyProfile>(&place)?;
                descriptor_keys =
                    registry.callable_query_keys::<_, DescriptorLookupKeyProfile>(&descriptor)?;
                let owner = MemberLookupKey::ingredient(db.zalsa());
                let class_slot = registry.passive_memo::<_, _, CopyMemoProfile>(owner, cl)?;
                let member_slot = registry.passive_memo::<_, _, CopyMemoProfile>(owner, ml)?;
                values =
                    registry.finite_interned_values_with_memos(owner, (class_slot, member_slot))?;
                strings = registry.finite_interned_values_with_memos(
                    StringLiteralType::ingredient(db.zalsa()),
                    (),
                )?;
                sources = LookupDeclarationSources {
                    db,
                    base: &base,
                    mro: mro.clone(),
                    unsupported: &unsupported,
                    enum_metadata: registry.register_final_source(db as &dyn Db, em, &metadata)?,
                    enum_classes: registry.register_final_source(
                        db as &dyn Db,
                        ec,
                        &enum_classes,
                    )?,
                    decorators: registry.register_final_source(
                        db as &dyn Db,
                        de,
                        &decorator_memos,
                    )?,
                    generators: registry.register_final_source(
                        db as &dyn Db,
                        cg,
                        &generator_memos,
                    )?,
                    implicit_names: registry.register_final_source(db as &dyn Db, ia, &names)?,
                    instances: registry.register_final_source(db as &dyn Db, ki, &instances)?,
                    known_keys: &prepared.known_keys,
                };
                let members = MemberQueries {
                    class: class.clone(),
                    member: member.clone(),
                    place: place.clone(),
                    descriptor: descriptor.clone(),
                    values: &values,
                    strings: &strings,
                    place_keys: &place_keys,
                    descriptor_keys: &descriptor_keys,
                };
                registry.bind_callable(
                    &mro,
                    ProtocolMroProvider {
                        route: mro.clone(),
                        sources: &base,
                    },
                )?;
                let interface_values = registry.finite_interned_values(
                    db as &dyn Db,
                    ProtocolInterface::ingredient(db.zalsa()),
                    protocol_interface_memo_ingredient(db),
                )?;
                registry.bind_callable(
                    &interface,
                    ProtocolInterfaceProvider {
                        sources: &base,
                        mro_route: mro,
                        values: interface_values,
                    },
                )?;
                let class_provider = ClassMemberProvider {
                    queries: members.clone(),
                    sources: &sources,
                };
                if target == Some(MemberRecoveryTarget::Class) {
                    registry.bind_callable(
                        &class,
                        MemberRecoveryProvider {
                            inner: class_provider,
                            route: class.clone(),
                            values: &class_recovery,
                            journal: &recovery,
                            endpoint: &endpoint_slot,
                        },
                    )?;
                } else {
                    registry.bind_callable(&class, class_provider)?;
                }
                let member_provider = InstanceMemberProvider {
                    queries: InterruptedMemberQueries {
                        inner: members.clone(),
                        sources: &sources,
                        control,
                        receiver: prepared.inputs.receiver,
                        expected: prepared.expected,
                        journal: &journal,
                    },
                    sources: &sources,
                };
                if target == Some(MemberRecoveryTarget::Member) {
                    registry.bind_callable(
                        &member,
                        MemberRecoveryProvider {
                            inner: member_provider,
                            route: member.clone(),
                            values: &member_recovery,
                            journal: &recovery,
                            endpoint: &endpoint_slot,
                        },
                    )?;
                } else {
                    registry.bind_callable(&member, member_provider)?;
                }
                let place_provider = PlaceProvider { sources: &sources };
                if target == Some(MemberRecoveryTarget::Place) {
                    registry.bind_callable(
                        &place,
                        MemberRecoveryProvider {
                            inner: place_provider,
                            route: place.clone(),
                            values: &place_recovery,
                            journal: &recovery,
                            endpoint: &endpoint_slot,
                        },
                    )?;
                } else {
                    registry.bind_callable(&place, place_provider)?;
                }
                let descriptor_provider = DescriptorGetProvider {
                    queries: members.clone(),
                    sources: &sources,
                };
                if target == Some(MemberRecoveryTarget::Descriptor) {
                    registry.bind_callable(
                        &descriptor,
                        MemberRecoveryProvider {
                            inner: descriptor_provider,
                            route: descriptor.clone(),
                            values: &descriptor_recovery,
                            journal: &recovery,
                            endpoint: &endpoint_slot,
                        },
                    )?;
                } else {
                    registry.bind_callable(&descriptor, descriptor_provider)?;
                }
                let queries = ProtocolQueries {
                    db,
                    object: object.clone(),
                    interface,
                    object_keys: &object_keys,
                    members,
                };
                registry.bind_callable(
                    &object,
                    ProtocolObjectProvider {
                        queries: queries.clone(),
                        environments: &environments,
                        builders: &builders,
                        owners: &owners,
                        observations: Some(&observations),
                    },
                )?;
                let environments = &environments;
                let builders = &builders;
                let owners = &owners;
                let observations = &observations;
                let sources = &sources;
                let admission = &admission;
                let journal = &journal;
                let member_recovery = &member_recovery;
                let class_recovery = &class_recovery;
                let place_recovery = &place_recovery;
                let descriptor_recovery = &descriptor_recovery;
                let values = &values;
                let place_keys = &place_keys;
                let descriptor_keys = &descriptor_keys;
                registry.seal()?.run(move |endpoint| async move {
                    if let Some(
                        target @ (MemberRecoveryTarget::Member | MemberRecoveryTarget::Class),
                    ) = target
                    {
                        let key = intern_member_lookup_key_from_str(
                            &endpoint,
                            values,
                            program,
                            prepared.inputs.receiver,
                            "value",
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .await;
                        if target == MemberRecoveryTarget::Member {
                            assert!(
                                member_recovery
                                    .selected
                                    .replace(Some((key.as_id(), key)))
                                    .is_none()
                            );
                            let returned = endpoint
                                .child_call(|| async {
                                    Ok(*endpoint.fetch_ref(&member, key.as_id())?.await?)
                                })
                                .await;
                            panic!(
                                "the member recovery control published a root result: {returned:?}"
                            );
                        } else {
                            assert!(
                                class_recovery
                                    .selected
                                    .replace(Some((key.as_id(), key)))
                                    .is_none()
                            );
                            let returned = endpoint
                                .child_call(|| async {
                                    Ok(*endpoint.fetch_ref(&class, key.as_id())?.await?)
                                })
                                .await;
                            panic!(
                                "the class recovery control published a root result: {returned:?}"
                            );
                        }
                    }
                    if target == Some(MemberRecoveryTarget::Place) {
                        let input = place_input.unwrap();
                        let id = endpoint.intern_query_key(place_keys, input).await;
                        assert!(place_recovery.selected.replace(Some((id, input))).is_none());
                        let returned = endpoint
                            .child_call(|| async { Ok(*endpoint.fetch_ref(&place, id)?.await?) })
                            .await;
                        panic!("the place recovery control published a root result: {returned:?}");
                    }
                    if target == Some(MemberRecoveryTarget::Descriptor) {
                        let id = endpoint
                            .intern_query_key(descriptor_keys, descriptor_input)
                            .await;
                        assert!(
                            descriptor_recovery
                                .selected
                                .replace(Some((id, descriptor_input)))
                                .is_none()
                        );
                        let returned = endpoint
                            .child_call(|| async {
                                Ok(*endpoint.fetch_ref(&descriptor, id)?.await?)
                            })
                            .await;
                        panic!(
                            "the descriptor recovery control published a root result: {returned:?}"
                        );
                    }
                    if control.is_direct_refusal() {
                        let effects = MemberEffects {
                            endpoint: &endpoint,
                            queries: &queries.members,
                            sources,
                            env: db.program_environment(),
                            fields: crate::types::mro::field_reads::MroFieldReads::new(db),
                        };
                        run_member_rejection_control(
                            &endpoint, &effects, control, admission, journal,
                        )
                        .await?;
                        panic!("a rejected operation continued");
                    }
                    let restricted = queries
                        .member_lookup(
                            &endpoint,
                            program,
                            prepared.inputs.receiver,
                            "value",
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .await?;
                    if !full {
                        return Ok(MemberEndpointOutput {
                            restricted,
                            presence: None,
                            missing: None,
                            object_equivalence: None,
                            required_default: None,
                        });
                    }
                    let env = environments.allocate(&endpoint, program).await;
                    let builder = builders.allocate(&endpoint).await;
                    let owner = owners.allocate(&endpoint, env, builder).await;
                    let presence = check_protocol_presence_for_test(
                        db,
                        endpoint.clone(),
                        owner.assignability(TypeVarSet::None),
                        prepared.inputs.receiver,
                        prepared.inputs.protocol,
                        queries.clone(),
                        Some(&observations),
                    )
                    .await?;
                    let missing = queries
                        .member_lookup(
                            &endpoint,
                            program,
                            prepared.inputs.receiver,
                            "missing",
                            MemberLookupPolicy::default(),
                        )
                        .await?;
                    let object_equivalence = queries
                        .object_equivalence(&endpoint, prepared.inputs.protocol)
                        .await?;
                    let required_default = if prepared.declaration.is_none() {
                        Some(
                            queries
                                .member_lookup(
                                    &endpoint,
                                    program,
                                    prepared.inputs.receiver,
                                    "value",
                                    MemberLookupPolicy::default(),
                                )
                                .await?,
                        )
                    } else {
                        None
                    };
                    let reused = queries
                        .member_lookup(
                            &endpoint,
                            program,
                            prepared.inputs.receiver,
                            "value",
                            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        )
                        .await?;
                    assert_restricted_member_result(db, prepared, restricted);
                    assert_restricted_member_result(db, prepared, reused);
                    assert_eq!(restricted, reused);
                    Ok(MemberEndpointOutput {
                        restricted,
                        presence: Some(presence),
                        missing: Some(missing),
                        object_equivalence: Some(object_equivalence),
                        required_default,
                    })
                })
            })();
            execution_error.set(result.as_ref().err().copied());
            result
        })
        .unwrap()
    })
    .unwrap();
    assert_eq!(captured.stamp, prepared_source_probe::Stamp::current(db));
    match (&captured.value, captured.check_root_reads()) {
        (_, Ok(())) => {}
        (AttemptOutcome::Incomplete(_), Err(prepared_source_probe::CaptureError::NoRootReads)) => {}
        (_, result) => panic!("invalid endpoint trace: {result:?}"),
    }
    assert_member_runtime_idle(db);
    if let AttemptOutcome::Incomplete(reason) = &captured.value {
        assert_eq!(execution_error.get(), Some(RunError::Refused(*reason)));
    }
    journal.assert_finished(control);
    let events = reader.take_salsa_events();
    salsa::attach(db, || {
        eprintln!(
            "GATE_A_RESULT {:?}; unsupported={:?}",
            captured.value,
            unsupported.borrow()
        );
        print_member_inventory_reads("gate_a", &preparation, &captured.reads, &events);
    });
    assert_eq!(observations.comparison_scopes.get(), 0);
    if let Some(target) = target {
        recovery.assert_finished();
        let (id, key) = match target {
            MemberRecoveryTarget::Member => {
                let (id, _) = member_recovery.selected.get().unwrap();
                let seed: MemberLookupResult<'_> = Place::bound(Type::divergent(id)).into();
                assert_eq!(member_recovery.initial.get(), Some(seed));
                let value = member_recovery.computed.get().unwrap();
                assert_restricted_member_result(db, prepared, value);
                assert_ne!(value, seed);
                (id, ml.database_key_index(id))
            }
            MemberRecoveryTarget::Class => {
                let id = assert_declared_recovery_values(db, prepared, &class_recovery);
                (id, cl.database_key_index(id))
            }
            MemberRecoveryTarget::Place => {
                let id = assert_declared_recovery_values(db, prepared, &place_recovery);
                (id, pp.database_key_index(id))
            }
            MemberRecoveryTarget::Descriptor => {
                let (id, _) = descriptor_recovery.selected.get().unwrap();
                // The real absent-descriptor seed and result agree, but recovery still runs.
                assert_eq!(descriptor_recovery.initial.get(), Some(Ok(None)));
                assert_eq!(descriptor_recovery.computed.get(), Some(Ok(None)));
                (id, dg.database_key_index(id))
            }
        };
        assert_eq!(recovery.cycle.get().map(|(id, _)| id), Some(id));
        assert!(captured.reads.iter().any(|read| read.key == key
            && read.parent == Some(key)
            && read.status == prepared_source_probe::Status::Provisional));
        assert!(
            captured
                .reads
                .iter()
                .all(|read| read.stamp == captured.stamp
                    && if read.key == key {
                        read.parent == Some(key)
                            && read.status == prepared_source_probe::Status::Provisional
                    } else {
                        read.status == prepared_source_probe::Status::Final
                    })
        );
    } else {
        assert!(
            captured
                .reads
                .iter()
                .all(|read| read.status == prepared_source_probe::Status::Final
                    && read.stamp == prepared_source_probe::Stamp::current(db))
        );
    }
    MemberEndpointRecord {
        outcome: captured.value,
        execution_error: execution_error.get(),
        reads: captured.reads,
        stamp: prepared_source_probe::Stamp::current(db),
        preparation,
        events,
        work: admission.work.into_inner(),
        unsupported: unsupported.into_inner(),
    }
}

#[test]
fn cold_defined_member_providers_complete_presence_missing_and_negative_object()
-> anyhow::Result<()> {
    let db = data_member_database()?;
    let prepared = prepare_member_endpoint(&db, true)?;
    let record = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &record);
    assert!(!record.work.is_empty());
    assert_member_endpoint_executed(&db, &prepared, &record);
    assert!(
        record
            .reads
            .iter()
            .all(|read| read.status == prepared_source_probe::Status::Final)
    );
    let repeated = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &repeated);
    assert_eq!(record.stamp, repeated.stamp);
    assert_same_member_root_address(&db, prepared.inputs, &record, &repeated);
    assert!(
        !repeated
            .events
            .iter()
            .any(|event| matches!(event.kind, salsa::EventKind::WillExecute { .. }))
    );
    let oracle = data_member_database()?;
    assert_ordinary_member_endpoint(&oracle)?;
    Ok(())
}

#[test]
fn cold_member_refusal_retains_missing_target_and_allows_retry() -> anyhow::Result<()> {
    for allowance in [0, 64, 256] {
        let db = data_member_database()?;
        let prepared = prepare_member_endpoint(&db, true)?;
        let refused = run_member_endpoint(&db, &prepared, allowance, false, true);
        assert!(
            matches!(
                refused.outcome,
                AttemptOutcome::Incomplete(Incomplete::Allowance)
            ),
            "{:?}",
            refused.outcome
        );
        assert_member_root_missing(&db, prepared.inputs);
        if allowance == 0 {
            assert!(refused.reads.is_empty());
            assert!(member_execution_keys(&refused).is_empty());
        }
        let complete = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
        assert_completed_member_endpoint(&db, &prepared, &complete);
    }
    let db = data_member_database()?;
    let prepared = prepare_member_endpoint(&db, true)?;
    let refused = run_member_endpoint(&db, &prepared, 1_000_000, false, false);
    assert!(matches!(
        refused.outcome,
        AttemptOutcome::Incomplete(Incomplete::Interrupted)
    ));
    assert_member_root_missing(&db, prepared.inputs);
    let complete = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &complete);
    Ok(())
}

fn absent_member_source<'db, C: Configuration>(
    _ingredient: &'db IngredientImpl<C>,
) -> Option<salsa::execution_probe::FinalSourceRoute<'db, C>> {
    None
}

fn native_member_id(
    db: &TestDb,
    ty: Type<'_>,
    name: &str,
    policy: MemberLookupPolicy,
) -> Option<salsa::Id> {
    let program = db.program_environment().program(db);
    let mut entries = MemberLookupKey::ingredient(db.zalsa())
        .entries(db.zalsa())
        .filter(|entry| {
            let fields = entry.value().fields();
            fields.0 == program && fields.1 == ty && fields.2.as_str() == name && fields.3 == policy
        });
    let id = entries.next().map(|entry| entry.key().key_index());
    assert!(entries.next().is_none());
    id
}

fn assert_member_root_missing(db: &TestDb, inputs: NativeDataMemberInputs<'_>) {
    if let Some(id) = native_member_id(
        db,
        inputs.receiver,
        "value",
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
    ) {
        assert!(matches!(
            FinalSourceMemo::certify(db as &dyn Db, member_lookup_ingredient(db), id),
            Err(FinalSourceError::MissingMemo)
        ));
    }
}

fn member_execution_keys(record: &MemberEndpointRecord<'_>) -> Vec<salsa::DatabaseKeyIndex> {
    record
        .events
        .iter()
        .filter_map(|event| match event.kind {
            salsa::EventKind::WillExecute { database_key } => Some(database_key),
            _ => None,
        })
        .collect()
}

fn assert_member_endpoint_executed(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    record: &MemberEndpointRecord<'_>,
) {
    let executed = member_execution_keys(record);
    let value = native_member_id(
        db,
        prepared.inputs.receiver,
        "value",
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
    )
    .unwrap();
    let pp = place_by_id_ingredient(db);
    let scope = prepared.inputs.classes[0].body_scope(db);
    let symbol = place_table(db, scope).symbol_id("value").unwrap();
    let place = existing_native_query_key(
        db,
        pp,
        &(
            scope,
            symbol.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        ),
    )
    .unwrap();
    let dg = descriptor_get_ingredient(db);
    let program = db.program_environment().program(db);
    let descriptor = existing_native_query_key(
        db,
        dg,
        &(
            program,
            prepared.expected,
            Some(prepared.inputs.receiver),
            prepared.owner,
        ),
    )
    .unwrap();
    let getter = native_member_id(
        db,
        prepared.expected,
        "__get__",
        MemberLookupPolicy::REQUIRE_CONCRETE,
    )
    .unwrap();
    let co = protocol_object_equivalence_ingredient(db);
    let object = native_object_key(db, co, prepared.inputs.protocol).unwrap();
    for key in [
        member_lookup_ingredient(db).database_key_index(value),
        class_member_lookup_ingredient(db).database_key_index(value),
        pp.database_key_index(place),
        dg.database_key_index(descriptor),
        class_member_lookup_ingredient(db).database_key_index(getter),
        protocol_interface_ingredient(db)
            .database_key_index(prepared.inputs.protocol_class.as_id()),
        co.database_key_index(object),
    ] {
        assert!(
            executed.contains(&key),
            "cold target did not execute: {key:?}"
        );
    }
}

fn assert_same_member_root_address(
    db: &TestDb,
    inputs: NativeDataMemberInputs<'_>,
    first: &MemberEndpointRecord<'_>,
    second: &MemberEndpointRecord<'_>,
) {
    let id = native_member_id(
        db,
        inputs.receiver,
        "value",
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
    )
    .unwrap();
    let key = member_lookup_ingredient(db).database_key_index(id);
    let address = |record: &MemberEndpointRecord<'_>| {
        record
            .reads
            .iter()
            .find(|read| read.parent.is_none() && read.key == key)
            .map(|read| read.memo_address)
            .unwrap()
    };
    assert_eq!(address(first), address(second));
}

#[test]
fn edited_declared_member_invalidates_and_recomputes_the_same_lookup() -> anyhow::Result<()> {
    let mut db = data_member_database()?;
    let (old_id, old_key, old_stamp) = {
        let prepared = prepare_member_endpoint(&db, true)?;
        assert!(prepared.expected.is_instance_of(&db, KnownClass::Int));
        let complete = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
        assert_completed_member_endpoint(&db, &prepared, &complete);
        let id = native_member_id(
            &db,
            prepared.inputs.receiver,
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        )
        .unwrap();
        (
            id,
            member_lookup_ingredient(&db).database_key_index(id),
            complete.stamp,
        )
    };
    db.write_file(DATA_MEMBER_PATH,"from typing import Protocol\n\nclass C:\n    value: object\n\nclass P(Protocol):\n    value: int\n\nc: C\np: P\n")?;
    let prepared = prepare_member_endpoint(&db, false)?;
    assert_eq!(prepared.expected, Type::object());
    assert_eq!(
        native_member_id(
            &db,
            prepared.inputs.receiver,
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK
        ),
        Some(old_id)
    );
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, member_lookup_ingredient(&db), old_id),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    let edited = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &edited);
    assert_ne!(edited.stamp, old_stamp);
    assert!(member_execution_keys(&edited).contains(&old_key));
    assert!(!edited.preparation.iter().any(|event| matches!(event.kind,salsa::EventKind::WillExecute { database_key } if database_key==old_key)));
    let warm = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &warm);
    assert_same_member_root_address(&db, prepared.inputs, &edited, &warm);
    assert!(member_execution_keys(&warm).is_empty());
    let mut oracle = data_member_database()?;
    oracle.write_file(DATA_MEMBER_PATH, "from typing import Protocol\n\nclass C:\n    value: object\n\nclass P(Protocol):\n    value: int\n\nc: C\np: P\n")?;
    assert_ordinary_member_endpoint(&oracle)?;
    Ok(())
}

#[test]
fn refused_member_root_reuses_the_completed_class_and_place_queries() -> anyhow::Result<()> {
    let db = data_member_database()?;
    let prepared = prepare_member_endpoint(&db, true)?;
    let stopped = run_member_endpoint_inner(
        &db,
        &prepared,
        1_000_000,
        false,
        true,
        MemberEndpointControl::AfterClass,
    );
    assert!(matches!(
        stopped.outcome,
        AttemptOutcome::Incomplete(Incomplete::Allowance)
    ));
    assert_completed_member_child_reuse(&db, &prepared, &stopped);
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemberRecoveryTarget {
    Member,
    Class,
    Place,
    Descriptor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemberEndpointControl {
    None,
    AfterClass,
    RefuseWithChild,
    NativePanic,
    NoQueries,
    SlotOverflow,
    DunderRead,
    DunderAdmission,
    InstanceAdmission,
    Recovery(MemberRecoveryTarget),
}

impl MemberEndpointControl {
    fn recovery_target(self) -> Option<MemberRecoveryTarget> {
        if let Self::Recovery(target) = self {
            Some(target)
        } else {
            None
        }
    }

    fn is_direct_refusal(self) -> bool {
        matches!(
            self,
            Self::NoQueries
                | Self::SlotOverflow
                | Self::DunderRead
                | Self::DunderAdmission
                | Self::InstanceAdmission
        )
    }
}

#[derive(Default)]
struct MemberOwnershipJournal {
    owner_live: Cell<bool>,
    child_queued: Cell<bool>,
    read_called: Cell<bool>,
    child_started: Cell<bool>,
    panic_identity: std::sync::Arc<()>,
    drops: RefCell<Vec<&'static str>>,
}

impl MemberOwnershipJournal {
    fn assert_finished(&self, control: MemberEndpointControl) {
        assert!(!self.owner_live.get());
        assert!(!self.read_called.get());
        assert!(!self.child_started.get());
        if matches!(
            control,
            MemberEndpointControl::RefuseWithChild | MemberEndpointControl::NativePanic
        ) {
            assert!(self.child_queued.get());
            assert_eq!(&*self.drops.borrow(), &["child", "owner"]);
        } else if control == MemberEndpointControl::DunderRead {
            assert_eq!(&*self.drops.borrow(), &["capture", "owner"]);
        } else if control == MemberEndpointControl::AfterClass || control.is_direct_refusal() {
            assert_eq!(&*self.drops.borrow(), &["owner"]);
        } else {
            assert!(!self.child_queued.get());
            assert!(self.drops.borrow().is_empty());
        }
    }
}

struct MemberOwnerGuard<'a>(&'a MemberOwnershipJournal);
impl<'a> MemberOwnerGuard<'a> {
    fn new(journal: &'a MemberOwnershipJournal) -> Self {
        assert!(!journal.owner_live.replace(true));
        Self(journal)
    }
}
impl Drop for MemberOwnerGuard<'_> {
    fn drop(&mut self) {
        assert!(self.0.owner_live.replace(false));
        self.0.drops.borrow_mut().push("owner");
    }
}
struct MemberQueuedChild<'a>(&'a MemberOwnershipJournal);
impl Drop for MemberQueuedChild<'_> {
    fn drop(&mut self) {
        assert!(
            self.0.owner_live.get(),
            "the refusing member owner must outlive its queued child"
        );
        self.0.drops.borrow_mut().push("child");
    }
}

struct MemberReadCapture<'a> {
    db: &'a dyn Db,
    journal: &'a MemberOwnershipJournal,
}

impl MemberReadCapture<'_> {
    fn read(&self) -> u8 {
        self.journal.read_called.set(true);
        0
    }
}

impl Drop for MemberReadCapture<'_> {
    fn drop(&mut self) {
        assert!(self.journal.owner_live.get());
        assert!(salsa::attempt_probe::is_incomplete(self.db));
        self.journal.drops.borrow_mut().push("capture");
    }
}

struct InterruptedMemberQueries<'run, 'db, Q, S> {
    inner: Q,
    sources: &'run S,
    control: MemberEndpointControl,
    receiver: Type<'db>,
    expected: Type<'db>,
    journal: &'run MemberOwnershipJournal,
}

impl<Q: Clone, S> Clone for InterruptedMemberQueries<'_, '_, Q, S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            sources: self.sources,
            control: self.control,
            receiver: self.receiver,
            expected: self.expected,
            journal: self.journal,
        }
    }
}

impl<'run, 'db: 'run, Q: MemberDependencies<'run, 'db>, S: MemberSourcesAccess<'run, 'db>>
    MemberQueryAccess<'run, 'db> for InterruptedMemberQueries<'run, 'db, Q, S>
{
    const AVAILABLE: bool = Q::AVAILABLE;
    async fn member<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &'call str,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        self.inner.member(endpoint, program, ty, name, policy).await
    }
}

impl<'run, 'db: 'run, Q: MemberDependencies<'run, 'db>, S: MemberSourcesAccess<'run, 'db>>
    MemberDependencies<'run, 'db> for InterruptedMemberQueries<'run, 'db, Q, S>
{
    async fn name_literal(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        name: &str,
    ) -> RunResult<Type<'db>> {
        self.inner.name_literal(endpoint, name).await
    }
    async fn class_lookup(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let result = self
            .inner
            .class_lookup(endpoint, program, ty, name, policy)
            .await?;
        if ty == self.receiver
            && name == "value"
            && policy == MemberLookupPolicy::NO_INSTANCE_FALLBACK
        {
            match self.control {
                MemberEndpointControl::AfterClass => {
                    let _owner = MemberOwnerGuard::new(self.journal);
                    assert_eq!(result.place.expect_type(), self.expected);
                    endpoint
                        .local_call(|| Err::<(), _>(RunError::Refused(Incomplete::Allowance)))
                        .await;
                    panic!("the interrupted member provider resumed");
                }
                MemberEndpointControl::RefuseWithChild | MemberEndpointControl::NativePanic => {
                    let _owner = MemberOwnerGuard::new(self.journal);
                    assert_eq!(result.place.expect_type(), self.expected);
                    let sources = MemberRefusalSources {
                        base: self.sources,
                        endpoint: endpoint.clone(),
                        journal: self.journal,
                        native_panic: self.control == MemberEndpointControl::NativePanic,
                    };
                    let effects = MemberEffects {
                        endpoint,
                        queries: &self.inner,
                        sources: &sources,
                        env: crate::ProgramEnvironment::from_program(program),
                        fields: crate::types::mro::field_reads::MroFieldReads::new(
                            self.sources.db(),
                        ),
                    };
                    crate::types::LookupDescriptorEffects::union(
                        &effects,
                        self.expected,
                        self.expected,
                    )
                    .await?;
                    panic!("the unsupported member operation resumed");
                }
                _ => {}
            }
        }
        Ok(result)
    }
    async fn public_place(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ty_python_core::scope::ScopeId<'db>,
        place: ty_python_core::place::ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.inner
            .public_place(endpoint, scope, place, reexport, definitions)
            .await
    }
    async fn descriptor_get(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        request: crate::types::descriptor::DescriptorRequest<'db>,
    ) -> RunResult<crate::types::descriptor::DescriptorResult<'db>> {
        self.inner.descriptor_get(endpoint, program, request).await
    }
}

struct MemberRefusalSources<'run, 'db: 'run, S> {
    native_panic: bool,
    base: &'run S,
    endpoint: TaskEndpoint<'run, 'db>,
    journal: &'run MemberOwnershipJournal,
}

impl<'run, 'db: 'run, S: MemberSourcesAccess<'run, 'db>> MemberSourcesAccess<'run, 'db>
    for MemberRefusalSources<'run, 'db, S>
{
    fn db(&self) -> &'db dyn Db {
        self.base.db()
    }
    fn record_unsupported(&self, operation: &'static str) {
        self.base.record_unsupported(operation);
        assert_eq!(operation, "union");
        assert!(!self.journal.child_queued.replace(true));
        let child = MemberQueuedChild(self.journal);
        let reply = self.endpoint.demand(move || {
            child.0.child_started.set(true);
            std::future::poll_fn(move |_| -> std::task::Poll<RunResult<()>> {
                let _child = &child;
                panic!("the refusing member callback must drain its child without executing it");
            })
        });
        assert!(
            reply.is_ok(),
            "queue the rejection observer in the active member task"
        );
        if self.native_panic {
            std::panic::panic_any(MemberNativePanic(self.journal.panic_identity.clone()));
        }
    }
    async fn places(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db ty_python_core::PlaceTable> {
        self.base.places(endpoint, scope).await
    }
    async fn uses(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db ty_python_core::UseDefMap<'db>> {
        self.base.uses(endpoint, scope).await
    }
    async fn public_place(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.base
            .public_place(endpoint, scope, place, reexport, definitions)
            .await
    }
    async fn binding_place<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &crate::ProgramEnvironment<'db>,
        bindings: ty_python_core::BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<crate::place::PlaceWithDefinition<'db>> {
        self.base.binding_place(endpoint, env, bindings).await
    }
    async fn declaration_place<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &crate::ProgramEnvironment<'db>,
        declarations: ty_python_core::DeclarationsIterator<'map, 'db>,
    ) -> RunResult<crate::place::PlaceFromDeclarationsResult<'db>> {
        self.base
            .declaration_place(endpoint, env, declarations)
            .await
    }
    async fn imported_final<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &crate::ProgramEnvironment<'db>,
        result: crate::place::PlaceFromDeclarationsResult<'db>,
        imported: ty_python_core::ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> RunResult<crate::place::PlaceFromDeclarationsResult<'db>> {
        self.base
            .imported_final(endpoint, env, result, imported)
            .await
    }
    async fn explicit_bases(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.base.explicit_bases(endpoint, class).await
    }
    async fn context(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::GenericContext<'db>>> {
        self.base.context(endpoint, class).await
    }
    async fn mro_start(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::ClassType<'db>,
    ) -> RunResult<crate::types::mro::iteration::MroCursor<'db>> {
        self.base.mro_start(endpoint, class).await
    }
    async fn mro_next(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        cursor: &mut crate::types::mro::iteration::MroCursor<'db>,
    ) -> RunResult<Option<crate::types::ClassBase<'db>>> {
        self.base.mro_next(endpoint, cursor).await
    }
    async fn known_class(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<Type<'db>> {
        self.base.known_class(endpoint, program, known).await
    }
    async fn known_instance(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<Type<'db>> {
        self.base.known_instance(endpoint, program, known).await
    }
    async fn enum_metadata(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<Option<&'db crate::types::enums::EnumMetadata<'db>>> {
        self.base.enum_metadata(endpoint, class).await
    }
    async fn enum_class(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::enums::EnumClassLiteral<'db>>> {
        self.base.enum_class(endpoint, class).await
    }
    async fn decorators(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        self.base.decorators(endpoint, class).await
    }
    async fn code_generator(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::class::CodeGeneratorKind<'db>>> {
        self.base.code_generator(endpoint, class).await
    }
    async fn implicit_names(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db [ruff_python_ast::name::Name]> {
        self.base.implicit_names(endpoint, scope).await
    }
}

async fn run_member_rejection_control<
    'run,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
>(
    endpoint: &TaskEndpoint<'run, 'db>,
    effects: &MemberEffects<'_, 'run, 'db, Q, S>,
    control: MemberEndpointControl,
    admission: &MemberEndpointAdmission,
    journal: &MemberOwnershipJournal,
) -> RunResult<()> {
    let _owner = MemberOwnerGuard::new(journal);
    assert!(!salsa::attempt_probe::is_incomplete(effects.sources.db()));
    match control {
        MemberEndpointControl::NoQueries => {
            let returned = NoMemberQueries
                .member(
                    endpoint,
                    effects.env.program(effects.sources.db()),
                    Type::object(),
                    "value",
                    MemberLookupPolicy::default(),
                )
                .await;
            panic!("the rejected NoMemberQueries operation returned: {returned:?}");
        }
        MemberEndpointControl::SlotOverflow => {
            let returned = crate::types::class::slots::SlotSelectorEffects::slot_checkpoint(
                effects,
                crate::types::class::slots::SlotSelectorWork::BaseCompare {
                    inline_bytes: usize::MAX,
                },
            )
            .await;
            panic!("the rejected slot operation returned: {returned:?}");
        }
        MemberEndpointControl::DunderRead => {
            let capture = MemberReadCapture {
                db: effects.sources.db(),
                journal,
            };
            let returned = crate::types::call::dunder::DunderEffects::read(
                effects,
                crate::types::call::dunder::DunderRead::UnionElements,
                move || capture.read(),
            )
            .await;
            panic!("the rejected dunder read returned: {returned:?}");
        }
        MemberEndpointControl::DunderAdmission => {
            let returned = crate::types::call::dunder::DunderEffects::admit(
                effects,
                crate::types::call::dunder::DunderWork::MissingUnionElement,
            )
            .await;
            panic!("the rejected dunder admission returned: {returned:?}");
        }
        MemberEndpointControl::InstanceAdmission => {
            admission.refuse_next.set(true);
            let returned = crate::types::instance::effects::InstanceEffects::checkpoint(
                effects,
                crate::types::instance::effects::InstanceWork::Dispatch,
            )
            .await;
            panic!("the rejected instance admission returned: {returned:?}");
        }
        _ => panic!("expected a direct refusal control"),
    }
}

fn assert_undefined_member_result(_db: &TestDb, result: MemberLookupResult<'_>) {
    let Ok(ResolvedMember::Plain(member)) = result else {
        panic!("expected a successful plain undefined member: {result:?}");
    };
    assert_eq!(member, Place::Undefined.into());
    assert!(member.qualifiers.is_empty());
    assert!(member.place.is_undefined());
}

fn assert_restricted_member_result(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    result: MemberLookupResult<'_>,
) {
    let Some(definition) = prepared.declaration else {
        return assert_undefined_member_result(db, result);
    };
    let Ok(ResolvedMember::Plain(member)) = result else {
        panic!("expected a successful plain declared member: {result:?}");
    };
    assert_eq!(member.qualifiers, TypeQualifiers::empty());
    assert_eq!(
        member.place,
        Place::Defined(DefinedPlace {
            ty: prepared.expected,
            origin: TypeOrigin::Declared,
            definedness: Definedness::AlwaysDefined,
            public_type_policy: PublicTypePolicy::Raw,
            provenance: Provenance::SingleDefinition(definition)
        })
    );
}

fn assert_completed_member_endpoint(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    record: &MemberEndpointRecord<'_>,
) {
    let AttemptOutcome::Complete(Ok(result)) = &record.outcome else {
        panic!("{:?}; unsupported={:?}", record.outcome, record.unsupported);
    };
    assert_restricted_member_result(db, prepared, result.restricted);
    assert_eq!(result.presence, Some(prepared.declaration.is_some()));
    assert_undefined_member_result(
        db,
        result.missing.expect("full endpoint missing-name result"),
    );
    assert_eq!(result.object_equivalence, Some(false));
    if prepared.declaration.is_none() {
        assert_undefined_member_result(
            db,
            result
                .required_default
                .expect("default lookup of the missing required name"),
        );
    } else {
        assert!(result.required_default.is_none());
    }
    assert!(record.unsupported.is_empty());
    assert_eq!(record.execution_error, None);
    assert!(!record.reads.is_empty());
    assert!(record.reads.iter().any(|read| read.parent.is_none()));
    assert!(
        record
            .reads
            .iter()
            .all(|read| read.status == prepared_source_probe::Status::Final
                && read.stamp == record.stamp)
    );
}

fn assert_ordinary_member_endpoint(db: &TestDb) -> anyhow::Result<()> {
    let prepared = prepare_member_endpoint(db, false)?;
    let (receiver, protocol) = data_member_operands(db)?;
    assert_eq!(receiver, prepared.inputs.receiver);
    assert_eq!(protocol, prepared.inputs.protocol);
    assert_restricted_member_result(
        db,
        &prepared,
        receiver.member_lookup_with_policy_and_receiver(
            db,
            &db.program_environment(),
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            None,
        ),
    );
    assert_eq!(
        crate::types::protocol_class::has_all_protocol_members_defined(
            db,
            &db.program_environment(),
            receiver,
            protocol
        ),
        prepared.declaration.is_some()
    );
    assert!(!protocol.is_equivalent_to_object(db));
    assert_undefined_member_result(
        db,
        receiver.member_lookup_with_policy_and_receiver(
            db,
            &db.program_environment(),
            "missing",
            MemberLookupPolicy::default(),
            None,
        ),
    );
    if prepared.declaration.is_none() {
        assert_undefined_member_result(
            db,
            receiver.member_lookup_with_policy_and_receiver(
                db,
                &db.program_environment(),
                "value",
                MemberLookupPolicy::default(),
                None,
            ),
        );
    }
    Ok(())
}

#[test]
fn registered_member_sources_sort_actual_scope_keys() -> anyhow::Result<()> {
    let db = data_member_database()?;
    let prepared = prepare_member_endpoint(&db, true)?;
    let ids = prepared
        .classes
        .iter()
        .map(|class| class.body_scope(&db).as_id())
        .rev()
        .collect::<Vec<_>>();
    let repeated = ids.iter().chain(&ids).copied();
    let places = member_sources(
        &db as &dyn ty_python_core::Db,
        place_table_ingredient(&db),
        repeated,
    );
    let uses = member_sources(
        &db as &dyn ty_python_core::Db,
        use_def_map_ingredient(&db),
        ids,
    );
    assert_eq!(
        places
            .iter()
            .map(|memo| memo.database_key().key_index())
            .collect::<Vec<_>>(),
        uses.iter()
            .map(|memo| memo.database_key().key_index())
            .collect::<Vec<_>>()
    );
    assert_native_member_targets_missing(&db, prepared.inputs, "sorted_source_keys");
    assert_member_endpoint_cold(&db, &prepared);
    Ok(())
}

#[test]
fn member_local_rejections_retain_their_callers() -> anyhow::Result<()> {
    for control in [
        MemberEndpointControl::NoQueries,
        MemberEndpointControl::SlotOverflow,
        MemberEndpointControl::DunderRead,
        MemberEndpointControl::DunderAdmission,
        MemberEndpointControl::InstanceAdmission,
    ] {
        let db = data_member_database()?;
        let prepared = prepare_member_endpoint(&db, true)?;
        let refused = run_member_endpoint_inner(&db, &prepared, 1_000_000, false, true, control);
        let expected = if matches!(
            control,
            MemberEndpointControl::SlotOverflow | MemberEndpointControl::InstanceAdmission
        ) {
            Incomplete::Allowance
        } else {
            Incomplete::Interrupted
        };
        assert!(
            matches!(refused.outcome, AttemptOutcome::Incomplete(reason) if reason == expected),
            "{control:?}: {:?}",
            refused.outcome
        );
        assert_member_root_missing(&db, prepared.inputs);
        let retry = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
        assert_completed_member_endpoint(&db, &prepared, &retry);
    }
    Ok(())
}

#[test]
fn registered_member_recovery_refusal_retains_inputs_and_drains_child() -> anyhow::Result<()> {
    assert_registered_member_recovery(MemberRecoveryTarget::Member)
}

#[test]
fn registered_class_recovery_refusal_retains_inputs_and_drains_child() -> anyhow::Result<()> {
    assert_registered_member_recovery(MemberRecoveryTarget::Class)
}

#[test]
fn registered_place_recovery_refusal_retains_inputs_and_drains_child() -> anyhow::Result<()> {
    assert_registered_member_recovery(MemberRecoveryTarget::Place)
}

#[test]
fn registered_descriptor_recovery_refusal_retains_equal_seed_and_drains_child() -> anyhow::Result<()>
{
    assert_registered_member_recovery(MemberRecoveryTarget::Descriptor)
}

fn member_recovery_key_status<C: Configuration>(
    db: &C::DbView,
    ingredient: &IngredientImpl<C>,
    id: salsa::Id,
) -> (salsa::DatabaseKeyIndex, Result<(), FinalSourceError>) {
    (
        ingredient.database_key_index(id),
        FinalSourceMemo::certify(db, ingredient, id).map(|_| ()),
    )
}

fn member_recovery_certification(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    target: MemberRecoveryTarget,
) -> (salsa::DatabaseKeyIndex, Result<(), FinalSourceError>) {
    match target {
        MemberRecoveryTarget::Member | MemberRecoveryTarget::Class => {
            let id = native_member_id(
                db,
                prepared.inputs.receiver,
                "value",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .unwrap();
            if target == MemberRecoveryTarget::Member {
                member_recovery_key_status(db as &dyn Db, member_lookup_ingredient(db), id)
            } else {
                member_recovery_key_status(db as &dyn Db, class_member_lookup_ingredient(db), id)
            }
        }
        MemberRecoveryTarget::Place => {
            let scope = prepared.inputs.classes[0].body_scope(db);
            let symbol = place_table(db, scope).symbol_id("value").unwrap();
            let ingredient = place_by_id_ingredient(db);
            let id = existing_native_query_key(
                db,
                ingredient,
                &(
                    scope,
                    symbol.into(),
                    RequiresExplicitReExport::No,
                    ConsideredDefinitions::EndOfScope,
                ),
            )
            .unwrap();
            member_recovery_key_status(db as &dyn Db, ingredient, id)
        }
        MemberRecoveryTarget::Descriptor => {
            let ingredient = descriptor_get_ingredient(db);
            let id = existing_native_query_key(
                db,
                ingredient,
                &(
                    db.program_environment().program(db),
                    prepared.expected,
                    Some(prepared.inputs.receiver),
                    prepared.owner,
                ),
            )
            .unwrap();
            member_recovery_key_status(db as &dyn Db, ingredient, id)
        }
    }
}

fn assert_registered_member_recovery(target: MemberRecoveryTarget) -> anyhow::Result<()> {
    let db = data_member_database()?;
    let prepared = prepare_member_endpoint(&db, true)?;
    let stopped = run_member_endpoint_inner(
        &db,
        &prepared,
        1_000_000,
        false,
        true,
        MemberEndpointControl::Recovery(target),
    );
    assert!(matches!(
        stopped.outcome,
        AttemptOutcome::Incomplete(Incomplete::Interrupted)
    ));
    assert_eq!(
        stopped.execution_error,
        Some(RunError::Refused(Incomplete::Interrupted))
    );
    assert!(stopped.unsupported.is_empty());
    let (key, certification) = member_recovery_certification(&db, &prepared, target);
    assert!(
        matches!(certification, Err(FinalSourceError::ProvisionalMemo)),
        "the refused recovery must leave its genuine provisional seed unpublished: {certification:?}"
    );
    if target != MemberRecoveryTarget::Member {
        assert_member_root_missing(&db, prepared.inputs);
    }
    assert_member_runtime_idle(&db);
    assert!(member_execution_keys(&stopped).contains(&key));

    // A returned refusal can retry in the same revision; native panic poison has a separate control.
    let retry = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &retry);
    assert_eq!(retry.stamp, stopped.stamp);
    assert!(member_execution_keys(&retry).contains(&key));
    assert_eq!(
        member_recovery_certification(&db, &prepared, target),
        (key, Ok(()))
    );
    let warm = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &warm);
    assert_eq!(warm.stamp, retry.stamp);
    assert_same_member_root_address(&db, prepared.inputs, &retry, &warm);
    assert!(member_execution_keys(&warm).is_empty());
    let oracle = data_member_database()?;
    assert_ordinary_member_endpoint(&oracle)?;
    Ok(())
}

#[test]
fn unsupported_member_operation_drains_queued_child_before_its_owner() -> anyhow::Result<()> {
    let db = data_member_database()?;
    let prepared = prepare_member_endpoint(&db, true)?;
    let stopped = run_member_endpoint_inner(
        &db,
        &prepared,
        1_000_000,
        false,
        true,
        MemberEndpointControl::RefuseWithChild,
    );
    assert!(matches!(
        stopped.outcome,
        AttemptOutcome::Incomplete(Incomplete::Interrupted)
    ));
    assert_eq!(stopped.unsupported, ["union"]);
    assert_member_root_missing(&db, prepared.inputs);
    assert_completed_member_child_reuse(&db, &prepared, &stopped);
    Ok(())
}

const MISSING_REQUIRED_MEMBER_SOURCE: &str = "from typing import Protocol\n\nclass C:\n    pass\n\nclass P(Protocol):\n    value: int\n\nc: C\np: P\n";

#[test]
fn missing_required_member_completes_false_presence_and_default_lookup() -> anyhow::Result<()> {
    let mut db = data_member_database()?;
    db.write_file(DATA_MEMBER_PATH, MISSING_REQUIRED_MEMBER_SOURCE)?;
    let prepared = prepare_member_endpoint(&db, true)?;
    assert!(prepared.declaration.is_none());
    let complete = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &complete);
    assert_missing_required_endpoint_executed(&db, &prepared, &complete);
    let warm = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &warm);
    assert!(member_execution_keys(&warm).is_empty());
    assert_same_member_root_address(&db, prepared.inputs, &complete, &warm);
    let mut oracle = data_member_database()?;
    oracle.write_file(DATA_MEMBER_PATH, MISSING_REQUIRED_MEMBER_SOURCE)?;
    assert_ordinary_member_endpoint(&oracle)?;
    Ok(())
}

#[test]
fn removing_required_declaration_invalidates_positive_member_presence() -> anyhow::Result<()> {
    let mut db = data_member_database()?;
    let (old_id, old_stamp) = {
        let prepared = prepare_member_endpoint(&db, true)?;
        let complete = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
        assert_completed_member_endpoint(&db, &prepared, &complete);
        (
            native_member_id(
                &db,
                prepared.inputs.receiver,
                "value",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
            )
            .unwrap(),
            complete.stamp,
        )
    };
    db.write_file(DATA_MEMBER_PATH, MISSING_REQUIRED_MEMBER_SOURCE)?;
    let prepared = prepare_member_endpoint(&db, false)?;
    assert!(prepared.declaration.is_none());
    assert_eq!(
        native_member_id(
            &db,
            prepared.inputs.receiver,
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK
        ),
        Some(old_id)
    );
    let ingredient = member_lookup_ingredient(&db);
    assert!(matches!(
        FinalSourceMemo::certify(&db as &dyn Db, ingredient, old_id),
        Err(FinalSourceError::UnverifiedMemo)
    ));
    let removed = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &removed);
    assert_ne!(removed.stamp, old_stamp);
    let key = ingredient.database_key_index(old_id);
    assert!(member_execution_keys(&removed).contains(&key));
    assert!(!removed.preparation.iter().any(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key)));
    let warm = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &warm);
    assert!(member_execution_keys(&warm).is_empty());
    assert_same_member_root_address(&db, prepared.inputs, &removed, &warm);
    let mut oracle = data_member_database()?;
    oracle.write_file(DATA_MEMBER_PATH, MISSING_REQUIRED_MEMBER_SOURCE)?;
    assert_ordinary_member_endpoint(&oracle)?;
    Ok(())
}

fn assert_missing_required_endpoint_executed(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    record: &MemberEndpointRecord<'_>,
) {
    let executed = member_execution_keys(record);
    for policy in [
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        MemberLookupPolicy::default(),
    ] {
        let id = native_member_id(db, prepared.inputs.receiver, "value", policy).unwrap();
        assert!(executed.contains(&member_lookup_ingredient(db).database_key_index(id)));
        assert!(executed.contains(&class_member_lookup_ingredient(db).database_key_index(id)));
    }
    let getter = native_member_id(
        db,
        prepared.inputs.receiver,
        "__getattr__",
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
    )
    .unwrap();
    assert!(executed.contains(&member_lookup_ingredient(db).database_key_index(getter)));
    assert!(
        executed.contains(
            &protocol_interface_ingredient(db)
                .database_key_index(prepared.inputs.protocol_class.as_id())
        )
    );
    let co = protocol_object_equivalence_ingredient(db);
    let object = native_object_key(db, co, prepared.inputs.protocol).unwrap();
    assert!(executed.contains(&co.database_key_index(object)));
}

fn assert_completed_member_child_reuse(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    stopped: &MemberEndpointRecord<'_>,
) {
    assert_member_root_missing(db, prepared.inputs);
    let id = native_member_id(
        db,
        prepared.inputs.receiver,
        "value",
        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
    )
    .unwrap();
    let class_key = class_member_lookup_ingredient(db).database_key_index(id);
    assert!(
        FinalSourceMemo::certify(db as &dyn Db, class_member_lookup_ingredient(db), id).is_ok()
    );
    let scope = prepared.inputs.classes[0].body_scope(db);
    let symbol = place_table(db, scope).symbol_id("value").unwrap();
    let pp = place_by_id_ingredient(db);
    let place_id = existing_native_query_key(
        db,
        pp,
        &(
            scope,
            symbol.into(),
            RequiresExplicitReExport::No,
            ConsideredDefinitions::EndOfScope,
        ),
    )
    .unwrap();
    let place_key = pp.database_key_index(place_id);
    assert!(FinalSourceMemo::certify(db as &dyn Db, pp, place_id).is_ok());
    let member_key = member_lookup_ingredient(db).database_key_index(id);
    let executed = member_execution_keys(stopped);
    assert!(
        executed.iter().position(|key| *key == member_key).unwrap()
            < executed.iter().position(|key| *key == class_key).unwrap()
    );
    let descriptor = descriptor_get_ingredient(db);
    if let Some(descriptor_id) = existing_native_query_key(
        db,
        descriptor,
        &(
            db.program_environment().program(db),
            prepared.expected,
            Some(prepared.inputs.receiver),
            prepared.owner,
        ),
    ) {
        assert!(!executed.contains(&descriptor.database_key_index(descriptor_id)));
        assert!(matches!(
            FinalSourceMemo::certify(db as &dyn Db, descriptor, descriptor_id),
            Err(FinalSourceError::MissingMemo)
        ));
    }
    let class_address = stopped
        .reads
        .iter()
        .find(|read| read.key == class_key && read.parent == Some(member_key))
        .unwrap()
        .memo_address;
    assert!(
        stopped
            .reads
            .iter()
            .any(|read| read.key == place_key && read.parent == Some(class_key))
    );
    let place_address = stopped
        .reads
        .iter()
        .find(|read| read.key == place_key)
        .unwrap()
        .memo_address;
    let retry = run_member_endpoint(db, prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(db, prepared, &retry);
    assert!(member_execution_keys(&retry).contains(&member_key));
    assert!(!member_execution_keys(&retry).contains(&class_key));
    assert!(!member_execution_keys(&retry).contains(&place_key));
    assert_eq!(
        retry
            .reads
            .iter()
            .find(|read| read.key == class_key)
            .unwrap()
            .memo_address,
        class_address
    );
    // A cached class read need not revisit PP; when validation does read it, it keeps the memo.
    for read in retry.reads.iter().filter(|read| read.key == place_key) {
        assert_eq!(read.memo_address, place_address);
    }
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let cached_place = prepared_source_probe::capture(db, || {
        pp.fetch(db as &dyn Db, db.zalsa(), db.zalsa_local(), place_id)
    })
    .unwrap();
    assert_eq!(cached_place.check_root_reads(), Ok(()));
    assert_eq!(cached_place.value.place.expect_type(), prepared.expected);
    assert_eq!(
        cached_place
            .reads
            .iter()
            .find(|read| read.key == place_key)
            .unwrap()
            .memo_address,
        place_address
    );
    assert!(cached_place.reads.iter().all(|read| read.status
        == prepared_source_probe::Status::Final
        && read.stamp == retry.stamp));
    assert_no_execution(&mut reader);
}

#[derive(Debug)]
struct MemberNativePanic(std::sync::Arc<()>);

fn assert_member_runtime_idle(db: &TestDb) {
    assert_eq!(
        salsa::attempt_probe::remaining_allowance_for_diagnostics(db),
        None
    );
    assert!(matches!(
        try_with_attempt(db, 0, || ()),
        Ok(AttemptOutcome::Complete(()))
    ));
}

#[test]
fn native_member_panic_preserves_payload_and_drains_child_before_owner() -> anyhow::Result<()> {
    let mut db = data_member_database()?;
    let (id, place_id, old_stamp, old_children) = {
        let prepared = prepare_member_endpoint(&db, true)?;
        let stamp = prepared_source_probe::Stamp::current(&db);
        let journal = MemberOwnershipJournal::default();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_member_endpoint_observed(
                &db,
                &prepared,
                1_000_000,
                false,
                true,
                MemberEndpointControl::NativePanic,
                &journal,
            )
        }));
        let Err(payload) = outcome else {
            panic!("the native payload must unwind from the driver");
        };
        let payload = payload
            .downcast::<MemberNativePanic>()
            .unwrap_or_else(|_| panic!("the original native payload type must survive"));
        assert!(std::sync::Arc::ptr_eq(&payload.0, &journal.panic_identity));
        journal.assert_finished(MemberEndpointControl::NativePanic);
        assert_member_runtime_idle(&db);
        assert_eq!(prepared_source_probe::Stamp::current(&db), stamp);
        let id = native_member_id(
            &db,
            prepared.inputs.receiver,
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        )
        .unwrap();
        let assert_poisoned = || {
            let certification =
                FinalSourceMemo::certify(&db as &dyn Db, member_lookup_ingredient(&db), id);
            assert!(
                matches!(certification, Err(FinalSourceError::ProvisionalMemo)),
                "the native panic must poison the current revision: {certification:?}"
            );
        };
        assert_poisoned();
        let scope = prepared.inputs.classes[0].body_scope(&db);
        let symbol = place_table(&db, scope).symbol_id("value").unwrap();
        let place_id = existing_native_query_key(
            &db,
            place_by_id_ingredient(&db),
            &(
                scope,
                symbol.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            ),
        )
        .unwrap();
        let children = observe_completed_member_children(&db, &prepared, id, place_id);

        // Removing the injector cannot recover a native query poisoned in this revision.
        let mut reader = db.clone();
        reader.clear_salsa_events();
        let retry = salsa::Cancelled::catch(std::panic::AssertUnwindSafe(|| {
            run_member_endpoint(&db, &prepared, 1_000_000, true, true)
        }));
        assert!(matches!(retry, Err(salsa::Cancelled::PropagatedPanic)));
        let member_key = member_lookup_ingredient(&db).database_key_index(id);
        assert!(!reader.take_salsa_events().iter().any(|event| matches!(
            event.kind,
            salsa::EventKind::WillExecute { database_key } if database_key == member_key
        )));
        assert_eq!(prepared_source_probe::Stamp::current(&db), stamp);
        assert_member_runtime_idle(&db);
        assert_poisoned();
        (id, place_id, stamp, children)
    };

    // A real source revision permits recovery while leaving the declarations and their ranges intact.
    db.write_file(DATA_MEMBER_PATH, "from typing import Protocol\n\nclass C:\n    value: int\n\nclass P(Protocol):\n    value: int\n\nc: C\np: P\n# Advance the revision after the native panic.\n")?;
    let prepared = prepare_member_endpoint(&db, false)?;
    assert!(prepared.expected.is_instance_of(&db, KnownClass::Int));
    assert_ne!(prepared_source_probe::Stamp::current(&db), old_stamp);
    assert_eq!(
        native_member_id(
            &db,
            prepared.inputs.receiver,
            "value",
            MemberLookupPolicy::NO_INSTANCE_FALLBACK,
        ),
        Some(id)
    );
    let scope = prepared.inputs.classes[0].body_scope(&db);
    let symbol = place_table(&db, scope).symbol_id("value").unwrap();
    assert_eq!(
        existing_native_query_key(
            &db,
            place_by_id_ingredient(&db),
            &(
                scope,
                symbol.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            ),
        ),
        Some(place_id)
    );
    let certification = FinalSourceMemo::certify(&db as &dyn Db, member_lookup_ingredient(&db), id);
    assert!(
        matches!(certification, Err(FinalSourceError::UnverifiedMemo)),
        "the poisoned memo must be unverified in the new revision: {certification:?}"
    );
    let retry = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &retry);
    assert_ne!(retry.stamp, old_stamp);
    let member_key = member_lookup_ingredient(&db).database_key_index(id);
    let executed = member_execution_keys(&retry);
    assert!(executed.contains(&member_key));
    assert!(!retry.preparation.iter().any(|event| matches!(
        event.kind,
        salsa::EventKind::WillExecute { database_key } if database_key == member_key
    )));
    for old in old_children {
        assert!(!executed.contains(&old.key));
        assert!(!retry.preparation.iter().any(|event| matches!(
            event.kind,
            salsa::EventKind::WillExecute { database_key } if database_key == old.key
        )));
        for read in retry.reads.iter().filter(|read| read.key == old.key) {
            assert_eq!(read.memo_address, old.memo_address);
        }
    }
    let children = observe_completed_member_children(&db, &prepared, id, place_id);
    for (old, current) in old_children.into_iter().zip(children) {
        assert_eq!(current.key, old.key);
        assert_eq!(current.memo_address, old.memo_address);
        assert_eq!(current.stamp, retry.stamp);
    }
    let warm = run_member_endpoint(&db, &prepared, 1_000_000, true, true);
    assert_completed_member_endpoint(&db, &prepared, &warm);
    assert_eq!(warm.stamp, retry.stamp);
    assert_same_member_root_address(&db, prepared.inputs, &retry, &warm);
    assert!(member_execution_keys(&warm).is_empty());
    let oracle = data_member_database()?;
    assert_ordinary_member_endpoint(&oracle)?;
    Ok(())
}

fn observe_completed_member_children(
    db: &TestDb,
    prepared: &PreparedMemberEndpoint<'_>,
    member_id: salsa::Id,
    place_id: salsa::Id,
) -> [prepared_source_probe::Read; 2] {
    let class = class_member_lookup_ingredient(db);
    let place = place_by_id_ingredient(db);
    assert!(FinalSourceMemo::certify(db as &dyn Db, class, member_id).is_ok());
    assert!(FinalSourceMemo::certify(db as &dyn Db, place, place_id).is_ok());
    let mut reader = db.clone();
    reader.clear_salsa_events();
    let captured = prepared_source_probe::capture(db, || {
        [
            *class.fetch(db as &dyn Db, db.zalsa(), db.zalsa_local(), member_id),
            *place.fetch(db as &dyn Db, db.zalsa(), db.zalsa_local(), place_id),
        ]
    })
    .unwrap();
    assert_eq!(captured.check_root_reads(), Ok(()));
    for value in captured.value {
        assert_restricted_member_result(db, prepared, Ok(ResolvedMember::Plain(value)));
    }
    assert!(
        captured
            .reads
            .iter()
            .all(|read| read.status == prepared_source_probe::Status::Final
                && read.stamp == prepared_source_probe::Stamp::current(db))
    );
    assert_no_execution(&mut reader);
    [
        class.database_key_index(member_id),
        place.database_key_index(place_id),
    ]
    .map(|key| {
        *captured
            .reads
            .iter()
            .find(|read| read.key == key && read.parent.is_none())
            .unwrap()
    })
}
