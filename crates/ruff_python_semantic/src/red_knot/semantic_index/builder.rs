use std::sync::Arc;

use hashbrown::hash_map::RawEntryMut;
use rustc_hash::FxHashMap;

use ruff_db::parsed::ParsedModule;
use ruff_index::IndexVec;
use ruff_python_ast as ast;
use ruff_python_ast::visitor::{walk_expr, walk_stmt, Visitor};

use crate::module::ModuleName;
use crate::name::Name;
use crate::red_knot::node_key::NodeKey;
use crate::red_knot::semantic_index::ast_ids::{
    AstIdsBuilder, LocalClassId, LocalFunctionId, LocalImportFromId, LocalImportId,
};
use crate::red_knot::semantic_index::definition::{
    Definition, ImportDefinition, ImportFromDefinition,
};
use crate::red_knot::semantic_index::symbol::{LocalSymbolId, Symbol, SymbolFlags, SymbolId};
use crate::red_knot::semantic_index::{
    NodeWithScope, Scope, ScopeId, ScopeKind, SemanticIndex, SymbolTable,
};

pub(super) struct SemanticIndexBuilder<'a> {
    // Builder state
    module: &'a ParsedModule,
    scope_stack: Vec<ScopeId>,

    // Semantic Index fields
    scopes: IndexVec<ScopeId, Scope>,
    symbol_tables: IndexVec<ScopeId, SymbolTable>,
    ast_ids: IndexVec<ScopeId, AstIdsBuilder>,
    expression_scopes: FxHashMap<NodeKey, ScopeId>,
    scopes_by_node: FxHashMap<NodeWithScope, ScopeId>,
}

impl<'a> SemanticIndexBuilder<'a> {
    pub(super) fn new(parsed: &'a ParsedModule) -> Self {
        let mut builder = Self {
            module: parsed,
            scopes: IndexVec::new(),
            symbol_tables: IndexVec::new(),
            scope_stack: Vec::new(),
            ast_ids: IndexVec::new(),
            expression_scopes: FxHashMap::default(),
            scopes_by_node: FxHashMap::default(),
        };

        builder.push_scope(ScopeKind::Module, &Name::new_static("<module>"), None, None);

        builder
    }

    fn current_scope(&self) -> ScopeId {
        *self
            .scope_stack
            .last()
            .expect("Always to have a root scope")
    }

    fn push_scope(
        &mut self,
        scope_kind: ScopeKind,
        name: &Name,
        defining_symbol: Option<LocalSymbolId>,
        definition: Option<Definition>,
    ) {
        let children_start = self.scopes.next_index() + 1;
        let parent = self.current_scope();

        let scope = Scope {
            name: name.clone(),
            parent: Some(parent),
            defining_symbol: defining_symbol.map(|local_id| SymbolId::new(parent, local_id)),
            definition,
            kind: scope_kind,
            descendents: children_start..children_start,
        };

        let scope_id = self.scopes.push(scope);
        self.symbol_tables.push(SymbolTable::new(scope_id));
        self.ast_ids.push(AstIdsBuilder::new());
        self.scope_stack.push(scope_id);
    }

    fn pop_scope(&mut self) -> ScopeId {
        let id = self.scope_stack.pop().expect("Root scope to be present");
        let children_end = self.scopes.next_index();
        let scope = &mut self.scopes[id];
        scope.descendents = scope.descendents.start..children_end;
        id
    }

    fn current_symbol_table(&mut self) -> &mut SymbolTable {
        let scope_id = self.current_scope();
        &mut self.symbol_tables[scope_id]
    }

    fn current_ast_ids(&mut self) -> &mut AstIdsBuilder {
        let scope_id = self.current_scope();
        &mut self.ast_ids[scope_id]
    }

    fn add_or_update_symbol(
        &mut self,
        name: Name,
        definition: Option<Definition>,
    ) -> LocalSymbolId {
        self.add_or_update_symbol_with_flags(name, SymbolFlags::IS_DEFINED, definition)
    }

    fn add_or_update_symbol_with_flags(
        &mut self,
        name: Name,
        flags: SymbolFlags,
        definition: Option<Definition>,
    ) -> LocalSymbolId {
        let scope = self.current_scope();
        let symbol_table = self.current_symbol_table();

        let hash = SymbolTable::hash_name(&name);
        let entry = symbol_table
            .symbols_by_name
            .raw_entry_mut()
            .from_hash(hash, |id| symbol_table.symbols[*id].name() == &name);

        match entry {
            RawEntryMut::Occupied(entry) => {
                let symbol = &mut symbol_table.symbols[*entry.key()];
                symbol.insert_flags(flags);

                if let Some(definition) = definition {
                    symbol.push_definition(definition);
                }

                *entry.key()
            }
            RawEntryMut::Vacant(entry) => {
                let mut symbol = Symbol::new(name, scope, definition);
                symbol.insert_flags(flags);

                let id = symbol_table.symbols.push(symbol);
                entry.insert_with_hasher(hash, id, (), |id| {
                    SymbolTable::hash_name(symbol_table.symbols[*id].name().as_str())
                });
                id
            }
        }
    }

