use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::FinalSourceMemo;
use salsa::prepared_source_probe::Status;

use super::*;
use crate::dunder_all::{dunder_all_names, dunder_all_names_ingredient};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::types::infer) enum Mutation {
    Insert,
    Remove,
    Clear,
    Finish,
    Discard,
}

#[derive(Clone, Debug, Default)]
struct Record {
    live_owners: usize,
    created: usize,
    dropped: usize,
    max_frames: usize,
    children: Vec<salsa::Id>,
    before: Vec<(Mutation, usize)>,
    after: Vec<Mutation>,
    initial: Vec<salsa::Id>,
    recovery: Vec<(salsa::Id, u32)>,
    recovered: Vec<salsa::Id>,
    completed: Vec<(salsa::Id, usize)>,
}

thread_local! {
    static RECORD: RefCell<Record> = RefCell::new(Record::default());
    static CANCEL_MUTATION: Cell<Option<Mutation>> = const { Cell::new(None) };
}

fn reset(cancel: Option<Mutation>) {
    assert_eq!(RECORD.with_borrow(|record| record.live_owners), 0);
    RECORD.with_borrow_mut(|record| *record = Record::default());
    CANCEL_MUTATION.set(cancel);
    observations::reset(None);
}

pub(in crate::types::infer) struct OwnerLifetime;

impl OwnerLifetime {
    pub(in crate::types::infer) fn new(_file: ProgramFile<'_>) -> Self {
        RECORD.with_borrow_mut(|record| {
            record.live_owners += 1;
            record.created += 1;
        });
        Self
    }
}

impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        RECORD.with_borrow_mut(|record| {
            record.live_owners -= 1;
            record.dropped += 1;
        });
    }
}

pub(in crate::types::infer) fn frame_depth(_db: &dyn Db, depth: usize) {
    RECORD.with_borrow_mut(|record| record.max_frames = record.max_frames.max(depth));
}

pub(in crate::types::infer) fn before_mutation(db: &dyn Db, mutation: Mutation) {
    if let Some(remaining) = salsa::attempt_probe::remaining_allowance_for_diagnostics(db) {
        RECORD.with_borrow_mut(|record| record.before.push((mutation, remaining)));
    }
    if CANCEL_MUTATION.get() == Some(mutation) {
        CANCEL_MUTATION.set(None);
        db.cancellation_token().cancel();
    }
}

pub(in crate::types::infer) fn after_mutation(_db: &dyn Db, mutation: Mutation) {
    RECORD.with_borrow_mut(|record| record.after.push(mutation));
}

pub(in crate::types::infer) fn child_request(_db: &dyn Db, file: ProgramFile<'_>) {
    RECORD.with_borrow_mut(|record| record.children.push(file.as_id()));
}

pub(in crate::types::infer) fn observe_initial(id: salsa::Id) {
    RECORD.with_borrow_mut(|record| record.initial.push(id));
}

pub(in crate::types::infer) fn observe_recovery(_db: &dyn Db, id: salsa::Id, iteration: u32) {
    RECORD.with_borrow_mut(|record| record.recovery.push((id, iteration)));
}

pub(in crate::types::infer) fn observe_recovered(id: salsa::Id) {
    RECORD.with_borrow_mut(|record| record.recovered.push(id));
}

pub(in crate::types::infer) fn observe_completed(db: &dyn Db, id: salsa::Id) {
    if let Some(remaining) = salsa::attempt_probe::remaining_allowance_for_diagnostics(db) {
        RECORD.with_borrow_mut(|record| record.completed.push((id, remaining)));
    }
}

fn database(source: &str, modules: &[(&str, &str)]) -> TestDb {
    let mut db = setup_db();
    db.write_file("src/main.py", source).unwrap();
    for (path, source) in modules {
        db.write_file(*path, *source).unwrap();
    }
    db
}

fn sorted(names: Option<&FxHashSet<Name>>) -> Option<Vec<String>> {
    names.map(|names| {
        let mut names: Vec<_> = names.iter().map(ToString::to_string).collect();
        names.sort_unstable();
        names
    })
}

