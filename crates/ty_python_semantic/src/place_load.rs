//! This module combines the semantics of name resolution with the results of
//! reaching definition analysis to expose a [`PlaceLoadResolution`], which
//! provides a lazy iterator over the steps that resolve the value read from a
//! place.
//!
//! More specifically, a [`PlaceLoadResolution`] iterates over a series of
//! [`PlaceLoadResolutionStep`] values, each of which represents a phase of the
//! process that ultimately either supplies a definite value for a load or ends
//! in explicit failure:
//!
//! - A source ([`PlaceLoadResolutionStep::Source`]) (and its associated type-
//!   narrowing constraints) which may supply the value for a load.
//! - A boolean condition ([`PlaceLoadResolutionStep::MemberResolutionCondition`])
//!   that determines whether resolution continues for member loads specifically
//!   (i.e., `foo.bar.baz` as opposed to the plain symbol `foo`). This describes
//!   the loads of member prefixes (e.g., `foo.bar` and `foo`) that must all be
//!   unbound before resolution continues.
//! - A marker ([`PlaceLoadResolutionStep::Exhausted`]) which declares that the
//!   resolution process ended in failure.
//!
//! We consume this model to different ends:
//!
//! - Type inference uses it to determine the type and definedness of a place
//! - The language server uses it to determine, e.g., what references to an
//!   imported module should be rewritten in response to the module itself being
//!   renamed
//!
//! ## Example
//!
//! ```py
//! from collections.abc import Callable
//!
//! def make_counter(start: int, enabled: bool) -> Callable[[], int | None]:
//!     if enabled:
//!         value = start
//!     else:
//!         value = None
//!
//!     def next_value() -> int | None:
//!         nonlocal value
//!         if value is not None:
//!             current = value  # load U
//!             value += 1
//!             return current
//!         return None
//!
//!     return next_value
//! ```
//!
//! [`PlaceLoadResolution`] at `U` combines reaching definition analysis with a
//! lexical scope walk:
//!
//! 1. Reaching definition analysis from the use-def module supplies the binding
//!    state for `value` at `U` in `next_value`. In this case, no value-binding
//!    definition reaches U because the `value += 1` assignment occurs after `U`,
//!    so the state records `value` as unbound.
//! 2. The use-def model also supplies the narrowing constraint `value is not None`
//!    that is associated with the source. That constraint can affect an
//!    inferred type but does not affect name resolution.
//! 3. Name resolution encounters the `nonlocal` declaration and continues
//!    the lexical scope walk into `make_counter`, where `value` is owned.
//! 4. Once name resolution reaches the scope that owns value, it records
//!    `make_counter.value` as an enclosing source of a potential value for `U`.
//!
//! Schematically, fully consuming the resulting [`PlaceLoadResolution`] yields:
//!
//! ```text
//! Source(Bindings(next_value.value at U))  // unbound
//! Source(DefinitionsFromOwningScope(make_counter.value))
//! Exhausted(UnboundFree)
//! ```
//!
//! While yielding those steps, the resolution accumulates `value is not None`
//! as a narrowing constraint and records that it crossed the `nonlocal`
//! declaration in `next_value`.
//!
//! The enclosing function's binding scope terminates name resolution, even if
//! none of its definitions supply a value at runtime. [`PlaceLoadResolution`]
//! therefore yields `Exhausted(UnboundFree)` if both sources are exhausted
//! instead of yielding module globals or builtins as later sources. In this
//! example, type inference establishes that the branches in `make_counter`
//! always define `value`, so the `Exhausted` step is unreachable.

use ruff_python_ast::{self as ast, name::Name};
use smallvec::SmallVec;
use ty_python_core::ast_ids::{HasScopedUseId, ScopedUseId};
use ty_python_core::definition::Definition;
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::{PlaceExpr, PlaceExprRef, ScopedPlaceId};
use ty_python_core::scope::{NodeWithScopeKind, ScopeId, ScopeKind};
use ty_python_core::symbol::{ScopedSymbolId, Symbol};
use ty_python_core::{
    AncestorsIter, BindingWithConstraintsIterator, BindingsSnapshotId, EnclosingSnapshotResult,
    FileScopeId, ProgramFile, SemanticIndex,
};

use crate::Db;

/// Returns an iterator over the steps that resolve a value for a place load.
pub(crate) fn resolve_place_load<'db, 'ast>(
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    scope: ScopeId<'db>,
    place_expr: PlaceExpr,
    mode: PlaceLoadMode<'ast>,
) -> PlaceLoadResolution<'db, 'ast> {
    PlaceLoadResolution::new(
        PlaceLoadResolutionContext {
            db,
            index,
            scope,
            file: scope.program_file(db),
            mode,
        },
        place_expr,
    )
}

