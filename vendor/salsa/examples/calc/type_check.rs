#[cfg(test)]
use expect_test::expect;
use salsa::Accumulator;
#[cfg(test)]
use test_log::test;

use crate::ir::{
    Diagnostic, Expression, Function, FunctionId, Program, Span, StatementData, VariableId,
};

// ANCHOR: parse_statements
#[salsa::tracked(returns(copy))]
pub fn type_check_program<'db>(db: &'db dyn crate::Db, program: Program<'db>) {
    for statement in program.statements(db) {
        match &statement.data {
            StatementData::Function(f) => type_check_function(db, *f, program),
            StatementData::Print(e) => CheckExpression::new(db, program, &[]).check(e),
        }
    }
}

#[salsa::tracked(returns(copy))]
pub fn type_check_function<'db>(
    db: &'db dyn crate::Db,
    function: Function<'db>,
    program: Program<'db>,
) {
    CheckExpression::new(db, program, function.args(db)).check(function.body(db))
}

#[salsa::tracked(returns(copy))]
pub fn find_function<'db>(
    db: &'db dyn crate::Db,
    program: Program<'db>,
    name: FunctionId<'db>,
) -> Option<Function<'db>> {
    program
        .statements(db)
        .iter()
        .flat_map(|s| match &s.data {
            StatementData::Function(f) if f.name(db) == name => Some(*f),
            _ => None,
        })
        .next()
}

struct CheckExpression<'input, 'db> {
    db: &'db dyn crate::Db,
    program: Program<'db>,
    names_in_scope: &'input [VariableId<'db>],
}

impl<'input, 'db> CheckExpression<'input, 'db> {
    pub fn new(
        db: &'db dyn crate::Db,
        program: Program<'db>,
        names_in_scope: &'input [VariableId<'db>],
    ) -> Self {
        CheckExpression {
            db,
            program,
            names_in_scope,
        }
    }
}

impl<'db> CheckExpression<'_, 'db> {
    fn check(&self, expression: &Expression<'db>) {
        match &expression.data {
            crate::ir::ExpressionData::Op(left, _, right) => {
                self.check(left);
                self.check(right);
            }
            crate::ir::ExpressionData::Number(_) => {}
            crate::ir::ExpressionData::Variable(v) => {
                if !self.names_in_scope.contains(v) {
                    self.report_error(
                        expression.span,
                        format!("the variable `{}` is not declared", v.text(self.db)),
                    );
                }
            }
            crate::ir::ExpressionData::Call(f, args) => {
                if self.find_function(*f).is_none() {
                    self.report_error(
                        expression.span,
                        format!("the function `{}` is not declared", f.text(self.db)),
                    );
                }
                for arg in args {
                    self.check(arg);
                }
            }
        }
    }

    fn find_function(&self, f: FunctionId<'db>) -> Option<Function<'db>> {
        find_function(self.db, self.program, f)
    }

    fn report_error(&self, span: Span, message: String) {
        Diagnostic::new(span.start(self.db), span.end(self.db), message).accumulate(self.db);
    }
}

/// Create a new database with the given source text and parse the result.
/// Returns the statements and the diagnostics generated.
#[cfg(test)]
fn check_string(
    source_text: &str,
    expected_diagnostics: expect_test::Expect,
    edits: &[(&str, expect_test::Expect)],
) {
    use salsa::{Database, Setter};

    use crate::db::CalcDatabaseImpl;
    use crate::ir::SourceProgram;
    use crate::parser::parse_statements;

    // Create the database
    let mut db = CalcDatabaseImpl::default();
    db.enable_logging();

    // Create the source program
    let source_program = SourceProgram::new(&db, source_text.to_string());

    // Invoke the parser
    let program = parse_statements(&db, source_program);

    // Read out any diagnostics
    db.attach(|db| {
        let rendered_diagnostics: String =
            type_check_program::accumulated::<Diagnostic>(db, program)
                .into_iter()
                .map(|d| d.render(db, source_program))
                .collect::<Vec<_>>()
                .join("\n");
        expected_diagnostics.assert_eq(&rendered_diagnostics);
    });

    // Apply edits and check diagnostics/logs after each one
    for (new_source_text, expected_diagnostics) in edits {
        source_program
            .set_text(&mut db)
            .to(new_source_text.to_string());

        db.attach(|db| {
            let program = parse_statements(db, source_program);
            expected_diagnostics
                .assert_debug_eq(&type_check_program::accumulated::<Diagnostic>(db, program));
        });
    }
}

#[test]
fn check_print() {
    check_string("print 1 + 2", expect![""], &[]);
}

#[test]
fn check_bad_variable_in_program() {
    check_string(
        "print a + b",
        expect![[r#"
            error: the variable `a` is not declared
             --> input:2:7
              |
            2 | print a + b
              |       ^^ here
            error: the variable `b` is not declared
             --> input:2:11
              |
            2 | print a + b
              |           ^ here"#]],
        &[],
    );
}

#[test]
fn check_bad_function_in_program() {
    check_string(
        "print a(22)",
        expect![[r#"
            error: the function `a` is not declared
             --> input:2:7
              |
            2 | print a(22)
              |       ^^^^^ here"#]],
        &[],
    );
}

#[test]
fn check_bad_variable_in_function() {
    check_string(
        "
            fn add_one(a) = a + b
            print add_one(22)
        ",
        expect![[r#"
            error: the variable `b` is not declared
             --> input:4:33
              |
            4 |               fn add_one(a) = a + b
              |  _________________________________^
            5 | |             print add_one(22)
              | |____________^ here"#]],
        &[],
    );
}

#[test]
fn check_bad_function_in_function() {
    check_string(
        "
            fn add_one(a) = add_two(a) + b
            print add_one(22)
        ",
        expect![[r#"
            error: the function `add_two` is not declared
             --> input:4:29
              |
            4 |             fn add_one(a) = add_two(a) + b
              |                             ^^^^^^^^^^ here
            error: the variable `b` is not declared
             --> input:4:42
              |
            4 |               fn add_one(a) = add_two(a) + b
              |  __________________________________________^
            5 | |             print add_one(22)
              | |____________^ here"#]],
        &[],
    );
}

#[test]
fn fix_bad_variable_in_function() {
    check_string(
        "
            fn double(a) = a * b
            fn quadruple(a) = double(double(a))
            print quadruple(2)
        ",
        expect![[r#"
            error: the variable `b` is not declared
             --> input:4:32
              |
            4 |               fn double(a) = a * b
              |  ________________________________^
            5 | |             fn quadruple(a) = double(double(a))
              | |____________^ here"#]],
        &[(
            "
                fn double(a) = a * 2
                fn quadruple(a) = double(double(a))
                print quadruple(2)
            ",
            expect![[r#"
                []
            "#]],
        )],
    );
}
