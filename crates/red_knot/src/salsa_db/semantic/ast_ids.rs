use super::{Db, Jar};
use crate::ast_ids::NodeKey;
use ruff_index::{newtype_index, IndexVec};
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::visitor::preorder::PreorderVisitor;
use ruff_python_ast::{
    Expr, ModModule, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFunctionDef, StmtImport,
    StmtImportFrom,
};
use ruff_python_parser::Parsed;
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::ops::Deref;
use std::sync::Arc;

use crate::salsa_db::source::{parse, File};

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar, no_eq, return_ref)]
pub fn ast_ids(db: &dyn Db, file: File) -> AstIds {
    let parsed = parse(db.upcast(), file);

    AstIds::from_parsed(&parsed)
}

pub struct AstIds {
    expressions: IndexVec<ExpressionId, AstNodeRef<Expr>>,

    /// Maps expressions to their expression id. Uses `NodeKey` because it avoids cloning [`Parsed`].
    expressions_map: FxHashMap<NodeKey, ExpressionId>,

    statements: IndexVec<StatementId, AstNodeRef<Stmt>>,

    statements_map: FxHashMap<NodeKey, StatementId>,
}

impl AstIds {
    fn from_parsed(parsed: &Arc<Parsed<ModModule>>) -> Self {
        let mut builder = AstIdsBuilder {
            parsed: parsed.clone(),
            expressions: IndexVec::new(),
            expressions_map: FxHashMap::default(),
            statements: IndexVec::new(),
            statements_map: FxHashMap::default(),
            deferred: VecDeque::new(),
        };

        builder.visit_body(&parsed.syntax().body);

        while let Some(deferred) = builder.deferred.pop_front() {
            builder.visit_body(deferred);
        }

        builder.finish()
    }

    #[allow(unused)]
    pub fn functions(&self) -> impl Iterator<Item = (FunctionId, &StmtFunctionDef)> {
        self.statements
            .iter_enumerated()
            .filter_map(|(index, stmt)| Some((FunctionId(index), stmt.as_function_def_stmt()?)))
    }

    #[allow(unused)]
    pub fn classes(&self) -> impl Iterator<Item = (ClassId, &StmtClassDef)> {
        self.statements
            .iter_enumerated()
            .filter_map(|(index, stmt)| Some((ClassId(index), stmt.as_class_def_stmt()?)))
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for AstIds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AstIds")
            .field("expressions", &self.expressions)
            .field("statements", &self.statements)
            .finish()
    }
}

impl PartialEq for AstIds {
    fn eq(&self, other: &Self) -> bool {
        self.expressions == other.expressions && self.statements == other.statements
    }
}

#[newtype_index]
pub struct ExpressionId;

#[newtype_index]
pub struct StatementId;

pub trait AstIdNode {
    type Id;

    /// Resolves the AST id of the node.
    ///
    /// ## Panics
    /// May panic if the node does not belong to the AST of `file` or it returns an incorrect node.
    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id;

    /// Resolves the AST node for `id`.
    ///
    /// ## Panics
    /// May panic if the `id` does not belong to the AST of `file` or it returns an incorrect node.
    #[allow(unused)]
    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self;
}

impl AstIdNode for Expr {
    type Id = ExpressionId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        ast_ids.expressions_map[&NodeKey::from_node(self.into())]
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        &ast_ids.expressions[id]
    }
}

impl AstIdNode for Stmt {
    type Id = StatementId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements_map[&NodeKey::from_node(self.into())]
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        &ast_ids.statements[id]
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct FunctionId(StatementId);

impl AstIdNode for StmtFunctionDef {
    type Id = FunctionId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        FunctionId(ast_ids.statements_map[&NodeKey::from_node(self.into())])
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements[id.0].as_function_def_stmt().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ClassId(StatementId);

impl AstIdNode for StmtClassDef {
    type Id = ClassId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        ClassId(ast_ids.statements_map[&NodeKey::from_node(self.into())])
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements[id.0].as_class_def_stmt().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct AssignmentId(StatementId);

impl AstIdNode for StmtAssign {
    type Id = AssignmentId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        AssignmentId(ast_ids.statements_map[&NodeKey::from_node(self.into())])
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements[id.0].as_assign_stmt().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct AnnotatedAssignmentId(StatementId);

impl AstIdNode for StmtAnnAssign {
    type Id = AnnotatedAssignmentId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        AnnotatedAssignmentId(ast_ids.statements_map[&NodeKey::from_node(self.into())])
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements[id.0].as_ann_assign_stmt().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ImportId(StatementId);

impl AstIdNode for StmtImport {
    type Id = ImportId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        ImportId(ast_ids.statements_map[&NodeKey::from_node(self.into())])
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements[id.0].as_import_stmt().unwrap()
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ImportFromId(StatementId);

impl AstIdNode for StmtImportFrom {
    type Id = ImportFromId;

