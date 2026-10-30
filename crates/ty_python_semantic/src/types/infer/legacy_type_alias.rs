//! Definitions and type parameters of legacy type aliases. These include implicit aliases
//! (`Alias = int`) and PEP 613 aliases (`Alias: TypeAlias = int`).

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
    BindingWithConstraintsIterator, BoundnessAnalysis, ProgramFile, global_scope, place_table,
    semantic_index, use_def_map,
};

use crate::place::{Place, class_body_implicit_symbol};
use crate::place_load::{
    ImplicitPlaceLoad, PlaceLoadMode, PlaceLoadResolutionProcedure, PlaceLoadResolutionStep,
    PlaceLoadSourceKind,
};
use crate::reachability::ReachabilityConstraintsExtension;
use crate::types::definition_resolution::{ImportAliasResolution, resolve_definition};
use crate::types::{
    BoundTypeVarInstance, GenericContext, KnownInstanceType, SpecialFormType, Type, TypeVarKind,
    binding_type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

use super::builder::TypeInferenceBuilder;

/// An eligible alias definition and its declaration style, independent of the defining AST.
#[derive(Debug, Clone, Copy, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct LegacyTypeAliasDefinition<'db> {
    pub(super) definition: Definition<'db>,
    /// True for an implicit alias and false for a PEP 613 declaration. If the right-hand side
    /// is invalid, we may treat an unannotated assignment as a variable. A PEP 613 declaration
    /// still declares an alias.
    ///
    /// This describes the resolved `definition`. For `B = A`, the flag is false when `A` is declared
    /// with `TypeAlias`. PEP 695 `type` statements are handled separately.
    pub(super) is_implicit: bool,
}

/// Resolve a definition as a possible legacy type alias.
///
/// Resolve imports and implicit forwarding assignments to the underlying alias definition.
/// Return `None` in these cases:
///
/// - An import cannot be resolved to a single definition.
/// - The definition is not an eligible assignment: for example, a class definition, an
///   unpacking assignment, an ordinary annotated variable, an unannotated assignment of a
///   string literal, or an assignment whose right-hand side is a call.
/// - An implicit name or attribute assignment forwards a class, special form, type variable,
///   PEP 695 alias, or module. References such as `List = list` use their existing value type
///   instead of re-inferring the assignment's right-hand side as an alias.
///
/// Thus, `None` does not establish that a reference is invalid. Conversely, `Some` identifies
/// a candidate whose right-hand side still needs to be validated by type-expression inference.
///
/// This query keeps callers independent of the defining file's AST.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn legacy_type_alias_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<LegacyTypeAliasDefinition<'db>> {
    let definition = resolve_imported_definition(db, definition)?;

    let module;
    let (value, annotation) = match definition.kind(db) {
        DefinitionKind::Assignment(assignment) if assignment.unpack().is_none() => {
            module = parsed_module(db, definition.python_file(db)).load(db);
            let value = assignment.value(&module);
            // The parser represents a missing right-hand side as an invalid name.
            if value.is_string_literal_expr()
                || value.as_name_expr().is_some_and(ast::ExprName::is_invalid)
            {
                return None;
            }
            (value, None)
        }
        DefinitionKind::AnnotatedAssignment(assignment) => {
            module = parsed_module(db, definition.python_file(db)).load(db);
            (
                assignment.value(&module)?,
                Some(assignment.annotation(&module)),
            )
        }
        _ => return None,
    };
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
    if let Some(annotation) = annotation {
        return crate::types::definition_expression_type(db, definition, annotation)
            .is_typealias_special_form()
            .then_some(LegacyTypeAliasDefinition {
                definition,
                is_implicit: false,
            });
    }
    if matches!(value, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
        // For `Items = list[T]; Forwarded = Items`, reuse Items's definition, including
        // its parameters and recursive identity. Do this before inspecting its value:
        // `A: TypeAlias = list; B = A` must retain A's default specialization even though
        // A's runtime value is the generic `list` class. PEP 613 declarations interpret
        // their own right-hand side as a normal type expression.
        let resolve_alias = |target| {
            if target == definition {
                return None;
            }
            legacy_type_alias_definition(db, target)
        };

        // Resolve straightforward forwarding without inferring the runtime value, which
        // could re-enter a recursive alias. If this does not identify an alias, normal
        // reference inference accounts for possibly-unbound sources, fallback scopes,
        // and assignments that narrow an attribute's declared type.
        if let Some(alias) =
            forwarding_alias_definition(db, definition, value).and_then(resolve_alias)
        {
            return Some(alias);
        }
        let env = ProgramEnvironment::from_definition(definition);
        let (value_ty, target) =
            TypeInferenceBuilder::for_definition(db, &env, definition, &module)
                .finish_implicit_alias_reference(definition, value);
        if let Some(alias) = target.and_then(resolve_alias) {
            return Some(alias);
        }

        // For `List = list`, `UnionAlias = Union`, or `Renamed = T`, the value already
        // carries the identity and generic information needed by ordinary type-expression
        // handling. The same is true of PEP 695 `TypeAliasType` values. Return `None` to
        // use that handling; this does not mean the reference is invalid. Module aliases
        // also remain ordinary values so parameter discovery can resolve their attributes.
        if matches!(
            value_ty,
            Type::ClassLiteral(_)
                | Type::ModuleLiteral(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(
                    KnownInstanceType::TypeVar(_) | KnownInstanceType::TypeAliasType(_)
                )
        ) {
            return None;
        }
    }
    Some(LegacyTypeAliasDefinition {
        definition,
        is_implicit: true,
    })
}

