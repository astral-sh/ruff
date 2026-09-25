//! Redundant-condition exemptions for assertions, environment guards, and defensive exits.
//!
//! "Environment guards" select code for a particular Python version, platform, or type-checking
//! context. For example, `if sys.version_info >= (3, 14)`, `if sys.platform == "win32"`,
//! `if os.name == "posix"`, and `if TYPE_CHECKING` are all environment guards. Their outcomes
//! may be fixed for the configured environment while still serving a purpose in code that is
//! designed to support multiple environments.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, helpers::any_over_expr, name::Name};
use rustc_hash::FxHashMap;
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{
    BindingWithConstraintsIterator, ProgramFile,
    definition::{Definition, DefinitionKind},
    global_scope,
    place::{PlaceExpr, ScopedPlaceId},
    place_table,
    predicate::{PredicateNode, ScopedPredicateId},
    reachability_constraints::ScopedReachabilityConstraintId,
    scope::ScopeId,
    semantic_index, use_def_map,
};

use crate::{
    Db, Program, ProgramEnvironment,
    place::{
        ConsideredDefinitions, Provenance, RequiresExplicitReExport, place_by_id,
        place_from_bindings,
    },
    place_load::{
        ImplicitPlaceLoad, PlaceExprPrefixLoad, PlaceLoadMode, PlaceLoadResolutionStep,
        PlaceLoadSourceKind, resolve_place_load,
    },
    types::{
        Type, TypeContext,
        definition_resolution::{
            ImportAliasResolution, ResolvedDefinition, definitions_for_attribute,
            definitions_for_name, resolve_definition,
        },
        diagnostic::REDUNDANT_CONDITION_STRICT,
        infer::{
            TypeInferenceBuilder,
            builder::redundant_conditions::{SuiteExitKind, suite_ends_with_exit},
        },
        infer_definition_types, infer_expression_types,
    },
};

use super::{ConditionKind, RedundantCondition};

/// The context in which a boolean test occurs.
///
/// This is used to help determine whether a test should be exempt from one or both
/// redundant-condition rules. For example, the same always-true comparison can be reported in
/// an `if` condition but exempt in an assertion.
///
/// [`ConditionKind`] determines the rule that will be applied if the condition is not exempted.
/// This context determines whether the test serves a purpose that makes reporting it undesirable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RedundantConditionContext {
    /// A boolean test checked without the additional exemptions represented by the other variants.
    ///
    /// This includes ordinary `if` conditions. For example:
    ///
    /// ```python
    /// def check(value: str):
    ///     if isinstance(value, str):  # Always true; flagged by `redundant-condition-strict`.
    ///         print(value)
    /// ```
    ///
    /// The special cases for assertions and checks that reject unexpected input are described
    /// by [`Self::Assertion`] and [`Self::DefensiveExit`].
    Standalone,

    /// A test within an assertion, including the complete assertion and tests in call arguments.
    ///
    /// Tests classified as [`ConditionKind::Boolean`] or [`ConditionKind::ShortCircuit`] are exempt.
    /// Other always-truthy or always-falsy values remain eligible for `redundant-condition`, or
    /// `redundant-condition-strict` if classified as [`ConditionKind::ContainsWalrus`].
    ///
    /// ```python
    /// def check(value: int, other: object, flag: bool):
    ///     assert isinstance(value, int)  # Defensive runtime check; exempt.
    ///     assert flag and (other or True)  # No diagnostic on `other or True`.
    ///     assert other or True  # Short-circuit assertion; exempt.
    /// ```
    ///
    /// An uncalled function in `assert not ready` is still reported: the function itself is an
    /// always-truthy value even though the complete assertion always fails.
    Assertion,

    /// Whether the branches of an `if` or `elif` test reject unexpected input or an
    /// unsupported operation.
    ///
    /// We call that rejection a "defensive exit". For example, a function might raise `TypeError`
    /// if its argument has the wrong type. Type annotations do not enforce this at runtime, so
    /// the check can still be useful when the function is called from untyped code. We therefore
    /// exempt conditions in [`ConditionKind::Boolean`] and [`ConditionKind::ShortCircuit`] when
    /// their fixed truthiness rules out taking a defensive branch.
    ///
    /// ```python
    /// def check(value: int):
    ///     # Always false according to the annotation,
    ///     # but exempted from diagnostics due to the defensive exit in the branch body:
    ///     if not isinstance(value, int):  
    ///         raise TypeError("expected an integer")
    /// ```
    ///
    /// The code that rejects the input can also be in an `else` branch:
    ///
    /// ```python
    /// def check(value: int):
    ///     # Always true according to the annotation,
    ///     # but exempted from diagnostics due to the defensive exit in the `else`-branch body:
    ///     if isinstance(value, int):  
    ///         ...
    ///     else:
    ///         raise TypeError("expected an integer")
    /// ```
    ///
    /// Or after an always-true final `if` or `elif` whose body ends in a recognized exit:
    ///
    /// ```python
    /// def check(value: int):
    ///     # Always true according to the annotation,
    ///     # but exempted from diagnostics due to the defensive exit in the body
    ///     # of the "implicit `else`" after the final `if`:
    ///     if isinstance(value, int):  # Always true according to the annotation; exempt.
    ///         return value
    ///     raise TypeError("expected an integer")
    /// ```
    ///
    /// [`suite_ends_with_exit`] describes the forms of rejection we recognize
    /// with [`SuiteExitKind::Defensive`].
    ///
    /// Boolean operands inherit these exemptions even when the complete condition has unknown
    /// truthiness. Negation reverses which branch their truthiness selects. Independent tests in
    /// call arguments do not inherit the exemptions, and mistakes such as testing an uncalled
    /// function can still be reported. Each field records whether that branch ends in a defensive exit.
    DefensiveExit {
        truthy_branch: bool,
        falsy_branch: bool,
    },
}