    fn with_type_params(
        &mut self,
        name: &Name,
        params: &Option<Box<ast::TypeParams>>,
        definition: Option<Definition>,
        defining_symbol: LocalSymbolId,
        nested: impl FnOnce(&mut Self) -> ScopeId,
    ) -> ScopeId {
        if let Some(type_params) = params {
            self.push_scope(
                ScopeKind::Annotation,
                name,
                Some(defining_symbol),
                definition,
            );
            for type_param in &type_params.type_params {
                let name = match type_param {
                    ast::TypeParam::TypeVar(ast::TypeParamTypeVar { name, .. }) => name,
                    ast::TypeParam::ParamSpec(ast::TypeParamParamSpec { name, .. }) => name,
                    ast::TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { name, .. }) => name,
                };
                self.add_or_update_symbol(Name::new(name), None);
            }
        }
        let nested_scope = nested(self);

        if params.is_some() {
            self.pop_scope();
        }

        nested_scope
    }

    pub(super) fn build(mut self) -> SemanticIndex {
        let module = self.module;
        self.visit_body(module.suite());

        // Pop the root scope
        self.pop_scope();
        assert!(self.scope_stack.is_empty());

        let mut symbol_tables: IndexVec<_, _> = self
            .symbol_tables
            .into_iter()
            .map(|mut table| {
                table.shrink_to_fit();
                Arc::new(table)
            })
            .collect();

        let mut ast_ids: IndexVec<_, _> = self
            .ast_ids
            .into_iter()
            .map(|builder| Arc::new(builder.finish()))
            .collect();

        self.scopes.shrink_to_fit();
        ast_ids.shrink_to_fit();
        symbol_tables.shrink_to_fit();
        self.expression_scopes.shrink_to_fit();
        self.scopes_by_node.shrink_to_fit();

        SemanticIndex {
            symbol_tables,
            scopes: self.scopes,
            ast_ids,
            expression_scopes: self.expression_scopes,
            scopes_by_node: self.scopes_by_node,
        }
    }
}