/// Find the original assignment for a computed value that cannot be used as a type.
///
/// Callers use this to reject references that value-to-type conversion would otherwise accept,
/// recovering with `Unknown` even when `invalid-type-form` is disabled. The returned definition
/// also lets diagnostics explain the invalid right-hand side at its original source location.
///
/// A class-valued assignment is not necessarily a type declaration: `(lambda: int)()`
/// produces the existing `int` class, whereas a recognized `NamedTuple` or `type` call
/// creates a class whose definition is the assignment itself. Unknown source definitions
/// remain eligible for the usual fallback, including unresolved and ambiguous imports.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(super) fn non_alias_assignment<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    ty: Type<'db>,
) -> Option<Definition<'db>> {
    let definition = resolve_imported_definition(db, definition)?;
    let DefinitionKind::Assignment(assignment) = definition.kind(db) else {
        return None;
    };
    let module = parsed_module(db, definition.python_file(db)).load(db);
    let value = assignment.value(&module);
    if matches!(value, ast::Expr::Name(_) | ast::Expr::Attribute(_)) {
        return forwarding_alias_definition(db, definition, value)
            .filter(|target| *target != definition)
            .and_then(|target| non_alias_assignment(db, target, ty));
    }
    if value.is_none_literal_expr() {
        return None;
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
        ) && forwarding_alias_definition(db, definition, &subscript.value)
            .is_none_or(|target| non_alias_assignment(db, target, base).is_none())
        {
            return None;
        }
    }

    let origin = match ty {
        Type::Union(union) => {
            return union
                .elements(db)
                .iter()
                .filter(|element| {
                    !element.is_none(db) && !element.is_unknown() && !element.is_divergent()
                })
                .find_map(|element| non_alias_assignment(db, definition, *element));
        }
        // Shared and nested factory calls are anchored to their expression rather than
        // a single assignment target, as in `A = B = TypedDict(...)`.
        Type::ClassLiteral(class) if class.definition(db).is_none() => {
            return (class.program_file(db) != definition.program_file(db)
                || !value.range().contains_range(class.header_range(db)))
            .then_some(definition);
        }
        Type::KnownInstance(KnownInstanceType::Sentinel(sentinel)) => Some(sentinel.definition(db)),
        _ => ty
            .definition(db, &ProgramEnvironment::from_definition(definition))
            .and_then(|origin| origin.definition()),
    };
    (origin != Some(definition)).then_some(definition)
}

/// Resolve an import to a single source definition, leaving non-import definitions unchanged.
///
/// Use the shared [`resolve_definition`] resolver to follow import aliases and re-exports,
/// accepting only a single `Definition` result. Multiple targets and module or file-range
/// targets return `None`. If resolution fails, the shared resolver falls back to the original
/// import definition; callers still need to check that the result is an assignment.
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