impl RedundantConditionContext {
    /// Identify the defensive branches of an `if` or `elif` once for the complete condition.
    ///
    /// This depends on the surrounding statements, not on whether the condition itself has
    /// fixed truthiness. A redundant operand can be part of an otherwise ambiguous condition.
    pub(super) fn for_if_statement(
        builder: &TypeInferenceBuilder<'_, '_>,
        body: &[ast::Stmt],
        following_clauses: &[ast::ElifElseClause],
        following_statements: &[ast::Stmt],
    ) -> Self {
        if !builder.context.is_lint_enabled(&REDUNDANT_CONDITION_STRICT) {
            return Self::Standalone;
        }

        let falsy_suite = match following_clauses {
            [else_clause] if else_clause.test.is_none() => Some(else_clause.body.as_slice()),
            [] if suite_ends_with_exit(builder, body, SuiteExitKind::Any) => {
                Some(following_statements)
            }
            _ => None,
        };

        Self::DefensiveExit {
            truthy_branch: suite_ends_with_exit(builder, body, SuiteExitKind::Defensive),
            falsy_branch: falsy_suite.is_some_and(|suite| {
                suite_ends_with_exit(builder, suite, SuiteExitKind::Defensive)
            }),
        }
    }

    /// Return `true` if a given diagnostic candidate should be exempted.
    ///
    /// Reasons for exemption could be that the condition serves a defensive-programming purpose,
    /// or that the condition depends on an environment/compatibility check such as `sys.version_info`
    /// or `sys.platform`.
    pub(super) fn exempts(
        self,
        builder: &TypeInferenceBuilder<'_, '_>,
        condition: &RedundantCondition<'_, '_>,
    ) -> bool {
        if matches!(
            condition.kind,
            ConditionKind::NoneUnion(_) | ConditionKind::Iterable | ConditionKind::Callable(_)
        ) {
            // These checks should always be considered suspicious.
            return false;
        }

        let defensive = match self {
            Self::Assertion => matches!(
                &condition.kind,
                ConditionKind::Boolean | ConditionKind::ShortCircuit
            ),
            Self::DefensiveExit {
                truthy_branch,
                falsy_branch,
            } => {
                let is_boolean_or_short_circuit = matches!(
                    &condition.kind,
                    ConditionKind::Boolean | ConditionKind::ShortCircuit
                );

                is_boolean_or_short_circuit
                    && if condition.truthiness.is_always_true() {
                        falsy_branch
                    } else {
                        truthy_branch
                    }
            }
            Self::Standalone => false,
        };

        if defensive {
            return true;
        }

        match condition.expression {
            // The inferred types for these will always be the same,
            // even if a subexpression is defined in terms of `sys.version_info`, `sys.platform`, `os.name`, or
            // `typing.TYPE_CHECKING`. We don't need to recurse into them *at the top level*, and it's more
            // accurate not to do so.
            //
            // Note that this is only true at the *top level* of the condition!
            //
            // We *do* need to recurse into subexpressions of a generator expression, for example, if that
            // generator is a subexpression of a call expression, e.g.
            // `any(x for x in range(10) if sys.version_info >= (3, 14))`. Similar concerns apply to lambdas,
            // which can be eagerly called, etc. etc.
            ast::Expr::StringLiteral(_)
            | ast::Expr::BytesLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::EllipsisLiteral(_)
            | ast::Expr::Dict(_)
            | ast::Expr::DictComp(_)
            | ast::Expr::Generator(_)
            | ast::Expr::Lambda(_)
            | ast::Expr::Set(_)
            | ast::Expr::SetComp(_)
            | ast::Expr::List(_)
            | ast::Expr::ListComp(_)
            | ast::Expr::TString(_)
            | ast::Expr::NoneLiteral(_) => false,

            // These expressions can contain subexpressions that are defined in terms of `sys.version_info`,
            // `sys.platform`, `os.name`, or `typing.TYPE_CHECKING`. We need to recurse into them to check for
            // those subexpressions, since the type of the overall expression can depend on the types of those
            // subexpressions.
            ast::Expr::Name(_)
            | ast::Expr::UnaryOp(_)
            | ast::Expr::If(_)
            | ast::Expr::FString(_)
            | ast::Expr::Yield(_)
            | ast::Expr::YieldFrom(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Named(_)
            | ast::Expr::Call(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::Await(_)
            | ast::Expr::BoolOp(_)
            | ast::Expr::Tuple(_)
            | ast::Expr::Slice(_)
            | ast::Expr::Starred(_)
            | ast::Expr::IpyEscapeCommand(_)
            | ast::Expr::Compare(_) => any_over_expr(condition.expression, |expression| {
                is_special_cased_condition_expression(
                    builder.db(),
                    builder.program_file(),
                    expression,
                    |expr| builder.expression_type(expr),
                )
            }),
        }
    }

    /// Reverse the defensive branches for the operand of `not`: a truthy operand makes the
    /// containing condition false, and a falsy operand makes it true.
    ///
    /// For example, the body of this `if` is a defensive exit:
    ///
    /// ```python
    /// def check(value: int):
    ///     if not isinstance(value, int):
    ///         raise TypeError("expected an integer")
    /// ```
    ///
    /// The `raise` is reached when `not isinstance(value, int)` is true, which means that
    /// `isinstance(value, int)` is false. When checking that operand, we therefore move the
    /// defensive-exit flag from `truthy_branch` to `falsy_branch`. This preserves the exemption:
    /// although the annotation tells us that `isinstance(value, int)` is always true, the check
    /// still protects against unexpected input at runtime.
    pub(super) const fn negated(self) -> Self {
        match self {
            Self::DefensiveExit {
                truthy_branch,
                falsy_branch,
            } => Self::DefensiveExit {
                truthy_branch: falsy_branch,
                falsy_branch: truthy_branch,
            },
            context => context,
        }
    }

    /// Return the context for a boolean test inside another expression, such as a `not`
    /// expression passed as a call argument.
    ///
    /// An [`Self::Assertion`] context also exempts boolean and integer tests within call arguments.
    /// For example, the assertion in `assert consume(not flag)` exempts the test of `flag` if it has
    /// a boolean or integer type.
    ///
    /// Tests within call arguments do not select the containing condition's branches, so they
    /// do not inherit its [defensive-exit exemptions](Self::DefensiveExit).
    pub(super) const fn nested_test(self) -> Self {
        match self {
            // Assertions also exempt boolean tests embedded in calls or other value expressions.
            Self::Assertion => self,
            Self::Standalone | Self::DefensiveExit { .. } => Self::Standalone,
        }
    }
}

/// Return `true` if any subexpression in `expression` is recognized as "tainted" by being defined
/// (directly or indirectly) with respect to `sys.version_info`, `sys.platform`, `os.name`, or
/// `typing.TYPE_CHECKING`.
///
/// Follow assignments and imports so aliases inherit the same exemption as the original guard.
fn is_special_cased_condition_expression<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    expression: &ast::Expr,
    mut expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> bool {
    match expression {
        ast::Expr::Name(ast::ExprName { id, .. }) if id == "TYPE_CHECKING" => return true,
        ast::Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => match &**attr {
            "TYPE_CHECKING" => return true,
            "name" => {
                let value_type = expression_type(value);
                if let Type::ModuleLiteral(module) = value_type
                    && module.module(db).is_known(db, KnownModule::Os)
                {
                    return true;
                }
                if value_type.is_never() {
                    return true;
                }
            }
            "version_info" | "platform" => {
                let value_type = expression_type(value);
                if let Type::ModuleLiteral(module) = value_type
                    && module.module(db).is_known(db, KnownModule::Sys)
                {
                    return true;
                }
                if value_type.is_never() {
                    return true;
                }
            }
            _ => {}
        },
        _ => {}
    }

    condition_definition_info(db, file, expression, expression_type)
        .contains_special_cased_condition
}

/// Resolves definitions that reach this read, falling back to member lookup for attributes without
/// a definite assignment. The receiver type comes from the caller to avoid re-entering inference
/// of the enclosing scope.
pub(super) fn condition_definition_info<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    expression: &ast::Expr,
    expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> ConditionDefinitionInfo<'db> {
    // Walking assignment expressions also visits their targets, which have no use-site bindings.
    match expression {
        ast::Expr::Name(name) if name.ctx.is_load() => {}
        ast::Expr::Attribute(attribute) if attribute.ctx.is_load() => {}
        _ => return ConditionDefinitionInfo::default(),
    }
    place_condition_definition_info(db, file, expression, expression_type)
}

/// Also accepts augmented-assignment targets, which have a recorded load despite their store context.
fn place_condition_definition_info<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    expression: &ast::Expr,
    mut expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> ConditionDefinitionInfo<'db> {
    let mut info = ConditionDefinitionInfo::default();
    let index = semantic_index(db, file);
    if let Some(scope) = index.try_expression_scope_id(expression)
        && let Some(place) = PlaceExpr::try_from_expr(expression)
    {
        let scope = scope.to_scope_id(db, file);
        let env = ProgramEnvironment::from_scope(scope);
        let resolution = resolve_place_load(
            db,
            index,
            scope,
            place,
            PlaceLoadMode::AtExpression(expression.into()),
        );
        for step in resolution {
            let source = match step {
                PlaceLoadResolutionStep::Source(source) => source,
                PlaceLoadResolutionStep::MemberResolutionCondition(prefixes) => {
                    let use_def = use_def_map(db, prefixes.scope());
                    if prefixes.iter().any(|prefix| {
                        let bindings = match prefix {
                            PlaceExprPrefixLoad::AtUse(id) => use_def.bindings_at_use(id),
                            PlaceExprPrefixLoad::AllReachable(id) => use_def.reachable_bindings(id),
                            PlaceExprPrefixLoad::DefinitelyBound => return true,
                        };
                        !place_from_bindings(db, &env, bindings).place.is_undefined()
                    }) {
                        break;
                    }
                    continue;
                }
                PlaceLoadResolutionStep::Exhausted(_) => break,
            };
            let (source_info, definitely_bound) = match source.kind {
                PlaceLoadSourceKind::Bindings(bindings) => {
                    let definitely_bound = place_from_bindings(db, &env, bindings.clone())
                        .place
                        .is_definitely_bound();
                    (
                        ConditionDefinitionInfo::from_bindings(db, bindings),
                        definitely_bound,
                    )
                }
                PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                    owning_scope_condition_definition_info(db, scope, id)
                }
                PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                    file,
                    name,
                }) => {
                    info.merge(name_condition_definition_info(
                        db,
                        global_scope(db, file),
                        name,
                    ));
                    break;
                }
                PlaceLoadSourceKind::Implicit(_) => continue,
            };
            info.merge(source_info);
            if definitely_bound {
                return info;
            }
        }
    }

    if let ast::Expr::Attribute(attribute) = expression {
        info.merge(attribute_condition_definition_info(
            db,
            file.program(db),
            expression_type(&attribute.value),
            attribute.attr.id.clone(),
        ));
    }
    info
}

