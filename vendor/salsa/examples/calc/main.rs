use db::CalcDatabaseImpl;
use ir::{Diagnostic, SourceProgram};
use salsa::Database as Db;

mod compile;
mod db;
mod ir;
mod parser;
mod type_check;

pub fn main() {
    let db: CalcDatabaseImpl = Default::default();
    let source_program = SourceProgram::new(&db, String::new());
    compile::compile(&db, source_program);
    let diagnostics = compile::compile::accumulated::<Diagnostic>(&db, source_program);
    eprintln!("{diagnostics:?}");
}
