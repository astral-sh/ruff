use crate::ast_ids::TypedNodeKey;
use crate::semantic::ModuleName;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;

// TODO storing TypedNodeKey for definitions means we have to search to find them again in the AST;
// this is at best O(log n). If looking up definitions is a bottleneck we should look for
// alternatives here.
// TODO intern Definitions in SymbolTable and reference using IDs?
#[derive(Clone, Debug)]
pub enum Definition {
    // For the import cases, we don't need reference to any arbitrary AST subtrees (annotations,
    // RHS), and referencing just the import statement node is imprecise (a single import statement
    // can assign many symbols, we'd have to re-search for the one we care about), so we just copy
    // the small amount of information we need from the AST.
    Import(ImportDefinition),
    ImportFrom(ImportFromDefinition),
    ClassDef(TypedNodeKey<ast::StmtClassDef>),
    FunctionDef(TypedNodeKey<ast::StmtFunctionDef>),
    Assignment(TypedNodeKey<ast::StmtAssign>),
    AnnotatedAssignment(TypedNodeKey<ast::StmtAnnAssign>),
    NamedExpr(TypedNodeKey<ast::ExprNamed>),
    /// represents the implicit initial definition of every name as "unbound"
    Unbound,
    // TODO with statements, except handlers, function args...
}

#[derive(Clone, Debug)]
pub struct ImportDefinition {
    pub module: ModuleName,
}

#[derive(Clone, Debug)]
pub struct ImportFromDefinition {
    pub module: Option<ModuleName>,
    pub name: Name,
    pub level: u32,
}

impl ImportFromDefinition {
    pub fn module(&self) -> Option<&ModuleName> {
        self.module.as_ref()
    }

    pub fn name(&self) -> &Name {
        &self.name
    }

    pub fn level(&self) -> u32 {
        self.level
    }
}