fn check(source: &str, modules: &[(&str, &str)], expected: Option<&[&str]>) {
    let db = database(source, modules);
    let prepared = prepare(&db);
    reset(None);
    let cold = capture(&db, || controlled(&prepared, &funded())).unwrap();
    let Ok(AnalysisOutcome::Complete(actual)) = cold.value else {
        panic!("{source}: {:?}", cold.value);
    };
    cold.check_root_reads().unwrap();
    let mut expected = expected.map(|names| {
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    });
    if let Some(expected) = expected.as_mut() {
        expected.sort_unstable();
    }
    assert_eq!(sorted(actual.as_ref()), expected, "{source}");
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            dunder_all_names_ingredient(&db),
            prepared.program_file().as_id()
        )
        .is_ok()
    );
    let ordinary_db = database(source, modules);
    let ordinary_prepared = prepare(&ordinary_db);
    assert_eq!(
        sorted(actual.as_ref()),
        sorted(dunder_all_names(
            &ordinary_db,
            ordinary_prepared.program_file()
        )),
        "{source}"
    );
    assert_eq!(
        RECORD.with_borrow(|record| record.created),
        RECORD.with_borrow(|record| record.dropped)
    );
    assert_no_active_attempt();
}

fn controlled<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    policy: &AnalysisPolicy,
) -> Result<AnalysisOutcome<&'db Option<FxHashSet<Name>>>, AnalysisFailure> {
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
        let weak = Rc::downgrade(&routes);
        let query_routes = Rc::clone(&routes);
        let values = &values;
        let result = catch_unwind(AssertUnwindSafe(|| {
            run.run(|endpoint| async move {
                let access = SourceQueryAccess {
                    session,
                    endpoint,
                    routes: query_routes,
                    values,
                };
                access.dunder_all_names(prepared.program_file()).await
            })
        }));
        assert_eq!(observations::counts().0, 0);
        assert_eq!(RECORD.with_borrow(|record| record.live_owners), 0);
        assert_eq!(Rc::strong_count(&routes), 1);
        drop(routes);
        assert!(weak.upgrade().is_none());
        match result {
            Ok(result) => result,
            Err(payload) => resume_unwind(payload),
        }
    })
}

#[test]
fn cold_export_collection_preserves_supported_updates_and_invalidity() {
    for (source, expected) in [
        ("pass\n", None),
        ("__all__ = []\n", Some(&[][..])),
        (
            "__all__ = ['a', 'lo' 'ng', 'a']\n",
            Some(&["a", "long"][..]),
        ),
        ("__all__ = ('a',)\n", Some(&["a"][..])),
        ("__all__: list[str] = ['a']\n", Some(&["a"][..])),
        ("__all__: list[str]\n", None),
        (
            "__all__ += ['ignored']\n__all__.append('ignored')\n__all__ = ['a']\n",
            Some(&["a"][..]),
        ),
        ("__all__ = ['old']\n__all__ = ['new']\n", Some(&["new"][..])),
        (
            "__all__ = ['a']\n__all__ += ('b',)\n__all__.extend({'c'})\n__all__.append('d')\n__all__.remove('b')\n__all__.remove('absent')\n",
            Some(&["a", "c", "d"][..]),
        ),
        ("__all__ = ['a']\n__all__ *= 2\n", Some(&["a"][..])),
        ("__all__ = other = ['a']\n", None),
        ("__all__ = {'a'}\n", None),
        ("__all__ = ['a', 1]\n", None),
        ("__all__ = [1]\n__all__ = ['a']\n", None),
        ("__all__ = []\n__all__.append(name='a')\n", None),
        ("__all__ = []\n__all__.extend(['a'], ['b'])\n", None),
        ("__all__ = []\n__all__.unknown('a')\n", None),
        ("__all__ = []\n__all__.remove(1)\n", None),
    ] {
        check(source, &[], expected);
    }
}

#[test]
fn export_collection_visits_compound_statements_in_source_order() {
    // Only `if` branches use inferred truthiness. Other control-flow statements contribute their
    // bodies syntactically in source order; nested function and class bodies are skipped.
    let source = "\
__all__ = []
for item in ():
    __all__.append('for')
else:
    __all__.append('for_else')
while False:
    __all__.append('while')
else:
    __all__.append('while_else')
with missing:
    __all__.append('with')
match missing:
    case 1:
        __all__.append('match_first')
    case _:
        __all__.remove('match_first')
        __all__.append('match_last')
try:
    __all__.append('try')
except Exception:
    __all__.remove('try')
    __all__.append('except')
else:
    __all__.append('try_else')
finally:
    __all__.append('finally')
def nested():
    __all__ = [1]
class Nested:
    __all__ = [1]
";
    check(
        source,
        &[],
        Some(&[
            "for",
            "for_else",
            "while",
            "while_else",
            "with",
            "match_last",
            "except",
            "try_else",
            "finally",
        ]),
    );
}

