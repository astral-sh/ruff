//! Recursion and parameters of implicit type aliases.

use ruff_db::parsed::{parsed_module, parsed_string_annotation};
use ruff_db::source::source_text;
use ruff_python_ast::name::Name;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, visitor as ast_visitor};
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::semantic_index;

use crate::types::definition_resolution::{ImportAliasResolution, definitions_for_name};
use crate::types::{
    BoundTypeVarInstance, GenericContext, KnownInstanceType, SpecialFormType, Type, TypeVarKind,
    binding_type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

/// Conservatively prove that an implicit alias has no recursive dependencies.
///
/// A resolved union can have lost a recursive member during value inference, so inspect the
/// definitions it references rather than treating its recovered value as proof. Ordinary alias
/// chains can then reuse their inferred values without a separate type-expression inference pass.
/// String annotations and references that cannot be resolved unambiguously retain that full pass.
///
/// Read binding types inside this query so `cycle_result` rejects proofs that depend on provisional
/// inference results. A `false` result means the proof is incomplete, not necessarily that the
/// alias is recursive.
#[salsa::tracked(
    returns(copy),
    cycle_result=|_, _, _| false,
    heap_size=ruff_memory_usage::heap_size,
)]
pub(in crate::types) fn implicit_alias_is_acyclic<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> bool {
    let file = definition.program_file(db);
    let parsed = parsed_module(db, file.python_file(db)).load(db);
    if !definition
        .kind(db)
        .category(definition.file(db).is_stub(db), &parsed)
        .is_binding()
    {
        return false;
    }
    match binding_type(db, definition) {
        Type::ClassLiteral(_)
        | Type::KnownInstance(KnownInstanceType::TypeVar(_))
        | Type::SpecialForm(_) => return true,
        Type::Dynamic(_) | Type::Divergent(_) | Type::Recursive(_) | Type::TypeAlias(_) => {
            return false;
        }
        _ => {}
    }

    // An eager class-body read can fall back to a global before a later class assignment.
    // Looking up all definitions in that class would follow the later assignment instead.
    if definition.scope(db).scope(db).kind().is_class() {
        return false;
    }

    let Some(value) = definition.kind(db).value(&parsed) else {
        return false;
    };
    ImplicitAliasAcyclicityProof { db, definition }.expression_is_acyclic(value)
}

/// Resolve a dependency once per name and scope, even when many aliases reference it.
/// In particular, an ambiguous name must not enumerate all its assignments for every alias.
#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
#[allow(clippy::needless_pass_by_value, reason = "Salsa owns the query key")]
fn unambiguous_alias_dependency<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
    name: Name,
) -> Option<Definition<'db>> {
    let definitions = definitions_for_name(db, scope, &name, ImportAliasResolution::ResolveAliases);
    let [resolved] = definitions.as_slice() else {
        return None;
    };
    resolved.definition()
}

struct ImplicitAliasAcyclicityProof<'db> {
    db: &'db dyn Db,
    definition: Definition<'db>,
}

impl ImplicitAliasAcyclicityProof<'_> {
    fn expression_is_acyclic(&self, expression: &ast::Expr) -> bool {
        match expression {
            ast::Expr::Name(name) => {
                let db = self.db;
                let index = semantic_index(db, self.definition.program_file(db));
                // A forwarding scope can assign a different value from the outer definition
                // returned by source-name lookup, including when read from a nested function.
                for (scope, _) in index.visible_ancestor_scopes(self.definition.file_scope(db)) {
                    let table = index.place_table(scope);
                    let Some(symbol) = table.symbol_id(&name.id) else {
                        continue;
                    };
                    let symbol = table.symbol(symbol);
                    if symbol.is_global() || symbol.is_nonlocal() {
                        return false;
                    }
                    if symbol.is_bound() || symbol.is_declared() {
                        break;
                    }
                }
                unambiguous_alias_dependency(db, self.definition.scope(db), name.id.clone())
                    .is_some_and(|definition| implicit_alias_is_acyclic(db, definition))
            }
            ast::Expr::BinOp(binary) if binary.op == ast::Operator::BitOr => {
                self.expression_is_acyclic(&binary.left)
                    && self.expression_is_acyclic(&binary.right)
            }
            ast::Expr::Subscript(subscript) => {
                self.expression_is_acyclic(&subscript.value)
                    && self.expression_is_acyclic(&subscript.slice)
            }
            ast::Expr::Tuple(tuple) => tuple
                .elts
                .iter()
                .all(|element| self.expression_is_acyclic(element)),
            ast::Expr::NoneLiteral(_)
            | ast::Expr::NumberLiteral(_)
            | ast::Expr::BooleanLiteral(_)
            | ast::Expr::EllipsisLiteral(_) => true,
            _ => false,
        }
    }
}

