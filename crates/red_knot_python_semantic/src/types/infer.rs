use std::sync::Arc;

use rustc_hash::FxHashMap;

use red_knot_module_resolver::{resolve_module, ModuleName};
use ruff_db::vfs::VfsFile;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::{ExprContext, TypeParams};

use crate::name::Name;
use crate::semantic_index::ast_ids::{ScopeAstIdNode, ScopeExpressionId};
use crate::semantic_index::definition::{Definition, ImportDefinition, ImportFromDefinition};
use crate::semantic_index::symbol::{FileScopeId, ScopeId, ScopeKind, ScopedSymbolId, SymbolTable};
use crate::semantic_index::{symbol_table, ChildrenIter, SemanticIndex};
use crate::types::intern::FileTypeStore;
use crate::types::{
    type_store_query, ClassType, FileModuleTypeId, FunctionType, Type, TypeId, UnionType,
    UnionTypeBuilder,
};
use crate::Db;

/// The inferred types for a single scope.
#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub(crate) struct TypeInference {
    /// The types of every expression in this scope.
    expression_tys: IndexVec<ScopeExpressionId, Type>,

    /// The public types of every symbol in this scope.
    symbol_tys: IndexVec<ScopedSymbolId, Type>,
}

impl TypeInference {
    #[allow(unused)]
    pub(super) fn expression_ty(&self, expression: ScopeExpressionId) -> Type {
        self.expression_tys[expression]
    }

    pub(super) fn symbol_ty(&self, symbol: ScopedSymbolId) -> Type {
        self.symbol_tys[symbol]
    }

    fn shrink_to_fit(&mut self) {
        self.expression_tys.shrink_to_fit();
        self.symbol_tys.shrink_to_fit();
    }
}

/// Builder to infer all types in a [`ScopeId`].
pub(super) struct TypeInferenceBuilder<'a> {
    db: &'a dyn Db,

    // Cached lookups
    index: &'a SemanticIndex,
    file_scope_id: FileScopeId,
    file_id: VfsFile,
    symbol_table: Arc<SymbolTable>,

    /// The type inference results
    types: TypeInference,
    definition_tys: FxHashMap<Definition, Type>,
    children_scopes: ChildrenIter<'a>,
}

impl<'db> TypeInferenceBuilder<'db> {
    /// Creates a new builder for inferring the types of `scope`.
    pub(super) fn new(db: &'db dyn Db, scope: ScopeId<'db>, index: &'db SemanticIndex) -> Self {
        let file_scope_id = scope.file_scope_id(db);
        let file = scope.file(db);
        let children_scopes = index.child_scopes(file_scope_id);
        let symbol_table = index.symbol_table(file_scope_id);

        Self {
            index,
            file_scope_id,
            file_id: file,
            symbol_table,

            db,
            types: TypeInference::default(),
            definition_tys: FxHashMap::default(),
            children_scopes,
        }
    }

    /// Infers the types of a `module`.
    pub(super) fn infer_module(&mut self, module: &ast::ModModule) {
        self.infer_body(&module.body);
    }

    pub(super) fn infer_class_type_params(&mut self, class: &ast::StmtClassDef) {
        if let Some(type_params) = class.type_params.as_deref() {
            self.infer_type_parameters(type_params);
        }
    }

    pub(super) fn infer_class_body(&mut self, class: &ast::StmtClassDef) {
        self.infer_body(&class.body);
    }

    pub(super) fn infer_function_type_params(&mut self, function: &ast::StmtFunctionDef) {
        if let Some(type_params) = function.type_params.as_deref() {
            self.infer_type_parameters(type_params);
        }
    }

    pub(super) fn infer_function_body(&mut self, function: &ast::StmtFunctionDef) {
        self.infer_body(&function.body);
    }

    fn infer_body(&mut self, suite: &[ast::Stmt]) {
        for statement in suite {
            self.infer_statement(statement);
        }
    }

