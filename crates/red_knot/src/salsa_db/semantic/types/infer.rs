use std::sync::Arc;

use tracing::debug;

use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{
    visitor, Expr, ExprAttribute, ExprNumberLiteral, ModModule, Number, Stmt, StmtAnnAssign,
    StmtAssign, StmtClassDef, StmtFunctionDef, StmtImport, StmtImportFrom,
};

use crate::ast_ids::NodeKey;
use crate::module::ModuleName;
use crate::salsa_db::semantic::ast_ids::{AstIdNode, ExpressionId};
use crate::salsa_db::semantic::definition::{ImportDefinition, ImportFromDefinition};
use crate::salsa_db::semantic::flow_graph::{flow_graph, FlowGraph, ReachableDefinition};
use crate::salsa_db::semantic::module::resolve_module_name;
use crate::salsa_db::semantic::symbol_table::{
    symbol_table, NodeWithScopeId, ScopeId, SymbolTable,
};
use crate::salsa_db::semantic::types::{
    typing_scopes, ClassType, ClassTypeId, ClassTypingScope, FunctionType, FunctionTypeId,
    FunctionTypingScope, GlobalTypeId, LocalTypeId, ModuleType, ModuleTypeId, Type, TypeInference,
    TypingContext, TypingScope, TypingScopes,
};
use crate::salsa_db::semantic::{global_symbol_type_by_name, Db, GlobalId, Jar};
use crate::salsa_db::source::{parse, File};
use crate::Name;

pub fn infer_expression_type(db: &dyn Db, expression_id: GlobalId<ExpressionId>) -> Type {
    let typing_scope = TypingScope::for_expression(db, expression_id);
    let types = infer_types(db, typing_scope);

    types.expression_ty(expression_id.local())
}

#[tracing::instrument(level = "debug", skip(db))]
pub fn infer_types(db: &dyn Db, scope: TypingScope) -> &TypeInference {
    match scope {
        TypingScope::Function(function) => infer_function_body(db, function),
        TypingScope::Class(class) => infer_class_body(db, class),
        TypingScope::Module(file) => infer_module_body(db, file),
    }
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar, return_ref)]
pub fn infer_function_body(db: &dyn Db, scope: FunctionTypingScope) -> TypeInference {
    let function = scope.node(db);

    let mut builder = TypeInferenceBuilder::new(db, scope.into());
    builder.lower_function_body(&function);
    builder.finish()
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar, return_ref)]
pub fn infer_class_body(db: &dyn Db, scope: ClassTypingScope) -> TypeInference {
    let class = scope.node(db);

    let mut builder = TypeInferenceBuilder::new(db, scope.into());
    builder.lower_class_body(&class);
    builder.finish()
}

#[tracing::instrument(level = "debug", skip(db))]
#[salsa::tracked(jar=Jar, return_ref)]
pub fn infer_module_body(db: &dyn Db, file: File) -> TypeInference {
    debug!("before parse");
    let parsed = parse(db.upcast(), file);

    let mut builder = TypeInferenceBuilder::new(db, file.into());
    builder.lower_module(&parsed.syntax());
    builder.finish()
}

struct TypeInferenceBuilder<'a> {
    db: &'a dyn Db,
    typing_scope: TypingScope,
    file: File,
    enclosing_scope: ScopeId,

    symbol_table: Arc<SymbolTable>,
    control_flow_graph: Arc<FlowGraph>,
    typing_scopes: &'a TypingScopes,

    // TODO: This is going to be somewhat slow because we need to map the AST node to the expression id for
    //   every expression in the body. That's a lot of hash map lookups.
    //  We can't use an `IndexVec` here because a) expression ids are per module and b) the type inference
    //  builder visits the expressions in evaluation order and not in pre-order.
    result: TypeInference,
}