/// Collect the formal type parameters of an implicit or PEP 613 alias from its right-hand side.
///
/// Resolve legacy type-variable references, including those inside string annotations, and bind
/// them to `definition` in order of first appearance, without duplicates. For example,
/// `NestedDict = dict[str, "NestedDict[T]"]` binds `T` even though it only occurs in a forward
/// reference. Literal values and `Annotated` metadata do not contribute parameters.
///
/// This collects parameters before the alias's type has been inferred, so
/// [`infer_implicit_alias_type`](super::infer_implicit_alias_type) can seed a generic
/// recursive reference. References to the alias itself are skipped when resolving type variables.
/// Return `None` when no parameters are found; a query cycle also provisionally returns `None`.
/// This can seed a separate non-generic alias query while parameter discovery is incomplete.
/// Parameters are part of the alias query's key: discovering them later selects a generic
/// constructor rather than reusing that provisional non-generic value.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(in crate::types) fn implicit_alias_parameters<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<GenericContext<'db>> {
    let file = definition.program_file(db);
    let parsed = parsed_module(db, file.python_file(db)).load(db);
    let value = definition.kind(db).value(&parsed)?;
    let mut collector = ImplicitAliasLegacyTypeVarCollector {
        db,
        alias_definition: definition,
        index: semantic_index(db, file),
        string_annotation_scope: None,
        variables: FxOrderSet::default(),
    };
    collector.visit_expr(value);
    (!collector.variables.is_empty()).then(|| {
        GenericContext::from_typevar_instances(
            db,
            &ProgramEnvironment::from_file(file),
            collector.variables,
        )
    })
}

struct ImplicitAliasLegacyTypeVarCollector<'a, 'db> {
    db: &'db dyn Db,
    alias_definition: Definition<'db>,
    index: &'a ty_python_core::SemanticIndex<'db>,
    string_annotation_scope: Option<FileScopeId>,
    variables: FxOrderSet<BoundTypeVarInstance<'db>>,
}

impl<'db> ImplicitAliasLegacyTypeVarCollector<'_, 'db> {
    fn expression_type(&self, expression: &ast::Expr) -> Option<Type<'db>> {
        let db = self.db;
        let file = self.alias_definition.program_file(db);
        match expression {
            ast::Expr::Name(name) if name.ctx.is_load() => {
                let definitions: Vec<_> = if let Some(scope) = self.string_annotation_scope {
                    let (scope, symbol) =
                        self.index
                            .visible_ancestor_scopes(scope)
                            .find_map(|(scope, _)| {
                                self.index
                                    .place_table(scope)
                                    .symbol_id(&name.id)
                                    .map(|symbol| (scope, symbol))
                            })?;
                    self.index
                        .use_def_map(scope)
                        .end_of_scope_symbol_bindings(symbol)
                        .filter_map(|binding| binding.binding.definition())
                        .collect()
                } else {
                    self.index
                        .use_def_map(self.index.expression_scope_id(expression))
                        .bindings_at_use(name.scoped_use_id(db, file))
                        .filter_map(|binding| binding.binding.definition())
                        .collect()
                };
                let [definition] = definitions.as_slice() else {
                    return None;
                };
                (*definition != self.alias_definition).then(|| binding_type(db, *definition))
            }
            ast::Expr::Attribute(attribute) => self
                .expression_type(&attribute.value)?
                .member(db, &ProgramEnvironment::from_file(file), &attribute.attr)
                .ignore_possibly_undefined(),
            _ => None,
        }
    }

    fn collect_typevar(&mut self, expression: &ast::Expr) {
        let Some(Type::KnownInstance(KnownInstanceType::TypeVar(typevar))) =
            self.expression_type(expression)
        else {
            return;
        };
        // References to aliases do not declare new type parameters.
        if matches!(
            typevar.kind(self.db),
            TypeVarKind::LegacyTypeVar
                | TypeVarKind::LegacyParamSpec
                | TypeVarKind::LegacyTypeVarTuple
        ) {
            self.variables
                .insert(typevar.with_binding_context(self.db, self.alias_definition));
        }
    }
}

impl<'ast> Visitor<'ast> for ImplicitAliasLegacyTypeVarCollector<'_, '_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        if let ast::Expr::StringLiteral(string) = expr
            && let Some(string_literal) = string.as_single_part_string()
        {
            let file = self.alias_definition.program_file(self.db);
            let source = source_text(self.db, file.python_file(self.db).file(self.db));
            if let Ok(parsed) = parsed_string_annotation(source.as_str(), string_literal) {
                let string_scope = self
                    .string_annotation_scope
                    .unwrap_or_else(|| self.index.expression_scope_id(expr));
                let previous_scope = self.string_annotation_scope.replace(string_scope);
                self.visit_expr(parsed.expr());
                self.string_annotation_scope = previous_scope;
                return;
            }
        }

        match expr {
            ast::Expr::Subscript(subscript) => match self.expression_type(&subscript.value) {
                // Literal values and Annotated metadata do not bind type parameters.
                Some(Type::SpecialForm(SpecialFormType::Literal)) => return,
                Some(Type::SpecialForm(SpecialFormType::Annotated)) => {
                    if let ast::Expr::Tuple(arguments) = subscript.slice.as_ref()
                        && let Some(annotation) = arguments.elts.first()
                    {
                        self.visit_expr(annotation);
                    }
                    return;
                }
                _ => {}
            },
            ast::Expr::Name(_) | ast::Expr::Attribute(_) => self.collect_typevar(expr),
            _ => {}
        }

        ast_visitor::walk_expr(self, expr);
    }
}
