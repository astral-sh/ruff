use std::fmt::Formatter;
use std::ops::{Deref, Range};
use std::sync::Arc;

use rustc_hash::FxHashMap;

use ruff_index::{newtype_index, IndexSlice, IndexVec};
use ruff_python_ast::{BoolOp, Expr, ModModule, Operator, Stmt, StmtClassDef, StmtFunctionDef};
use ruff_python_parser::Parsed;

use crate::db::Upcast;
use crate::salsa_db::source;
use crate::Name;

use super::source::File;

#[salsa::tracked(jar=Jar)]
pub struct Function {
    pub file: File,

    pub parent: WithBody,

    #[return_ref]
    pub name: Name,
}

#[salsa::tracked(jar=Jar)]
pub struct Class {
    pub file: File,

    pub parent: WithBody,

    #[return_ref]
    pub name: Name,
}

#[salsa::tracked(jar=Jar)]
pub struct Body {
    #[return_ref]
    pub ast: BodyAst,

    #[return_ref]
    pub source_map: BodySourceMap,
}

pub trait HastNode {
    type AstNode;

    fn ast_node(&self, db: &dyn Db) -> AstNodeRef<Self::AstNode>;
}

impl HastNode for Function {
    type AstNode = ruff_python_ast::StmtFunctionDef;

    fn ast_node(&self, db: &dyn Db) -> AstNodeRef<Self::AstNode> {
        let parent = self.parent(db);
        let parent_body = parent.body(db);

        let parent_source_map = parent_body.source_map(db);
        parent_source_map[*self].clone()
    }
}

impl HastNode for Class {
    type AstNode = ruff_python_ast::StmtClassDef;