/// The information needed for condition exemptions and annotation hints.
///
/// Retaining only the unique definition and the provenance result lets both uses share a lookup
/// without caching a potentially large list of bindings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ConditionDefinitionInfo<'db> {
    provenance: Provenance<'db>,
    contains_special_cased_condition: bool,
}

impl<'db> ConditionDefinitionInfo<'db> {
    pub(super) fn single_definition(self) -> Option<Definition<'db>> {
        self.provenance.definition()
    }

    fn merge(&mut self, other: Self) {
        self.provenance = self.provenance.or(other.provenance);
        self.contains_special_cased_condition |= other.contains_special_cased_condition;
    }

    /// Follow only bindings retained at the read. Keep environment-dependent alternatives even
    /// when their predicates are false for the configured platform or Python version.
    fn from_bindings(db: &'db dyn Db, bindings: BindingWithConstraintsIterator<'_, 'db>) -> Self {
        let mut info = Self::default();
        for binding in bindings {
            if binding.reachability_constraint == ScopedReachabilityConstraintId::ALWAYS_FALSE {
                continue;
            }
            let Some(definition) = binding.binding.definition() else {
                continue;
            };
            info.merge(resolved_condition_definition_info(db, definition));
        }
        info
    }

    /// Summarizes resolved definitions, following assignments to establish environment provenance.
    fn from_definitions(db: &'db dyn Db, definitions: &[ResolvedDefinition<'db>]) -> Self {
        let contains_special_cased_condition = definitions
            .iter()
            .filter_map(ResolvedDefinition::definition)
            .any(|definition| definition_contains_special_cased_condition(db, definition));

        let provenance = match definitions {
            [] => Provenance::Unknown,
            [ResolvedDefinition::Definition(definition)] => {
                Provenance::SingleDefinition(*definition)
            }
            _ => Provenance::MultipleDefinitions,
        };

        Self {
            provenance,
            contains_special_cased_condition,
        }
    }
}

