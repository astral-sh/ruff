use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxContext, SemanticSyntaxError, SemanticSyntaxErrorKind,
};
use ruff_text_size::TextRange;

use crate::definition::{Definition, DefinitionCategory};
use crate::place::ScopedPlaceId;
use crate::reachability_constraints::ScopedReachabilityConstraintId;
use crate::scope::{FileScopeId, ScopeKind};
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
    /// Whether the named expression target is reached unconditionally inside its comprehension.
    reachability: DeferredWalrusReachability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeferredWalrusReachability {
    Always,
    Never,
    Conditional,
}

impl DeferredWalrusReachability {
    fn from_constraint(reachability: ScopedReachabilityConstraintId) -> Self {
        if reachability == ScopedReachabilityConstraintId::ALWAYS_TRUE {
            Self::Always
        } else if reachability == ScopedReachabilityConstraintId::ALWAYS_FALSE {
            Self::Never
        } else {
            Self::Conditional
        }
    }

    fn combine(self, other: Self) -> Self {
        match (self, other) {
            (Self::Never, _) | (_, Self::Never) => Self::Never,
            (Self::Always, reachability) | (reachability, Self::Always) => reachability,
            (Self::Conditional, Self::Conditional) => Self::Conditional,
        }
    }
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
            let already_reported_rebound =
                self.has_rebound_comprehension_variable_error(target.range);
            let active_target_rebound = matches!(
                walrus_target_scope,
                WalrusTargetScope::Enclosing { .. }
                    | WalrusTargetScope::InvalidClassBodyComprehension
            ) && !invalid_in_comprehension_iterable
                && self.is_active_comprehension_target(&target.id);
            let invalid_rebound_comprehension_variable =
                already_reported_rebound || active_target_rebound;

