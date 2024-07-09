use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::sync::Arc;

use red_knot_module_resolver::{resolve_module, ModuleName};
use ruff_db::files::File;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::{ExprContext, TypeParams};

use crate::semantic_index::ast_ids::ScopedExpressionId;
use crate::semantic_index::definition::{Definition, DefinitionNodeRef};
use crate::semantic_index::symbol::{
    FileScopeId, NodeWithScopeRef, ScopeId, ScopedSymbolId, SymbolTable,
};
use crate::semantic_index::{symbol_table, SemanticIndex};
use crate::types::{infer_types, ClassType, FunctionType, Name, Type, UnionTypeBuilder};
use crate::Db;

/// The inferred types for a single scope.
#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub(crate) struct TypeInference<'db> {
    /// The types of every expression in this scope.
    expressions: IndexVec<ScopedExpressionId, Type<'db>>,

    /// The public types of every symbol in this scope.
    symbols: IndexVec<ScopedSymbolId, Type<'db>>,

    /// The type of a definition.
    definitions: FxHashMap<Definition<'db>, Type<'db>>,
}

impl<'db> TypeInference<'db> {
    #[allow(unused)]
    pub(crate) fn expression_ty(&self, expression: ScopedExpressionId) -> Type<'db> {
        self.expressions[expression]
    }

    pub(super) fn symbol_ty(&self, symbol: ScopedSymbolId) -> Type<'db> {
        self.symbols[symbol]
    }

    pub(crate) fn definition_ty(&self, definition: Definition<'db>) -> Type<'db> {
        self.definitions[&definition]
    }

    fn shrink_to_fit(&mut self) {
        self.expressions.shrink_to_fit();
        self.symbols.shrink_to_fit();
        self.definitions.shrink_to_fit();
    }
}

/// Builder to infer all types in a [`ScopeId`].
pub(super) struct TypeInferenceBuilder<'db> {
    db: &'db dyn Db,

    // Cached lookups
    index: &'db SemanticIndex<'db>,
    file_scope_id: FileScopeId,
    file_id: File,
    symbol_table: Arc<SymbolTable<'db>>,

    /// The type inference results
    types: TypeInference<'db>,
}

impl<'db> TypeInferenceBuilder<'db> {
    /// Creates a new builder for inferring the types of `scope`.
    pub(super) fn new(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        index: &'db SemanticIndex<'db>,
    ) -> Self {
        let file_scope_id = scope.file_scope_id(db);
        let file = scope.file(db);
        let symbol_table = index.symbol_table(file_scope_id);

        Self {
            index,
            file_scope_id,
            file_id: file,
            symbol_table,

            db,
            types: TypeInference::default(),
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

        let decorator_tys = decorator_list
            .iter()
            .map(|decorator| self.infer_decorator(decorator))
            .collect();

        // TODO: Infer parameters

        if let Some(return_ty) = returns {
            self.infer_expression(return_ty);
        }

        let function_ty =
            Type::Function(FunctionType::new(self.db, name.id.clone(), decorator_tys));

        let definition = self.index.definition(function);
        self.types.definitions.insert(definition, function_ty);
    }

    fn infer_class_definition_statement(&mut self, class: &ast::StmtClassDef) {
        let ast::StmtClassDef {
            range: _,
            name,
            type_params: _,
            decorator_list,
            arguments,
            body: _,
        } = class;

        for decorator in decorator_list {
            self.infer_decorator(decorator);
        }

        let bases = arguments
            .as_deref()
            .map(|arguments| self.infer_arguments(arguments))
            .unwrap_or(Vec::new());

        let body_scope = self
            .index
            .node_scope(NodeWithScopeRef::Class(class))
            .to_scope_id(self.db, self.file_id);

        let class_ty = Type::Class(ClassType::new(self.db, name.id.clone(), bases, body_scope));

        let definition = self.index.definition(class);
        self.types.definitions.insert(definition, class_ty);
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

            self.types.definitions.insert(
                self.index.definition(DefinitionNodeRef::Target(target)),
                value_ty,
            );
        }
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

        self.types.definitions.insert(
            self.index.definition(DefinitionNodeRef::Target(target)),
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

        for alias in names {
            let ast::Alias {
                range: _,
                name,
                asname: _,
            } = alias;

            let module_name = ModuleName::new(&name.id);
            let module = module_name.and_then(|name| resolve_module(self.db.upcast(), name));
            let module_ty = module
                .map(|module| Type::Module(module.file()))
                .unwrap_or(Type::Unknown);

            let definition = self.index.definition(alias);

            self.types.definitions.insert(definition, module_ty);
        }
    }

    fn infer_import_from_statement(&mut self, import: &ast::StmtImportFrom) {
        let ast::StmtImportFrom {
            range: _,
            module,
            names,
            level: _,
        } = import;

        let module_name = ModuleName::new(module.as_deref().expect("Support relative imports"));

        let module =
            module_name.and_then(|module_name| resolve_module(self.db.upcast(), module_name));
        let module_ty = module
            .map(|module| Type::Module(module.file()))
            .unwrap_or(Type::Unknown);

        for alias in names {
            let ast::Alias {
                range: _,
                name,
                asname: _,
            } = alias;

            let ty = module_ty
                .member(self.db, &Name::new(&name.id))
                .unwrap_or(Type::Unknown);

            let definition = self.index.definition(alias);
            self.types.definitions.insert(definition, ty);
        }
    }

    fn infer_decorator(&mut self, decorator: &ast::Decorator) -> Type<'db> {
        let ast::Decorator {
            range: _,
            expression,
        } = decorator;

        self.infer_expression(expression)
    }