/// Cache the original guards separately from the bindings selected at each read. A read inside
/// an environment guard does not make an earlier, unconditional assignment environment-dependent.
/// Index once per place to avoid scanning all its assignments for each condition.
#[salsa::tracked(returns(ref), heap_size = ruff_memory_usage::heap_size)]
fn definition_reachability<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place: ScopedPlaceId,
) -> FxHashMap<Definition<'db>, ScopedReachabilityConstraintId> {
    // A place is a variable or attribute, and several definitions can bind the same place.
    // Each map entry records the paths from the start of the scope to that binding. For example:
    //
    // ```python
    // if sys.platform == "win32":
    //     prefix = "\n"
    // else:
    //     prefix = ""
    // ```
    //
    // The map for `prefix` has two entries: the `"\n"` binding is guarded by the platform
    // comparison, and the `""` binding by its negation. The constraint IDs refer to those
    // formulas in the scope's use-def map, not the conditions' evaluated truthiness.
    let mut reachability: FxHashMap<_, _> = use_def_map(db, scope)
        .reachable_bindings(place)
        .filter_map(|binding| {
            Some((
                binding.binding.definition()?,
                binding.reachability_constraint,
            ))
        })
        .collect();
    reachability.shrink_to_fit();
    reachability
}

#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| ConditionDefinitionInfo::default(),
    heap_size = ruff_memory_usage::heap_size
)]
fn resolved_condition_definition_info<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> ConditionDefinitionInfo<'db> {
    let scope = definition.scope(db);
    let places = place_table(db, scope);
    let symbol_name = places
        .place(definition.place(db))
        .as_symbol()
        .map(|symbol| symbol.name().as_str());
    ConditionDefinitionInfo::from_definitions(
        db,
        &resolve_definition(
            db,
            &ProgramEnvironment::from_scope(scope),
            definition,
            symbol_name,
            ImportAliasResolution::ResolveAliases,
        ),
    )
}

