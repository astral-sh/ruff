//! Test helpers for working with Salsa databases

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::EnvFilter;

pub fn assert_function_query_was_not_run<Db, Q, QDb, I, R>(
    db: &Db,
    query: Q,
    input: I,
    events: &[salsa::Event],
) where
    Db: salsa::Database,
    Q: Fn(QDb, I) -> R,
    I: salsa::plumbing::AsId + std::fmt::Debug + Copy,
{
    let id = input.as_id().as_u32();
    let (query_name, will_execute_event) = find_will_execute_event(db, query, input, events);

    db.attach(|_| {
        if let Some(will_execute_event) = will_execute_event {
            panic!("Expected query {query_name}({id}) not to have run but it did: {will_execute_event:?}\n\n{events:#?}");
        }
    });
}

pub fn assert_const_function_query_was_not_run<Db, Q, QDb, R>(
    db: &Db,
    query: Q,
    events: &[salsa::Event],
) where
    Db: salsa::Database,
    Q: Fn(QDb) -> R,
{
    // Salsa now interns singleton ingredients. But we know that it is a singleton, so we can just search for
    // any event of that ingredient.
    let query_name = query_name(&query);

    let event = events.iter().find(|event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            db.ingredient_debug_name(database_key.ingredient_index()) == query_name
        } else {
            false
        }
    });

    db.attach(|_| {
        if let Some(will_execute_event) = event {
            panic!(
                "Expected query {query_name}() not to have run but it did: {will_execute_event:?}\n\n{events:#?}"
            );
        }
    });
}

/// Assert that the Salsa query described by the generic parameter `C`
/// was executed at least once with the input `input`
/// in the history span represented by `events`.
pub fn assert_function_query_was_run<Db, Q, QDb, I, R>(
    db: &Db,
    query: Q,
    input: I,
    events: &[salsa::Event],
) where
    Db: salsa::Database,
    Q: Fn(QDb, I) -> R,
    I: salsa::plumbing::AsId + std::fmt::Debug + Copy,
{
    let id = input.as_id().as_u32();
    let (query_name, will_execute_event) = find_will_execute_event(db, query, input, events);

    db.attach(|_| {
        assert!(
            will_execute_event.is_some(),
            "Expected query {query_name}({id:?}) to have run but it did not:\n{events:#?}"
        );
    });
}

pub fn find_will_execute_event<'a, Q, I>(
    db: &dyn salsa::Database,
    query: Q,
    input: I,
    events: &'a [salsa::Event],
) -> (&'static str, Option<&'a salsa::Event>)
where
    I: salsa::plumbing::AsId,
{
    let query_name = query_name(&query);

    let event = events.iter().find(|event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            db.ingredient_debug_name(database_key.ingredient_index()) == query_name
                && database_key.key_index() == input.as_id()
        } else {
            false
        }
    });

    (query_name, event)
}

fn query_name<Q>(_query: &Q) -> &'static str {
    let full_qualified_query_name = std::any::type_name::<Q>();
    full_qualified_query_name
        .rsplit_once("::")
        .map(|(_, name)| name)
        .unwrap_or(full_qualified_query_name)
}

/// Sets up logging for the current thread. It captures all `red_knot` and `ruff` events.
///
/// Useful for capturing the tracing output in a failing test.
///
/// # Examples
/// ```
/// use ruff_db::testing::setup_logging;
/// let _logging = setup_logging();
///
/// tracing::info!("This message will be printed to stderr");
/// ```
pub fn setup_logging() -> LoggingGuard {
    LoggingBuilder::new().build()
}

/// Sets up logging for the current thread and uses the passed filter to filter the shown events.
/// Useful for capturing the tracing output in a failing test.
///
/// # Examples
/// ```
/// use ruff_db::testing::setup_logging_with_filter;
/// let _logging = setup_logging_with_filter("red_knot_module_resolver::resolver");
/// ```
///
/// # Filter
/// See [`tracing_subscriber::EnvFilter`] for the `filter`'s syntax.
///
pub fn setup_logging_with_filter(filter: &str) -> Option<LoggingGuard> {
    LoggingBuilder::with_filter(filter).map(LoggingBuilder::build)
}

#[derive(Debug)]
pub struct LoggingBuilder {
    filter: EnvFilter,
    hierarchical: bool,
}

impl LoggingBuilder {
    pub fn new() -> Self {
        Self {
            filter: EnvFilter::default()
                .add_directive(
                    "red_knot=trace"
                        .parse()
                        .expect("Hardcoded directive to be valid"),
                )
                .add_directive(
                    "ruff=trace"
                        .parse()
                        .expect("Hardcoded directive to be valid"),
                ),
            hierarchical: false,
        }
    }

    pub fn with_filter(filter: &str) -> Option<Self> {
        let filter = EnvFilter::builder().parse(filter).ok()?;

        Some(Self {
            filter,
            hierarchical: false,
        })
    }