    fn infer_arguments(&mut self, arguments: &ast::Arguments) -> Vec<Type<'db>> {
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

    fn infer_expression(&mut self, expression: &ast::Expr) -> Type<'db> {
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

        self.types.expressions.push(ty);

        ty
    }

    #[allow(clippy::unused_self)]
    fn infer_number_literal_expression(&mut self, literal: &ast::ExprNumberLiteral) -> Type<'db> {
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

    fn infer_named_expression(&mut self, named: &ast::ExprNamed) -> Type<'db> {
        let ast::ExprNamed {
            range: _,
            target,
            value,
        } = named;

        let value_ty = self.infer_expression(value);
        self.infer_expression(target);

        self.types
            .definitions
            .insert(self.index.definition(named), value_ty);

        value_ty
    }

    fn infer_if_expression(&mut self, if_expression: &ast::ExprIf) -> Type<'db> {
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

        Type::Union(union)
    }

    fn infer_name_expression(&mut self, name: &ast::ExprName) -> Type<'db> {
        let ast::ExprName { range: _, id, ctx } = name;

        match ctx {
            ExprContext::Load => {
                let ancestors = self.index.ancestor_scopes(self.file_scope_id);

                for (ancestor_id, _) in ancestors {
                    // TODO: Skip over class scopes unless the they are a immediately-nested type param scope.
                    // TODO: Support built-ins

                    let (symbol_table, ancestor_scope) = if ancestor_id == self.file_scope_id {
                        (Cow::Borrowed(&self.symbol_table), None)
                    } else {
                        let ancestor_scope = ancestor_id.to_scope_id(self.db, self.file_id);
                        (
                            Cow::Owned(symbol_table(self.db, ancestor_scope)),
                            Some(ancestor_scope),
                        )
                    };

                    if let Some(symbol_id) = symbol_table.symbol_id_by_name(id) {
                        let symbol = symbol_table.symbol(symbol_id);

                        if !symbol.is_defined() {
                            continue;
                        }

                        return if let Some(ancestor_scope) = ancestor_scope {
                            let types = infer_types(self.db, ancestor_scope);
                            types.symbol_ty(symbol_id)
                        } else {
                            self.local_definition_ty(symbol_id)
                        };
                    }
                }
                Type::Unknown
            }
            ExprContext::Del => Type::None,
            ExprContext::Invalid => Type::Unknown,
            ExprContext::Store => Type::None,
        }
    }

    fn infer_attribute_expression(&mut self, attribute: &ast::ExprAttribute) -> Type<'db> {
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

    fn infer_binary_expression(&mut self, binary: &ast::ExprBinOp) -> Type<'db> {
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

    pub(super) fn finish(mut self) -> TypeInference<'db> {
        let symbol_tys: IndexVec<_, _> = self
            .index
            .symbol_table(self.file_scope_id)
            .symbol_ids()
            .map(|symbol| self.local_definition_ty(symbol))
            .collect();

        self.types.symbols = symbol_tys;
        self.types.shrink_to_fit();
        self.types
    }

    fn local_definition_ty(&mut self, symbol: ScopedSymbolId) -> Type<'db> {
        let symbol = self.symbol_table.symbol(symbol);
        let mut definitions = symbol
            .definitions()
            .iter()
            .filter_map(|definition| self.types.definitions.get(definition).copied());

        let Some(first) = definitions.next() else {
            return Type::Unbound;
        };

        if let Some(second) = definitions.next() {
            let mut builder = UnionTypeBuilder::new(self.db);
            builder = builder.add(first).add(second);

            for variant in definitions {
                builder = builder.add(variant);
            }

            Type::Union(builder.build())
        } else {
            first
        }
    }
}