/// Selects the binding state used for a place load's own scope.
#[derive(Clone, Copy)]
pub(crate) enum PlaceLoadMode<'ast> {
    /// Resolve bindings live at an expression occurrence.
    ///
    /// For example, a caller resolving `value` in `print(value)` uses this mode so that only
    /// bindings that reach that occurrence are considered.
    AtExpression(ast::ExprRef<'ast>),
    /// Resolve a plain name using bindings and constraints retained at an earlier point in
    /// the same scope. Enclosing-scope resolution still follows the name's lexical scope.
    AtNameSnapshot(BindingsSnapshotId),
    /// Resolve all bindings reachable in the scope.
    ///
    /// A caller uses this mode for an annotation in any of these contexts:
    ///
    /// - A stub file.
    /// - A module containing `from __future__ import annotations`.
    /// - Python 3.14 or later.
    ///
    /// Callers also use this mode for other deferred type expressions, including type-parameter
    /// bounds and defaults and, in stub files, class bases and type alias values.
    ///
    /// For example, `Model` in `item: Model` can resolve to a class defined later in the scope.
    Deferred,
    /// Resolve reachable bindings in a parsed string annotation.
    ///
    /// A caller uses this mode for a name such as `Model` after parsing `item: "Model"`. The
    /// parsed expression is not part of the original semantic index, so it may not have its own
    /// place-table entry.
    StringAnnotation,
}

/// Exposes an iterator over the steps that resolve the value for a place load.
pub(crate) struct PlaceLoadResolution<'db, 'ast> {
    /// The place expression whose loaded value is being resolved.
    place_expr: PlaceExpr,
    /// Read-only context shared by every source-selection phase.
    context: PlaceLoadResolutionContext<'db, 'ast>,
    /// The next node to visit in the resolution graph, or `None` after reaching a leaf.
    next_node: Option<PlaceLoadResolutionNode<'db>>,
    /// Narrowing constraints accumulated while resolution advances.
    constraints: PlaceLoadConstraints,
    /// Whether resolution has crossed a `global` or `nonlocal` declaration so far.
    crosses_scope_declaration: bool,
}

impl<'db> Iterator for PlaceLoadResolution<'db, '_> {
    type Item = PlaceLoadResolutionStep<'db>;

    /// Lazily yields [`PlaceLoadResolutionStep`] values to describe the resolution process.
    ///
    /// Internally, this traverses a directed, acyclic graph that models the resolution process.
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(current_node) = self.next_node.take() {
            match current_node {
                PlaceLoadResolutionNode::LocalSource => {
                    self.next_node =
                        Some(PlaceLoadResolutionNode::AskConsumerWhetherToContinueForMember);

                    if let Some((kind, exit_constraint)) =
                        self.context.local_source(self.place_expr())
                    {
                        let source = self.constraints.source(
                            kind,
                            PlaceLoadSourceRole::Ordinary,
                            exit_constraint,
                        );
                        return Some(PlaceLoadResolutionStep::Source(source));
                    }
                }
                PlaceLoadResolutionNode::AskConsumerWhetherToContinueForMember => {
                    self.next_node = Some(PlaceLoadResolutionNode::DecideResolutionPath);

                    if let Some(prefix_loads) =
                        self.context.place_expr_prefix_loads(self.place_expr())
                    {
                        return Some(PlaceLoadResolutionStep::MemberResolutionCondition(
                            prefix_loads,
                        ));
                    }
                }
                PlaceLoadResolutionNode::DecideResolutionPath => {
                    self.next_node = Some(self.decide_resolution_path());
                }
                PlaceLoadResolutionNode::DunderClassSource {
                    definition,
                    enclosing_scopes,
                } => {
                    self.next_node = Some(PlaceLoadResolutionNode::EnclosingScopeSource(
                        enclosing_scopes,
                    ));

                    return Some(PlaceLoadResolutionStep::Source(
                        PlaceLoadConstraints::unnarrowed_source(
                            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::DunderClass(
                                definition,
                            )),
                            PlaceLoadSourceRole::Ordinary,
                        ),
                    ));
                }
                PlaceLoadResolutionNode::EnclosingScopeSource(mut scopes) => {
                    let (next_node, source) = self.resolve_enclosing_scopes(&mut scopes);
                    self.next_node = Some(next_node);

                    if let Some(source) = source {
                        return Some(PlaceLoadResolutionStep::Source(source));
                    }
                }
                PlaceLoadResolutionNode::ImplicitClassBodySource(forwarded_global_snapshot) => {
                    self.next_node = Some(forwarded_global_snapshot.map_or(
                        PlaceLoadResolutionNode::ExplicitGlobalSource(
                            PlaceLoadSourceRole::Ordinary,
                        ),
                        PlaceLoadResolutionNode::ForwardedGlobalSnapshotSource,
                    ));

                    if self.context.is_class_body_scope()
                        && let Some(name) = self.loaded_symbol_name()
                    {
                        let source = self.constraints.source(
                            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ClassBodySymbol(
                                name.clone(),
                            )),
                            PlaceLoadSourceRole::Ordinary,
                            None,
                        );
                        return Some(PlaceLoadResolutionStep::Source(source));
                    }
                }
                PlaceLoadResolutionNode::ForwardedGlobalSnapshotSource(snapshot) => {
                    let ForwardedGlobalSnapshot {
                        bindings,
                        enclosing_scope,
                    } = snapshot;
                    let global_place_table = self.context.index.place_table(FileScopeId::global());
                    let has_explicit_global = self
                        .loaded_symbol_name()
                        .and_then(|name| global_place_table.symbol_id(name))
                        .is_some_and(|symbol_id| {
                            let symbol = global_place_table.symbol(symbol_id);
                            symbol.is_bound() || symbol.is_declared()
                        });

                    // Nested global assignments create synthetic module bindings even when the
                    // module never defines the name itself. Do not let those bindings hide an
                    // implicit global or builtin when the forwarded assignment did not run.
                    self.next_node = Some(if has_explicit_global {
                        PlaceLoadResolutionNode::ExplicitGlobalSource(PlaceLoadSourceRole::Ordinary)
                    } else {
                        PlaceLoadResolutionNode::ImplicitGlobalSource
                    });

                    let source = self.constraints.source(
                        PlaceLoadSourceKind::Bindings(bindings),
                        PlaceLoadSourceRole::Ordinary,
                        Some((
                            enclosing_scope,
                            ConstraintKey::NestedScope(
                                self.context.scope.file_scope_id(self.context.db),
                            ),
                        )),
                    );
                    return Some(PlaceLoadResolutionStep::Source(source));
                }
                PlaceLoadResolutionNode::ExplicitGlobalSource(role) => {
                    self.next_node = Some(PlaceLoadResolutionNode::ImplicitGlobalSource);

                    if let Some(source) = self.resolve_global(role) {
                        return Some(PlaceLoadResolutionStep::Source(source));
                    }
                }
                PlaceLoadResolutionNode::ImplicitGlobalSource => {
                    if let Some(name) = self.loaded_symbol_name().cloned() {
                        self.next_node = Some(PlaceLoadResolutionNode::BuiltinSource(name.clone()));

                        let source = self.constraints.source(
                            PlaceLoadSourceKind::Implicit(
                                ImplicitPlaceLoad::ModuleImplicitGlobal {
                                    file: self.context.file,
                                    name,
                                },
                            ),
                            PlaceLoadSourceRole::Ordinary,
                            None,
                        );
                        return Some(PlaceLoadResolutionStep::Source(source));
                    }

                    self.next_node =
                        Some(PlaceLoadResolutionNode::Failure(PlaceLoadFailure::NotFound));
                }
                PlaceLoadResolutionNode::BuiltinSource(name) => {
                    self.next_node =
                        Some(PlaceLoadResolutionNode::Failure(PlaceLoadFailure::NotFound));

                    return Some(PlaceLoadResolutionStep::Source(
                        PlaceLoadConstraints::unnarrowed_source(
                            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::Builtin(name)),
                            PlaceLoadSourceRole::Ordinary,
                        ),
                    ));
                }
                PlaceLoadResolutionNode::Failure(failure) => {
                    return Some(PlaceLoadResolutionStep::Exhausted(failure));
                }
            }
        }

        None
    }
}

