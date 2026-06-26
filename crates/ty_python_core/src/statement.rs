use crate::ast_node_ref::AstNodeRef;
use crate::db::Db;
use crate::definition::Definition;
use crate::expression::Expression;
use crate::node_key::NodeKey;
use crate::scope::{FileScopeId, ScopeId};
use crate::semantic_index;
use ruff_db::files::File;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;
use salsa;

/// An independently type-inferable statement.
///
/// Many statements can be treated directly as definitions or expressions,
/// and so do not require a separate Salsa allocation.
#[derive(
    Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Supertype, salsa::Update, get_size2::GetSize,
)]
pub enum Statement<'db> {
    Expression(Expression<'db>),
    Definition(Definition<'db>),
    Other(StatementInner<'db>),
}

/// An independently type-inferable statement.
///
/// ## Module-local type
/// This type should not be used as part of any cross-module API because
/// it holds a reference to the AST node. Range-offset changes
/// then propagate through all usages, and deserialization requires
/// reparsing the entire module.
///
/// E.g. don't use this type in:
///
/// * a return type of a cross-module query
/// * a field of a type that is a return type of a cross-module query
/// * an argument of a cross-module query
#[salsa::tracked(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct StatementInner<'db> {
    /// The file in which the statement occurs.
    pub file: File,

    /// The scope in which the statement occurs.
    pub file_scope: FileScopeId,

    /// The statement node.
    #[no_eq]
    #[tracked]
    #[returns(ref)]
    pub node_ref: AstNodeRef<ast::Stmt>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StatementInner<'_> {}

impl<'db> StatementInner<'db> {
    pub fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.file_scope(db).to_scope_id(db, self.file(db))
    }
}

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, salsa::Update, get_size2::GetSize,
)]
pub struct StatementNodeKey(NodeKey);

impl From<&ast::Stmt> for StatementNodeKey {
    fn from(node: &ast::Stmt) -> Self {
        Self(NodeKey::from_node(node))
    }
}

impl StatementNodeKey {
    fn node(self, module: &ParsedModuleRef) -> &ast::Stmt {
        module
            .get_by_index(self.0.index())
            .try_into()
            .expect("statement key should point to a statement")
    }
}

/// Returns the statement containing `lambda`.
///
/// This is intentionally a pure AST lookup because contextual lambda inference is the only caller.
pub fn enclosing_lambda_statement(
    module: &ParsedModuleRef,
    lambda: &ast::ExprLambda,
) -> Option<StatementNodeKey> {
    let lambda_key = NodeKey::from_node(lambda);
    let mut found_lambda = false;

    for ancestor in
        ast::find_node::covering_node(module.syntax().into(), lambda.range()).ancestors()
    {
        if NodeKey::from_node(ancestor) == lambda_key {
            found_lambda = true;
        } else if found_lambda && let Some(statement) = ancestor.as_stmt_ref() {
            return Some(StatementNodeKey(NodeKey::from_node(statement)));
        }
    }

    None
}

/// Materializes the tracked [`Statement`] ingredient for a standalone statement.
#[salsa::tracked]
pub fn standalone_statement(
    db: &dyn Db,
    file: File,
    statement_key: StatementNodeKey,
) -> Statement<'_> {
    let index = semantic_index(db, file);
    let file_scope = index.standalone_statement_scope(statement_key);
    let module = parsed_module(db, file).load(db);
    let statement = statement_key.node(&module);

    match statement {
        ast::Stmt::FunctionDef(function) => {
            Statement::Definition(index.expect_single_definition(function))
        }
        ast::Stmt::ClassDef(class) => Statement::Definition(index.expect_single_definition(class)),
        ast::Stmt::Expr(ast::StmtExpr { value, .. }) => {
            Statement::Expression(index.expression(value))
        }
        ast::Stmt::Assign(assign) => {
            if let [ast::Expr::Name(name)] = &assign.targets[..] {
                Statement::Definition(index.expect_single_definition(name))
            } else {
                Statement::Other(StatementInner::new(
                    db,
                    file,
                    file_scope,
                    AstNodeRef::new(&module, statement),
                ))
            }
        }
        ast::Stmt::AnnAssign(assign) if assign.target.is_name_expr() => {
            Statement::Definition(index.expect_single_definition(assign))
        }
        ast::Stmt::AugAssign(assign) if assign.target.is_name_expr() => {
            Statement::Definition(index.expect_single_definition(assign))
        }
        ast::Stmt::TypeAlias(alias) => Statement::Definition(index.expect_single_definition(alias)),
        _ => Statement::Other(StatementInner::new(
            db,
            file,
            file_scope,
            AstNodeRef::new(&module, statement),
        )),
    }
}
