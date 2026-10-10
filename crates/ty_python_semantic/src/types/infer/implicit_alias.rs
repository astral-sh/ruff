//! Definitions and parameters of implicit type aliases.

use ruff_db::parsed::{parsed_module, parsed_string_annotation};
use ruff_db::source::source_text;
use ruff_python_ast::helpers::is_dotted_name;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, visitor as ast_visitor};
use ruff_text_size::Ranged;
use ty_python_core::definition::{Definition, DefinitionKind, DefinitionState};
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::FileScopeId;
use ty_python_core::{
    BindingWithConstraintsIterator, BoundnessAnalysis, global_scope, place_table, semantic_index,
    use_def_map,
};

use crate::place::{Place, class_body_implicit_symbol};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadMode, PlaceLoadResolutionStep, PlaceLoadSourceKind,
    resolve_place_load,
};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::definition_resolution::{ImportAliasResolution, resolve_definition};
use crate::types::{
    BoundTypeVarInstance, GenericContext, KnownInstanceType, SpecialFormType, Type, TypeVarKind,
    binding_type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

/// An eligible alias definition and its declaration style, independent of the defining AST.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ImplicitAliasDefinition<'db> {
    pub(super) definition: Definition<'db>,
    pub(super) is_explicit: bool,
}

/// Resolve an alias reference and check its definition without making the caller depend on the
/// defining file's AST. Ordinary annotated variables and string values are not implicit aliases.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn implicit_alias_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<ImplicitAliasDefinition<'db>> {
    let definition = resolve_imported_definition(db, definition)?;

    if !matches!(
        definition.kind(db),
        DefinitionKind::Assignment(_) | DefinitionKind::AnnotatedAssignment(_)
    ) {
        return None;
    }
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let value = definition.kind(db).value(&module)?;
    if !matches!(
        value,
        ast::Expr::Name(_)
            | ast::Expr::Attribute(_)
            | ast::Expr::Subscript(_)
            | ast::Expr::BinOp(_)
            | ast::Expr::StringLiteral(_)
    ) {
        return None;
    }
    let is_explicit = match definition.kind(db) {
        DefinitionKind::Assignment(assignment)
            if assignment.unpack().is_none() && !value.is_string_literal_expr() =>
        {
            false
        }
        DefinitionKind::AnnotatedAssignment(assignment)
            if crate::types::definition_expression_type(
                db,
                definition,
                assignment.annotation(&module),
            )
            .is_typealias_special_form() =>
        {
            true
        }
        _ => return None,
    };
    // An implicit forwarding alias shares its target's parameters and recursive identity.
    // Resolve the source before inferring its value, which could re-enter a recursive alias.
    // Explicit aliases instead interpret their right-hand side as a type expression,
    // applying the default specialization to bare generic references.
    if !is_explicit
        && let Some(target) = forwarding_alias_definition(db, definition, value)
        && target != definition
        && let Some(alias) = implicit_alias_definition(db, target)
    {
        return Some(alias);
    }
    // Assignments such as `List = list`, `UnionAlias = Union`, and `Renamed = T` preserve
    // the original class, special form, or type variable. A bare reference to a PEP 695
    // alias likewise preserves its type parameters.
    if !is_explicit
        && matches!(value, ast::Expr::Name(_) | ast::Expr::Attribute(_))
        && matches!(
            crate::types::definition_expression_type(db, definition, value),
            Type::ClassLiteral(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(
                    KnownInstanceType::TypeVar(_) | KnownInstanceType::TypeAliasType(_)
                )
        )
    {
        return None;
    }
    Some(ImplicitAliasDefinition {
        definition,
        is_explicit,
    })
}

/// Whether a failed alias interpretation refers to an ordinary computed value.
///
/// A class-valued assignment is not necessarily a type declaration: `(lambda: int)()`
/// produces the existing `int` class, whereas a recognized `NamedTuple` or `type` call
/// creates a class whose definition is the assignment itself. Unknown source definitions
/// remain eligible for the usual fallback, including unresolved and ambiguous imports.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _| false, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn is_non_alias_assignment<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    ty: Type<'db>,
) -> bool {
    let Some(definition) = resolve_imported_definition(db, definition) else {
        return false;
    };
    let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
        return false;
    };
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let value = assignment.value(&module);
    if matches!(value, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
        return forwarding_alias_definition(db, definition, value)
            .is_some_and(|target| target != definition && is_non_alias_assignment(db, target, ty));
    }
    if value.is_none_literal_expr() {
        return false;
    }
    // These forms can describe an alias even when one of its type arguments is invalid.
    // Type-expression inference diagnoses those arguments and preserves the outer type.
    if let ast::Expr::Subscript(subscript) = value
        && is_dotted_name(&subscript.value)
    {
        let base = crate::types::definition_expression_type(db, definition, &subscript.value);
        if matches!(
            base,
            Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(KnownInstanceType::TypeAliasType(_))
        ) && !forwarding_alias_definition(db, definition, &subscript.value)
            .is_some_and(|target| is_non_alias_assignment(db, target, base))
        {
            return false;
        }
    }

    let origin = match ty {
        Type::Union(union) => {
            return union.elements(db).iter().any(|element| {
                !element.is_none(db)
                    && !element.is_unknown()
                    && !element.is_divergent()
                    && is_non_alias_assignment(db, definition, *element)
            });
        }
        // Shared and nested factory calls are anchored to their expression rather than
        // a single assignment target, as in `A = B = TypedDict(...)`.
        Type::ClassLiteral(class) if class.definition(db).is_none() => {
            return class.program_file(db) != definition.program_file(db)
                || !value.range().contains_range(class.header_range(db));
        }
        Type::KnownInstance(KnownInstanceType::Sentinel(sentinel)) => Some(sentinel.definition(db)),
        _ => ty
            .definition(db, &ProgramEnvironment::from_definition(definition))
            .and_then(|origin| origin.definition()),
    };
    origin != Some(definition)
}