impl<'db, 'ast> PlaceLoadResolution<'db, 'ast> {
    fn new(context: PlaceLoadResolutionContext<'db, 'ast>, place_expr: PlaceExpr) -> Self {
        let crosses_scope_declaration =
            context.symbol_has_scope_declaration(PlaceExprRef::from(&place_expr));
        Self {
            context,
            place_expr,
            next_node: Some(PlaceLoadResolutionNode::LocalSource),
            constraints: PlaceLoadConstraints::default(),
            crosses_scope_declaration,
        }
    }

    fn decide_resolution_path(&mut self) -> PlaceLoadResolutionNode<'db> {
        let db = self.context.db;
        let scope = self.context.scope;
        let file_scope = scope.file_scope_id(db);
        let place_table = self.context.index.place_table(file_scope);

        let mut symbol_is_local = false;
        let place_expr = PlaceExprRef::from(&self.place_expr);
        if let Some(symbol) = place_expr.as_symbol()
            && let Some(symbol_id) = place_table.symbol_id(symbol.name())
        {
            let indexed_symbol = place_table.symbol(symbol_id);
            symbol_is_local = indexed_symbol.is_local();

            let class_body_global_fallback = self.context.is_class_body_scope() && symbol_is_local;
            if self.context.skips_non_global_scopes(symbol_id) || class_body_global_fallback {
                return PlaceLoadResolutionNode::ExplicitGlobalSource(
                    if class_body_global_fallback {
                        PlaceLoadSourceRole::ClassBodyGlobalFallback
                    } else {
                        PlaceLoadSourceRole::Ordinary
                    },
                );
            }
        }

        if symbol_is_local {
            return if scope.node(db).scope_kind().is_module() {
                PlaceLoadResolutionNode::ImplicitGlobalSource
            } else {
                PlaceLoadResolutionNode::Failure(PlaceLoadFailure::UnboundLocal)
            };
        }

        let mut scopes = self.context.index.ancestor_scopes(file_scope);
        // The first scope is the input scope itself; skip it to arrive at the first true ancestor.
        scopes.next();

