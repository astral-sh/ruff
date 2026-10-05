//! Exercises the canonical override queries after ordinary source and class preparation.
//! Initial requests check that their target memos are cold before controlled execution; these tests do
//! not establish cold completion of an entire file or of every override check.

use std::cell::RefCell;

use ruff_python_ast::name::Name;
use salsa::execution_probe::QueryKeyProfile;
use salsa::plumbing::ZalsaDatabase;
use salsa::plumbing::function::{IngredientImpl, InternedQueryConfiguration};
use salsa::prepared_source_probe::{Read, Status};
use ty_python_core::scope::NodeWithScopeRef;
use ty_python_core::symbol::ScopedSymbolId;

use super::nominal_members::{MemberOperation, controlled_member_operation};
use super::*;
use crate::types::overrides::runtime::{
    EffectiveVariableKindConfiguration, EffectiveVariableKindProfile,
};
use crate::types::overrides::{
    VariableKind, effective_superclass_variable_kind,
    effective_superclass_variable_kind_ingredient, is_function_definition,
    is_function_definition_ingredient,
};

struct EffectiveRequest<'db, 'name> {
    class: ClassType<'db>,
    name: &'name Name,
    remaining_work: Option<usize>,
}

impl<'db> MemberOperation<'db> for EffectiveRequest<'db, '_> {
    type Output = Option<VariableKind>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        if let Some(remaining_work) = self.remaining_work {
            let endpoint = access.endpoint();
            endpoint
                .local_call(|| {
                    let remaining =
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(access.db())
                            .unwrap();
                    endpoint.admit_work(remaining.checked_sub(remaining_work).unwrap())
                })
                .await;
        }
        access.effective_variable_kind(self.class, self.name).await
    }
}

#[derive(Clone, Copy)]
struct FunctionRequest<'db>(ScopeId<'db>, ScopedSymbolId);

impl<'db> MemberOperation<'db> for FunctionRequest<'db> {
    type Output = bool;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        _program: Program<'db>,
    ) -> RunResult<bool>
    where
        'db: 'run,
    {
        access.is_function_definition(self.0, self.1).await
    }
}

fn database(source: &str) -> TestDb {
    TestDbBuilder::new()
        .with_file("src/main.py", source)
        .with_salsa_event_callback(query_entry)
        .build()
        .unwrap()
}

fn class_node<'a>(prepared: &'a PreparedAnalysisFile<'_>, name: &str) -> &'a ast::StmtClassDef {
    prepared
        .parsed_module()
        .syntax()
        .body
        .iter()
        .filter_map(Stmt::as_class_def_stmt)
        .find(|class| class.name.as_str() == name)
        .unwrap()
}

/// Obtains the ordinary class identity and rejects setup that has evaluated either target query.
fn ordinary_class<'db>(
    db: &'db TestDb,
    prepared: &PreparedAnalysisFile<'db>,
    name: &str,
) -> ClassType<'db> {
    let definition = prepared
        .semantic_index()
        .expect_single_definition(class_node(prepared, name));
    let class = infer_definition_types(db, definition)
        .original_class_type(definition)
        .unwrap();
    assert_no_memos(db, effective_superclass_variable_kind_ingredient(db));
    assert_no_memos(db, is_function_definition_ingredient(db));
    ClassType::NonGeneric(class)
}

fn function_input<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    class: &str,
    name: &str,
) -> FunctionRequest<'db> {
    let index = prepared.semantic_index();
    let scope = index.node_scope(NodeWithScopeRef::Class(class_node(prepared, class)));
    FunctionRequest(
        index.scope_id(scope),
        index.place_table(scope).symbol_id(name).unwrap(),
    )
}

fn existing_key<'db, C: InternedQueryConfiguration>(
    db: &'db TestDb,
    ingredient: &IngredientImpl<C>,
    input: &C::Input<'db>,
) -> Option<salsa::DatabaseKeyIndex> {
    let arguments = C::argument_ingredient(db.zalsa());
    let mut entries = arguments
        .entries(db.zalsa())
        .filter(|entry| entry.value().fields() == input);
    let key = entries
        .next()
        .map(|entry| ingredient.database_key_index(entry.key().key_index()));
    assert!(
        entries.next().is_none(),
        "duplicate canonical argument tuple"
    );
    key
}