#[test]
fn export_collection_uses_canonical_condition_expression_results() {
    check(
        "\
__all__ = []
if True:
    __all__.append('true')
else:
    __all__ = [1]
if False:
    __all__ = [1]
elif False:
    __all__ = [1]
elif True:
    __all__.append('elif')
else:
    __all__ = [1]
if False:
    __all__ = [1]
else:
    __all__.append('else')
",
        &[],
        Some(&["true", "elif", "else"]),
    );
    check(
        "\
def condition(): ...
__all__ = ['retained']
if condition():
    __all__ = [1]
else:
    __all__ = [1]
if False:
    __all__ = [1]
elif condition():
    __all__ = [1]
else:
    __all__ = [1]
",
        &[],
        Some(&["retained"]),
    );
}

#[test]
fn imported_export_lists_preserve_origin_replacement_and_none() {
    let modules = [
        ("src/dependency.py", "__all__ = ['exported']\n"),
        ("src/listed.py", "__all__ = ['__all__', 'listed']\n"),
        ("src/absent.py", "pass\n"),
        ("src/package/__init__.py", "__all__ = ['package']\n"),
        (
            "src/package/names.py",
            "from . import __all__\n__all__.append('relative')\n",
        ),
    ];
    for (source, expected) in [
        ("from dependency import __all__\n", Some(&["exported"][..])),
        (
            "from dependency import __all__ as __all__\n",
            Some(&["exported"][..]),
        ),
        ("from dependency import __all__ as other\n", None),
        (
            "__all__ = ['old']\nfrom dependency import __all__\n",
            Some(&["exported"][..]),
        ),
        (
            "from dependency import __all__\n__all__ = ['new']\n",
            Some(&["new"][..]),
        ),
        (
            "__all__ = ['old']\nfrom dependency import *\n",
            Some(&["old"][..]),
        ),
        (
            "__all__ = ['old']\nfrom listed import *\n",
            Some(&["__all__", "listed"][..]),
        ),
        ("from absent import __all__\n__all__ = ['new']\n", None),
        ("from absent import *\n__all__ = ['new']\n", None),
        (
            "from package.names import __all__\n",
            Some(&["package", "relative"][..]),
        ),
        (
            "import dependency\n__all__ = []\n__all__ += dependency.__all__\n",
            Some(&["exported"][..]),
        ),
        (
            "import dependency\n__all__ = []\n__all__.extend(dependency.__all__)\n",
            Some(&["exported"][..]),
        ),
    ] {
        check(source, &modules, expected);
    }
}

#[test]
fn imported_export_queries_record_dependencies_and_reuse_canonical_memos() {
    let db = database(
        "from dependency import __all__\n",
        &[("src/dependency.py", "__all__ = ['exported']\n")],
    );
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    reset(None);
    let cold = capture(&db, || controlled(&prepared, &funded())).unwrap();
    let Ok(AnalysisOutcome::Complete(actual)) = cold.value else {
        panic!("{:?}", cold.value)
    };
    cold.check_root_reads().unwrap();
    let ingredient = dunder_all_names_ingredient(&db);
    let root_key = ingredient.database_key_index(prepared.program_file().as_id());
    let root = cold
        .reads
        .iter()
        .find(|read| read.key == root_key && read.parent.is_none())
        .unwrap();
    let child = cold
        .reads
        .iter()
        .find(|read| {
            read.parent == Some(root_key)
                && db.ingredient_debug_name(read.key.ingredient_index()) == "dunder_all_names"
        })
        .unwrap();
    assert_ne!(root.key, child.key);
    assert!(FinalSourceMemo::certify(&db as &dyn Db, ingredient, child.key.key_index()).is_ok());
    let mut reader = db.clone();
    reader.take_salsa_events();
    let native = capture(&db, || dunder_all_names(&db, prepared.program_file())).unwrap();
    assert!(std::ptr::eq(
        native.value.unwrap(),
        actual.as_ref().unwrap()
    ));
    let reused = native
        .reads
        .iter()
        .find(|read| read.key == root_key && read.parent.is_none())
        .unwrap();
    assert_eq!(reused.status, Status::Final);
    assert_eq!(reused.memo_address, root.memo_address);
    assert_eq!(reused.stamp, root.stamp);
    assert_eq!(controlled(&prepared, &funded()), cold.value);
    assert_function_query_was_not_run_by_name(
        &db,
        "dunder_all_names",
        None,
        &reader.take_salsa_events(),
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn cyclic_export_imports_use_the_canonical_none_initializer_and_recovery() {
    // Importing the cycle's `None` initializer invalidates collection. Invalidity is sticky,
    // so the later literal assignments cannot restore a valid export list.
    let source = "from dependency import __all__\n__all__ = ['main']\n";
    let modules = [(
        "src/dependency.py",
        "from main import __all__\n__all__ = ['dependency']\n",
    )];
    let db = database(source, &modules);
    let prepared = prepare(&db);
    reset(None);
    let cold = capture(&db, || controlled(&prepared, &funded())).unwrap();
    assert_eq!(cold.value, Ok(AnalysisOutcome::Complete(&None)));
    cold.check_root_reads().unwrap();
    let record = RECORD.with_borrow(Clone::clone);
    assert!(!record.initial.is_empty(), "{record:?}");
    assert!(!record.recovery.is_empty(), "{record:?}");
    assert_eq!(record.recovered.len(), record.recovery.len());
    assert_eq!(record.created, record.dropped);
    let ordinary_db = database(source, &modules);
    let ordinary_prepared = prepare(&ordinary_db);
    assert_eq!(
        dunder_all_names(&ordinary_db, ordinary_prepared.program_file()),
        None
    );
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            dunder_all_names_ingredient(&db),
            prepared.program_file().as_id()
        )
        .is_ok()
    );
    reset(None);
    assert_eq!(controlled(&prepared, &funded()), cold.value);
    assert!(RECORD.with_borrow(|record| record.recovery.is_empty()));
    assert_no_active_attempt();
}

