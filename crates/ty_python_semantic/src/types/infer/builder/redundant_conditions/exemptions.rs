//! Redundant-condition exemptions for environment guards and deliberately unreachable branches.
//!
//! "Environment guards" select code for a particular Python version, platform, or type-checking
//! context. For example, `if sys.version_info >= (3, 14)`, `if sys.platform == "win32"`,
//! `if os.name == "posix"`, and `if TYPE_CHECKING` are all environment guards. Their outcomes
//! may be fixed for the configured environment while still serving a purpose in code that is
//! designed to support multiple environments.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, helpers::any_over_expr, name::Name};
use ty_module_resolver::{KnownModule, file_to_module};
use ty_python_core::{
    ProgramFile,
    definition::{Definition, DefinitionKind},
    scope::ScopeId,
    semantic_index,
};

use crate::{
    Db, Program, ProgramEnvironment,
    types::{
        KnownClass, Type, TypeContext,
        definition_resolution::{
            ImportAliasResolution, ResolvedDefinition, definitions_for_attribute,
            definitions_for_name,
        },
        infer::TypeInferenceBuilder,
        infer_definition_types, infer_expression_types,
    },
};

/// Which exits are accepted when checking the final statement of a suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SuiteExitKind {
    /// Accept any return statement, as well as defensive exits.
    Any,
    /// Only accept exits that can indicate a defensive runtime check.
    Defensive,
}

impl TypeInferenceBuilder<'_, '_> {
    /// Return `true` if `suite` ends in an exit recognized by the redundant-condition heuristic.
    ///
    /// Both kinds of exit accept the following final statements:
    /// - a `raise` statement
    /// - a potentially failing assertion
    /// - a call returning `Never`
    /// - or a nested `if` statement with an explicit `else` where every branch of the
    ///   `if`/`elif`/`else` ends in a recognized exit of the requested kind.
    ///
    /// [`SuiteExitKind::Any`] also accepts every `return` statement, while
    /// [`SuiteExitKind::Defensive`] only accepts `return NotImplemented`.
    ///
    /// Potentially failing assertions count as exits even when they might succeed. This heuristic
    /// prioritises avoiding false positives on intentional runtime checks.
    pub(super) fn suite_ends_with_exit(&self, suite: &[ast::Stmt], kind: SuiteExitKind) -> bool {
        let db = self.db();
        let env = self.program_environment();

        suite.last().is_some_and(|stmt| match stmt {
            ast::Stmt::Raise(_) => true,
            ast::Stmt::Assert(ast::StmtAssert { test, .. }) => {
                self.condition_truthiness(test).may_be_false()
            }
            ast::Stmt::Expr(ast::StmtExpr { value, .. }) if value.is_call_expr() => self
                .expression_type(value)
                .is_equivalent_to(db, env, Type::Never),
            ast::Stmt::Return(ast::StmtReturn { value, .. }) => match kind {
                SuiteExitKind::Any => true,
                SuiteExitKind::Defensive => value.as_ref().is_some_and(|expr| {
                    // Known limitation: `Any`, `Unknown`, and `Never` are also assignable to
                    // `NotImplementedType`, so an ordinary return *can* suppress a diagnostic here.
                    // We prioritise minimising false positives over minimising false negatives
                    // when recognizing potentially deliberate defensive checks.
                    self.expression_type(expr).is_assignable_to(
                        db,
                        env,
                        KnownClass::NotImplementedType.to_instance(db, env),
                    )
                }),
            },
            ast::Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                elif_else_clauses
                    .last()
                    .is_some_and(|last_clause| last_clause.test.is_none())
                    && self.suite_ends_with_exit(body, kind)
                    && elif_else_clauses
                        .iter()
                        .all(|clause| self.suite_ends_with_exit(&clause.body, kind))
            }
            _ => false,
        })
    }
}

