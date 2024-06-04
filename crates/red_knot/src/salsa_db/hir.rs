use crate::db::Upcast;
use crate::salsa_db::source;

#[salsa::jar(db=Db)]
pub struct Jar();

pub trait Db: source::Db + salsa::DbWithJar<Jar> + Upcast<dyn source::Db> {}