#[test]
fn export_mutation_work_refusal_cleans_up_before_same_revision_retry() {
    let source = "__all__ = ['a', 'long_name']\n__all__.remove('a')\n__all__ = ['replacement']\n";
    let measured = database(source, &[]);
    let prepared = prepare(&measured);
    reset(None);
    assert!(matches!(
        controlled(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let record = RECORD.with_borrow(Clone::clone);
    for mutation in [
        Mutation::Insert,
        Mutation::Remove,
        Mutation::Clear,
        Mutation::Finish,
    ] {
        let position = record
            .before
            .iter()
            .position(|(kind, _)| *kind == mutation)
            .unwrap();
        let remaining = record.before[position].1;
        let db = database(source, &[]);
        let prepared = prepare(&db);
        let revision = salsa::plumbing::current_revision(&db);
        reset(None);
        let policy = AnalysisPolicy {
            semantic_work_limit: funded().semantic_work_limit - remaining,
            ..funded()
        };
        assert_eq!(
            controlled(&prepared, &policy),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
            "{mutation:?}"
        );
        let interrupted = RECORD.with_borrow(Clone::clone);
        assert_eq!(interrupted.after, record.after[..position], "{mutation:?}");
        assert_eq!(interrupted.created, interrupted.dropped);
        assert!(
            FinalSourceMemo::certify(
                &db as &dyn Db,
                dunder_all_names_ingredient(&db),
                prepared.program_file().as_id()
            )
            .is_err()
        );
        assert_no_active_attempt();
        reset(None);
        let Ok(AnalysisOutcome::Complete(actual)) = controlled(&prepared, &funded()) else {
            panic!("retry at {mutation:?}")
        };
        assert_eq!(sorted(actual.as_ref()), Some(vec!["replacement".into()]));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
        assert_no_active_attempt();
    }
}

#[test]
fn native_cancellation_during_export_collection_preserves_payload_and_retry() {
    let source = "from dependency import __all__\n__all__.append('main')\n";
    let modules = [("src/dependency.py", "__all__ = ['dependency']\n")];
    let db = database(source, &modules);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    reset(Some(Mutation::Insert));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(|| controlled(&prepared, &funded())));
    assert!(matches!(result, Err(salsa::Cancelled::Local)), "{result:?}");
    assert_eq!(CANCEL_MUTATION.get(), None);
    assert_eq!(
        RECORD.with_borrow(|record| record.created),
        RECORD.with_borrow(|record| record.dropped)
    );
    assert_no_active_attempt();
    reset(None);
    let Ok(AnalysisOutcome::Complete(actual)) = controlled(&prepared, &funded()) else {
        panic!("export retry")
    };
    assert_eq!(
        sorted(actual.as_ref()),
        Some(vec!["dependency".into(), "main".into()])
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn export_set_allocation_refusal_precedes_insertion_and_allows_retry() {
    let source = "__all__ = ['name']\n";
    let mut lower = 0;
    let mut upper = funded().requested_bytes_limit;
    while lower + 1 < upper {
        let middle = lower + (upper - lower) / 2;
        let db = database(source, &[]);
        let prepared = prepare(&db);
        reset(None);
        let policy = AnalysisPolicy {
            requested_bytes_limit: middle,
            ..funded()
        };
        let result = controlled(&prepared, &policy);
        if RECORD.with_borrow(|record| record.after.contains(&Mutation::Insert)) {
            upper = middle;
        } else {
            assert_eq!(
                result,
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    completed: ()
                })
            );
            lower = middle;
        }
    }
    let db = database(source, &[]);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    reset(None);
    let policy = AnalysisPolicy {
        requested_bytes_limit: upper - 1,
        ..funded()
    };
    assert_eq!(
        controlled(&prepared, &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::RequestedAllocationLimit,
            completed: ()
        })
    );
    let record = RECORD.with_borrow(Clone::clone);
    assert!(
        record
            .before
            .iter()
            .any(|(kind, _)| *kind == Mutation::Insert)
    );
    assert!(!record.after.contains(&Mutation::Insert));
    assert_eq!(record.created, record.dropped);
    assert_no_active_attempt();
    reset(None);
    let Ok(AnalysisOutcome::Complete(actual)) = controlled(&prepared, &funded()) else {
        panic!("allocation retry")
    };
    assert_eq!(sorted(actual.as_ref()), Some(vec!["name".into()]));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn unfinished_export_publication_does_not_publish_a_partial_memo() {
    let source = "__all__ = ['name']\n";
    let measured = database(source, &[]);
    let prepared = prepare(&measured);
    reset(None);
    assert!(matches!(
        controlled(&prepared, &funded()),
        Ok(AnalysisOutcome::Complete(_))
    ));
    let remaining = RECORD.with_borrow(|record| record.completed.last().unwrap().1);
    let db = database(source, &[]);
    let prepared = prepare(&db);
    let revision = salsa::plumbing::current_revision(&db);
    reset(None);
    let policy = AnalysisPolicy {
        semantic_work_limit: funded().semantic_work_limit - remaining,
        ..funded()
    };
    assert_eq!(
        controlled(&prepared, &policy),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        })
    );
    assert_eq!(RECORD.with_borrow(|record| record.completed.len()), 1);
    assert!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            dunder_all_names_ingredient(&db),
            prepared.program_file().as_id()
        )
        .is_err()
    );
    assert_no_active_attempt();
    reset(None);
    let Ok(AnalysisOutcome::Complete(actual)) = controlled(&prepared, &funded()) else {
        panic!("publication retry")
    };
    assert_eq!(sorted(actual.as_ref()), Some(vec!["name".into()]));
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