    fn infer_statement(&mut self, statement: &ast::Stmt) {
        match statement {
            ast::Stmt::FunctionDef(function) => self.infer_function_definition_statement(function),
            ast::Stmt::ClassDef(class) => self.infer_class_definition_statement(class),
            ast::Stmt::Expr(ast::StmtExpr { range: _, value }) => {
                self.infer_expression(value);
            }
            ast::Stmt::If(if_statement) => self.infer_if_statement(if_statement),
            ast::Stmt::Assign(assign) => self.infer_assignment_statement(assign),
            ast::Stmt::AnnAssign(assign) => self.infer_annotated_assignment_statement(assign),
            ast::Stmt::For(for_statement) => self.infer_for_statement(for_statement),
            ast::Stmt::Import(import) => self.infer_import_statement(import),
            ast::Stmt::ImportFrom(import) => self.infer_import_from_statement(import),
            ast::Stmt::Break(_) | ast::Stmt::Continue(_) | ast::Stmt::Pass(_) => {
                // No-op
            }
            _ => {}
        }
    }

    fn infer_function_definition_statement(&mut self, function: &ast::StmtFunctionDef) {
        let ast::StmtFunctionDef {
            range: _,
            is_async: _,
            name,
            type_params: _,
            parameters: _,
            returns,
            body: _,
            decorator_list,
        } = function;

        let function_id = function.scope_ast_id(self.db, self.file_id, self.file_scope_id);
        let decorator_tys = decorator_list
            .iter()
            .map(|decorator| self.infer_decorator(decorator))
            .collect();

        // TODO: Infer parameters

        if let Some(return_ty) = returns {
            self.infer_expression(return_ty);
        }

        let function_ty = self.function_ty(FunctionType {
            name: Name::new(&name.id),
            decorators: decorator_tys,
        });

        // Skip over the function or type params child scope.
        let (_, scope) = self.children_scopes.next().unwrap();

        assert!(matches!(
            scope.kind(),
            ScopeKind::Function | ScopeKind::Annotation
        ));

        self.definition_tys
            .insert(Definition::FunctionDef(function_id), function_ty);
    }

    fn infer_class_definition_statement(&mut self, class: &ast::StmtClassDef) {
        let ast::StmtClassDef {
            range: _,
            name,
            type_params,
            decorator_list,
            arguments,
            body: _,
        } = class;

        let class_id = class.scope_ast_id(self.db, self.file_id, self.file_scope_id);

        for decorator in decorator_list {
            self.infer_decorator(decorator);
        }

        let bases = arguments
            .as_deref()
            .map(|arguments| self.infer_arguments(arguments))
            .unwrap_or(Vec::new());

        // If the class has type parameters, then the class body scope is the first child scope of the type parameter's scope
        // Otherwise the next scope must be the class definition scope.
        let (class_body_scope_id, class_body_scope) = if type_params.is_some() {
            let (type_params_scope, _) = self.children_scopes.next().unwrap();
            self.index.child_scopes(type_params_scope).next().unwrap()
        } else {
            self.children_scopes.next().unwrap()
        };

        assert_eq!(class_body_scope.kind(), ScopeKind::Class);

        let class_ty = self.class_ty(ClassType {
            name: Name::new(name),
            bases,
            body_scope: class_body_scope_id,
        });

        self.definition_tys
            .insert(Definition::ClassDef(class_id), class_ty);
    }

    fn infer_if_statement(&mut self, if_statement: &ast::StmtIf) {
        let ast::StmtIf {
            range: _,
            test,
            body,
            elif_else_clauses,
        } = if_statement;

        self.infer_expression(test);
        self.infer_body(body);

        for clause in elif_else_clauses {
            let ast::ElifElseClause {
                range: _,
                test,
                body,
            } = clause;

            if let Some(test) = &test {
                self.infer_expression(test);
            }

            self.infer_body(body);
        }
    }