fn assert_no_memos<C>(db: &TestDb, ingredient: &IngredientImpl<C>)
where
    C: InternedQueryConfiguration + Configuration<DbView = dyn Db>,
{
    for entry in C::argument_ingredient(db.zalsa()).entries(db.zalsa()) {
        assert_eq!(
            FinalSourceMemo::certify(db as &dyn Db, ingredient, entry.key().key_index())
                .map(|_| ()),
            Err(FinalSourceError::MissingMemo),
            "target query already has a memo",
        );
    }
}

fn assert_read(reads: &[Read], stamp: Stamp, key: salsa::DatabaseKeyIndex) -> Read {
    let read = *reads.iter().find(|read| read.key == key).unwrap();
    assert_eq!(read.status, Status::Final);
    assert_eq!(read.stamp, stamp);
    read
}

fn execution_count(events: &[salsa::Event], key: salsa::DatabaseKeyIndex) -> usize {
    events
        .iter()
        .filter(|event| matches!(event.kind, salsa::EventKind::WillExecute { database_key } if database_key == key))
        .count()
}

fn controlled_kind<'db>(
    prepared: &PreparedAnalysisFile<'db>,
    class: ClassType<'db>,
    name: &Name,
) -> Result<AnalysisOutcome<Option<VariableKind>>, AnalysisFailure> {
    controlled_member_operation(
        prepared,
        EffectiveRequest {
            class,
            name,
            remaining_work: None,
        },
        &funded(),
    )
}

/// Inline and separately allocated equal names use the ordinary generated argument key and memo.
#[test]
fn effective_kind_reuses_canonical_inline_and_heap_names() {
    for name in [Name::new_inline("field").unwrap(), Name::new_heap("field")] {
        let db = database("class Product:\n    field: bool\n");
        let prepared = prepare(&db);
        let class = ordinary_class(&db, &prepared, "Product");
        let ingredient = effective_superclass_variable_kind_ingredient(&db);
        assert!(existing_key(&db, ingredient, &(class, name.clone())).is_none());
        let mut events_db = db.clone();
        events_db.take_salsa_events();

        let cold = capture(&db, || controlled_kind(&prepared, class, &name)).unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(Some(VariableKind::Instance)))
        );
        let key = existing_key(&db, ingredient, &(class, name.clone())).unwrap();
        let first = assert_read(&cold.reads, cold.stamp, key);
        assert_eq!(execution_count(&events_db.take_salsa_events(), key), 1);
        let boolean = function_input(&prepared, "Product", "field");
        let boolean_key = existing_key(
            &db,
            is_function_definition_ingredient(&db),
            &(boolean.0, boolean.1),
        )
        .unwrap();
        let boolean_read = assert_read(&cold.reads, cold.stamp, boolean_key);
        assert_eq!(boolean_read.parent, Some(key));

        let equal_name = Name::new_heap("field");
        let hit = capture(&db, || controlled_kind(&prepared, class, &equal_name)).unwrap();
        assert_eq!(hit.value, cold.value);
        assert_eq!(
            existing_key(&db, ingredient, &(class, equal_name.clone())),
            Some(key)
        );
        assert_eq!(
            assert_read(&hit.reads, hit.stamp, key).memo_address,
            first.memo_address
        );
        let ordinary = capture(&db, || {
            effective_superclass_variable_kind(&db, class, equal_name)
        })
        .unwrap();
        assert_eq!(ordinary.value, Some(VariableKind::Instance));
        assert_eq!(
            assert_read(&ordinary.reads, ordinary.stamp, key).memo_address,
            first.memo_address
        );
        assert_eq!(execution_count(&events_db.take_salsa_events(), key), 0);
        assert_no_active_attempt();
    }
}

