//! Parameters of implicit type aliases, including parameters used only in forward references.

use ruff_db::parsed::{parsed_module, parsed_string_annotation};
use ruff_db::source::source_text;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, visitor as ast_visitor};
use ty_python_core::ast_ids::HasScopedUseId;
use ty_python_core::definition::Definition;
use ty_python_core::scope::FileScopeId;
use ty_python_core::semantic_index;

use crate::types::{
    BoundTypeVarInstance, GenericContext, KnownInstanceType, SpecialFormType, Type, TypeVarKind,
    binding_type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

/// Collect the formal type parameters of an implicit or PEP 613 alias from its right-hand side.
///
/// Resolve legacy type-variable references, including those inside string annotations, and bind
/// them to `definition` in order of first appearance, without duplicates. For example,
/// `NestedDict = dict[str, "NestedDict[T]"]` binds `T` even though it only occurs in a forward
/// reference. Literal values and `Annotated` metadata do not contribute parameters.
///
/// This collects parameters before the alias's type has been inferred, so
/// [`infer_recursive_implicit_alias`](super::infer_recursive_implicit_alias) can seed a generic
/// recursive reference. References to the alias itself are skipped when resolving type variables.
/// Return `None` when no parameters are found; a query cycle also provisionally returns `None`.
/// This can seed a separate non-generic alias query while parameter discovery is incomplete.
/// Parameters are part of the alias query's key: discovering them later selects a generic
/// constructor rather than reusing that provisional non-generic value.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn implicit_alias_parameters<'db>(
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