fn resolve_imported_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<Definition<'db>> {
    if !definition.kind(db).is_import() {
        return Some(definition);
    }
    let table = place_table(db, definition.scope(db));
    let symbol = table.symbol(definition.place(db).as_symbol()?);
    let definitions = resolve_definition(
        db,
        &ProgramEnvironment::from_definition(definition),
        definition,
        Some(symbol.name().as_str()),
        ImportAliasResolution::ResolveAliases,
    );
    let [resolved] = definitions.as_slice() else {
        return None;
    };
    resolved.definition()
}

/// Find a unique source for a forwarding assignment without inferring a plain name's value.
fn forwarding_alias_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    value: &ast::Expr,
) -> Option<Definition<'db>> {
    let name = match value {
        ast::Expr::Name(name) if name.ctx.is_load() => name,
        ast::Expr::Attribute(attribute) if attribute.ctx.is_load() => {
            let env = ProgramEnvironment::from_definition(definition);
            let receiver =
                crate::types::definition_expression_type(db, definition, &attribute.value);
            let member = receiver.member(db, &env, &attribute.attr);
            return match member.place {
                Place::Defined(place) if member.place.is_definitely_bound() => {
                    place.provenance.definition()
                }
                _ => None,
            };
        }
        _ => return None,
    };
    let index = semantic_index(db, definition.program_file(db));
    let mode = if definition.file(db).is_stub(db) {
        PlaceLoadMode::Deferred
    } else {
        PlaceLoadMode::AtExpression(name.into())
    };
    let resolution = resolve_place_load(
        db,
        index,
        index
            .expression_scope_id(value)
            .to_scope_id(db, definition.program_file(db)),
        PlaceExpr::from_expr_name(name),
        mode,
    );
    for step in resolution {
        let PlaceLoadResolutionStep::Source(source) = step else {
            return None;
        };
        let (bindings, owning_place) = match source.kind {
            PlaceLoadSourceKind::Bindings(bindings) => (bindings, None),
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => (
                use_def_map(db, scope).reachable_bindings(id),
                Some((scope, id)),
            ),
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file,
                name,
            }) => {
                let scope = global_scope(db, file);
                let id = place_table(db, scope).symbol_id(&name)?;
                (
                    use_def_map(db, scope).reachable_symbol_bindings(id),
                    Some((scope, id.into())),
                )
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ClassBodySymbol(name)) => {
                if class_body_implicit_symbol(
                    db,
                    &ProgramEnvironment::from_definition(definition),
                    &name,
                )
                .place
                .is_definitely_bound()
                {
                    return None;
                }
                continue;
            }
            PlaceLoadSourceKind::Implicit(_) => return None,
        };
        let Some(target) = unique_forwarding_target(db, bindings).ok()? else {
            continue;
        };
        let target_use_def = use_def_map(db, target.scope(db));
        let mut declarations = if let Some((scope, id)) = owning_place {
            use_def_map(db, scope).reachable_declarations(id)
        } else {
            target_use_def.declarations_at_binding(target)
        };
        // A separate annotation can determine the loaded type instead of the assignment value.
        if declarations.any(|declaration| {
            declaration
                .declaration
                .definition()
                .is_some_and(|declaration| declaration != target)
        }) {
            return None;
        }
        return Some(target);
    }
    None
}

/// Return no target for an unbound source, and reject ambiguous or possibly unbound sources.
fn unique_forwarding_target<'db>(
    db: &'db dyn Db,
    mut bindings: BindingWithConstraintsIterator<'_, 'db>,
) -> Result<Option<Definition<'db>>, ()> {
    let assume_bound = bindings.boundness_analysis() == BoundnessAnalysis::AssumeBound;
    let mut target = None;
    let mut unbound = false;
    while let Some(binding) = bindings.next() {
        if bindings
            .reachability_constraints()
            .evaluate(db, bindings.predicates(), binding.reachability_constraint)
            .is_always_false()
        {
            continue;
        }
        match binding.binding {
            DefinitionState::Defined(definition) => {
                if target.is_some_and(|target| target != definition) {
                    return Err(());
                }
                target = Some(definition);
            }
            DefinitionState::Undefined => unbound = true,
            DefinitionState::Deleted => return Err(()),
        }
    }
    if target.is_some() && unbound && !assume_bound {
        return Err(());
    }
    Ok(target)
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
                    Vec::from_iter(forwarding_alias_definition(
                        db,
                        self.alias_definition,
                        expression,
                    ))
                };
                let [definition] = definitions.as_slice() else {
                    return None;
                };
                // Another alias's parameters are not free parameters of this alias. Inferring
                // its runtime value here can also re-enter parameter discovery before recursive
                // aliases have stable generic contexts.
                (*definition != self.alias_definition
                    && implicit_alias_definition(db, *definition).is_none())
                .then(|| binding_type(db, *definition))
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