    fn infer_assignment_statement(&mut self, assignment: &ast::StmtAssign) {
        let ast::StmtAssign {
            range: _,
            targets,
            value,
        } = assignment;

        let value_ty = self.infer_expression(value);

        for target in targets {
            self.infer_expression(target);
        }

        let assign_id = assignment.scope_ast_id(self.db, self.file_id, self.file_scope_id);

        // TODO: Handle multiple targets.
        self.definition_tys
            .insert(Definition::Assignment(assign_id), value_ty);
    }

    fn infer_annotated_assignment_statement(&mut self, assignment: &ast::StmtAnnAssign) {
        let ast::StmtAnnAssign {
            range: _,
            target,
            annotation,
            value,
            simple: _,
        } = assignment;

        if let Some(value) = value {
            let _ = self.infer_expression(value);
        }

        let annotation_ty = self.infer_expression(annotation);
        self.infer_expression(target);

        self.definition_tys.insert(
            Definition::AnnotatedAssignment(assignment.scope_ast_id(
                self.db,
                self.file_id,
                self.file_scope_id,
            )),
            annotation_ty,
        );
    }

    fn infer_for_statement(&mut self, for_statement: &ast::StmtFor) {
        let ast::StmtFor {
            range: _,
            target,
            iter,
            body,
            orelse,
            is_async: _,
        } = for_statement;

        self.infer_expression(iter);
        self.infer_expression(target);
        self.infer_body(body);
        self.infer_body(orelse);
    }

    fn infer_import_statement(&mut self, import: &ast::StmtImport) {
        let ast::StmtImport { range: _, names } = import;

        let import_id = import.scope_ast_id(self.db, self.file_id, self.file_scope_id);

        for (i, alias) in names.iter().enumerate() {
            let ast::Alias {
                range: _,
                name,
                asname: _,
            } = alias;

            let module_name = ModuleName::new(&name.id);
            let module = module_name.and_then(|name| resolve_module(self.db.upcast(), name));
            let module_ty = module
                .map(|module| TypeInferenceBuilder::module_ty(module.file()))
                .unwrap_or(Type::Unknown);

            self.definition_tys.insert(
                Definition::Import(ImportDefinition {
                    import_id,
                    alias: u32::try_from(i).unwrap(),
                }),
                module_ty,
            );
        }
    }

    fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        let ast::StmtImportFrom {
            range: _,
            module,
            names,
            level: _,
        } = import;

        let import_id = import.scope_ast_id(self.db, self.file_id, self.file_scope_id);
        let module_name = ModuleName::new(module.as_deref().expect("Support relative imports"));

        let module =
            module_name.and_then(|module_name| resolve_module(self.db.upcast(), module_name));
        let module_ty = module
            .map(|module| TypeInferenceBuilder::module_ty(module.file()))
            .unwrap_or(Type::Unknown);