/// Return `true` if any subexpression in `expression` is recognized as "tainted" by being defined
/// (directly or indirectly) with respect to `sys.version_info`, `sys.platform`, `os.name`, or
/// `typing.TYPE_CHECKING`.
///
/// Follow assignments and imports so aliases inherit the same exemption as the original guard.
pub(super) fn is_special_cased_condition_expression<'db>(
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

    // We don't recurse through definitions in a flow-sensitive way, but there isn't really any need to.
    // The main objective here is to avoid false positives. Flow-sensitive definitions of variables/attributes
    // where some paths define the place in terms of `sys.version_info` but other paths don't are pretty rare.
    // It's okay to have a small number of false negatives for these very rare edge cases. Attempting to
    // recurse through definitions in a flow-sensitive way would be significantly more complicated.
    condition_definition_info(db, file, expression, expression_type)
        .contains_special_cased_condition
}

/// Resolves the condition's source definitions using a scope or an already-inferred receiver type.
pub(super) fn condition_definition_info<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    expression: &ast::Expr,
    mut expression_type: impl FnMut(&ast::Expr) -> Type<'db>,
) -> ConditionDefinitionInfo<'db> {
    match expression {
        ast::Expr::Name(name) => {
            let index = semantic_index(db, file);
            let Some(scope) = index.try_expression_scope_id(&ast::ExprRef::Name(name)) else {
                return ConditionDefinitionInfo::default();
            };
            name_condition_definition_info(db, scope.to_scope_id(db, file), name.id.clone())
        }
        ast::Expr::Attribute(attribute) => attribute_condition_definition_info(
            db,
            file.program(db),
            expression_type(&attribute.value),
            attribute.attr.id.clone(),
        ),
        _ => ConditionDefinitionInfo::default(),
    }
}

/// The information needed for condition exemptions and annotation hints.
///
/// Retaining only the unique definition and the provenance result lets both uses share a lookup
/// without caching a potentially large list of bindings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ConditionDefinitionInfo<'db> {
    pub(super) single_definition: Option<Definition<'db>>,
    contains_special_cased_condition: bool,
}

impl<'db> ConditionDefinitionInfo<'db> {
    /// Summarizes resolved definitions, following assignments to establish environment provenance.
    fn from_definitions(db: &'db dyn Db, definitions: Vec<ResolvedDefinition<'db>>) -> Self {
        let single_definition = match definitions.as_slice() {
            [ResolvedDefinition::Definition(definition)] => Some(*definition),
            _ => None,
        };
        let contains_special_cased_condition = definitions
            .into_iter()
            .filter_map(|resolved| resolved.definition())
            .any(|definition| definition_contains_special_cased_condition(db, definition));
        Self {
            single_definition,
            contains_special_cased_condition,
        }
    }
}

/// Caches definition information across uses of the same name in a scope.
///
/// Name lookup considers every reachable binding, so repeating it for every condition can be
/// quadratic in the number of assignments. Caching only the per-definition traversal does not
/// avoid collecting and resolving those bindings again.
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
        definitions_for_name(db, scope, &name, ImportAliasResolution::ResolveAliases),
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
        definitions_for_attribute(
            db,
            &ProgramEnvironment::from_program(program),
            receiver,
            &name,
        ),
    )
}

/// Determines whether a definition originates from an environment-dependent guard.
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
        DefinitionKind::AugmentedAssignment(assignment) => Some(&*assignment.node(&module).value),
        DefinitionKind::For(for_statement) => Some(for_statement.iterable(&module)),
        DefinitionKind::Comprehension(comprehension) => Some(comprehension.iterable(&module)),
        DefinitionKind::WithItem(with_item) => Some(with_item.context_expr(&module)),
        DefinitionKind::MatchPattern(pattern) => {
            Some(pattern.predicate().subject(db).node_ref(db).node(&module))
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
        | DefinitionKind::TypeVarTuple(_)
        | DefinitionKind::LoopHeader(_)
        | DefinitionKind::NestedBindings(_) => None,
    };
    let Some(source_expression) = source_expression else {
        return false;
    };

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