/// Classification preserves inherited ClassVar assignments and forward MRO order.
/// Methods, properties, Final attributes, and descriptor values are excluded as own declarations,
/// so classification still falls back to the bases.
#[test]
fn effective_kind_preserves_classification_and_forward_mro() {
    for (source, expected) in [
        (
            "from typing import ClassVar\nclass Base:\n    field: ClassVar[bool]\nclass Product(Base):\n    field = True\n",
            Some(VariableKind::Class),
        ),
        (
            "from typing import ClassVar\nclass Base:\n    field: ClassVar[bool]\nclass Product(Base):\n    field: bool\n",
            Some(VariableKind::Instance),
        ),
        ("class Product:\n    def field(self):\n        pass\n", None),
        (
            "class Product:\n    @property\n    def field(self) -> bool:\n        return True\n",
            None,
        ),
        (
            "from typing import Final\nclass Product:\n    field: Final[bool] = True\n",
            None,
        ),
        (
            "from typing import ClassVar\nclass Base:\n    field: ClassVar[bool]\nclass Product(Base):\n    def field(self):\n        pass\n",
            Some(VariableKind::Class),
        ),
        (
            "from typing import ClassVar, Final\nclass Base:\n    field: ClassVar[bool]\nclass Product(Base):\n    field: Final[bool] = True\n",
            Some(VariableKind::Class),
        ),
        (
            "from typing import ClassVar\nclass First:\n    field: ClassVar[bool]\nclass Second:\n    field: bool\nclass Product(First, Second):\n    pass\n",
            Some(VariableKind::Class),
        ),
        (
            "from typing import ClassVar\nclass First:\n    field: ClassVar[bool]\nclass Second:\n    field: bool\nclass Product(Second, First):\n    pass\n",
            Some(VariableKind::Instance),
        ),
        (
            "class Descriptor:\n    def __get__(self, instance, owner):\n        return True\nclass Product:\n    field = Descriptor()\n",
            None,
        ),
        (
            "class Descriptor:\n    def __get__(self, instance, owner):\n        return True\nclass Product:\n    field: Descriptor\n",
            Some(VariableKind::Instance),
        ),
    ] {
        let db = database(source);
        let prepared = prepare(&db);
        let class = ordinary_class(&db, &prepared, "Product");
        let name = Name::new_inline("field").unwrap();
        assert_eq!(
            controlled_kind(&prepared, class, &name),
            Ok(AnalysisOutcome::Complete(expected)),
            "{source}"
        );
        assert_eq!(
            effective_superclass_variable_kind(&db, class, name),
            expected,
            "{source}"
        );
        assert_no_active_attempt();
    }
}

/// Methods and properties are distinguished from variable declarations; one conditional function binding suffices.
/// Controlled and ordinary requests reuse the canonical key and memo.
#[test]
fn function_definition_preserves_bindings_and_canonical_memos() {
    for (body, expected) in [
        ("def member(self):\n        pass", true),
        (
            "@property\n    def member(self):\n        return True",
            true,
        ),
        ("member = True", false),
        ("member: bool", false),
        (
            "if unknown:\n        def member(self):\n            pass\n    else:\n        member = True",
            true,
        ),
    ] {
        let db = database(&format!("class Product:\n    {body}\n"));
        let prepared = prepare(&db);
        let input = function_input(&prepared, "Product", "member");
        let ingredient = is_function_definition_ingredient(&db);
        assert!(existing_key(&db, ingredient, &(input.0, input.1)).is_none());
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let cold = capture(&db, || {
            controlled_member_operation(&prepared, input, &funded())
        })
        .unwrap();
        assert_eq!(
            cold.value,
            Ok(AnalysisOutcome::Complete(expected)),
            "{body}"
        );
        let key = existing_key(&db, ingredient, &(input.0, input.1)).unwrap();
        let first = assert_read(&cold.reads, cold.stamp, key);
        assert_eq!(execution_count(&events_db.take_salsa_events(), key), 1);
        let hit = capture(&db, || {
            controlled_member_operation(&prepared, input, &funded())
        })
        .unwrap();
        assert_eq!(hit.value, cold.value);
        assert_eq!(
            assert_read(&hit.reads, hit.stamp, key).memo_address,
            first.memo_address
        );
        let ordinary = capture(&db, || is_function_definition(&db, input.0, input.1)).unwrap();
        assert_eq!(ordinary.value, expected);
        assert_eq!(
            assert_read(&ordinary.reads, ordinary.stamp, key).memo_address,
            first.memo_address
        );
        assert_eq!(execution_count(&events_db.take_salsa_events(), key), 0);
    }
}