/// Find the definition referenced by `A` in a forwarding assignment such as `B = A` or
/// `B = module.A`.
///
/// For `B = A`, use the semantic index's bindings and declarations to identify A's source
/// definition. Do not request A's inferred value type: that could re-enter inference of a
/// recursive alias. Callers can then reuse the source alias's parameters and recursive identity.
/// For `B = module.A`, infer the receiver's type and obtain the definition from member lookup.
///
/// Return `None` when lookup cannot identify a unique usable definition, for example when
/// bindings are ambiguous or a separate annotation determines the name's type.
fn forwarding_alias_definition<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    value: &ast::Expr,
) -> Option<Definition<'db>> {
    match value {
        ast::Expr::Name(name) if name.ctx.is_load() => {
            referenced_name_definition(db, definition.program_file(db), name, None)
        }
        ast::Expr::Attribute(attribute) if attribute.ctx.is_load() => {
            let env = ProgramEnvironment::from_definition(definition);
            let receiver =
                crate::types::definition_expression_type(db, definition, &attribute.value);
            match receiver.member(db, &env, &attribute.attr).place {
                Place::Defined(place) if place.is_definitely_defined() => {
                    place.provenance.definition()
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Resolve a name to its unique reachable definition. Names parsed from a string annotation
/// use the enclosing string's scope because they have no use site in the semantic index.
fn referenced_name_definition<'db>(
    db: &'db dyn Db,
    file: ProgramFile<'db>,
    name: &ast::ExprName,
    string_annotation_scope: Option<FileScopeId>,
) -> Option<Definition<'db>> {
    let index = semantic_index(db, file);
    let (scope, mode) = if let Some(scope) = string_annotation_scope {
        (scope, PlaceLoadMode::StringAnnotation)
    } else {
        let expression = ast::ExprRef::Name(name);
        let mode = if file.file(db).is_stub(db) {
            PlaceLoadMode::Deferred
        } else {
            PlaceLoadMode::AtExpression(expression)
        };
        (index.expression_scope_id(&expression), mode)
    };
    let place_expr = PlaceExpr::from_expr_name(name);
    let mut procedure = PlaceLoadResolutionProcedure::new(
        db,
        index,
        scope.to_scope_id(db, file),
        &place_expr,
        mode,
    );
    let from_owning_scope = |scope, id| {
        let use_def = use_def_map(db, scope);
        (
            use_def.reachable_bindings(id),
            Some(use_def.reachable_declarations(id)),
        )
    };
    let env = ProgramEnvironment::from_file(file);
    while let Some(PlaceLoadResolutionStep::Source(source)) = procedure.next() {
        let (bindings, declarations) = match source.kind {
            PlaceLoadSourceKind::Bindings(bindings) => (bindings, None),
            PlaceLoadSourceKind::DefinitionsFromOwningScope { scope, id } => {
                from_owning_scope(scope, id)
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ExplicitGlobalSymbol {
                file,
                name,
            }) => {
                let scope = global_scope(db, file);
                let id = place_table(db, scope).symbol_id(&name)?;
                from_owning_scope(scope, id.into())
            }
            PlaceLoadSourceKind::Implicit(ImplicitPlaceLoad::ClassBodySymbol(name))
                if !class_body_implicit_symbol(db, &env, &name)
                    .place
                    .is_definitely_bound() =>
            {
                continue;
            }
            PlaceLoadSourceKind::Implicit(_) => return None,
        };
        let Some(target) = unique_forwarding_target(db, bindings).ok()? else {
            continue;
        };
        let declarations = declarations
            .unwrap_or_else(|| use_def_map(db, target.scope(db)).declarations_at_binding(target));
        // A separate annotation can determine the loaded type instead of the assignment value.
        return declarations
            .filter_map(|declaration| declaration.declaration.definition())
            .all(|declaration| declaration == target)
            .then_some(target);
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

/// Collect the formal type parameters of a legacy type alias from its right-hand side.
///
/// Resolve legacy type-variable references, including those inside string annotations, and bind
/// them to `definition` in order of first appearance, without duplicates. For example,
/// `NestedDict = dict[str, "NestedDict[T]"]` binds `T` even though it only occurs in a forward
/// reference. Literal values and `Annotated` metadata do not contribute parameters.
///
/// This collects parameters before the alias's type has been inferred, so
/// [`infer_legacy_type_alias`](super::infer_legacy_type_alias) can seed a generic
/// recursive reference. References to the alias itself are skipped when resolving type variables.
/// Return `None` when no parameters are found; a query cycle also provisionally returns `None`.
/// This can seed a separate non-generic alias query while parameter discovery is incomplete.
/// Parameters are part of the alias query's key: discovering them later selects a generic
/// constructor rather than reusing that provisional non-generic value.
#[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
pub(in crate::types) fn legacy_type_alias_parameters<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> Option<GenericContext<'db>> {
    let file = definition.program_file(db);
    let parsed = parsed_module(db, file.python_file(db)).load(db);
    let value = definition.kind(db).value(&parsed)?;
    let mut collector = LegacyTypeVarCollector {
        db,
        alias_definition: definition,
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

struct LegacyTypeVarCollector<'db> {
    db: &'db dyn Db,
    alias_definition: Definition<'db>,
    string_annotation_scope: Option<FileScopeId>,
    variables: FxOrderSet<BoundTypeVarInstance<'db>>,
}

impl<'db> LegacyTypeVarCollector<'db> {
    fn expression_type(&self, expression: &ast::Expr) -> Option<Type<'db>> {
        let db = self.db;
        let file = self.alias_definition.program_file(db);
        match expression {
            ast::Expr::Name(name) if name.ctx.is_load() => {
                let definition =
                    referenced_name_definition(db, file, name, self.string_annotation_scope)?;
                // Another alias's parameters are not free parameters of this alias. Inferring
                // its runtime value here can also re-enter parameter discovery before recursive
                // aliases have stable generic contexts.
                (definition != self.alias_definition
                    && legacy_type_alias_definition(db, definition).is_none())
                .then(|| binding_type(db, definition))
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

impl<'ast> Visitor<'ast> for LegacyTypeVarCollector<'_> {
    fn visit_expr(&mut self, expr: &'ast ast::Expr) {
        if let ast::Expr::StringLiteral(string) = expr
            && let Some(string_literal) = string.as_single_part_string()
        {
            let file = self.alias_definition.program_file(self.db);
            let source = source_text(self.db, file.python_file(self.db).file(self.db));
            if let Ok(parsed) = parsed_string_annotation(source.as_str(), string_literal) {
                let string_scope = self
                    .string_annotation_scope
                    .unwrap_or_else(|| semantic_index(self.db, file).expression_scope_id(expr));
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