/// Lazy closure reads share all reachable definitions in the owning scope. Cache both the provenance
/// traversal and boundness analysis so repeated conditions do not rescan all those bindings.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _| (ConditionDefinitionInfo::default(), false),
    heap_size = ruff_memory_usage::heap_size
)]
fn owning_scope_condition_definition_info<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    place: ScopedPlaceId,
) -> (ConditionDefinitionInfo<'db>, bool) {
    let use_def = use_def_map(db, scope);
    let mut info = ConditionDefinitionInfo::from_bindings(db, use_def.reachable_bindings(place));
    for declaration in use_def.reachable_declarations(place) {
        if let Some(definition) = declaration.declaration.definition() {
            info.merge(resolved_condition_definition_info(db, definition));
        }
    }
    let definitely_bound = place_by_id(
        db,
        scope,
        place,
        RequiresExplicitReExport::No,
        ConsideredDefinitions::AllReachable,
    )
    .place
    .is_definitely_bound();
    (info, definitely_bound)
}

/// Caches definition information for global reads in lazy scopes.
///
/// Any reachable module binding can supply such a read. Repeating this lookup for every condition
/// can be quadratic in the number of assignments, even when the per-definition traversal is cached.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _| ConditionDefinitionInfo::default(),
    heap_size = ruff_memory_usage::heap_size
)]
// Salsa copies this attribute to both the query wrapper and its inner function. The wrapper
// consumes `name`, so `#[expect]` would produce an unfulfilled lint expectation there.
#[allow(clippy::needless_pass_by_value, reason = "Salsa owns the query key")]
fn name_condition_definition_info<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: Name,
) -> ConditionDefinitionInfo<'db> {
    ConditionDefinitionInfo::from_definitions(
        db,
        &definitions_for_name(db, scope, &name, ImportAliasResolution::ResolveAliases),
    )
}