impl Visitor<'_> for SemanticIndexBuilder<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        let module = self.module;
        #[allow(unsafe_code)]
        let statement_id = unsafe {
            // SAFETY: The builder only visits nodes that are part of `module`. This guarantees that
            // the current statement must be a child of `module`.
            self.current_ast_ids().record_statement(stmt, module)
        };
        match stmt {
            ast::Stmt::FunctionDef(function_def) => {
                for decorator in &function_def.decorator_list {
                    self.visit_decorator(decorator);
                }
                let name = Name::new(&function_def.name.id);
                let definition = Definition::FunctionDef(LocalFunctionId(statement_id));
                let symbol = self.add_or_update_symbol(name.clone(), Some(definition));

                self.with_type_params(
                    &name,
                    &function_def.type_params,
                    Some(definition),
                    symbol,
                    |builder| {
                        builder.visit_parameters(&function_def.parameters);
                        for expr in &function_def.returns {
                            builder.visit_annotation(expr);
                        }

                        builder.push_scope(
                            ScopeKind::Function,
                            &name,
                            Some(symbol),
                            Some(definition),
                        );
                        builder.visit_body(&function_def.body);
                        let function_scope = builder.pop_scope();
                        builder
                            .scopes_by_node
                            .insert(function_def.into(), function_scope);
                        function_scope
                    },
                );
            }
            ast::Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }

                let name = Name::new(&class.name.id);
                let definition = Definition::from(LocalClassId(statement_id));
                let id = self.add_or_update_symbol(name.clone(), Some(definition));
                self.with_type_params(&name, &class.type_params, Some(definition), id, |builder| {
                    if let Some(arguments) = &class.arguments {
                        builder.visit_arguments(arguments);
                    }

                    builder.push_scope(ScopeKind::Class, &name, Some(id), Some(definition));
                    builder.visit_body(&class.body);

                    let class_scope = builder.pop_scope();
                    builder.scopes_by_node.insert(class.into(), class_scope);
                    class_scope
                });
            }
            ast::Stmt::Import(ast::StmtImport { names, .. }) => {
                for (i, alias) in names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.split('.').next().unwrap()
                    };

                    let module = ModuleName::new(&alias.name.id);

                    let def = Definition::Import(ImportDefinition {
                        import_id: LocalImportId(statement_id),
                        alias: u32::try_from(i).unwrap(),
                    });
                    self.add_or_update_symbol(Name::new(symbol_name), Some(def));
                    // self.symbol_table_builder
                    //     .add_dependency(Dependency::Module(module));
                }
            }
            ast::Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                ..
            }) => {
                let module = module.as_ref().map(|m| ModuleName::new(&m.id));

                for (i, alias) in names.iter().enumerate() {
                    let symbol_name = if let Some(asname) = &alias.asname {
                        asname.id.as_str()
                    } else {
                        alias.name.id.as_str()
                    };
                    let def = Definition::ImportFrom(ImportFromDefinition {
                        import_id: LocalImportFromId(statement_id),
                        name: u32::try_from(i).unwrap(),
                    });
                    self.add_or_update_symbol(Name::new(symbol_name), Some(def));
                }

                // let dependency = if let Some(module) = module {
                //     match NonZeroU32::new(*level) {
                //         Some(level) => Dependency::Relative {
                //             level,
                //             module: Some(module),
                //         },
                //         None => Dependency::Module(module),
                //     }
                // } else {
                //     Dependency::Relative {
                //         level: NonZeroU32::new(*level)
                //             .expect("Import without a module to have a level > 0"),
                //         module,
                //     }
                // };
                //
                // self.symbol_table_builder.add_dependency(dependency);
            }
            ast::Stmt::Assign(node) => {
                // debug_assert!(self.current_definition.is_none());
                // self.current_definition =
                //     Some(Definition::Assignment(TypedNodeKey::from_node(node)));
                walk_stmt(self, stmt);
                // self.current_definition = None;
            }
            ast::Stmt::If(node) => {
                // we visit the if "test" condition first regardless
                self.visit_expr(&node.test);

                // create branch node: does the if test pass or not?
                // let if_branch = self.flow_graph_builder.add_branch(self.current_flow_node());

                // visit the body of the `if` clause
                // self.set_current_flow_node(if_branch);
                self.visit_body(&node.body);

                // Flow node for the last if/elif condition branch; represents the "no branch
                // taken yet" possibility (where "taking a branch" means that the condition in an
                // if or elif evaluated to true and control flow went into that clause).
                // let mut prior_branch = if_branch;

                // Flow node for the state after the prior if/elif/else clause; represents "we have
                // taken one of the branches up to this point." Initially set to the post-if-clause
                // state, later will be set to the phi node joining that possible path with the
                // possibility that we took a later if/elif/else clause instead.
                // let mut post_prior_clause = self.current_flow_node();

                // Flag to mark if the final clause is an "else" -- if so, that means the "match no
                // clauses" path is not possible, we have to go through one of the clauses.
                // let mut last_branch_is_else = false;

                for clause in &node.elif_else_clauses {
                    if clause.test.is_some() {
                        // This is an elif clause. Create a new branch node. Its predecessor is the
                        // previous branch node, because we can only take one branch in an entire
                        // if/elif/else chain, so if we take this branch, it can only be because we
                        // didn't take the previous one.
                        // prior_branch = self.flow_graph_builder.add_branch(prior_branch);
                        // self.set_current_flow_node(prior_branch);
                    } else {
                        // This is an else clause. No need to create a branch node; there's no
                        // branch here, if we haven't taken any previous branch, we definitely go
                        // into the "else" clause.
                        // self.set_current_flow_node(prior_branch);
                        // last_branch_is_else = true;
                    }
                    self.visit_elif_else_clause(clause);
                    // Update `post_prior_clause` to a new phi node joining the possibility that we
                    // took any of the previous branches with the possibility that we took the one
                    // just visited.
                    // post_prior_clause = self
                    //     .flow_graph_builder
                    //     .add_phi(self.current_flow_node(), post_prior_clause);
                }

                // if !last_branch_is_else {
                // Final branch was not an "else", which means it's possible we took zero
                // branches in the entire if/elif chain, so we need one more phi node to join
                // the "no branches taken" possibility.
                // post_prior_clause = self
                //     .flow_graph_builder
                //     .add_phi(post_prior_clause, prior_branch);
                // }

                // Onward, with current flow node set to our final Phi node.
                // self.set_current_flow_node(post_prior_clause);
            }
            _ => {
                walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'_ ast::Expr) {
        let module = self.module;
        #[allow(unsafe_code)]
        let _id = unsafe {
            // SAFETY: The builder only visits nodes that are part of `module`. This guarantees that
            // the current expression must be a child of `module`.
            self.current_ast_ids().record_expression(expr, module)
        };

        self.expression_scopes
            .insert(NodeKey::from_node(expr), self.current_scope());

        walk_expr(self, expr);
    }
}