        if let PlaceExprRef::Symbol(symbol) = place_expr
            && symbol.name() == "__class__"
            && let Some(definition) = self.context.dunder_class_cell_definition()
        {
            PlaceLoadResolutionNode::DunderClassSource {
                definition,
                enclosing_scopes: scopes,
            }
        } else {
            PlaceLoadResolutionNode::EnclosingScopeSource(scopes)
        }
    }

    fn resolve_enclosing_scopes(
        &mut self,
        scopes: &mut AncestorsIter<'db>,
    ) -> (PlaceLoadResolutionNode<'db>, Option<PlaceLoadSource<'db>>) {
        let db = self.context.db;
        let scope = self.context.scope;
        let file_scope = scope.file_scope_id(db);

        for (enclosing_file_scope, _) in scopes {
            if enclosing_file_scope.is_global() {
                break;
            }

            let enclosing_scope = self.context.index.scope(enclosing_file_scope);
            let is_lexical_enclosing_scope = self
                .context
                .is_lexical_enclosing_scope(enclosing_file_scope);

            let enclosing_place_table = self.context.index.place_table(enclosing_file_scope);
            let place_expr = PlaceExprRef::from(&self.place_expr);
            let enclosing_place_id = enclosing_place_table.place_id(place_expr);
            let enclosing_place = enclosing_place_id.map(|id| enclosing_place_table.place(id));
            // A `global` declaration forwards the place to the module instead of making this
            // enclosing scope its owner. A possibly-unbound snapshot must still fall through.
            let forwards_to_global = is_lexical_enclosing_scope
                && enclosing_place
                    .is_some_and(|place| place.as_symbol().is_some_and(Symbol::is_global));
            let root_place_was_reassigned = || {
                enclosing_place_table
                    .parents(place_expr)
                    .any(|root| enclosing_place_table.place(root).is_bound())
            };

            let mut eagerly_undefined = false;
            if self.context.uses_enclosing_snapshots() {
                match self.context.index.enclosing_snapshot(
                    enclosing_file_scope,
                    place_expr,
                    file_scope,
                ) {
                    EnclosingSnapshotResult::FoundConstraint(constraint) => {
                        self.constraints.push(
                            enclosing_file_scope,
                            ConstraintKey::NarrowingConstraint(constraint),
                        );
                        if scope.scope(db).is_eager() {
                            eagerly_undefined = true;
                        }
                    }
                    EnclosingSnapshotResult::FoundBindings(bindings) => {
                        if forwards_to_global {
                            self.crosses_scope_declaration = true;
                            return (
                                PlaceLoadResolutionNode::ImplicitClassBodySource(Some(
                                    ForwardedGlobalSnapshot {
                                        bindings,
                                        enclosing_scope: enclosing_file_scope,
                                    },
                                )),
                                None,
                            );
                        }

                        return (
                            Self::node_after_enclosing_scope(enclosing_scope.kind()),
                            Some(self.constraints.source(
                                PlaceLoadSourceKind::Bindings(bindings),
                                PlaceLoadSourceRole::Ordinary,
                                Some((
                                    enclosing_file_scope,
                                    ConstraintKey::NestedScope(file_scope),
                                )),
                            )),
                        );
                    }
                    EnclosingSnapshotResult::NotFound => {
                        if root_place_was_reassigned() {
                            return (
                                Self::node_after_enclosing_scope(enclosing_scope.kind()),
                                None,
                            );
                        }
                        continue;
                    }
                    EnclosingSnapshotResult::NoLongerInEagerContext => {
                        if root_place_was_reassigned() {
                            return (
                                Self::node_after_enclosing_scope(enclosing_scope.kind()),
                                None,
                            );
                        }
                    }
                }
            }

            if !is_lexical_enclosing_scope {
                continue;
            }

            let (Some(enclosing_place_id), Some(enclosing_place)) =
                (enclosing_place_id, enclosing_place)
            else {
                continue;
            };

            if forwards_to_global {
                self.crosses_scope_declaration = true;
                return (PlaceLoadResolutionNode::ImplicitClassBodySource(None), None);
            }
            // Keep walking across `nonlocal` declarations until reaching the owning scope.
            if enclosing_place.as_symbol().is_some_and(Symbol::is_nonlocal) {
                self.crosses_scope_declaration = true;
                continue;
            }
            if !(enclosing_place.is_bound() || enclosing_place.is_declared()) {
                continue;
            }

            // The first bound or declared place owns the load. Its public value includes nested
            // writes represented by synthetic definitions in this scope.
            return (
                Self::node_after_enclosing_scope(enclosing_scope.kind()),
                (!eagerly_undefined).then(|| {
                    self.constraints.source(
                        PlaceLoadSourceKind::DefinitionsFromOwningScope {
                            scope: enclosing_file_scope.to_scope_id(db, self.context.file),
                            id: enclosing_place_id,
                        },
                        PlaceLoadSourceRole::Ordinary,
                        None,
                    )
                }),
            );
        }

        (PlaceLoadResolutionNode::ImplicitClassBodySource(None), None)
    }

    /// Resolves a load that has reached the module's explicit global scope.
    ///
    /// An eager nested scope uses the global snapshot captured when it began, so a class body
    /// cannot see a module binding created only after that body finishes.
    fn resolve_global(&mut self, role: PlaceLoadSourceRole) -> Option<PlaceLoadSource<'db>> {
        let current_scope = self.context.scope.file_scope_id(self.context.db);
        if current_scope.is_global() {
            return None;
        }

        if self.context.uses_enclosing_snapshots() {
            match self.context.index.enclosing_snapshot(
                FileScopeId::global(),
                PlaceExprRef::from(&self.place_expr),
                current_scope,
            ) {
                EnclosingSnapshotResult::FoundConstraint(constraint) => {
                    self.constraints.push(
                        FileScopeId::global(),
                        ConstraintKey::NarrowingConstraint(constraint),
                    );
                    return None;
                }
                EnclosingSnapshotResult::FoundBindings(bindings) => {
                    return Some(self.constraints.source(
                        PlaceLoadSourceKind::Bindings(bindings),
                        role,
                        Some((
                            FileScopeId::global(),
                            ConstraintKey::NestedScope(current_scope),
                        )),
                    ));
                }
                EnclosingSnapshotResult::NotFound => return None,
                EnclosingSnapshotResult::NoLongerInEagerContext => {}
            }
        }

        let name = self.loaded_symbol_name()?.clone();
        Some(self.constraints.source(
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file: self.context.file,
                name,
            }),
            role,
            None,
        ))
    }

    fn node_after_enclosing_scope(kind: ScopeKind) -> PlaceLoadResolutionNode<'db> {
        if kind.is_class() {
            PlaceLoadResolutionNode::ImplicitGlobalSource
        } else {
            PlaceLoadResolutionNode::Failure(PlaceLoadFailure::UnboundFree)
        }
    }

    pub(crate) fn narrowing_constraints_for(
        &self,
        source: &PlaceLoadSource<'_>,
    ) -> &[(FileScopeId, ConstraintKey)] {
        self.constraints.narrowing_constraints_for(source)
    }

    pub(crate) fn into_constraints(self) -> Vec<(FileScopeId, ConstraintKey)> {
        self.constraints.into_constraints()
    }

    pub(crate) fn place_expr(&self) -> PlaceExprRef<'_> {
        PlaceExprRef::from(&self.place_expr)
    }

    /// Returns the loaded symbol's name, or `None` when the loaded place is a member.
    ///
    /// For example, this returns a name for `value`, but not for `value.attr` or `value[0]`.
    fn loaded_symbol_name(&self) -> Option<&Name> {
        self.place_expr().as_symbol().map(Symbol::name)
    }
}

