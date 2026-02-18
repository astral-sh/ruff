use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxContext, SemanticSyntaxError, SemanticSyntaxErrorKind,
};

use crate::definition::{Definition, DefinitionCategory};
use crate::place::ScopedPlaceId;
use crate::predicate::DeferredWalrusReachabilityPredicate;
use crate::reachability_constraints::ScopedReachabilityConstraintId;
use crate::scope::{FileScopeId, NodeWithScopeKind, ScopeKind};
use crate::symbol::Symbol;
use crate::use_def::PreviousDefinitions;

use super::SemanticIndexBuilder;

#[derive(Clone, Copy, Debug)]
pub(super) struct DeferredWalrusDefinition<'db> {
    /// The scope that should receive the actual binding once all intervening comprehension scopes
    /// have finished.
    target_scope: FileScopeId,
    /// The place in `target_scope` that the named expression binds.
    target_place: ScopedPlaceId,
    /// The place in `visible_after_scope` currently carrying the temporary binding.
    visible_place: ScopedPlaceId,
    /// The definition associated with the named expression target.
    definition: Definition<'db>,
    /// The comprehension scope after which the binding next becomes visible.
    visible_after_scope: FileScopeId,
    /// The reachability condition after the comprehension may have performed zero iterations.
    reachability: ScopedReachabilityConstraintId,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum WalrusTargetScope {
    Current,
    Enclosing { file_scope_id: FileScopeId },
    InvalidClassBodyComprehension,
}