    fn ast_node(&self, db: &dyn Db) -> AstNodeRef<Self::AstNode> {
        let parent = self.parent(db);
        let parent_body = parent.body(db);

        let parent_source_map = parent_body.source_map(db);
        parent_source_map[*self].clone()
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BodyAst {
    statements: IndexVec<StatementId, Statement>,
    expressions: IndexVec<ExpressionId, Expression>,
}

impl BodyAst {
    pub fn statements(&self) -> &IndexSlice<StatementId, Statement> {
        &self.statements
    }

    fn shrink_to_fit(&mut self) {
        self.statements.shrink_to_fit();
        self.expressions.shrink_to_fit();
    }
}

impl std::ops::Index<ExpressionId> for BodyAst {
    type Output = Expression;

    #[inline]
    fn index(&self, index: ExpressionId) -> &Self::Output {
        &self.expressions[index]
    }
}

impl std::ops::Index<Range<ExpressionId>> for BodyAst {
    type Output = [Expression];

    #[inline]
    fn index(&self, index: Range<ExpressionId>) -> &Self::Output {
        &self.expressions[index]
    }
}

impl std::ops::Index<StatementId> for BodyAst {
    type Output = Statement;

    #[inline]
    fn index(&self, index: StatementId) -> &Self::Output {
        &self.statements[index]
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct BodySourceMap {
    // TODO: use typed node key or similar here
    statements_map: IndexVec<StatementId, AstNodeRef<Stmt>>,
    expressions_map: IndexVec<ExpressionId, AstNodeRef<Expr>>,

    functions_map: FxHashMap<Function, AstNodeRef<StmtFunctionDef>>,
    class_map: FxHashMap<Class, AstNodeRef<StmtClassDef>>,
}

impl BodySourceMap {
    fn shrink_to_fit(&mut self) {
        self.statements_map.shrink_to_fit();
        self.expressions_map.shrink_to_fit();
        self.functions_map.shrink_to_fit();
        self.class_map.shrink_to_fit();
    }
}

impl std::ops::Index<ExpressionId> for BodySourceMap {
    type Output = AstNodeRef<Expr>;

    #[inline]
    fn index(&self, index: ExpressionId) -> &Self::Output {
        &self.expressions_map[index]
    }
}

impl std::ops::Index<StatementId> for BodySourceMap {
    type Output = AstNodeRef<Stmt>;

    #[inline]
    fn index(&self, index: StatementId) -> &Self::Output {
        &self.statements_map[index]
    }
}

impl std::ops::Index<Function> for BodySourceMap {
    type Output = AstNodeRef<StmtFunctionDef>;

    #[inline]
    fn index(&self, index: Function) -> &Self::Output {
        &self.functions_map[&index]
    }
}

impl std::ops::Index<Class> for BodySourceMap {
    type Output = AstNodeRef<StmtClassDef>;

    #[inline]
    fn index(&self, index: Class) -> &Self::Output {
        &self.class_map[&index]
    }
}

#[newtype_index]
pub struct StatementId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Statement {
    If {
        test: ExpressionId,
        body: Range<StatementId>,
        // FIXME elif_else_clause
    },
    Raise {
        exception: Option<ExpressionId>,
        cause: Option<ExpressionId>,
    },
    Assert {
        test: ExpressionId,
        msg: Option<ExpressionId>,
    },
    Pass,
    Break,
    Continue,
    While {
        test: ExpressionId,
        body: Range<StatementId>,
        or_else: Range<StatementId>,
    },
    For {
        is_async: bool,
        target: ExpressionId,
        iterator: ExpressionId,
        body: Range<StatementId>,
        or_else: Range<StatementId>,
    },
    Assignment {
        targets: Range<ExpressionId>,
        value: ExpressionId,
    },
    AnnotatedAssignment {
        target: ExpressionId,
        annotation: ExpressionId,
        value: Option<ExpressionId>,
        simple: bool,
    },
    AugmentedAssignment {
        target: ExpressionId,
        op: Operator,
        value: ExpressionId,
    },
    Delete {
        targets: Range<ExpressionId>,
    },
    Return {
        value: Option<ExpressionId>,
    },
    Function(Function),
    Class(Class),
    Expression(ExpressionId),
}

#[newtype_index]
pub struct ExpressionId;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Expression {
    BoolOp {
        op: BoolOp,
        values: Range<ExpressionId>,
    },

    Call {
        func: ExpressionId,
    },

    Name(Name),
    NoneLiteral,
    BooleanLiteral(bool),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum WithBody {
    Function(Function),
    Class(Class),
    Module(File),
}

impl WithBody {
    pub fn body(self, db: &dyn Db) -> Body {
        match self {
            WithBody::Function(function) => function_body(db, function),
            WithBody::Module(module) => module_body(db, module),
            WithBody::Class(_) => todo!(),
        }
    }

    pub fn file(self, db: &dyn Db) -> File {
        match self {
            WithBody::Module(file) => file,
            WithBody::Function(function) => function.file(db),
            WithBody::Class(class) => class.file(db),
        }
    }
}

// I wonder if we should build the entire HIR in a single pass
// That would also simplify looking up the `FunctionLoc`
//
#[salsa::tracked(jar=Jar)]
pub fn module_body(db: &dyn Db, file: File) -> Body {
    BodyBuilder::lower_module(db, file)
}

#[salsa::tracked(jar=Jar)]
pub fn function_body(db: &dyn Db, function: Function) -> Body {
    BodyBuilder::lower_function_body(db, function)
}

// TODO rust analyzer defines something like AssocItemLoc
// which stores the ItemTreeId with the ItemId. In our case
// it would be BodyId + FunctionId.
// Main challenge, we don't know the body id just yet.

#[salsa::jar(db=Db)]
pub struct Jar(Function, Class, Body, function_body, module_body);

pub trait Db: source::Db + salsa::DbWithJar<Jar> + Upcast<dyn source::Db> {}

#[derive(Clone)]
pub struct AstNodeRef<T> {
    /// Holds on to the root to ensure that the parsed AST isn't dropped.
    /// That guarantees us that, for as long as the parsed AST isn't dropped, the pointer
    /// to the node must remain valid too.
    _root: Arc<Parsed<ModModule>>,
    node: std::ptr::NonNull<T>,
}
#[allow(unsafe_code)]
impl<T> AstNodeRef<T> {
    /// ## Safety
    /// The caller must ensure that `node` is part of the `parsed` syntax tree.
    pub unsafe fn new(node: &T, parsed: Arc<Parsed<ModModule>>) -> Self {
        Self {
            _root: parsed,
            node: std::ptr::NonNull::from(node),
        }
    }

    pub fn node(&self) -> &T {
        // SAFETY: Holding a reference to the root ensures that the child
        // node can't be dropped.

        unsafe { self.node.as_ref() }
    }
}

impl<T> Deref for AstNodeRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.node()
    }
}

impl<T> PartialEq for AstNodeRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node
    }
}

impl<T> Eq for AstNodeRef<T> {}

impl<T: std::fmt::Debug> std::fmt::Debug for AstNodeRef<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AstNodeRef").field(self.node()).finish()
    }
}

#[allow(unsafe_code)]
unsafe impl<T: Send + Sync> Send for AstNodeRef<T> {}
#[allow(unsafe_code)]
unsafe impl<T: Send + Sync> Sync for AstNodeRef<T> {}

pub struct BodyBuilder<'db> {
    db: &'db dyn Db,
    ast: BodyAst,
    parent: WithBody,
    file: File,
    parsed: Arc<Parsed<ModModule>>,
    source_map: BodySourceMap,
}

impl<'db> BodyBuilder<'db> {
    fn lower_function_body(db: &'db dyn Db, function: Function) -> Body {
        let file = function.file(db);
        let parsed = source::parse(db.upcast(), file);
        let function_node = function.ast_node(db);

        let mut builder = Self {
            db,
            file,
            parent: WithBody::Function(function),
            parsed,
            ast: BodyAst::default(),
            source_map: BodySourceMap::default(),
        };

        builder.lower_suite(&function_node.body);

        builder.finish()
    }

    fn lower_module(db: &'db dyn Db, file: File) -> Body {
        let parsed = source::parse(db.upcast(), file);
        let module = parsed.syntax();

        let mut builder = Self {
            db,
            file,
            parent: WithBody::Module(file),
            parsed: parsed.clone(),
            ast: BodyAst::default(),
            source_map: BodySourceMap::default(),
        };

        builder.lower_suite(&module.body);

        builder.finish()
    }