/// Caches definition information for a member of an already-inferred receiver type.
///
/// Attribute lookup can also repeatedly collect many bindings. Include the receiver type in the
/// key because narrowing or rebinding a receiver can change which definitions its members resolve
/// to. Taking that type from the caller avoids re-entering inference of the use-site scope.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _, _| ConditionDefinitionInfo::default(),
    heap_size = ruff_memory_usage::heap_size
)]
// Salsa copies this attribute to both the query wrapper and its inner function. The wrapper
// consumes `name`, so `#[expect]` would produce an unfulfilled lint expectation there.
#[allow(clippy::needless_pass_by_value, reason = "Salsa owns the query key")]
fn attribute_condition_definition_info<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    receiver: Type<'db>,
    name: Name,
) -> ConditionDefinitionInfo<'db> {
    ConditionDefinitionInfo::from_definitions(
        db,
        &definitions_for_attribute(
            db,
            &ProgramEnvironment::from_program(program),
            receiver,
            &name,
        ),
    )
}

/// Determines whether a definition originates from an environment-dependent guard.
///
/// Checks both the assigned value and the predicates controlling whether the binding is reached.
/// For example, `if sys.platform == "win32": prefix = "\n"` makes `prefix` environment-dependent
/// even though the assigned string does not mention `sys.platform`.
///
/// Follows aliases recursively and recognizes stub declarations for `sys.version_info`,
/// `sys.platform`, `os.name`, and `typing.TYPE_CHECKING`.
///
/// This Salsa-tracked query reads the definition's AST behind its own incremental boundary, so
/// callers do not depend directly on another file's syntax tree. Cyclic aliases recover as `false`.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _| false,
    heap_size = ruff_memory_usage::heap_size
)]
fn definition_contains_special_cased_condition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> bool {
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let definition_kind = definition.kind(db);
    let file = definition.file(db);
    let program_file = definition.program_file(db);

    let in_known_module = |known| {
        file_to_module(db, program_file.resolver_file(db))
            .is_some_and(|module| module.is_known(db, known))
    };

    if let DefinitionKind::AnnotatedAssignment(annotated_assignment) = definition_kind
        && file.is_stub(db)
        && let ast::Expr::Name(ast::ExprName { id, .. }) = annotated_assignment.target(&module)
    {
        match &**id {
            "version_info" | "platform" if in_known_module(KnownModule::Sys) => {
                return true;
            }
            "name" if in_known_module(KnownModule::Os) => {
                return true;
            }
            "TYPE_CHECKING" if in_known_module(KnownModule::Typing) => {
                return true;
            }
            _ => {}
        }
    }

    let source_expression = match definition_kind {
        DefinitionKind::Assignment(assignment) => Some(assignment.value(&module)),
        DefinitionKind::AnnotatedAssignment(assignment) => assignment.value(&module),
        DefinitionKind::NamedExpression(named) => Some(&*named.node(&module).value),
        DefinitionKind::For(for_statement) => Some(for_statement.iterable(&module)),
        DefinitionKind::Comprehension(comprehension) => Some(comprehension.iterable(&module)),
        DefinitionKind::WithItem(with_item) => Some(with_item.context_expr(&module)),
        DefinitionKind::MatchPattern(pattern) => {
            Some(pattern.predicate().subject(db).node_ref(db).node(&module))
        }
        DefinitionKind::LoopHeader(header) => {
            let use_def = use_def_map(db, definition.scope(db));
            return use_def
                .loop_header(header.loop_header_id())
                .bindings_for_place(header.place())
                .filter(|binding| {
                    binding.reachability_constraint()
                        != ScopedReachabilityConstraintId::ALWAYS_FALSE
                })
                .filter_map(|binding| use_def.definition(binding.binding()).definition())
                .any(|source| {
                    resolved_condition_definition_info(db, source).contains_special_cased_condition
                });
        }
        DefinitionKind::NestedBindings(nested) => {
            return nested
                .visible_binding_sources(
                    semantic_index(db, program_file),
                    definition.file_scope(db),
                )
                .any(|bindings| {
                    ConditionDefinitionInfo::from_bindings(db, bindings)
                        .contains_special_cased_condition
                });
        }
        DefinitionKind::AugmentedAssignment(assignment) => {
            let assignment = assignment.node(&module);
            if place_condition_definition_info(db, program_file, &assignment.target, |expr| {
                infer_definition_types(db, definition).expression_type(expr)
            })
            .contains_special_cased_condition
            {
                return true;
            }
            Some(&*assignment.value)
        }
        DefinitionKind::Import(_)
        | DefinitionKind::ImportFrom(_)
        | DefinitionKind::ImportFromSubmodule(_)
        | DefinitionKind::StarImport(_)
        | DefinitionKind::Function(_)
        | DefinitionKind::Class(_)
        | DefinitionKind::TypeAlias(_)
        | DefinitionKind::DictKeyAssignment(_)
        | DefinitionKind::Parameter(_)
        | DefinitionKind::LambdaParameter(_)
        | DefinitionKind::ExceptHandler(_)
        | DefinitionKind::TypeVar(_)
        | DefinitionKind::ParamSpec(_)
        | DefinitionKind::TypeVarTuple(_) => None,
    };
    let Some(source_expression) = source_expression else {
        return false;
    };

    // A version guard can select a different function signature without changing the fact that
    // the function object is always truthy. Only definitions with a source expression above
    // inherit the provenance of their guards.
    let reachability = definition_reachability(db, definition.scope(db), definition.place(db))
        .get(&definition)
        .copied()
        .unwrap_or(ScopedReachabilityConstraintId::ALWAYS_TRUE);
    if reachability_contains_special_cased_condition(db, definition.scope(db), reachability) {
        return true;
    }

    // Binding inference does not always retain the source expression's types: unpacked targets
    // share a source, and a comprehension's first iterable belongs to the enclosing scope.
    // Read those types from the standalone expression query, without re-entering scope inference.
    let standalone = semantic_index(db, program_file).try_expression(source_expression);
    let mut expression_inference = None;
    let mut definition_inference = None;

    any_over_expr(source_expression, |expression| {
        is_special_cased_condition_expression(db, program_file, expression, |expr| {
            if let Some(standalone) = standalone {
                expression_inference
                    .get_or_insert_with(|| {
                        infer_expression_types(db, standalone, TypeContext::default())
                    })
                    .expression_type(expr)
            } else {
                definition_inference
                    .get_or_insert_with(|| infer_definition_types(db, definition))
                    .expression_type(expr)
            }
        })
    })
}