pub(crate) enum PlaceLoadResolutionStep<'db> {
    // A source that can supply the value for a load.
    Source(PlaceLoadSource<'db>),
    // A condition that the caller must evaluate to determine whether resolution should continue
    // for a member load.
    MemberResolutionCondition(PlaceExprPrefixLoads<'db>),
    // A marker that declares that resolution ended in a explicit failure.
    Exhausted(PlaceLoadFailure),
}

/// One source that can supply the value of a place load, along with the
/// type narrowing constraints that apply to it.
///
/// ## How constraint tracking is implemented
///
/// [`PlaceLoadResolution`] stores one shared list of constraint keys. Each
/// source maintains an `entry_checkpoint` into that list, which identifies the
/// constraints used to narrow the source.
///
/// When a key identifies the binding state used to construct a source, that key
/// becomes active after the source is requested, but applying it to the same
/// source again would duplicate work.
///
/// ### Example
///
/// ```py
/// from collections.abc import Callable
///
/// def make_counter(start: int, enabled: bool) -> Callable[[], int | None]:
///     if enabled:
///         value = start
///     else:
///         value = None
///
///     def next_value() -> int | None:
///         nonlocal value
///         if value is not None:
///             current = value  # load U
///             value += 1
///             return current
///         return None
///
///     return next_value
/// ```
///
/// For `U` above, the constraint representation after both sources have been
/// requested is schematically:
///
/// ```text
/// PlaceLoadResolution {
///     constraint_keys: [
///         (next_value, UseId(U)),
///     ],
/// }
/// PlaceLoadSource {
///     kind: Bindings(next_value at U),
///     entry_checkpoint: 0,
/// }
/// PlaceLoadSource {
///     kind: DefinitionsFromOwningScope(make_counter.value),
///     entry_checkpoint: 1,
/// }
/// ```
///
/// The first source already comes from `bindings_at_use(U)`, so its `UseId` key
/// becomes active when the source is requested but is not applied on entry. If
/// that source is undefined and the consumer requests the next source, the
/// `UseId` key narrows the enclosing `int | None` place to `int`. If both
/// sources are exhausted, the key remains active for expression-level narrowing.
pub(crate) struct PlaceLoadSource<'db> {
    /// How this source supplies the loaded value.
    pub(crate) kind: PlaceLoadSourceKind<'db>,
    /// Selects the constraints used to narrow this source.
    entry_checkpoint: usize,
    /// The role this source plays in the load.
    role: PlaceLoadSourceRole,
}

impl PlaceLoadSource<'_> {
    /// Returns whether this source is the module fallback for a class-local name.
    pub(crate) fn is_class_body_global_fallback(&self) -> bool {
        self.role == PlaceLoadSourceRole::ClassBodyGlobalFallback
    }

    /// Returns whether this source is considered after lexical name resolution.
    pub(crate) fn is_post_lexical(&self) -> bool {
        matches!(
            self.kind,
            PlaceLoadSourceKind::Implicit(
                ImplicitPlaceLoad::ModuleImplicitGlobal { .. } | ImplicitPlaceLoad::Builtin(_)
            )
        )
    }
}

