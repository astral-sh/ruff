use crate::db::tests::{TestDb, TestDbBuilder};
use std::sync::{Arc, Mutex, OnceLock};

static CACHED_DB: OnceLock<Arc<Mutex<TestDb>>> = OnceLock::new();

/// The path to the module containing definitions for property testing.
pub(crate) const PROPERTY_TEST_MODULE_PATH: &str = "/src/type_candidates.py";

pub(crate) fn get_cached_db() -> TestDb {
    let db = CACHED_DB.get_or_init(|| {
        let db = TestDbBuilder::new()
            .with_file(
                PROPERTY_TEST_MODULE_PATH,
                "\
from typing import NewType

NewTypeOfInt = NewType('NewTypeOfInt', int)
SubNewTypeOfInt = NewType('SubNewTypeOfInt', NewTypeOfInt)
SubSubNewTypeOfInt = NewType('SubSubNewTypeOfInt', SubNewTypeOfInt)
NewTypeOfFloat = NewType('NewTypeOfFloat', float)
SubNewTypeOfFloat = NewType('SubNewTypeOfFloat', NewTypeOfFloat)
NewTypeOfComplex = NewType('NewTypeOfComplex', complex)
NewTypeOfStr = NewType('NewTypeOfStr', str)",
            )
            .build()
            .unwrap();
        Arc::new(Mutex::new(db))
    });
    db.lock().unwrap().clone()
}