#[cfg(test)]
mod tests {
    use red_knot_module_resolver::{
        set_module_resolution_settings, RawModuleResolutionSettings, TargetVersion,
    };
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_python_ast::name::Name;

    use crate::db::tests::TestDb;
    use crate::types::{public_symbol_ty_by_name, Type};

    fn setup_db() -> TestDb {
        let mut db = TestDb::new();

        set_module_resolution_settings(
            &mut db,
            RawModuleResolutionSettings {
                target_version: TargetVersion::Py38,
                extra_paths: Vec::new(),
                workspace_root: SystemPathBuf::from("/src"),
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
        let mut db = setup_db();

        db.write_files([
            ("src/a.py", "from b import C as D; E = D"),
            ("src/b.py", "class C: pass"),
        ])?;

        assert_public_ty(&db, "src/a.py", "E", "Literal[C]");

        Ok(())
    }

    #[test]
    fn resolve_base_class_by_name() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
            "src/mod.py",
            r#"
class Base:
    pass

class Sub(Base):
    pass"#,
        )?;

        let mod_file = system_path_to_file(&db, "src/mod.py").expect("Expected file to exist.");
        let ty = public_symbol_ty_by_name(&db, mod_file, "Sub").expect("Symbol type to exist");

        let Type::Class(class) = ty else {
            panic!("Sub is not a Class")
        };

        let base_names: Vec<_> = class
            .bases(&db)
            .iter()
            .map(|base_ty| format!("{}", base_ty.display(&db)))
            .collect();

        assert_eq!(base_names, vec!["Literal[Base]"]);

        Ok(())
    }

    #[test]
    fn resolve_method() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
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

        let member_ty = class_id.class_member(&db, &Name::new_static("f"));

        let Some(Type::Function(func)) = member_ty else {
            panic!("C.f is not a Function");
        };

        assert_eq!(func.name(&db), "f");

        Ok(())
    }

    #[test]
    fn resolve_module_member() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_files([
            ("src/a.py", "import b; D = b.C"),
            ("src/b.py", "class C: pass"),
        ])?;

        assert_public_ty(&db, "src/a.py", "D", "Literal[C]");

        Ok(())
    }

    #[test]
    fn resolve_literal() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1]");

        Ok(())
    }

    #[test]
    fn resolve_union() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
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
        let mut db = setup_db();

        db.write_file(
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
        let mut db = setup_db();

        db.write_file("src/a.py", "x = (y := 1) + 1")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[2]");
        assert_public_ty(&db, "src/a.py", "y", "Literal[1]");

        Ok(())
    }

    #[test]
    fn ifexpr() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1 if flag else 2")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2]");

        Ok(())
    }

    #[test]
    fn ifexpr_walrus() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file(
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
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1 if flag else 2 if flag2 else 3")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1, 2, 3]");

        Ok(())
    }

    #[test]
    fn none() -> anyhow::Result<()> {
        let mut db = setup_db();

        db.write_file("src/a.py", "x = 1 if flag else None")?;

        assert_public_ty(&db, "src/a.py", "x", "Literal[1] | None");
        Ok(())
    }
}