#[test]
fn adjacent_export_literals_admit_growth_and_share_expression_materialization() {
    let first = "a".repeat(1024);
    let second = "b".repeat(1024);
    let expected = format!("{first}{second}c");
    let adjacent = format!("'{first}' '{second}' 'c'");
    let single_source = format!("__all__ = ['{expected}']\n");
    let adjacent_source = format!("__all__ = [{adjacent}]\n");
    let minimum_bytes = |source: &str| {
        let mut lower = 0;
        let mut upper = funded().requested_bytes_limit;
        while lower + 1 < upper {
            let middle = lower + (upper - lower) / 2;
            let db = database(source, &[]);
            let prepared = prepare(&db);
            reset(None);
            let policy = AnalysisPolicy {
                requested_bytes_limit: middle,
                ..funded()
            };
            match controlled(&prepared, &policy) {
                Ok(AnalysisOutcome::Complete(actual)) => {
                    assert_eq!(sorted(actual.as_ref()), Some(vec![expected.clone()]));
                    upper = middle;
                }
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::RequestedAllocationLimit,
                    ..
                }) => lower = middle,
                result => panic!("{result:?}"),
            }
        }
        upper
    };
    // Joining these parts grows the String through 1024, 2048 and 4096-byte buffers,
    // then shrinks it to 2049 bytes. The single literal needs none of those allocations.
    assert!(
        minimum_bytes(&adjacent_source) - minimum_bytes(&single_source)
            >= 1024 + 2048 + 4096 + 2049
    );
    check(&adjacent_source, &[], Some(&[&expected]));

    let db = database(&format!("left = right = {adjacent}\n"), &[]);
    let prepared = prepare(&db);
    reset(None);
    let cold = capture(&db, || {
        expression_type_with_policy(&prepared, expression_key(&prepared), &funded())
    })
    .unwrap();
    assert_eq!(
        cold.value,
        Ok(AnalysisOutcome::Complete(Type::string_literal(
            &db,
            expected.as_str()
        )))
    );
    cold.check_root_reads().unwrap();
    assert_no_active_attempt();
}