    pub fn with_hierarchical(mut self, hierarchical: bool) -> Self {
        self.hierarchical = hierarchical;
        self
    }

    pub fn build(self) -> LoggingGuard {
        let registry = tracing_subscriber::registry().with(self.filter);

        let guard = if self.hierarchical {
            let subscriber = registry.with(
                tracing_tree::HierarchicalLayer::default()
                    .with_indent_lines(true)
                    .with_indent_amount(2)
                    .with_bracketed_fields(true)
                    .with_thread_ids(true)
                    .with_targets(true)
                    .with_writer(std::io::stderr)
                    .with_timer(tracing_tree::time::Uptime::default()),
            );

            tracing::subscriber::set_default(subscriber)
        } else {
            let subscriber = registry.with(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_writer(std::io::stderr)
                    .with_timer(tracing_subscriber::fmt::time()),
            );

            tracing::subscriber::set_default(subscriber)
        };

        LoggingGuard { _guard: guard }
    }
}

impl Default for LoggingBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[must_use = "Dropping the guard unregisters the tracing subscriber."]
pub struct LoggingGuard {
    _guard: tracing::subscriber::DefaultGuard,
}

#[test]
fn query_was_not_run() {
    use crate::tests::TestDb;
    use salsa::prelude::*;

    #[salsa::input(debug)]
    struct Input {
        text: String,
    }

    #[salsa::tracked]
    fn len(db: &dyn salsa::Database, input: Input) -> usize {
        input.text(db).len()
    }

    let mut db = TestDb::new();

    let hello = Input::new(&db, "Hello, world!".to_string());
    let goodbye = Input::new(&db, "Goodbye!".to_string());

    assert_eq!(len(&db, hello), 13);
    assert_eq!(len(&db, goodbye), 8);

    // Change the input of one query
    goodbye.set_text(&mut db).to("Bye".to_string());
    db.clear_salsa_events();

    assert_eq!(len(&db, goodbye), 3);
    let events = db.take_salsa_events();

    assert_function_query_was_run(&db, len, goodbye, &events);
    assert_function_query_was_not_run(&db, len, hello, &events);
}

#[test]
#[should_panic(expected = "Expected query len(0) not to have run but it did:")]
fn query_was_not_run_fails_if_query_was_run() {
    use crate::tests::TestDb;
    use salsa::prelude::*;

    #[salsa::input(debug)]
    struct Input {
        text: String,
    }

    #[salsa::tracked]
    fn len(db: &dyn salsa::Database, input: Input) -> usize {
        input.text(db).len()
    }

    let mut db = TestDb::new();

    let hello = Input::new(&db, "Hello, world!".to_string());

    assert_eq!(len(&db, hello), 13);

    // Change the input
    hello.set_text(&mut db).to("Hy".to_string());
    db.clear_salsa_events();

    assert_eq!(len(&db, hello), 2);
    let events = db.take_salsa_events();

    assert_function_query_was_not_run(&db, len, hello, &events);
}

#[test]
#[should_panic(expected = "Expected query len() not to have run but it did:")]
fn const_query_was_not_run_fails_if_query_was_run() {
    use crate::tests::TestDb;
    use salsa::prelude::*;

    #[salsa::input]
    struct Input {
        text: String,
    }

    #[salsa::tracked]
    fn len(db: &dyn salsa::Database) -> usize {
        db.report_untracked_read();
        5
    }

    let mut db = TestDb::new();
    let hello = Input::new(&db, "Hello, world!".to_string());
    assert_eq!(len(&db), 5);

    // Create a new revision
    db.clear_salsa_events();
    hello.set_text(&mut db).to("Hy".to_string());

    assert_eq!(len(&db), 5);
    let events = db.take_salsa_events();

    assert_const_function_query_was_not_run(&db, len, &events);
}

#[test]
#[should_panic(expected = "Expected query len(0) to have run but it did not:")]
fn query_was_run_fails_if_query_was_not_run() {
    use crate::tests::TestDb;
    use salsa::prelude::*;

    #[salsa::input(debug)]
    struct Input {
        text: String,
    }

    #[salsa::tracked]
    fn len(db: &dyn salsa::Database, input: Input) -> usize {
        input.text(db).len()
    }

    let mut db = TestDb::new();

    let hello = Input::new(&db, "Hello, world!".to_string());
    let goodbye = Input::new(&db, "Goodbye!".to_string());

    assert_eq!(len(&db, hello), 13);
    assert_eq!(len(&db, goodbye), 8);

    // Change the input of one query
    goodbye.set_text(&mut db).to("Bye".to_string());
    db.clear_salsa_events();

    assert_eq!(len(&db, goodbye), 3);
    let events = db.take_salsa_events();

    assert_function_query_was_run(&db, len, hello, &events);
}