        for (i, alias) in names.iter().enumerate() {
            let ast::Alias {
                range: _,
                name,
                asname: _,
            } = alias;

            let ty = module_ty
                .member(self.db, &Name::new(&name.id))
                .unwrap_or(Type::Unknown);

            self.definition_tys.insert(
                Definition::ImportFrom(ImportFromDefinition {
                    import_id,
                    name: u32::try_from(i).unwrap(),
                }),
                ty,
            );
        }
    }

    fn infer_decorator(&mut self, decorator: &ast::Decorator) -> Type {
        let ast::Decorator {
            range: _,
            expression,
        } = decorator;

        self.infer_expression(expression)
    }

    fn infer_arguments(&mut self, arguments: &ast::Arguments) -> Vec<Type> {
        let mut types = Vec::with_capacity(
            arguments
                .args
                .len()
                .saturating_add(arguments.keywords.len()),
        );

        types.extend(arguments.args.iter().map(|arg| self.infer_expression(arg)));

        types.extend(arguments.keywords.iter().map(
            |ast::Keyword {
                 range: _,
                 arg: _,
                 value,
             }| self.infer_expression(value),
        ));

        types
    }

    fn infer_expression(&mut self, expression: &ast::Expr) -> Type {
        let ty = match expression {
            ast::Expr::NoneLiteral(ast::ExprNoneLiteral { range: _ }) => Type::None,
            ast::Expr::NumberLiteral(literal) => self.infer_number_literal_expression(literal),
            ast::Expr::Name(name) => self.infer_name_expression(name),
            ast::Expr::Attribute(attribute) => self.infer_attribute_expression(attribute),
            ast::Expr::BinOp(binary) => self.infer_binary_expression(binary),
            ast::Expr::Named(named) => self.infer_named_expression(named),
            ast::Expr::If(if_expression) => self.infer_if_expression(if_expression),

            _ => todo!("expression type resolution for {:?}", expression),
        };

        self.types.expression_tys.push(ty);

        ty
    }

    #[allow(clippy::unused_self)]
    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type {
        let ast::ExprNumberLiteral { range: _, value } = literal;

        match value {
            ast::Number::Int(n) => {
                // TODO support big int literals
                n.as_i64().map(Type::IntLiteral).unwrap_or(Type::Unknown)
            }
            // TODO builtins.float or builtins.complex
            _ => Type::Unknown,
        }
    }

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type {
        let ast::ExprNamed {
            range: _,
            target,
            value,
        } = named;

        let value_ty = self.infer_expression(value);
        self.infer_expression(target);

        self.definition_tys.insert(
            Definition::NamedExpr(named.scope_ast_id(self.db, self.file_id, self.file_scope_id)),
            value_ty,
        );

        value_ty
    }

    fn infer_if_expression(&mut self, if_expression: &ast::ExprIf) -> Type {
        let ast::ExprIf {
            range: _,
            test,
            body,
            orelse,
        } = if_expression;

        self.infer_expression(test);

        // TODO detect statically known truthy or falsy test
        let body_ty = self.infer_expression(body);
        let orelse_ty = self.infer_expression(orelse);

        let union = UnionTypeBuilder::new(self.db)
            .add(body_ty)
            .add(orelse_ty)
            .build();

        self.union_ty(union)
    }

    fn infer_name_expression(&mut self, name: &ast::ExprName) -> Type {
        let ast::ExprName { range: _, id, ctx } = name;

        match ctx {
            ExprContext::Load => {
                if let Some(symbol_id) = self
                    .index
                    .symbol_table(self.file_scope_id)
                    .symbol_id_by_name(id)
                {
                    self.local_definition_ty(symbol_id)
                } else {
                    let ancestors = self.index.ancestor_scopes(self.file_scope_id).skip(1);

                    for (ancestor_id, _) in ancestors {
                        // TODO: Skip over class scopes unless the they are a immediately-nested type param scope.
                        // TODO: Support built-ins

                        let symbol_table =
                            symbol_table(self.db, ancestor_id.to_scope_id(self.db, self.file_id));

                        if let Some(_symbol_id) = symbol_table.symbol_id_by_name(id) {
                            todo!("Return type for symbol from outer scope");
                        }
                    }
                    Type::Unknown
                }
            }
            ExprContext::Del => Type::None,
            ExprContext::Invalid => Type::Unknown,
            ExprContext::Store => Type::None,
        }
    }

    fn infer_attribute_expression(&mut self, attribute: &ast::ExprAttribute) -> Type {
        let ast::ExprAttribute {
            value,
            attr,
            range: _,
            ctx,
        } = attribute;

        let value_ty = self.infer_expression(value);
        let member_ty = value_ty
            .member(self.db, &Name::new(&attr.id))
            .unwrap_or(Type::Unknown);

        match ctx {
            ExprContext::Load => member_ty,
            ExprContext::Store | ExprContext::Del => Type::None,
            ExprContext::Invalid => Type::Unknown,
        }
    }

    fn infer_binary_expression(&mut self, binary: &ast::ExprBinOp) -> Type {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = binary;

        let left_ty = self.infer_expression(left);
        let right_ty = self.infer_expression(right);

        match left_ty {
            Type::Any => Type::Any,
            Type::Unknown => Type::Unknown,
            Type::IntLiteral(n) => {
                match right_ty {
                    Type::IntLiteral(m) => {
                        match op {
                            ast::Operator::Add => n
                                .checked_add(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Sub => n
                                .checked_sub(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Mult => n
                                .checked_mul(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Div => n
                                .checked_div(m)
                                .map(Type::IntLiteral)
                                // TODO builtins.int
                                .unwrap_or(Type::Unknown),
                            ast::Operator::Mod => n
                                .checked_rem(m)
                                .map(Type::IntLiteral)
                                // TODO division by zero error
                                .unwrap_or(Type::Unknown),
                            _ => todo!("complete binop op support for IntLiteral"),
                        }
                    }
                    _ => todo!("complete binop right_ty support for IntLiteral"),
                }
            }
            _ => todo!("complete binop support"),
        }
    }

    fn infer_type_parameters(&mut self, _type_parameters: &TypeParams) {
        todo!("Infer type parameters")
    }

    pub(super) fn finish(mut self) -> TypeInference {
        let symbol_tys: IndexVec<_, _> = self
            .index
            .symbol_table(self.file_scope_id)
            .symbol_ids()
            .map(|symbol| self.local_definition_ty(symbol))
            .collect();

        self.types.symbol_tys = symbol_tys;
        self.types.shrink_to_fit();
        self.types
    }

    fn type_store(&self) -> Arc<FileTypeStore> {
        type_store_query(self.db, self.file_id).clone()
    }

    fn module_ty(file: VfsFile) -> Type {
        Type::Module(TypeId {
            file,
            local: FileModuleTypeId,
        })
    }

    fn union_ty(&self, ty: UnionType) -> Type {
        let mut store = self.type_store();
        let types = Arc::get_mut(&mut store).unwrap();
        Type::Union(TypeId {
            file: self.file_id,
            local: types.add_union(ty),
        })
    }

    fn function_ty(&self, ty: FunctionType) -> Type {
        let mut store = self.type_store();
        let types = Arc::get_mut(&mut store).unwrap();
        Type::Function(TypeId {
            file: self.file_id,
            local: types.add_function(ty),
        })
    }

    fn class_ty(&self, ty: ClassType) -> Type {
        let mut store = self.type_store();
        let types = Arc::get_mut(&mut store).unwrap();
        Type::Class(TypeId {
            file: self.file_id,
            local: types.add_class(ty),
        })
    }

    fn local_definition_ty(&mut self, symbol: ScopedSymbolId) -> Type {
        let symbol = self.symbol_table.symbol(symbol);
        let mut definitions = symbol
            .definitions()
            .iter()
            .filter_map(|definition| self.definition_tys.get(definition).copied());

        let Some(first) = definitions.next() else {
            return Type::Unbound;
        };

        if let Some(second) = definitions.next() {
            let mut builder = UnionTypeBuilder::new(self.db);
            builder = builder.add(first).add(second);

            for variant in definitions {
                builder = builder.add(variant);
            }

            self.union_ty(builder.build())
        } else {
            first
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::file_system::FileSystemPathBuf;
    use ruff_db::vfs::system_path_to_file;

    use crate::db::tests::TestDb;
    use crate::name::Name;
    use crate::types::{public_symbol_ty_by_name, Type};
    use red_knot_module_resolver::{set_module_resolution_settings, ModuleResolutionSettings};

    fn setup_db() -> TestDb {
        let mut db = TestDb::new();

        set_module_resolution_settings(
            &mut db,
            ModuleResolutionSettings {
                extra_paths: Vec::new(),
                workspace_root: FileSystemPathBuf::from("/src"),
                site_packages: None,
                custom_typeshed: None,
            },
        );

        db
    }

    fn assert_public_ty(db: &TestDb, file_name: &str, symbol_name: &str, expected: &str) {
        let file = system_path_to_file(db, file_name).expect("Expected file to exist.");

        let ty = public_symbol_ty_by_name(db, file, symbol_name).unwrap_or(Type::Unknown);
        assert_eq!(ty.display(db).to_string(), expected);
    }

    #[test]
    fn follow_import_to_class() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_files([
            ("src/a.py", "from b import C as D; E = D"),
            ("src/b.py", "class C: pass"),
        ])?;

        assert_public_ty(&db, "src/a.py", "E", "Literal[C]");

        Ok(())
    }

    #[test]
    fn resolve_base_class_by_name() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file(
            "src/mod.py",
            r#"
class Base:
    pass

class Sub(Base):
    pass"#,
        )?;

        let mod_file = system_path_to_file(&db, "src/mod.py").expect("Expected file to exist.");
        let ty = public_symbol_ty_by_name(&db, mod_file, "Sub").expect("Symbol type to exist");

        let Type::Class(class_id) = ty else {
            panic!("Sub is not a Class")
        };

        let base_names: Vec<_> = class_id
            .lookup(&db)
            .bases()
            .iter()
            .map(|base_ty| format!("{}", base_ty.display(&db)))
            .collect();

        assert_eq!(base_names, vec!["Literal[Base]"]);

        Ok(())
    }

    #[test]
    fn resolve_method() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file(
            "src/mod.py",
            "
class C:
    def f(self): pass
            ",
        )?;

        let mod_file = system_path_to_file(&db, "src/mod.py").unwrap();
        let ty = public_symbol_ty_by_name(&db, mod_file, "C").unwrap();

        let Type::Class(class_id) = ty else {
            panic!("C is not a Class");
        };

        let member_ty = class_id.class_member(&db, &Name::new("f"));

        let Some(Type::Function(func_id)) = member_ty else {
            panic!("C.f is not a Function");
        };

        let function_ty = func_id.lookup(&db);
        assert_eq!(function_ty.name(), "f");

        Ok(())
    }

    #[test]
    fn resolve_module_member() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_files([
            ("src/a.py", "import b; D = b.C"),
            ("src/b.py", "class C: pass"),
        ])?;

        assert_public_ty(&db, "src/a.py", "D", "Literal[C]");

        Ok(())
    }

    #[test]
    fn resolve_literal() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file("src/a.py", "x = 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1]");

        Ok(())
    }

    #[test]
    fn resolve_union() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file(
            "src/a.py",
            "
if flag:
    x = 1
else:
    x = 2
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn literal_int_arithmetic() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file(
            "src/a.py",
            "
a = 2 + 1
b = a - 4
c = a * b
d = c / 3
e = 5 % 3
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "a", "Literal[3]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "c", "Literal[-3]");
        assert_public_ty(&db, "src/a.py", "d", "Literal[-1]");
        assert_public_ty(&db, "src/a.py", "e", "Literal[2]");

        Ok(())
    }

    #[test]
    fn walrus() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system()
            .write_file("src/a.py", "x = (y := 1) + 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[2]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn ifexpr() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system()
            .write_file("src/a.py", "x = 1 if flag else 2")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn ifexpr_walrus() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system().write_file(
            "src/a.py",
            "
y = z = 0
x = (y := 1) if flag else (z := 2)
a = y
b = z
            ",
        )?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");
        assert_public_ty(&db, "src/a.py", "a", "Literal[0, 1]");
        assert_public_ty(&db, "src/a.py", "b", "Literal[0, 2]");

        Ok(())
    }

    #[test]
    fn ifexpr_nested() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system()
            .write_file("src/a.py", "x = 1 if flag else 2 if flag2 else 3")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2, 3]");

        Ok(())
    }

    #[test]
    fn none() -> anyhow::Result<()> {
        let db = setup_db();

        db.memory_file_system()
            .write_file("src/a.py", "x = 1 if flag else None")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1] | None");
        Ok(())
    }
}