/// Describes how a source can supply a place's value.
pub(crate) enum PlaceLoadSourceKind<'db> {
    /// Bindings already selected for this load state.
    ///
    /// For an ordinary expression, these are the bindings that reach that point:
    ///
    /// ```py
    /// value = 1
    /// reveal_type(value)  # Only the first binding reaches this load.
    /// value = "later"
    /// ```
    ///
    /// An enclosing eager snapshot is likewise a point-in-time view. A deferred load instead
    /// selects all bindings reachable in its scope.
    Bindings(BindingWithConstraintsIterator<'db, 'db>),
    /// The whole place in the scope that owns it.
    ///
    /// A free-variable load in a lazy nested scope can observe any definition reachable for the
    /// owning place, rather than the state at a single point:
    ///
    /// ```py
    /// def outer():
    ///     value: int | str = 1
    ///
    ///     def inner():
    ///         return value
    ///
    ///     value = "later"
    /// ```
    ///
    /// Keeping the scope and place ID lets inference evaluate both the declaration and all
    /// reachable bindings for `outer.value`; an already-selected binding iterator does not retain
    /// that whole-place information.
    DefinitionsFromOwningScope {
        /// The scope containing the place.
        scope: ScopeId<'db>,
        /// The place within `scope`.
        id: ScopedPlaceId,
    },
    /// A source represented by a specialized query or rule.
    Implicit(ImplicitPlaceLoad<'db>),
}

/// A source that consumers evaluate using a specialized query or rule.
pub(crate) enum ImplicitPlaceLoad<'db> {
    /// The implicit `__class__` cell for a method, lambda, or generator expression defined directly
    /// in a class body, e.g.:
    ///
    /// ```py
    /// class C:
    ///     def method(self):
    ///         return __class__
    /// ```
    DunderClass(Definition<'db>),
    /// An implicit symbol supplied directly in a class body, e.g.:
    ///
    /// ```py
    /// class C:
    ///     defining_module = __module__
    /// ```
    ClassBodySymbol(Name),
    /// A symbol in the module's explicit global namespace, e.g.:
    ///
    /// ```py
    /// answer = 42
    ///
    /// def get_answer():
    ///     return answer
    /// ```
    ExplicitGlobalSymbol { file: ProgramFile<'db>, name: Name },
    /// An implicit attribute supplied by a module, such as `__name__`.
    ModuleImplicitGlobal { file: ProgramFile<'db>, name: Name },
    /// A name supplied by the builtin namespace.
    Builtin(Name),
}

/// The role a source plays in a place load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlaceLoadSourceRole {
    /// The source follows ordinary Python name resolution rules.
    Ordinary,
    /// The source follows Python’s class-local-to-module fallback rules.
    ClassBodyGlobalFallback,
}

/// The reason resolution stops if the preceding sources do not supply a value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaceLoadFailure {
    /// No additional place-load source applies.
    ///
    /// For a symbol load, this means runtime lookup raises `NameError`.
    NotFound,
    /// The current function-like binding scope owns the loaded symbol but
    /// supplies no value.
    ///
    /// Loading the symbol at runtime raises `UnboundLocalError`.
    UnboundLocal,
    /// An enclosing function-like binding scope owns the place, so resolution
    /// cannot continue to module globals or builtins.
    ///
    /// For a symbol load, an empty closure cell raises `NameError` at runtime.
    UnboundFree,
}

/// Compact descriptions of loads for the tracked prefixes of a place expression.
///
/// Resolution continues past the local source only if every tracked prefix is locally undefined.
///
/// For example, the enclosing binding of `obj.value` cannot supply the value read in `inner`:
///
/// ```python
/// class Outer:
///     value: int
///
/// class Inner:
///     value: str
///
/// def outer():
///     obj = Outer()
///     obj.value = 1
///
///     def inner():
///         obj = Inner()
///         reveal_type(obj.value)  # revealed: str
/// ```
///
/// The nested scope binds `obj` to a different object, so normal member lookup on the local
/// `obj` must handle the load instead.
pub(crate) struct PlaceExprPrefixLoads<'db> {
    scope: ScopeId<'db>,
    loads: SmallVec<[PlaceExprPrefixLoad; 2]>,
}

impl<'db> PlaceExprPrefixLoads<'db> {
    /// Creates prefix loads, returning `None` when the iterator is empty.
    fn from_iter(
        scope: ScopeId<'db>,
        loads: impl IntoIterator<Item = PlaceExprPrefixLoad>,
    ) -> Option<Self> {
        let loads = loads.into_iter().collect::<SmallVec<_>>();
        (!loads.is_empty()).then_some(Self { scope, loads })
    }

    /// Returns the scope containing the prefix loads.
    pub(crate) fn scope(&self) -> ScopeId<'db> {
        self.scope
    }

    /// Iterates over the prefix loads.
    pub(crate) fn iter(&self) -> impl Iterator<Item = PlaceExprPrefixLoad> + '_ {
        self.loads.iter().copied()
    }
}

/// Describes how a consumer can evaluate one prefix of a place expression.
#[derive(Clone, Copy)]
pub(crate) enum PlaceExprPrefixLoad {
    /// Use the bindings that reach this expression occurrence.
    AtUse(ScopedUseId),
    /// Use every binding reachable for this place in its scope.
    AllReachable(ScopedPlaceId),
    /// The syntax itself guarantees that the prefix is bound.
    DefinitelyBound,
}