struct EntryObservation {
    db: TestDb,
    ingredient: salsa::IngredientIndex,
    key: Option<salsa::DatabaseKeyIndex>,
    entries: usize,
    exhaust_work: bool,
}

thread_local! {
    static ENTRY: RefCell<Option<EntryObservation>> = const { RefCell::new(None) };
}

struct EntryRecording;

impl EntryRecording {
    fn start(db: &TestDb, ingredient: salsa::IngredientIndex) -> Self {
        ENTRY.with_borrow_mut(|entry| {
            assert!(entry.is_none());
            *entry = Some(EntryObservation {
                db: db.clone(),
                ingredient,
                key: None,
                entries: 0,
                exhaust_work: true,
            });
        });
        Self
    }

    fn snapshot(&self) -> (Option<salsa::DatabaseKeyIndex>, usize) {
        ENTRY.with_borrow(|entry| {
            let entry = entry.as_ref().unwrap();
            (entry.key, entry.entries)
        })
    }
}

impl Drop for EntryRecording {
    fn drop(&mut self) {
        ENTRY.with_borrow_mut(|entry| *entry = None);
    }
}

fn query_entry(event: &salsa::EventKind) {
    if let salsa::EventKind::WillExecute { database_key } = event {
        ENTRY.with_borrow_mut(|entry| {
            if let Some(entry) = entry
                && database_key.ingredient_index() == entry.ingredient
            {
                entry.key = Some(*database_key);
                entry.entries += 1;
                if std::mem::take(&mut entry.exhaust_work) {
                    let remaining =
                        salsa::attempt_probe::remaining_allowance_for_diagnostics(&entry.db)
                            .unwrap();
                    assert!(remaining > 0);
                    salsa::attempt_probe::charge(&entry.db, remaining).unwrap();
                }
            }
        });
    }
}

/// Refusing either query's native quotation leaves both memos unpublished and permits the same keys to retry.
#[test]
fn native_quotation_refusal_retries_the_same_keys() {
    for stop_in_child in [false, true] {
        let db = database("class Product:\n    field: bool\n");
        let prepared = prepare(&db);
        let class = ordinary_class(&db, &prepared, "Product");
        let name = Name::new_heap("field");
        let ingredient = effective_superclass_variable_kind_ingredient(&db);
        let boolean = function_input(&prepared, "Product", "field");
        let boolean_ingredient = is_function_definition_ingredient(&db);
        let revision = salsa::plumbing::current_revision(&db);
        let selected = if stop_in_child {
            boolean_ingredient
                .database_key_index(boolean.0.as_id())
                .ingredient_index()
        } else {
            ingredient
                .database_key_index(boolean.0.as_id())
                .ingredient_index()
        };
        let recording = EntryRecording::start(&db, selected);

        // WillExecute precedes input conversion. Emptying work there rejects the selected
        // query's first native quotation guard before its tuple clone or provider body runs.
        // Selecting the boolean child also interrupts an already-running effective-kind body.
        assert_eq!(
            controlled_kind(&prepared, class, &name),
            Ok(AnalysisOutcome::Incomplete {
                reason: AnalysisIncomplete::WorkLimit,
                completed: ()
            }),
        );
        let key = existing_key(&db, ingredient, &(class, name.clone())).unwrap();
        let boolean_key = existing_key(&db, boolean_ingredient, &(boolean.0, boolean.1));
        let refused_key = if stop_in_child {
            boolean_key.unwrap()
        } else {
            key
        };
        assert_eq!(recording.snapshot(), (Some(refused_key), 1));
        assert_no_memos(&db, ingredient);
        assert_no_memos(&db, boolean_ingredient);
        if !stop_in_child {
            assert!(
                boolean_key.is_none(),
                "the effective-kind body has not started"
            );
        }
        assert_no_active_attempt();

        let retry = capture(&db, || controlled_kind(&prepared, class, &name)).unwrap();
        assert_eq!(
            retry.value,
            Ok(AnalysisOutcome::Complete(Some(VariableKind::Instance)))
        );
        assert_eq!(
            existing_key(&db, ingredient, &(class, name.clone())),
            Some(key)
        );
        if let Some(boolean_key) = boolean_key {
            assert_eq!(
                existing_key(&db, boolean_ingredient, &(boolean.0, boolean.1)),
                Some(boolean_key)
            );
        }
        assert_read(&retry.reads, retry.stamp, key);
        assert_read(&retry.reads, retry.stamp, refused_key);
        assert_eq!(recording.snapshot(), (Some(refused_key), 2));
        assert_eq!(salsa::plumbing::current_revision(&db), revision);
    }
}

