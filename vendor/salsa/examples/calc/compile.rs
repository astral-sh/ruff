use crate::ir::SourceProgram;
use crate::parser::parse_statements;
use crate::type_check::type_check_program;

#[salsa::tracked(returns(copy))]
pub fn compile(db: &dyn crate::Db, source_program: SourceProgram) {
    let program = parse_statements(db, source_program);
    type_check_program(db, program);
}