/// Read-only context used to select sources for a place load.
#[derive(Clone, Copy)]
struct PlaceLoadResolutionContext<'db, 'ast> {
    db: &'db dyn Db,
    index: &'db SemanticIndex<'db>,
    scope: ScopeId<'db>,
    file: ProgramFile<'db>,
    mode: PlaceLoadMode<'ast>,
}

impl<'db> PlaceLoadResolutionContext<'db, '_> {
    fn symbol_has_scope_declaration(self, place_expr: PlaceExprRef) -> bool {
        let Some(symbol) = place_expr.as_symbol() else {
            return false;
        };
        let scope = self.scope.file_scope_id(self.db);
        let table = self.index.place_table(scope);
        let Some(symbol_id) = table.symbol_id(symbol.name()) else {
            return false;
        };
        let symbol = table.symbol(symbol_id);
        symbol.is_global() || symbol.is_nonlocal()
    }

    fn is_class_body_scope(self) -> bool {
        self.scope.node(self.db).scope_kind().is_class()
    }

    fn uses_enclosing_snapshots(self) -> bool {
        matches!(
            self.mode,
            PlaceLoadMode::AtExpression(_) | PlaceLoadMode::AtNameSnapshot(_)
        )
    }

    fn is_lexical_enclosing_scope(self, enclosing_scope: FileScopeId) -> bool {
        self.index.scope(enclosing_scope).kind().is_function_like()
            || (self.scope.is_annotation(self.db)
                && self.scope.scope(self.db).parent() == Some(enclosing_scope))
    }

    fn local_source(
        self,
        place_expr: PlaceExprRef,
    ) -> Option<(
        PlaceLoadSourceKind<'db>,
        Option<(FileScopeId, ConstraintKey)>,
    )> {
        let scope = self.scope.file_scope_id(self.db);
        let table = self.index.place_table(scope);
        let use_def = self.index.use_def_map(scope);

        match self.mode {
            PlaceLoadMode::AtExpression(expr_ref) => {
                if expr_ref
                    .as_name_expr()
                    .is_some_and(|name| name.is_invalid())
                {
                    return None;
                }

                let use_id = expr_ref.scoped_use_id(self.db, self.file);
                Some((
                    PlaceLoadSourceKind::Bindings(use_def.bindings_at_use(use_id)),
                    Some((scope, ConstraintKey::UseId(use_id))),
                ))
            }
            PlaceLoadMode::AtNameSnapshot(snapshot) => Some((
                PlaceLoadSourceKind::Bindings(use_def.bindings_at_snapshot(snapshot)),
                Some((scope, ConstraintKey::Snapshot(snapshot))),
            )),
            PlaceLoadMode::Deferred | PlaceLoadMode::StringAnnotation => {
                let source = table
                    .place_id(place_expr)
                    .map(|id| PlaceLoadSourceKind::Bindings(use_def.reachable_bindings(id)));
                assert!(
                    source.is_some() || matches!(self.mode, PlaceLoadMode::StringAnnotation),
                    "Expected the place table to create a place for every valid PlaceExpr node"
                );
                source.map(|source| (source, None))
            }
        }
    }

    /// Describes how to evaluate the tracked prefixes of `place_expr` in this scope.
    fn place_expr_prefix_loads(
        self,
        place_expr: PlaceExprRef,
    ) -> Option<PlaceExprPrefixLoads<'db>> {
        let table = self.index.place_table(self.scope.file_scope_id(self.db));

        PlaceExprPrefixLoads::from_iter(
            self.scope,
            table
                .parents(place_expr)
                .filter_map(|prefix_id| match self.mode {
                    PlaceLoadMode::AtNameSnapshot(_) => None,
                    PlaceLoadMode::Deferred | PlaceLoadMode::StringAnnotation => {
                        Some(PlaceExprPrefixLoad::AllReachable(prefix_id))
                    }
                    PlaceLoadMode::AtExpression(mut prefix_expr_ref) => {
                        let prefix = table.place(prefix_id);
                        for _ in
                            0..(place_expr.num_member_segments() - prefix.num_member_segments())
                        {
                            prefix_expr_ref = match prefix_expr_ref {
                                ast::ExprRef::Attribute(attribute) => {
                                    ast::ExprRef::from(&attribute.value)
                                }
                                ast::ExprRef::Subscript(subscript) => {
                                    ast::ExprRef::from(&subscript.value)
                                }
                                _ => return None,
                            };
                        }

                        if prefix_expr_ref
                            .as_name_expr()
                            .is_some_and(|name| name.is_invalid())
                        {
                            return None;
                        }

                        if let ast::ExprRef::Named(named) = prefix_expr_ref {
                            return named
                                .target
                                .is_name_expr()
                                .then_some(PlaceExprPrefixLoad::DefinitelyBound);
                        }

                        Some(PlaceExprPrefixLoad::AtUse(
                            prefix_expr_ref.scoped_use_id(self.db, self.file),
                        ))
                    }
                }),
        )
    }

    fn skips_non_global_scopes(self, symbol: ScopedSymbolId) -> bool {
        let scope = self.scope.file_scope_id(self.db);
        !scope.is_global() && self.index.symbol_is_global_in_scope(symbol, scope)
    }

    fn dunder_class_cell_definition(self) -> Option<Definition<'db>> {
        let current_scope = self.scope.file_scope_id(self.db);
        if let Some(definition) = self.index.class_definition_of_method(current_scope) {
            return Some(definition);
        }

        let scope = self.index.scope(current_scope);
        if !matches!(
            scope.node(),
            NodeWithScopeKind::Lambda(_) | NodeWithScopeKind::GeneratorExpression(_)
        ) {
            return None;
        }
        let class = self.index.parent_scope(current_scope)?.node().as_class()?;
        Some(self.index.expect_single_definition(class))
    }
}

