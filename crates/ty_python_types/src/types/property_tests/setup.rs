use std::sync::{Arc, Mutex, OnceLock};
use ty_python_semantic::db::tests::{TestDb, setup_db};

static CACHED_DB: OnceLock<Arc<Mutex<TestDb>>> = OnceLock::new();

pub(crate) fn get_cached_db() -> TestDb {
    let db = CACHED_DB.get_or_init(|| Arc::new(Mutex::new(setup_db())));
    db.lock().unwrap().clone()
}