    fn ast_id(&self, db: &dyn Db, file: File) -> Self::Id {
        let ast_ids = ast_ids(db, file);
        ImportFromId(ast_ids.statements_map[&NodeKey::from_node(self.into())])
    }

    fn lookup(db: &dyn Db, file: File, id: Self::Id) -> &Self {
        let ast_ids = ast_ids(db, file);
        ast_ids.statements[id.0].as_import_from_stmt().unwrap()
    }
}

#[derive(Debug)]
struct AstIdsBuilder<'a> {
    parsed: Arc<Parsed<ModModule>>,
    expressions: IndexVec<ExpressionId, AstNodeRef<Expr>>,
    expressions_map: FxHashMap<NodeKey, ExpressionId>,
    statements: IndexVec<StatementId, AstNodeRef<Stmt>>,
    statements_map: FxHashMap<NodeKey, StatementId>,

    deferred: VecDeque<&'a [Stmt]>,
}

impl AstIdsBuilder<'_> {
    fn finish(mut self) -> AstIds {
        self.expressions.shrink_to_fit();
        self.expressions_map.shrink_to_fit();
        self.statements.shrink_to_fit();
        self.statements_map.shrink_to_fit();

        AstIds {
            expressions: self.expressions,
            expressions_map: self.expressions_map,
            statements: self.statements,
            statements_map: self.statements_map,
        }
    }
}

#[allow(unsafe_code)]
impl<'a> PreorderVisitor<'a> for AstIdsBuilder<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let statement_id = self
            .statements
            .push(unsafe { AstNodeRef::new(self.parsed.clone(), stmt) });
        self.statements_map
            .insert(NodeKey::from_node(stmt.into()), statement_id);

        match stmt {
            Stmt::FunctionDef(StmtFunctionDef {
                parameters,
                body,
                decorator_list,
                returns,
                type_params,
                is_async: _,
                name: _,
                range: _,
            }) => {
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                if let Some(type_params) = type_params {
                    self.visit_type_params(type_params);
                }

                self.visit_parameters(parameters);

                for expr in returns {
                    self.visit_annotation(expr);
                }

                self.deferred.push_back(body);
            }

            Stmt::ClassDef(StmtClassDef {
                arguments,
                body,
                decorator_list,
                type_params,
                name: _,
                range: _,
            }) => {
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }

                if let Some(type_params) = type_params {
                    self.visit_type_params(type_params);
                }

                if let Some(arguments) = arguments {
                    self.visit_arguments(arguments);
                }

                self.deferred.push_back(body);
            }

            _ => preorder::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        let expression_id = self
            .expressions
            .push(unsafe { AstNodeRef::new(self.parsed.clone(), expr) });

        self.expressions_map
            .insert(NodeKey::from_node(expr.into()), expression_id);

        preorder::walk_expr(self, expr);
    }
}

#[derive(Clone)]
struct AstNodeRef<T> {
    _parsed: Arc<Parsed<ModModule>>,
    node: std::ptr::NonNull<T>,
}

impl<T> AstNodeRef<T> {
    /// Creates a new `AstNodeRef` that reference `node`. The `parsed` is the parsed module to which
    /// the `AstNodeRef` belongs.
    ///
    /// The function is marked as `unsafe` because it is the caller's responsibility to ensure that
    /// `node` is part of `parsed`'s AST. Dereferencing the value if that's not the case is UB
    /// because it can then point to a no longer valid memory location.
    #[allow(unsafe_code)]
    unsafe fn new(parsed: Arc<Parsed<ModModule>>, node: &T) -> Self {
        Self {
            _parsed: parsed,
            node: std::ptr::NonNull::from(node),
        }
    }

    pub fn node(&self) -> &T {
        // SAFETY: Holding on to `parsed` ensures that the AST to which `node` belongs is still alive
        // and not moved.
        #[allow(unsafe_code)]
        unsafe {
            self.node.as_ref()
        }
    }
}

impl<T> Deref for AstNodeRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.node()
    }
}

impl<T> std::fmt::Debug for AstNodeRef<T>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AstNodeRef").field(&self.node()).finish()
    }
}

impl<T> PartialEq for AstNodeRef<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.node() == other.node()
    }
}

impl<T> Eq for AstNodeRef<T> where T: Eq {}

#[allow(unsafe_code)]
unsafe impl<T> Send for AstNodeRef<T> where T: Send {}
#[allow(unsafe_code)]
unsafe impl<T> Sync for AstNodeRef<T> where T: Sync {}