/// A node in the acyclic graph traversed by a [`PlaceLoadResolution`].
///
/// Source-named nodes may yield a [`PlaceLoadResolutionStep::Source`]. The two verb-named nodes
/// either ask the consumer whether traversal should continue or decide which outgoing edge to
/// follow. Every transition advances toward a [`PlaceLoadResolutionNode::Failure`] leaf; no node
/// is revisited.
enum PlaceLoadResolutionNode<'db> {
    /// The source selected from the load's own scope, if one exists.
    LocalSource,
    /// Ask the consumer whether resolution should continue for a member load.
    AskConsumerWhetherToContinueForMember,
    /// Decide whether resolution ends, continues through enclosing scopes, or moves to the module
    /// scope.
    DecideResolutionPath,
    /// The implicit `__class__` source, followed by enclosing scopes.
    DunderClassSource {
        definition: Definition<'db>,
        enclosing_scopes: AncestorsIter<'db>,
    },
    /// A source from the remaining enclosing scopes, if one exists.
    EnclosingScopeSource(AncestorsIter<'db>),
    /// The implicit class-body source, followed by the applicable global source.
    ImplicitClassBodySource(Option<ForwardedGlobalSnapshot<'db>>),
    /// Bindings from an enclosing `global` declaration that were visible when the nested eager
    /// scope began.
    ForwardedGlobalSnapshotSource(ForwardedGlobalSnapshot<'db>),
    /// An explicit global source with the given role, if one exists.
    ExplicitGlobalSource(PlaceLoadSourceRole),
    /// An implicit global considered after explicit lookup, if one exists.
    ImplicitGlobalSource,
    /// The builtin with the given name.
    BuiltinSource(Name),
    /// The failure that ends resolution.
    Failure(PlaceLoadFailure),
}

struct ForwardedGlobalSnapshot<'db> {
    bindings: BindingWithConstraintsIterator<'db, 'db>,
    enclosing_scope: FileScopeId,
}

/// Narrowing constraints accumulated while a consumer advances through a place load.
#[derive(Default)]
struct PlaceLoadConstraints {
    constraint_keys: Vec<(FileScopeId, ConstraintKey)>,
}

impl PlaceLoadConstraints {
    /// Creates a source narrowed by the constraints accumulated before it.
    ///
    /// `exit_constraint`, when present, is activated only after the source is requested (that
    /// source was already selected from the binding state identified by the constraint, so it is
    /// deliberately not reapplied to the same source).
    ///
    /// For example, consider the load at `U`:
    ///
    /// ```py
    /// def outer(value: int | None):
    ///     def inner():
    ///         if value is not None:
    ///             return value  # U
    /// ```
    ///
    /// The local source is selected by `bindings_at_use(U)`. Its `UseId(U)` is the exit constraint:
    /// it is not applied again to that source, but becomes active if the source is unbound so that
    /// the enclosing `outer.value` source is narrowed from `int | None` to `int`.
    fn source<'db>(
        &mut self,
        kind: PlaceLoadSourceKind<'db>,
        role: PlaceLoadSourceRole,
        exit_constraint: Option<(FileScopeId, ConstraintKey)>,
    ) -> PlaceLoadSource<'db> {
        let entry_checkpoint = self.constraint_keys.len();
        self.constraint_keys.extend(exit_constraint);
        PlaceLoadSource {
            kind,
            entry_checkpoint,
            role,
        }
    }

    /// Creates a source without applying accumulated narrowing constraints to it.
    fn unnarrowed_source(
        kind: PlaceLoadSourceKind<'_>,
        role: PlaceLoadSourceRole,
    ) -> PlaceLoadSource<'_> {
        PlaceLoadSource {
            kind,
            entry_checkpoint: 0,
            role,
        }
    }

    /// Extends the list of constraints used by subsequent sources.
    fn push(&mut self, scope: FileScopeId, key: ConstraintKey) {
        self.constraint_keys.push((scope, key));
    }

    /// Returns the constraints used to narrow `source`.
    fn narrowing_constraints_for(
        &self,
        source: &PlaceLoadSource<'_>,
    ) -> &[(FileScopeId, ConstraintKey)] {
        &self.constraint_keys[..source.entry_checkpoint]
    }

    /// Returns the constraints activated by the sources that were requested.
    fn into_constraints(self) -> Vec<(FileScopeId, ConstraintKey)> {
        self.constraint_keys
    }
}