impl<'db, 'ast> SemanticIndexBuilder<'db, 'ast> {
    pub(super) fn visit_named_expression(&mut self, node: &'ast ast::ExprNamed) {
        self.visit_expr(&node.value);

        // See https://peps.python.org/pep-0572/#differences-between-assignment-expressions-and-assignment-statements
        if node.target.is_name_expr() {
            let walrus_target_scope = self.walrus_target_scope();
            let invalid_in_comprehension_iterable = self.in_comprehension_iterable();
            let target = node
                .target
                .as_name_expr()
                .expect("named expression target was checked as a name");
            let rebinds_comprehension_variable = !invalid_in_comprehension_iterable
                && self.rebinds_comprehension_variable(&target.id);

            if invalid_in_comprehension_iterable {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::NamedExpressionInComprehensionIterable,
                    range: node.range,
                    python_version: self.python_version,
                });
            } else if !rebinds_comprehension_variable
                && walrus_target_scope == WalrusTargetScope::InvalidClassBodyComprehension
            {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::NamedExpressionInClassBodyComprehension,
                    range: node.range,
                    python_version: self.python_version,
                });
            }

            if invalid_in_comprehension_iterable {
                self.visit_expr(&node.target);
                return;
            }

            if rebinds_comprehension_variable {
                return;
            }

            // PEP 572: walrus in comprehension binds in the enclosing scope.
            // Make the value a standalone expression so inference can evaluate
            // it in the comprehension scope where the iteration variables are visible.
            if matches!(walrus_target_scope, WalrusTargetScope::Enclosing { .. }) {
                self.add_standalone_expression(&node.value);
            }
            self.push_assignment(node.into());
            self.visit_expr(&node.target);
            self.pop_assignment();
        } else {
            self.visit_expr(&node.target);
        }
    }

    pub(super) fn record_named_expression_definition(
        &mut self,
        place_id: ScopedPlaceId,
        named: &'ast ast::ExprNamed,
    ) {
        let WalrusTargetScope::Enclosing {
            file_scope_id: enclosing_scope,
        } = self.walrus_target_scope()
        else {
            self.add_definition(place_id, named);
            return;
        };

        // PEP 572: walrus in comprehension binds in enclosing scope.
        let target_name = named
            .target
            .as_name_expr()
            .expect("target should be a Name expression")
            .id
            .clone();
        let (symbol_id, added) =
            self.place_tables[enclosing_scope].add_symbol(Symbol::new(target_name));
        if added {
            self.use_def_maps[enclosing_scope].add_place(symbol_id.into());
        }
        let (definition, num_definitions, category, is_loop_header) =
            self.create_definition_in_scope(enclosing_scope, symbol_id.into(), named);
        debug_assert_eq!(
            num_definitions, 1,
            "Attempted to create multiple `Definition`s associated with AST node {named:?}"
        );
        debug_assert!(matches!(category, DefinitionCategory::Binding));
        debug_assert!(!is_loop_header);

        let iteration_reachability = self.current_use_def_map().reachability;
        self.record_temporary_walrus_definition(place_id, definition, iteration_reachability);

        let deferred_reachability = self
            .current_reachability_constraints_mut()
            .add_and_constraint(
                iteration_reachability,
                ScopedReachabilityConstraintId::AMBIGUOUS,
            );

        self.deferred_walrus_definitions
            .push(DeferredWalrusDefinition {
                target_scope: enclosing_scope,
                target_place: symbol_id.into(),
                visible_place: place_id,
                definition,
                visible_after_scope: self.current_scope(),
                // The comprehension body can run zero times, so the binding that
                // leaks to the enclosing scope is never guaranteed by iteration alone.
                reachability: deferred_reachability,
            });
    }

    pub(super) fn propagate_deferred_walrus_definitions(&mut self, popped_scope: FileScopeId) {
        if self.deferred_walrus_definitions.is_empty() {
            return;
        }

        for mut deferred in std::mem::take(&mut self.deferred_walrus_definitions) {
            if deferred.visible_after_scope != popped_scope {
                self.deferred_walrus_definitions.push(deferred);
                continue;
            }

            let current_scope = self.current_scope();
            let is_live_binding = self.use_def_maps[popped_scope]
                .place_has_live_binding(deferred.visible_place, deferred.definition);
            if !is_live_binding && current_scope == deferred.target_scope {
                self.record_shadowed_deferred_walrus_definition_for_try_snapshots(
                    popped_scope,
                    deferred,
                );
                continue;
            }

            let propagated_reachability =
                self.propagate_deferred_walrus_reachability(popped_scope, deferred.reachability);
            if current_scope == deferred.target_scope {
                self.record_deferred_walrus_definition(
                    deferred.target_place,
                    deferred.definition,
                    propagated_reachability,
                );
            } else {
                debug_assert_eq!(
                    self.scopes[current_scope].kind(),
                    ScopeKind::Comprehension,
                    "deferred walrus bindings should only propagate through comprehension scopes",
                );

                let name = self.place_tables[deferred.target_scope]
                    .place(deferred.target_place)
                    .as_symbol()
                    .expect("deferred walrus target should be a symbol")
                    .name()
                    .clone();
                let (symbol_id, added) =
                    self.place_tables[current_scope].add_symbol(Symbol::new(name));
                if added {
                    self.use_def_maps[current_scope].add_place(symbol_id.into());
                }
                let current_reachability = self.current_use_def_map().reachability;
                let iteration_reachability = self
                    .current_reachability_constraints_mut()
                    .add_and_constraint(propagated_reachability, current_reachability);
                deferred.reachability = self
                    .current_reachability_constraints_mut()
                    .add_and_constraint(
                        iteration_reachability,
                        ScopedReachabilityConstraintId::AMBIGUOUS,
                    );
                self.record_temporary_walrus_definition(
                    symbol_id.into(),
                    deferred.definition,
                    iteration_reachability,
                );
                deferred.visible_after_scope = current_scope;
                deferred.visible_place = symbol_id.into();
                self.deferred_walrus_definitions.push(deferred);
            }
        }
    }

    fn propagate_deferred_walrus_reachability(
        &mut self,
        source_scope: FileScopeId,
        reachability: ScopedReachabilityConstraintId,
    ) -> ScopedReachabilityConstraintId {
        if reachability.is_terminal() {
            return reachability;
        }

        self.use_def_maps[source_scope]
            .reachability_constraints
            .mark_used(reachability);

        let predicate = DeferredWalrusReachabilityPredicate {
            file: self.file,
            file_scope: source_scope,
            reachability,
        };
        let predicate_id = self.add_predicate(predicate.into());
        self.current_reachability_constraints_mut()
            .add_atom(predicate_id)
    }

    /// Returns the scope that owns a walrus target.
    ///
    /// Per [PEP 572], named expressions in comprehensions bind in the first enclosing scope that
    /// is not a comprehension, except that assignment expressions within comprehensions are not
    /// allowed in class bodies.
    ///
    /// [PEP 572]: https://peps.python.org/pep-0572/#scope-of-the-target
    fn walrus_target_scope(&self) -> WalrusTargetScope {
        if self.scopes[self.current_scope()].kind() != ScopeKind::Comprehension {
            return WalrusTargetScope::Current;
        }

        self.scope_stack
            .iter()
            .rev()
            .skip(1)
            .find_map(|info| match self.scopes[info.file_scope_id].kind() {
                ScopeKind::Comprehension => None,
                ScopeKind::Class => Some(WalrusTargetScope::InvalidClassBodyComprehension),
                _ => Some(WalrusTargetScope::Enclosing {
                    file_scope_id: info.file_scope_id,
                }),
            })
            .unwrap_or(WalrusTargetScope::Current)
    }

    /// Returns whether a named expression evaluated in the current comprehension chain
    /// rebinds one of that chain's iteration variables.
    fn rebinds_comprehension_variable(&self, name: &ast::name::Name) -> bool {
        self.scope_stack
            .iter()
            .rev()
            .map(|info| &self.scopes[info.file_scope_id])
            .take_while(|scope| scope.kind() == ScopeKind::Comprehension)
            .any(|scope| {
                let generators: &[ast::Comprehension] = match scope.node() {
                    NodeWithScopeKind::ListComprehension(node) => {
                        &node.node(self.module).generators
                    }
                    NodeWithScopeKind::SetComprehension(node) => &node.node(self.module).generators,
                    NodeWithScopeKind::DictComprehension(node) => {
                        &node.node(self.module).generators
                    }
                    NodeWithScopeKind::GeneratorExpression(node) => {
                        &node.node(self.module).generators
                    }
                    _ => unreachable!("comprehension scope should have a comprehension node"),
                };

                generators
                    .iter()
                    .any(|generator| target_binds_name(&generator.target, name))
            })
    }

    fn discard_deferred_walrus_definition(&mut self, deferred: DeferredWalrusDefinition<'db>) {
        debug_assert_eq!(self.current_scope(), deferred.target_scope);
        self.mark_place_bound(deferred.target_place);
        self.current_use_def_map_mut()
            .record_binding_context(deferred.target_place, deferred.definition);
    }

    fn record_shadowed_deferred_walrus_definition_for_try_snapshots(
        &mut self,
        source_scope: FileScopeId,
        deferred: DeferredWalrusDefinition<'db>,
    ) {
        debug_assert_eq!(self.current_scope(), deferred.target_scope);

        let target_scope_state = self.current_use_def_map().snapshot();
        let propagated_reachability =
            self.propagate_deferred_walrus_reachability(source_scope, deferred.reachability);
        self.record_deferred_walrus_definition(
            deferred.target_place,
            deferred.definition,
            propagated_reachability,
        );
        self.current_use_def_map_mut().restore(target_scope_state);

        self.discard_deferred_walrus_definition(deferred);
    }

    fn record_deferred_walrus_definition(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        reachability: ScopedReachabilityConstraintId,
    ) {
        if reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
            self.mark_place_bound(place);
            self.current_use_def_map_mut()
                .record_binding_context(place, definition);
            return;
        }

        if reachability == ScopedReachabilityConstraintId::ALWAYS_TRUE {
            self.record_existing_binding(place, definition);
            return;
        }

        let symbol = place
            .as_symbol()
            .expect("deferred walrus target should be a symbol");
        let associated_member_ids = self
            .current_place_table()
            .associated_place_ids(place)
            .to_vec();
        let pre_definition = self
            .current_use_def_map()
            .single_symbol_snapshot(symbol, &associated_member_ids);
        let pre_definition_reachability = self.current_use_def_map().reachability;
        let walrus_reachability = reachability;
        let definition_reachability = self
            .current_reachability_constraints_mut()
            .add_and_constraint(pre_definition_reachability, walrus_reachability);
        self.current_use_def_map_mut().reachability = definition_reachability;

        self.record_existing_binding(place, definition);

        self.current_use_def_map_mut()
            .record_and_negate_single_symbol_reachability_constraint(
                walrus_reachability,
                symbol,
                pre_definition,
            );
        self.current_use_def_map_mut().reachability = pre_definition_reachability;
    }

    fn record_temporary_walrus_definition(
        &mut self,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        reachability: ScopedReachabilityConstraintId,
    ) {
        if reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
            self.current_use_def_map_mut()
                .record_binding_context(place, definition);
            return;
        }

        self.current_use_def_map_mut().record_binding(
            place,
            definition,
            PreviousDefinitions::AreShadowed,
        );
        self.delete_associated_bindings(place);

        let mut try_node_stack_manager = std::mem::take(&mut self.try_node_context_stack_manager);
        try_node_stack_manager.record_definition(self);
        self.try_node_context_stack_manager = try_node_stack_manager;
    }
}

fn target_binds_name(target: &ast::Expr, name: &ast::name::Name) -> bool {
    match target {
        ast::Expr::Name(target) => &target.id == name,
        ast::Expr::Tuple(target) => target
            .elts
            .iter()
            .any(|element| target_binds_name(element, name)),
        ast::Expr::List(target) => target
            .elts
            .iter()
            .any(|element| target_binds_name(element, name)),
        ast::Expr::Starred(target) => target_binds_name(&target.value, name),
        _ => false,
    }
}