impl<'a> TypeInferenceBuilder<'a> {
    fn new(db: &'a dyn Db, scope: TypingScope) -> Self {
        let file = scope.file(db);
        let symbol_table = symbol_table(db, file);
        let control_flow_graph = flow_graph(db, file);

        let symbol_scope = match scope {
            TypingScope::Function(function) => {
                symbol_table.scope_id_for_node(NodeWithScopeId::Function(function.id(db)))
            }
            TypingScope::Class(class) => {
                symbol_table.scope_id_for_node(NodeWithScopeId::Class(class.id(db)))
            }
            TypingScope::Module(file) => SymbolTable::root_scope_id(),
        };

        let builder = TypeInferenceBuilder {
            db,
            typing_scope: scope,
            file,
            enclosing_scope: symbol_scope,

            symbol_table,
            control_flow_graph,
            typing_scopes: typing_scopes(db, file),

            result: TypeInference::default(),
        };
        builder
    }

    fn typing_context(&self) -> TypingContext {
        TypingContext::local(self.db, self.typing_scope, &self.result)
    }

    fn lower_module(&mut self, module: &ModModule) {
        self.result.module = Some(ModuleType::new(self.typing_scope.file(self.db)));

        self.visit_body(&module.body);
    }

    fn lower_class_body(&mut self, class: &StmtClassDef) {
        self.visit_body(&class.body);
    }

    fn lower_function_body(&mut self, function: &StmtFunctionDef) {
        self.visit_body(&function.body);
    }

    fn lower_import(&mut self, import: &StmtImport) {
        let import_id = import.ast_id(self.db, self.file);

        for (i, name) in import.names.iter().enumerate() {
            let ty = if let Some(module) = resolve_module_name(self.db, ModuleName::new(&name.name))
            {
                // TODO: It feels hacky to resolve the module ID out of the blue like this.
                Type::Module(GlobalTypeId::new(
                    TypingScope::Module(module.file()),
                    ModuleTypeId,
                ))
            } else {
                Type::Unknown
            };

            self.result.local_definitions.insert(
                ImportDefinition {
                    import: import_id,
                    name: u32::try_from(i).unwrap(),
                }
                .into(),
                ty,
            );
        }
    }

    fn lower_import_from(&mut self, import: &StmtImportFrom) {
        assert!(matches!(import.level, 0));

        let import_id = import.ast_id(self.db, self.file);
        let module_name = import.module.as_ref().expect("TODO relative imports");
        let module = resolve_module_name(self.db, ModuleName::new(module_name));

        for (i, name) in import.names.iter().enumerate() {
            let ty = if let Some(module) = &module {
                global_symbol_type_by_name(self.db, module.file(), &name.name)
                    .unwrap_or(Type::Unknown)
            } else {
                Type::Unknown
            };

            self.result.local_definitions.insert(
                ImportFromDefinition {
                    import: import_id,
                    name: u32::try_from(i).unwrap(),
                }
                .into(),
                ty,
            );
        }
    }

    fn lower_class_definition(&mut self, class_def: &StmtClassDef) {
        let bases = class_def
            .bases()
            .iter()
            .map(|base| self.infer_expression(base))
            .collect();

        let class_id = class_def.ast_id(self.db, self.file);

        // TODO decorators

        let class_type = ClassTypeId::intern(
            ClassType {
                name: Name::new(&class_def.name.id),
                bases,
                typing_scope: self.typing_scopes[class_id],
            },
            &mut self.result,
        );

        let ty = Type::Class(GlobalTypeId::new(self.typing_scope, class_type));

        self.result.local_definitions.insert(class_id.into(), ty);
    }

    fn lower_function_definition(&mut self, function_def: &StmtFunctionDef) {
        let decorators = function_def
            .decorator_list
            .iter()
            .map(|decorator| self.infer_expression(&decorator.expression))
            .collect();

        // TODO lower parameters, return type, etc.

        let function_id = function_def.ast_id(self.db, self.file);
        let function_type = FunctionTypeId::intern(
            FunctionType {
                name: Name::new(&function_def.name.id),
                typing_scope: self.typing_scopes[function_id],
                decorators,
            },
            &mut self.result,
        );

        self.result.local_definitions.insert(
            function_id.into(),
            Type::Function(GlobalTypeId::new(self.typing_scope, function_type)),
        );
    }

    fn lower_assignment(&mut self, assignment: &StmtAssign) {
        let value_ty = self.infer_expression(&assignment.value);

        for target in &assignment.targets {
            self.result
                .expression_types
                .insert(target.ast_id(self.db, self.file), value_ty);
        }

        let assignment_id = assignment.ast_id(self.db, self.file);
        self.result
            .local_definitions
            .insert(assignment_id.into(), value_ty);
    }