/// Summarize environment provenance for a reachability node and all its descendants.
///
/// Successive statements often share most of their reachability graph. Cache each node's
/// summary so checking another definition does not traverse the shared portion again.
/// All three branches contribute, regardless of the configured environment's truthiness.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _| false,
    heap_size = ruff_memory_usage::heap_size
)]
fn reachability_contains_special_cased_condition<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    reachability: ScopedReachabilityConstraintId,
) -> bool {
    if matches!(
        reachability,
        ScopedReachabilityConstraintId::ALWAYS_TRUE
            | ScopedReachabilityConstraintId::ALWAYS_FALSE
            | ScopedReachabilityConstraintId::AMBIGUOUS
    ) {
        return false;
    }

    let node = use_def_map(db, scope)
        .reachability_constraints()
        .get_interior_node(reachability);
    predicate_contains_special_cased_condition(db, scope, node.atom())
        || [node.if_true(), node.if_ambiguous(), node.if_false()]
            .into_iter()
            .any(|child| reachability_contains_special_cased_condition(db, scope, child))
}

/// Check whether a control-flow predicate depends on an environment guard, regardless of whether
/// the binding is reached when the predicate is true or when it is false.
///
/// Predicate expressions have their own inference queries, so checking a guard does not require
/// completing inference of the scope containing the definition. Cache the result because many
/// definitions can share the same guard, and recover as `false` if the guard refers to the definition.
#[salsa::tracked(
    returns(copy),
    cycle_initial = |_, _, _, _| false,
    heap_size = ruff_memory_usage::heap_size
)]
fn predicate_contains_special_cased_condition<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    predicate: ScopedPredicateId,
) -> bool {
    let expression = match use_def_map(db, scope).predicates()[predicate].node {
        PredicateNode::Expression(expression)
        | PredicateNode::Condition(expression)
        | PredicateNode::ChainedComparisonCondition(expression)
        | PredicateNode::IsNonEmptyIterable(expression) => expression,
        PredicateNode::Pattern(pattern) => pattern.subject(db),
        PredicateNode::SubjectElementPattern(element) => element.pattern.subject(db),
        // These predicates describe implicit control flow rather than a tested value. In
        // particular, a statement such as `print(sys.platform)` should not make every later
        // definition environment-dependent merely because it is reached after that call returns.
        PredicateNode::IsNonTerminalCall(_)
        | PredicateNode::ContextManagerSuppresses { .. }
        | PredicateNode::FinallyNormalPathImpossible { .. }
        | PredicateNode::OrPatternAlternative(_)
        | PredicateNode::StarImportPlaceholder(_) => return false,
    };
    let file = expression.program_file(db);
    let module = parsed_module(db, expression.python_file(db)).load(db);
    let mut inference = None;

    any_over_expr(expression.node_ref(db).node(&module), |subexpression| {
        is_special_cased_condition_expression(db, file, subexpression, |expr| {
            inference
                .get_or_insert_with(|| {
                    infer_expression_types(db, expression, TypeContext::default())
                })
                .expression_type(expr)
        })
    })
}
