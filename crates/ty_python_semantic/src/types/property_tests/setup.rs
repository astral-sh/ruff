use crate::db::tests::{TestDb, setup_db};
use std::sync::{Arc, Mutex, OnceLock};

static CACHED_DB: OnceLock<Arc<Mutex<TestDb>>> = OnceLock::new();

pub(crate) fn get_cached_db() -> TestDb {
    let db = CACHED_DB.get_or_init(|| Arc::new(Mutex::new(setup_db())));
    db.lock().unwrap().clone()
}