fn input_quote<'db, C: EffectiveVariableKindConfiguration>(
    _ingredient: &IngredientImpl<C>,
    class: ClassType<'db>,
    name: &Name,
) -> usize {
    <EffectiveVariableKindProfile as QueryKeyProfile<C>>::input_work(&(class, name.clone()))
        .unwrap()
}

/// Work below the long-name key quote refuses before interning; a funded retry creates the key.
#[test]
fn long_name_quote_refuses_before_interning_and_retries() {
    let name = Name::new_heap("field".repeat(256));
    let db = database(&format!("class Product:\n    {name}: bool\n"));
    let prepared = prepare(&db);
    let class = ordinary_class(&db, &prepared, "Product");
    let ingredient = effective_superclass_variable_kind_ingredient(&db);
    let revision = salsa::plumbing::current_revision(&db);
    let quote = input_quote(ingredient, class, &name);
    assert!(quote > name.len());
    assert!(existing_key(&db, ingredient, &(class, name.clone())).is_none());
    assert_eq!(
        controlled_member_operation(
            &prepared,
            EffectiveRequest {
                class,
                name: &name,
                remaining_work: Some(quote - 1)
            },
            &funded()
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::WorkLimit,
            completed: ()
        }),
    );
    assert!(existing_key(&db, ingredient, &(class, name.clone())).is_none());
    assert_no_memos(&db, ingredient);
    assert_no_active_attempt();
    assert_eq!(
        controlled_kind(&prepared, class, &name),
        Ok(AnalysisOutcome::Complete(Some(VariableKind::Instance)))
    );
    assert!(existing_key(&db, ingredient, &(class, name)).is_some());
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
}

/// A query issued from another file reuses an unchanged definition and invalidates when it changes.
#[test]
fn function_definition_tracks_a_different_file() {
    let mut db = database("pass\n");
    db.write_file(
        "src/base.py",
        "class Base:\n    def member(self):\n        pass\n",
    )
    .unwrap();
    let mut prior_key = None;
    for (change, expected, executions) in [
        (None, true, 1),
        (Some(("src/main.py", "unrelated = True\n")), true, 0),
        (
            Some(("src/base.py", "class Base:\n    member = True\n")),
            false,
            1,
        ),
    ] {
        if let Some((path, source)) = change {
            db.write_file(path, source).unwrap();
        }
        let prepared = prepare(&db);
        let base_file = system_path_to_file(&db, "src/base.py").unwrap();
        let base = prepare_file(&db, base_file).unwrap();
        let input = function_input(&base, "Base", "member");
        let ingredient = is_function_definition_ingredient(&db);
        if prior_key.is_none() {
            assert!(existing_key(&db, ingredient, &(input.0, input.1)).is_none());
        }
        let mut events_db = db.clone();
        events_db.take_salsa_events();
        let result = capture(&db, || {
            controlled_member_operation(&prepared, input, &funded())
        })
        .unwrap();
        assert_eq!(result.value, Ok(AnalysisOutcome::Complete(expected)));
        let key = existing_key(&db, ingredient, &(input.0, input.1)).unwrap();
        if let Some(prior_key) = prior_key {
            assert_eq!(key, prior_key);
        }
        assert_read(&result.reads, result.stamp, key);
        assert_eq!(
            execution_count(&events_db.take_salsa_events(), key),
            executions
        );
        assert_eq!(is_function_definition(&db, input.0, input.1), expected);
        prior_key = Some(key);
        assert_no_active_attempt();
    }
}
