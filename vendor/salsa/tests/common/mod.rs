//! Utility for tests that lets us log when notable events happen.

#![allow(dead_code, unused_imports)]

use std::sync::{Arc, Mutex};

use salsa::{Database, Storage};

/// Logging userdata: provides [`LogDatabase`][] trait.
///
/// If you wish to use it along with other userdata,
/// you can also embed it in another struct and implement [`HasLogger`][] for that struct.
#[derive(Clone, Default)]
pub struct Logger {
    logs: Arc<Mutex<Vec<String>>>,
}

impl Logger {
    pub fn push_log(&self, string: String) {
        self.logs.lock().unwrap().push(string);
    }
}

/// Trait implemented by databases that lets them log events.
pub trait HasLogger {
    /// Return a reference to the logger from the database.
    fn logger(&self) -> &Logger;
}

#[salsa::db]
pub trait LogDatabase: HasLogger + Database {
    /// Log an event from inside a tracked function.
    fn push_log(&self, string: String) {
        self.logger().logs.lock().unwrap().push(string);
    }

    fn clear_logs(&self) {
        std::mem::take(&mut *self.logger().logs.lock().unwrap());
    }

    /// Asserts what the (formatted) logs should look like,
    /// clearing the logged events. This takes `&mut self` because
    /// it is meant to be run from outside any tracked functions.
    #[track_caller]
    fn assert_logs(&self, expected: expect_test::Expect) {
        let logs = std::mem::take(&mut *self.logger().logs.lock().unwrap());
        expected.assert_eq(&format!("{logs:#?}"));
    }

    /// Asserts the length of the logs,
    /// clearing the logged events. This takes `&mut self` because
    /// it is meant to be run from outside any tracked functions.
    #[track_caller]
    fn assert_logs_len(&self, expected: usize) {
        let logs = std::mem::take(&mut *self.logger().logs.lock().unwrap());
        assert_eq!(logs.len(), expected, "Actual logs: {logs:#?}");
    }
}

#[salsa::db]
impl<Db: HasLogger + Database> LogDatabase for Db {}

/// Database that provides logging but does not log salsa event.
#[salsa::db]
#[derive(Clone, Default)]
pub struct LoggerDatabase {
    storage: Storage<Self>,
    logger: Logger,
}

impl HasLogger for LoggerDatabase {
    fn logger(&self) -> &Logger {
        &self.logger
    }
}

#[salsa::db]
impl Database for LoggerDatabase {}

/// Database that provides logging and logs salsa events.
#[salsa::db]
#[derive(Clone)]
pub struct EventLoggerDatabase {
    storage: Storage<Self>,
    logger: Logger,
}

impl Default for EventLoggerDatabase {
    fn default() -> Self {
        let logger = Logger::default();
        Self {
            storage: Storage::new(Some(Box::new({
                let logger = logger.clone();
                move |event| logger.push_log(format!("{:?}", event.kind))
            }))),
            logger,
        }
    }
}

#[salsa::db]
impl Database for EventLoggerDatabase {}

impl HasLogger for EventLoggerDatabase {
    fn logger(&self) -> &Logger {
        &self.logger
    }
}

#[salsa::db]
#[derive(Clone)]
pub struct DiscardLoggerDatabase {
    storage: Storage<Self>,
    logger: Logger,
}

impl Default for DiscardLoggerDatabase {
    fn default() -> Self {
        let logger = Logger::default();
        Self {
            storage: Storage::new(Some(Box::new({
                let logger = logger.clone();
                move |event| match event.kind {
                    salsa::EventKind::WillDiscardStaleOutput { .. }
                    | salsa::EventKind::DidDiscard { .. } => {
                        logger.push_log(format!("salsa_event({:?})", event.kind));
                    }
                    _ => {}
                }
            }))),
            logger,
        }
    }
}

#[salsa::db]
impl Database for DiscardLoggerDatabase {}

impl HasLogger for DiscardLoggerDatabase {
    fn logger(&self) -> &Logger {
        &self.logger
    }
}

#[salsa::db]
#[derive(Clone)]
pub struct ExecuteValidateLoggerDatabase {
    storage: Storage<Self>,
    logger: Logger,
}

impl Default for ExecuteValidateLoggerDatabase {
    fn default() -> Self {
        let logger = Logger::default();
        Self {
            storage: Storage::new(Some(Box::new({
                let logger = logger.clone();
                move |event| match event.kind {
                    salsa::EventKind::WillExecute { .. }
                    | salsa::EventKind::WillIterateCycle { .. }
                    | salsa::EventKind::DidFinalizeCycle { .. }
                    | salsa::EventKind::DidValidateInternedValue { .. }
                    | salsa::EventKind::DidValidateMemoizedValue { .. } => {
                        logger.push_log(format!("salsa_event({:?})", event.kind));
                    }
                    _ => {}
                }
            }))),
            logger,
        }
    }
}
impl Database for ExecuteValidateLoggerDatabase {}

impl HasLogger for ExecuteValidateLoggerDatabase {
    fn logger(&self) -> &Logger {
        &self.logger
    }
}

/// Trait implemented by databases that lets them provide a fixed u32 value.
pub trait HasValue {
    fn get_value(&self) -> u32;
}

#[salsa::db]
pub trait ValueDatabase: HasValue + Database {}

#[salsa::db]
impl<Db: HasValue + Database> ValueDatabase for Db {}

#[salsa::db]
#[derive(Clone, Default)]
pub struct DatabaseWithValue {
    storage: Storage<Self>,
    value: u32,
}

impl HasValue for DatabaseWithValue {
    fn get_value(&self) -> u32 {
        self.value
    }
}

#[salsa::db]
impl Database for DatabaseWithValue {}

impl DatabaseWithValue {
    pub fn new(value: u32) -> Self {
        Self {
            storage: Default::default(),
            value,
        }
    }
}