    fn lower_annotated_assignment(&mut self, assignment: &StmtAnnAssign) {
        // TODO actually look at the annotation
        let value_ty = if let Some(value) = &assignment.value {
            self.infer_expression(value)
        } else {
            Type::Unknown
        };

        self.result
            .expression_types
            .insert(assignment.target.ast_id(self.db, self.file), value_ty);

        let assignment_id = assignment.ast_id(self.db, self.file);

        self.result
            .local_definitions
            .insert(assignment_id.into(), value_ty);
    }

    fn infer_expression(&mut self, expr: &Expr) -> Type {
        let id = expr.ast_id(self.db, self.file);

        let ty = match expr {
            Expr::NumberLiteral(ExprNumberLiteral { value, .. }) => {
                match value {
                    Number::Int(n) => {
                        // TODO support big int literals
                        n.as_i64().map(Type::IntLiteral).unwrap_or(Type::Unknown)
                    }
                    // TODO builtins.float or builtins.complex
                    _ => Type::Unknown,
                }
            }
            Expr::Name(name) => {
                let Some((scope, symbol)) = self
                    .symbol_table
                    .ancestors(self.enclosing_scope)
                    .find_map(|(scope_id, _)| {
                        Some((
                            scope_id,
                            self.symbol_table.symbol_id_by_name(scope_id, &name.id)?,
                        ))
                    })
                else {
                    return Type::Unknown;
                };

                let definition_type = self.typing_context().infer_definitions(
                    self.control_flow_graph.reachable_definitions(symbol, id),
                    GlobalId::new(self.file, scope),
                );

                definition_type.into_type(&mut self.result)
            }
            Expr::Attribute(ExprAttribute { value, attr, .. }) => {
                let value_type = self.infer_expression(value);
                let attr_name = &Name::new(&attr.id);
                value_type
                    .member(&self.typing_context(), attr_name)
                    .unwrap_or(Type::Unknown)
            }
            _ => todo!("full expression type resolution"),
        };

        let existing = self.result.expression_types.insert(id, ty);
        debug_assert_eq!(existing, None);

        ty
    }

    fn finish(mut self) -> TypeInference {
        let symbol_table = &self.symbol_table;
        let mut public_symbol_types = std::mem::take(&mut self.result.public_symbol_types);

        for symbol in symbol_table.symbol_ids_for_scope(self.enclosing_scope) {
            let definition_type = self.typing_context().infer_definitions(
                symbol_table
                    .definitions(symbol)
                    .iter()
                    .map(|definition| ReachableDefinition::Definition(*definition)),
                GlobalId::new(self.file, self.enclosing_scope),
            );

            public_symbol_types.insert(symbol, definition_type.into_type(&mut self.result));
        }

        self.result.public_symbol_types = public_symbol_types;
        self.result.shrink_to_fit();

        self.result
    }
}

impl Visitor<'_> for TypeInferenceBuilder<'_> {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(function_def) => {
                self.lower_function_definition(function_def);
            }
            Stmt::ClassDef(class_def) => {
                self.lower_class_definition(class_def);
            }
            Stmt::Assign(assignment) => self.lower_assignment(assignment),

            Stmt::AnnAssign(assignment) => self.lower_annotated_assignment(assignment),
            Stmt::Return(_) | Stmt::Delete(_) | Stmt::AugAssign(_) => {}
            Stmt::Expr(expression) => {
                self.infer_expression(&expression.value);
            }
            Stmt::Import(import) => self.lower_import(import),
            Stmt::ImportFrom(import_from) => self.lower_import_from(import_from),

            Stmt::TypeAlias(_)
            | Stmt::For(_)
            | Stmt::While(_)
            | Stmt::If(_)
            | Stmt::With(_)
            | Stmt::Match(_)
            | Stmt::Raise(_)
            | Stmt::Try(_)
            | Stmt::Assert(_)
            | Stmt::Global(_)
            | Stmt::Nonlocal(_)
            | Stmt::Pass(_)
            | Stmt::Break(_)
            | Stmt::Continue(_)
            | Stmt::IpyEscapeCommand(_) => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        self.infer_expression(expr);

        // No need to walk the expression here because the type inference will do the walking itself.
    }
}
