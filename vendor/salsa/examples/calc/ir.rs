#![allow(clippy::needless_borrow)]

use ordered_float::OrderedFloat;

// ANCHOR: input
#[salsa::input(debug)]
pub struct SourceProgram {
    #[returns(deref)]
    pub text: String,
}
// ANCHOR_END: input

// ANCHOR: interned_ids
#[salsa::interned(debug)]
pub struct VariableId<'db> {
    #[returns(deref)]
    pub text: String,
}

#[salsa::interned(debug)]
pub struct FunctionId<'db> {
    #[returns(deref)]
    pub text: String,
}
// ANCHOR_END: interned_ids

// ANCHOR: program
#[salsa::tracked(debug)]
pub struct Program<'db> {
    #[tracked]
    #[returns(deref)]
    pub statements: Vec<Statement<'db>>,
}
// ANCHOR_END: program

// ANCHOR: statements_and_expressions
#[derive(Eq, PartialEq, Debug, Hash, salsa::SalsaValue)]
pub struct Statement<'db> {
    pub span: Span<'db>,

    pub data: StatementData<'db>,
}

impl<'db> Statement<'db> {
    pub fn new(span: Span<'db>, data: StatementData<'db>) -> Self {
        Statement { span, data }
    }
}

#[derive(Eq, PartialEq, Debug, Hash, salsa::SalsaValue)]
pub enum StatementData<'db> {
    /// Defines `fn <name>(<args>) = <body>`
    Function(Function<'db>),
    /// Defines `print <expr>`
    Print(Expression<'db>),
}

#[derive(Eq, PartialEq, Debug, Hash, salsa::SalsaValue)]
pub struct Expression<'db> {
    pub span: Span<'db>,

    pub data: ExpressionData<'db>,
}

impl<'db> Expression<'db> {
    pub fn new(span: Span<'db>, data: ExpressionData<'db>) -> Self {
        Expression { span, data }
    }
}

#[derive(Eq, PartialEq, Debug, Hash, salsa::SalsaValue)]
pub enum ExpressionData<'db> {
    Op(Box<Expression<'db>>, Op, Box<Expression<'db>>),
    Number(#[salsa_value(unsafe(prove_safe_to_retain_manually))] OrderedFloat<f64>),
    Variable(VariableId<'db>),
    Call(FunctionId<'db>, Vec<Expression<'db>>),
}

#[derive(Eq, PartialEq, Copy, Clone, Hash, Debug, salsa::SalsaValue)]
pub enum Op {
    Add,
    Subtract,
    Multiply,
    Divide,
}
// ANCHOR_END: statements_and_expressions

// ANCHOR: functions
#[salsa::tracked(debug)]
pub struct Function<'db> {
    #[returns(copy)]
    pub name: FunctionId<'db>,

    #[returns(copy)]
    name_span: Span<'db>,

    #[tracked]
    #[returns(deref)]
    pub args: Vec<VariableId<'db>>,

    #[tracked]
    pub body: Expression<'db>,
}
// ANCHOR_END: functions

#[salsa::tracked(debug)]
pub struct Span<'db> {
    #[tracked]
    #[returns(copy)]
    pub start: usize,
    #[tracked]
    #[returns(copy)]
    pub end: usize,
}

// ANCHOR: diagnostic
#[salsa::accumulator]
#[derive(Debug)]
#[allow(dead_code)] // Debug impl uses them
pub struct Diagnostic {
    pub start: usize,
    pub end: usize,
    pub message: String,
}
// ANCHOR_END: diagnostic

impl Diagnostic {
    pub fn new(start: usize, end: usize, message: String) -> Self {
        Diagnostic {
            start,
            end,
            message,
        }
    }

    #[cfg(test)]
    pub fn render(&self, db: &dyn crate::Db, src: SourceProgram) -> String {
        use annotate_snippets::*;
        let line_start = src.text(db)[..self.start].lines().count() + 1;
        let report = [Level::ERROR.primary_title(&self.message).element(
            Snippet::source(src.text(db))
                .line_start(line_start)
                .path("input")
                .fold(true)
                .annotation(
                    AnnotationKind::Primary
                        .span(self.start..self.end)
                        .label("here"),
                ),
        )];
        Renderer::plain().render(&report).to_string()
    }
}