    fn lower_statement(&mut self, stmt: &Stmt) -> StatementId {
        let statement = match stmt {
            Stmt::If(if_stmt) => Statement::If {
                test: self.lower_expression(&if_stmt.test),
                body: self.lower_suite(&if_stmt.body),
            },
            Stmt::While(while_stmt) => Statement::While {
                test: self.lower_expression(&while_stmt.test),
                body: self.lower_suite(&while_stmt.body),
                or_else: self.lower_suite(&while_stmt.orelse),
            },
            Stmt::For(for_stmt) => Statement::For {
                is_async: for_stmt.is_async,
                target: self.lower_expression(&for_stmt.target),
                iterator: self.lower_expression(&for_stmt.iter),
                body: self.lower_suite(&for_stmt.body),
                or_else: self.lower_suite(&for_stmt.orelse),
            },
            Stmt::Assign(assign) => Statement::Assignment {
                targets: self.lower_expressions(&assign.targets),
                value: self.lower_expression(&assign.value),
            },
            Stmt::AnnAssign(assign) => Statement::AnnotatedAssignment {
                target: self.lower_expression(&assign.target),
                annotation: self.lower_expression(&assign.annotation),
                value: assign
                    .value
                    .as_ref()
                    .map(|expr| self.lower_expression(expr)),
                simple: assign.simple,
            },
            Stmt::AugAssign(assign) => Statement::AugmentedAssignment {
                target: self.lower_expression(&assign.target),
                op: assign.op,
                value: self.lower_expression(&assign.value),
            },
            Stmt::Delete(delete) => Statement::Delete {
                targets: self.lower_expressions(&delete.targets),
            },
            Stmt::Return(return_stmt) => Statement::Return {
                value: return_stmt
                    .value
                    .as_ref()
                    .map(|expr| self.lower_expression(expr)),
            },
            Stmt::Raise(raise) => Statement::Raise {
                exception: raise.exc.as_ref().map(|expr| self.lower_expression(expr)),
                cause: raise.cause.as_ref().map(|expr| self.lower_expression(expr)),
            },
            Stmt::FunctionDef(function_def) => {
                let function = Function::new(
                    self.db,
                    self.file,
                    self.parent,
                    Name::new(&function_def.name),
                );
                #[allow(unsafe_code)]
                self.source_map.functions_map.insert(function, unsafe {
                    AstNodeRef::new(function_def, self.parsed.clone())
                });

                Statement::Function(function)
            }

            Stmt::ClassDef(class_def) => {
                let class = Class::new(self.db, self.file, self.parent, Name::new(&class_def.name));
                #[allow(unsafe_code)]
                self.source_map.class_map.insert(class, unsafe {
                    AstNodeRef::new(class_def, self.parsed.clone())
                });

                Statement::Class(class)
            }
            Stmt::Pass(_) => Statement::Pass,
            Stmt::Break(_) => Statement::Break,
            Stmt::Continue(_) => Statement::Continue,

            _ => todo!(),
        };

        let statement_id = self.ast.statements.push(statement);
        #[allow(unsafe_code)]
        let source_map_id = self
            .source_map
            .statements_map
            .push(unsafe { AstNodeRef::new(stmt, self.parsed.clone()) });

        debug_assert_eq!(statement_id, source_map_id);

        statement_id
    }

    fn lower_suite(&mut self, stmts: &[Stmt]) -> Range<StatementId> {
        let start = self.ast.statements.next_index();

        for statement in stmts {
            self.lower_statement(statement);
        }

        Range {
            start,
            end: self.ast.statements.next_index(),
        }
    }

    fn lower_expression(&mut self, expr: &Expr) -> ExpressionId {
        let expression = match expr {
            Expr::BoolOp(bool_op) => Expression::BoolOp {
                op: bool_op.op,
                values: self.lower_expressions(&bool_op.values),
            },
            Expr::Call(call) => Expression::Call {
                func: self.lower_expression(&call.func),
            },
            Expr::NoneLiteral(_) => Expression::NoneLiteral,
            Expr::BooleanLiteral(bool) => Expression::BooleanLiteral(bool.value),
            Expr::Name(name) => Expression::Name(Name::new(&name.id)),
            _ => todo!(),
        };

        let expression_id = self.ast.expressions.push(expression);
        #[allow(unsafe_code)]
        let source_map_id = self
            .source_map
            .expressions_map
            .push(unsafe { AstNodeRef::new(expr, self.parsed.clone()) });
        debug_assert_eq!(expression_id, source_map_id);

        expression_id
    }

    fn lower_expressions(&mut self, exprs: &[Expr]) -> Range<ExpressionId> {
        let start = self.ast.expressions.next_index();

        for expr in exprs {
            self.lower_expression(expr);
        }

        Range {
            start,
            end: self.ast.expressions.next_index(),
        }
    }

    fn finish(mut self) -> Body {
        self.ast.shrink_to_fit();
        self.source_map.shrink_to_fit();

        Body::new(self.db, self.ast, self.source_map)
    }
}