            if invalid_in_comprehension_iterable {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::NamedExpressionInComprehensionIterable,
                    range: node.range,
                    python_version: self.python_version,
                });
            } else if active_target_rebound && !already_reported_rebound {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::ReboundComprehensionVariable,
                    range: target.range,
                    python_version: self.python_version,
                });
            } else if !invalid_rebound_comprehension_variable
                && walrus_target_scope == WalrusTargetScope::InvalidClassBodyComprehension
            {
                self.report_semantic_error(SemanticSyntaxError {
                    kind: SemanticSyntaxErrorKind::NamedExpressionInClassBodyComprehension,
                    range: node.range,
                    python_version: self.python_version,
                });
            }

            if invalid_in_comprehension_iterable || invalid_rebound_comprehension_variable {
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

        let reachability =
            DeferredWalrusReachability::from_constraint(self.current_use_def_map().reachability);
        self.record_temporary_walrus_definition_in_scope(
            self.current_scope(),
            self.scope_stack.len() - 1,
            place_id,
            definition,
            reachability,
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
                reachability: reachability.combine(DeferredWalrusReachability::Conditional),
            });
    }

    pub(super) fn propagate_deferred_walrus_definitions(&mut self, popped_scope: FileScopeId) {
        if self.deferred_walrus_definitions.is_empty() {
            return;
        }

        let mut deferred_definitions = std::mem::take(&mut self.deferred_walrus_definitions);
        for mut deferred in deferred_definitions.drain(..) {
            if deferred.visible_after_scope != popped_scope {
                self.deferred_walrus_definitions.push(deferred);
                continue;
            }

            if !self.use_def_maps[popped_scope]
                .place_has_live_binding(deferred.visible_place, deferred.definition)
            {
                self.discard_deferred_walrus_definition(deferred);
                continue;
            }

            let current_scope = self.current_scope();
            if current_scope == deferred.target_scope {
                let Some(scope_index) = self.scope_stack_index(deferred.target_scope) else {
                    debug_assert!(false, "deferred walrus target scope should still be active");
                    continue;
                };
                self.record_deferred_walrus_definition_in_scope(
                    deferred.target_scope,
                    scope_index,
                    deferred.target_place,
                    deferred.definition,
                    deferred.reachability,
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
                deferred.reachability =
                    deferred
                        .reachability
                        .combine(DeferredWalrusReachability::from_constraint(
                            self.current_use_def_map().reachability,
                        ));
                let scope_index = self.scope_stack.len() - 1;
                self.record_temporary_walrus_definition_in_scope(
                    current_scope,
                    scope_index,
                    symbol_id.into(),
                    deferred.definition,
                    deferred.reachability,
                );
                deferred.visible_after_scope = current_scope;
                deferred.visible_place = symbol_id.into();
                self.deferred_walrus_definitions.push(deferred);
            }
        }
    }

    pub(super) fn discard_deferred_walrus_definitions(&mut self, popped_scope: FileScopeId) {
        if self.deferred_walrus_definitions.is_empty() {
            return;
        }

        let mut deferred_definitions = std::mem::take(&mut self.deferred_walrus_definitions);
        for deferred in deferred_definitions.drain(..) {
            if deferred.visible_after_scope == popped_scope {
                self.discard_deferred_walrus_definition(deferred);
            } else {
                self.deferred_walrus_definitions.push(deferred);
            }
        }
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

    fn has_rebound_comprehension_variable_error(&self, range: TextRange) -> bool {
        self.semantic_syntax_errors.borrow().iter().any(|error| {
            matches!(
                error.kind,
                SemanticSyntaxErrorKind::ReboundComprehensionVariable
            ) && error.range == range
        })
    }

    fn discard_deferred_walrus_definition(&mut self, deferred: DeferredWalrusDefinition<'db>) {
        self.place_tables[deferred.target_scope].mark_bound(deferred.target_place);
        self.use_def_maps[deferred.target_scope]
            .record_binding_context(deferred.target_place, deferred.definition);
    }

    fn walrus_reachability_constraint(
        reachability: DeferredWalrusReachability,
    ) -> ScopedReachabilityConstraintId {
        match reachability {
            DeferredWalrusReachability::Always => ScopedReachabilityConstraintId::ALWAYS_TRUE,
            DeferredWalrusReachability::Never => ScopedReachabilityConstraintId::ALWAYS_FALSE,
            DeferredWalrusReachability::Conditional => ScopedReachabilityConstraintId::AMBIGUOUS,
        }
    }

    fn record_deferred_walrus_definition_in_scope(
        &mut self,
        scope: FileScopeId,
        scope_index: usize,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        reachability: DeferredWalrusReachability,
    ) {
        if reachability == DeferredWalrusReachability::Never {
            self.place_tables[scope].mark_bound(place);
            self.use_def_maps[scope].record_binding_context(place, definition);
            return;
        }

        if reachability == DeferredWalrusReachability::Always {
            self.record_existing_definition_in_scope(
                scope,
                scope_index,
                place,
                definition,
                DefinitionCategory::Binding,
                false,
            );
            return;
        }

        let symbol = place
            .as_symbol()
            .expect("deferred walrus target should be a symbol");
        let associated_member_ids = self.place_tables[scope]
            .associated_place_ids(place)
            .to_vec();
        let pre_definition =
            self.use_def_maps[scope].single_symbol_snapshot(symbol, &associated_member_ids);
        let pre_definition_reachability = self.use_def_maps[scope].reachability;
        let walrus_reachability = Self::walrus_reachability_constraint(reachability);
        self.use_def_maps[scope].reachability = self.use_def_maps[scope]
            .reachability_constraints
            .add_and_constraint(pre_definition_reachability, walrus_reachability);

        self.record_existing_definition_in_scope(
            scope,
            scope_index,
            place,
            definition,
            DefinitionCategory::Binding,
            false,
        );

        self.use_def_maps[scope].record_and_negate_single_symbol_reachability_constraint(
            walrus_reachability,
            symbol,
            pre_definition,
        );
        self.use_def_maps[scope].reachability = pre_definition_reachability;
    }

    fn record_temporary_walrus_definition_in_scope(
        &mut self,
        scope: FileScopeId,
        scope_index: usize,
        place: ScopedPlaceId,
        definition: Definition<'db>,
        reachability: DeferredWalrusReachability,
    ) {
        if reachability == DeferredWalrusReachability::Never {
            self.use_def_maps[scope].record_binding_context(place, definition);
            return;
        }

        self.use_def_maps[scope].record_binding(
            place,
            definition,
            PreviousDefinitions::AreShadowed,
        );
        self.delete_associated_bindings_in_scope(scope, place);

        self.try_node_context_stack_manager
            .record_definition(scope_index, &self.use_def_maps[scope]);
    }
}
