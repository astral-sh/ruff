use std::collections::HashMap;

use crate::FxIndexSet;
use crate::place::builtins_module_scope;
use crate::reachability::is_range_reachable;
use crate::types::call::{CallArguments, CallError, MatchedArgument};
use crate::types::class::{DynamicClassAnchor, DynamicEnumAnchor, DynamicNamedTupleAnchor};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::signatures::{ParameterForm, ParametersKind, Signature};
use crate::types::{
    CallDunderError, CallableTypes, ClassBase, ClassLiteral, ClassType, KnownClass, KnownUnion,
    Type, TypeContext, UnionType,
};
use crate::{Db, DisplaySettings, HasDefinition, HasType, SemanticModel};
use itertools::Either;
use ruff_db::files::FileRange;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_python_ast::{self as ast, AnyNodeRef, name::Name};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::{attribute_scopes, global_scope, semantic_index, use_def_map};

mod unreachable_code;
#[path = "ide_support/unused_bindings.rs"]
mod unused_binding_support;

pub use resolve_definition::{ImportAliasResolution, ResolvedDefinition, map_stub_definition};
use resolve_definition::{find_symbol_in_scope, resolve_definition};
pub use unreachable_code::{UnreachableKind, UnreachableRange, unreachable_ranges};
pub use unused_binding_support::{UnusedBinding, unused_bindings};

/// Get the primary definition kind for a name expression within a specific file.
/// Returns the first definition kind that is reachable for this name in its scope.
/// This is useful for IDE features like semantic tokens.
pub fn definition_for_name<'db>(
    model: &SemanticModel<'db>,
    name: &ast::ExprName,
    alias_resolution: ImportAliasResolution,
) -> Option<Definition<'db>> {
    let definitions = definitions_for_name(model, name.id.as_str(), name.into(), alias_resolution);

    // Find the first valid definition and return its kind
    for declaration in definitions {
        if let Some(def) = declaration.definition() {
            return Some(def);
        }
    }

    None
}

/// Returns all definitions for a name. If any definitions are imports, they
/// are resolved (recursively) to the original definitions or module files.
pub fn definitions_for_name<'db>(
    model: &SemanticModel<'db>,
    name_str: &str,
    node: AnyNodeRef<'_>,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    let db = model.db();
    let file = model.file();
    let index = semantic_index(db, file);

    // Get the scope for this name expression
    let Some(file_scope) = model.scope(node) else {
        return vec![];
    };

    let mut all_definitions = FxIndexSet::default();

    // Search through the scope hierarchy: start from the current scope and
    // traverse up through parent scopes to find definitions
    for (scope_id, _scope) in index.visible_ancestor_scopes(file_scope) {
        let place_table = index.place_table(scope_id);

        let Some(symbol_id) = place_table.symbol_id(name_str) else {
            continue; // Name not found in this scope, try parent scope
        };

        // Check if this place is marked as global or nonlocal
        let place_expr = place_table.symbol(symbol_id);
        let is_global = place_expr.is_global();
        let is_nonlocal = place_expr.is_nonlocal();

        // TODO: The current algorithm doesn't return definitions or bindings
        // for other scopes that are outside of this scope hierarchy that target
        // this name using a nonlocal or global binding. The semantic analyzer
        // doesn't appear to track these in a way that we can easily access
        // them from here without walking all scopes in the module.

        // If marked as global, skip to global scope
        if is_global {
            let global_scope_id = global_scope(db, file);
            let global_place_table = ty_python_core::place_table(db, global_scope_id);

            if let Some(global_symbol_id) = global_place_table.symbol_id(name_str) {
                let global_use_def_map = ty_python_core::use_def_map(db, global_scope_id);
                all_definitions.extend(reachable_definitions(
                    db,
                    global_use_def_map
                        .reachable_symbol_bindings(global_symbol_id)
                        .filter_map(|binding| binding.binding.definition())
                        .chain(
                            global_use_def_map
                                .reachable_symbol_declarations(global_symbol_id)
                                .filter_map(|declaration| declaration.declaration.definition()),
                        ),
                ));
            }
            break;
        }

        // If marked as nonlocal, skip current scope and search in ancestor scopes
        if is_nonlocal {
            // Continue searching in parent scopes, but skip the current scope
            continue;
        }

        let use_def_map = index.use_def_map(scope_id);

        // Get all definitions (both bindings and declarations) for this place
        all_definitions.extend(reachable_definitions(
            db,
            use_def_map
                .reachable_symbol_bindings(symbol_id)
                .filter_map(|binding| binding.binding.definition())
                .chain(
                    use_def_map
                        .reachable_symbol_declarations(symbol_id)
                        .filter_map(|declaration| declaration.declaration.definition()),
                ),
        ));

        // If we found definitions in this scope, we can stop searching
        if !all_definitions.is_empty() {
            break;
        }
    }

    // Resolve import definitions to their targets
    let mut resolved_definitions = Vec::new();

    for definition in &all_definitions {
        let resolved = resolve_definition(db, *definition, Some(name_str), alias_resolution);
        resolved_definitions.extend(resolved);
    }

    // If we didn't find any definitions in scopes, fallback to builtins
    if resolved_definitions.is_empty()
        && let Some(builtins_scope) = builtins_module_scope(db)
    {
        // Special cases for `float` and `complex` in type annotation positions.
        // We don't know whether we're in a type annotation position, so we'll just ask `Name`'s type,
        // which resolves to `int | float` or `int | float | complex` if `float` or `complex` is used in
        // a type annotation position and `float` or `complex` otherwise.
        //
        // https://typing.python.org/en/latest/spec/special-types.html#special-cases-for-float-and-complex
        if matches!(name_str, "float" | "complex")
            && let Some(expr) = node.expr_name()
            && let Some(ty) = expr.inferred_type(model)
            && let Some(union) = ty.as_union()
            && is_float_or_complex_annotation(db, union, name_str)
        {
            return union
                .elements(db)
                .iter()
                // Use `rev` so that `complex` and `float` come first.
                // This is required for hover to pick up the docstring of `complex` and `float`
                // instead of `int` (hover only shows the docstring of the first definition).
                .rev()
                .filter_map(|ty| ty.as_nominal_instance())
                .filter_map(|instance| {
                    let definition = instance.class_literal(db).definition(db)?;
                    Some(ResolvedDefinition::Definition(definition))
                })
                .collect();
        }

        find_symbol_in_scope(db, builtins_scope, name_str)
            .into_iter()
            .filter(|def| def.is_reexported(db))
            .flat_map(|def| {
                resolve_definition(
                    db,
                    def,
                    Some(name_str),
                    ImportAliasResolution::ResolveAliases,
                )
            })
            .collect()
    } else {
        resolved_definitions
    }
}

fn is_float_or_complex_annotation(db: &dyn Db, ty: UnionType, name: &str) -> bool {
    let float_or_complex_ty = match name {
        "float" => KnownUnion::Float.to_type(db),
        "complex" => KnownUnion::Complex.to_type(db),
        _ => return false,
    }
    .expect_union();

    ty == float_or_complex_ty
}

/// Returns all resolved definitions for an attribute expression `x.y`.
/// This function duplicates much of the functionality in the semantic
/// analyzer, but it has somewhat different behavior so we've decided
/// to keep it separate for now. One key difference is that this function
/// doesn't model the descriptor protocol when accessing attributes.
/// For "go to definition", we want to get the type of the descriptor object
/// rather than "invoking" its `__get__` or `__set__` method.
/// If this becomes a maintenance burden in the future, it may be worth
/// changing the corresponding logic in the semantic analyzer to conditionally
/// handle this case through the use of mode flags.
pub fn definitions_for_attribute<'db>(
    model: &SemanticModel<'db>,
    attribute: &ast::ExprAttribute,
) -> Vec<ResolvedDefinition<'db>> {
    let db = model.db();
    let name_str = attribute.attr.as_str();

    let mut resolved = Vec::new();

    // Determine the type of the LHS
    let Some(lhs_ty) = attribute.value.inferred_type(model) else {
        return resolved;
    };

    let tys = match lhs_ty {
        Type::Union(union) => union.elements(model.db()),
        _ => std::slice::from_ref(&lhs_ty),
    };

    // Expand intersections for each subtype into their components
    let expanded_tys = tys
        .iter()
        .flat_map(|ty| match ty {
            Type::Intersection(intersection) => Either::Left(intersection.positive(db).iter()),
            _ => Either::Right(std::iter::once(ty)),
        })
        .copied();

    for ty in expanded_tys {
        // Handle modules
        if let Type::ModuleLiteral(module_literal) = ty {
            if let Some(module_file) = module_literal.module(db).file(db) {
                let module_scope = global_scope(db, module_file);
                for def in find_symbol_in_scope(db, module_scope, name_str) {
                    resolved.extend(resolve_definition(
                        db,
                        def,
                        Some(name_str),
                        ImportAliasResolution::ResolveAliases,
                    ));
                }
            }
            continue;
        }

        // Prevent lookup on BoundSuper proxy object
        if matches!(ty, Type::BoundSuper(_)) {
            continue;
        }

        let meta_type = ty.to_meta_type(db);

        // Look up the attribute first on the meta-type, unless it's already a class-like type.
        let lookup_type = match ty {
            Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_) => ty,
            _ => meta_type,
        };

        let class_literal = match lookup_type {
            Type::ClassLiteral(class_literal) => class_literal,
            Type::SubclassOf(subclass) => match subclass.subclass_of().into_class(db) {
                Some(cls) => match cls.static_class_literal(db) {
                    Some((lit, _)) => ClassLiteral::Static(lit),
                    None => continue,
                },
                None => continue,
            },
            _ => continue,
        };

        resolved.extend(definitions_for_attribute_in_class_hierarchy(
            &class_literal,
            model,
            name_str,
        ));

        // The metaclass of a derived class must be a subclass of the metaclasses of all of
        // its base classes. This is why we only have to look at the metaclass of the
        // class_literal.
        // Only look up definitions on the metaclass if the type is a class object to begin with in
        // order to prevent looking up instance members on the class metaclass
        if resolved.is_empty() && meta_type != lookup_type {
            let class_literal = match meta_type {
                Type::ClassLiteral(class_literal) => class_literal,
                Type::SubclassOf(subclass) => match subclass.subclass_of().into_class(db) {
                    Some(cls) => match cls.static_class_literal(db) {
                        Some((lit, _)) => ClassLiteral::Static(lit),
                        None => continue,
                    },
                    None => continue,
                },
                _ => continue,
            };

            resolved.extend(definitions_for_attribute_in_class_hierarchy(
                &class_literal,
                model,
                name_str,
            ));
        }
    }

    resolved
}

/// Returns the descriptor object type for an attribute expression `x.y`, without invoking the
/// descriptor protocol. This corresponds to `inspect.getattr_static(x, "y")` at the type level.
pub fn static_member_type_for_attribute<'db>(
    model: &SemanticModel<'db>,
    attribute: &ast::ExprAttribute,
) -> Option<Type<'db>> {
    let lhs_ty = attribute.value.inferred_type(model)?;
    lhs_ty
        .static_member(model.db(), attribute.attr.as_str())
        .ignore_possibly_undefined()
}

fn definitions_for_attribute_in_class_hierarchy<'db>(
    class_literal: &ClassLiteral<'db>,
    model: &SemanticModel<'db>,
    attribute_name: &str,
) -> Vec<ResolvedDefinition<'db>> {
    let db = model.db();
    let mut resolved = Vec::new();
    'scopes: for ancestor in class_literal
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .filter_map(|cls: ClassType<'db>| cls.static_class_literal(db).map(|(lit, _)| lit))
    {
        let class_scope = ancestor.body_scope(db);
        let class_place_table = ty_python_core::place_table(db, class_scope);

        // Look for class-level declarations and bindings
        if let Some(place_id) = class_place_table.symbol_id(attribute_name) {
            let use_def = use_def_map(db, class_scope);
            let resolved_in_scope = resolve_reachable_definitions(
                db,
                attribute_name,
                use_def
                    .reachable_symbol_declarations(place_id)
                    .filter_map(|declaration| declaration.declaration.definition())
                    .chain(
                        use_def
                            .reachable_symbol_bindings(place_id)
                            .filter_map(|binding| binding.binding.definition()),
                    ),
            );
            if !resolved_in_scope.is_empty() {
                resolved.extend(resolved_in_scope);
                break 'scopes;
            }
        }

        // Look for instance attributes in method scopes (e.g., self.x = 1)
        let file = class_scope.file(db);
        let index = semantic_index(db, file);

        for function_scope_id in attribute_scopes(db, class_scope) {
            if let Some(place_id) = index
                .place_table(function_scope_id)
                .member_id_by_instance_attribute_name(attribute_name)
            {
                let use_def = index.use_def_map(function_scope_id);
                let resolved_in_scope = resolve_reachable_definitions(
                    db,
                    attribute_name,
                    use_def
                        .reachable_member_declarations(place_id)
                        .filter_map(|declaration| declaration.declaration.definition())
                        .chain(
                            use_def
                                .reachable_member_bindings(place_id)
                                .filter_map(|binding| binding.binding.definition()),
                        ),
                );
                if !resolved_in_scope.is_empty() {
                    resolved.extend(resolved_in_scope);
                    break 'scopes;
                }
            }
        }
    }

    resolved
}

fn reachable_definitions<'db>(
    db: &'db dyn Db,
    definitions: impl IntoIterator<Item = Definition<'db>>,
) -> FxIndexSet<Definition<'db>> {
    definitions
        .into_iter()
        .filter(|definition| definition.kind(db).is_user_visible())
        .collect()
}

fn resolve_reachable_definitions<'db>(
    db: &'db dyn Db,
    symbol_name: &str,
    definitions: impl IntoIterator<Item = Definition<'db>>,
) -> Vec<ResolvedDefinition<'db>> {
    reachable_definitions(db, definitions)
        .into_iter()
        .flat_map(|definition| {
            resolve_definition(
                db,
                definition,
                Some(symbol_name),
                ImportAliasResolution::ResolveAliases,
            )
        })
        .collect()
}

pub struct TypedDictKeyHover<'db> {
    pub owner: String,
    pub key: String,
    pub declared_ty: Type<'db>,
    pub docstring: Option<String>,
}

pub fn typed_dict_key_definition<'db>(
    model: &SemanticModel<'db>,
    subscript: &ast::ExprSubscript,
    key: &str,
) -> Option<ResolvedDefinition<'db>> {
    let value_ty = subscript.value.inferred_type(model)?;
    let typed_dict = value_ty.as_typed_dict()?;
    let field = typed_dict.items(model.db()).get(key)?;
    let definition = field.first_declaration()?;
    Some(ResolvedDefinition::Definition(definition))
}

pub fn typed_dict_key_hover<'db>(
    model: &SemanticModel<'db>,
    subscript: &ast::ExprSubscript,
) -> Option<TypedDictKeyHover<'db>> {
    let key = subscript
        .slice
        .as_string_literal_expr()
        .map(|literal| literal.value.to_str())?;
    let value_ty = subscript.value.inferred_type(model)?;
    let typed_dict = value_ty.as_typed_dict()?;
    let owner = value_ty.display(model.db()).to_string();
    let field = typed_dict.items(model.db()).get(key)?;
    let docstring = field
        .first_declaration()
        .and_then(|declaration| declaration.docstring(model.db()));

    Some(TypedDictKeyHover {
        owner,
        key: key.to_string(),
        declared_ty: field.declared_ty,
        docstring,
    })
}

/// Returns definitions for a keyword argument in a call expression.
/// This resolves the keyword argument to the corresponding parameter(s) in the callable's signature(s).
pub fn definitions_for_keyword_argument<'db>(
    model: &SemanticModel<'db>,
    keyword: &ast::Keyword,
    call_expr: &ast::ExprCall,
) -> Vec<ResolvedDefinition<'db>> {
    let db = model.db();
    let Some(func_type) = call_expr.func.inferred_type(model) else {
        return Vec::new();
    };

    let Some(keyword_name) = keyword.arg.as_ref() else {
        return Vec::new();
    };
    let keyword_name_str = keyword_name.as_str();

    let mut resolved_definitions = Vec::new();

    if let Some(callable_type) = func_type
        .try_upcast_to_callable(db)
        .and_then(CallableTypes::exactly_one)
    {
        let signatures = callable_type.signatures(db);

        // For each signature, find the parameter with the matching name
        for signature in signatures {
            if let Some((_param_index, param)) =
                signature.parameters().keyword_by_name(keyword_name_str)
                && let Some(definition) = param.definition()
            {
                resolved_definitions.push(ResolvedDefinition::Definition(definition));
            }
        }
    }

    resolved_definitions
}

/// Find the definitions for a symbol imported via `from x import y as z` statement.
/// This function handles the case where the cursor is on the original symbol name `y`.
/// Returns the same definitions as would be found for the alias `z`.
/// The `alias_resolution` parameter controls whether symbols imported with local import
/// aliases (like "x" in "from a import b as x") are resolved to their targets or kept
/// as aliases.
pub fn definitions_for_imported_symbol<'db>(
    model: &SemanticModel<'db>,
    import_node: &ast::StmtImportFrom,
    symbol_name: &str,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    let mut visited = FxHashSet::default();
    resolve_definition::resolve_from_import_definitions(
        model.db(),
        model.file(),
        import_node,
        symbol_name,
        &mut visited,
        alias_resolution,
    )
}

/// Returns the definition and overload co-definitions for a function declaration.
///
/// For overloaded functions this includes sibling overload declarations and the
/// implementation, if present.
pub fn definitions_and_overloads_for_function<'db>(
    model: &SemanticModel<'db>,
    function: &ast::StmtFunctionDef,
) -> Vec<ResolvedDefinition<'db>> {
    if let Some(function_type) = function
        .inferred_type(model)
        .and_then(Type::as_function_literal)
    {
        function_type
            .iter_overloads_and_implementation(model.db())
            .filter_map(|overload| overload.signature(model.db()).definition())
            .map(ResolvedDefinition::Definition)
            .collect()
    } else {
        vec![ResolvedDefinition::Definition(function.definition(model))]
    }
}

/// Details about a callable signature for IDE support.
#[derive(Debug, Clone)]
pub struct CallSignatureDetails<'db> {
    /// The signature itself
    pub signature: Signature<'db>,

    /// The display label for this signature (e.g., "(param1: str, param2: int) -> str")
    pub label: String,

    /// The displayed parameters for this signature, in left-to-right order.
    pub parameters: Vec<CallSignatureParameter<'db>>,

    /// The definition where this callable was originally defined (useful for
    /// extracting docstrings).
    pub definition: Option<Definition<'db>>,

    /// Mapping from argument indices to parameter indices. This helps
    /// determine which parameter corresponds to which argument position.
    pub argument_to_parameter_mapping: Vec<MatchedArgument<'db>>,

    /// Mapping from argument indices to displayed parameter indices. This accounts for
    /// displayed signatures that synthesize parameters, like bare `ParamSpec` signatures.
    pub argument_to_displayed_parameter_mapping: Vec<Option<usize>>,
}

/// A single displayed parameter in a callable signature for IDE support.
#[derive(Debug, Clone)]
pub struct CallSignatureParameter<'db> {
    /// The rendered label of the parameter as shown in the signature.
    pub label: String,

    /// The rendered name of the parameter, used for downstream IDE features.
    pub name: String,

    /// Annotated type of the parameter after applying any inferred specialization.
    pub ty: Type<'db>,

    /// True if the parameter is positional-only.
    pub is_positional_only: bool,

    /// True if the parameter can absorb arbitrarily many positional arguments.
    pub is_variadic: bool,

    /// True if the parameter can absorb arbitrarily many keyword arguments.
    pub is_keyword_variadic: bool,
}

impl<'db> CallSignatureDetails<'db> {
    fn from_binding(db: &'db dyn Db, binding: &crate::types::call::Binding<'db>) -> Self {
        let argument_to_parameter_mapping = binding.argument_matches().to_vec();
        let specialization = binding.specialization();
        let signature = binding.signature.clone();
        let display_details = signature.display(db).to_string_parts();
        let (parameters, parameter_to_displayed_parameter_mapping) =
            displayed_parameters_for_signature(db, &signature, &display_details, specialization);
        let argument_to_displayed_parameter_mapping = argument_to_parameter_mapping
            .iter()
            .map(|mapping| {
                mapping.parameters.iter().find_map(|parameter_index| {
                    parameter_to_displayed_parameter_mapping
                        .get(*parameter_index)
                        .copied()
                        .flatten()
                })
            })
            .collect();

        CallSignatureDetails {
            definition: signature.definition(),
            signature,
            label: display_details.label,
            parameters,
            argument_to_parameter_mapping,
            argument_to_displayed_parameter_mapping,
        }
    }
}

/// Build the parameter list shown for a rendered signature.
///
/// Returns both the displayed parameters and a mapping from each parameter in
/// `signature` to its displayed parameter index, if any. This accounts for
/// rendered signatures that synthesize or omit parameters, such as bare
/// `ParamSpec` signatures, and applies any inferred specialization to the
/// displayed parameter types.
fn displayed_parameters_for_signature<'db>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    display_details: &crate::types::display::SignatureDisplayDetails,
    specialization: Option<crate::types::generics::Specialization<'db>>,
) -> (Vec<CallSignatureParameter<'db>>, Vec<Option<usize>>) {
    // Apply any inferred specialization to displayed parameter types so
    // call-site substitutions are reflected in the rendered signature. For
    // example, if `_KT` was inferred as `str`, display `str` instead of `_KT`.
    let apply_specialization =
        |ty: Type<'db>| specialization.map_or(ty, |spec| ty.apply_specialization(db, spec));
    let parameters = signature.parameters();

    match parameters.kind() {
        ParametersKind::Standard | ParametersKind::Concatenate(_) => {
            let mut displayed_parameters = Vec::new();
            let mut parameter_to_displayed_parameter_mapping = vec![None; parameters.len()];

            for (parameter_index, parameter) in parameters.iter().enumerate() {
                let Some(range) = display_details
                    .parameter_ranges
                    .get(parameter_index)
                    .copied()
                else {
                    continue;
                };
                let Some(name) = display_details
                    .parameter_names
                    .get(parameter_index)
                    .cloned()
                else {
                    continue;
                };
                let Some(label) = display_details
                    .label
                    .get(range.to_std_range())
                    .map(ToString::to_string)
                else {
                    continue;
                };

                parameter_to_displayed_parameter_mapping[parameter_index] =
                    Some(displayed_parameters.len());
                displayed_parameters.push(CallSignatureParameter {
                    label,
                    name,
                    ty: apply_specialization(parameter.annotated_type()),
                    is_positional_only: parameter.is_positional_only(),
                    is_variadic: parameter.is_variadic(),
                    is_keyword_variadic: parameter.is_keyword_variadic(),
                });
            }

            (
                displayed_parameters,
                parameter_to_displayed_parameter_mapping,
            )
        }
        ParametersKind::ParamSpec(typevar) => {
            let parameter_name = format!("**{}", typevar.name(db));
            let label = display_details
                .parameter_ranges
                .first()
                .and_then(|range| {
                    display_details
                        .label
                        .get(range.to_std_range())
                        .map(ToString::to_string)
                })
                .unwrap_or_else(|| parameter_name.clone());
            let name = display_details
                .parameter_names
                .first()
                .cloned()
                .unwrap_or(parameter_name);

            (
                vec![CallSignatureParameter {
                    label,
                    name,
                    ty: Type::TypeVar(typevar),
                    is_positional_only: false,
                    is_variadic: true,
                    is_keyword_variadic: true,
                }],
                vec![Some(0); parameters.len()],
            )
        }
        ParametersKind::Gradual | ParametersKind::Top => (Vec::new(), vec![None; parameters.len()]),
    }
}

/// Extract signature details from a function call expression.
/// This function analyzes the callable being invoked and returns zero or more
/// `CallSignatureDetails` objects, each representing one possible signature
/// (in case of overloads or union types).
pub fn call_signature_details<'db>(
    model: &SemanticModel<'db>,
    call_expr: &ast::ExprCall,
) -> Vec<CallSignatureDetails<'db>> {
    let Some(func_type) = call_expr.func.inferred_type(model) else {
        return Vec::new();
    };

    let db = model.db();

    // Use into_callable to handle all the complex type conversions
    if let Some(callable_type) = func_type
        .try_upcast_to_callable(db)
        .map(|callables| callables.into_type(db))
    {
        // Use from_arguments_typed so that check_types can infer TypeVar
        // specializations from the actual argument types at this call site.
        let call_arguments =
            CallArguments::from_arguments_typed(&call_expr.arguments, |splatted_value| {
                splatted_value
                    .inferred_type(model)
                    .unwrap_or(Type::unknown())
            });
        let mut bindings = callable_type
            .bindings(db)
            .match_parameters(db, &call_arguments);

        // Run type checking to resolve TypeVar bindings from argument types.
        // For example, calling `dict[str, int].get("a")` resolves the `_KT`
        // TypeVar to `str`. We ignore errors since we still want signature
        // details even if the call has type errors.
        let constraints = ConstraintSetBuilder::new();
        let _ = bindings.check_types_impl(
            db,
            &constraints,
            &call_arguments,
            TypeContext::default(),
            &[],
        );

        // Extract signature details from all callable bindings
        bindings
            .iter_flat()
            .flatten()
            .map(|binding| CallSignatureDetails::from_binding(db, binding))
            .collect()
    } else {
        // Type is not callable, return empty signatures
        vec![]
    }
}

/// Resolve overloads for a callable type using call arguments,
/// returning the single matching signature if exactly one matches.
fn resolve_single_overload<'db>(
    model: &SemanticModel<'db>,
    callable_type: Type<'db>,
    call_expr: &ast::ExprCall,
) -> Option<Signature<'db>> {
    let db = model.db();
    let bindings = callable_type.bindings(db);

    let args = CallArguments::from_arguments_typed(&call_expr.arguments, |splatted_value| {
        splatted_value
            .inferred_type(model)
            .unwrap_or(Type::unknown())
    });

    let constraints = ConstraintSetBuilder::new();
    let mut resolved: Vec<_> = bindings
        .match_parameters(db, &args)
        .check_types(db, &constraints, &args, TypeContext::default(), &[])
        .iter()
        .flat_map(super::call::bind::Bindings::iter_flat)
        .flat_map(|binding| {
            binding
                .matching_overloads()
                .map(|(_, overload)| overload.signature.clone())
        })
        .collect();

    if resolved.len() != 1 {
        return None;
    }

    resolved.pop()
}

/// Whether a call argument is interpreted as a value expression or a type expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallArgumentForm {
    Unknown,
    Value,
    Type,
}

/// Fully binds and type-checks a call expression against the given callee type.
///
/// This is the expensive path for call-argument form analysis: it matches call-site arguments to
/// parameters, checks argument types, and preserves the resulting bindings even when the call is
/// invalid so IDE features can inspect the best available binding information.
fn full_type_bindings_for_call<'db>(
    model: &SemanticModel<'db>,
    func_type: Type<'db>,
    call_expr: &ast::ExprCall,
) -> crate::types::call::Bindings<'db> {
    let db = model.db();
    let call_arguments =
        CallArguments::from_arguments_typed(&call_expr.arguments, |splatted_value| {
            splatted_value
                .inferred_type(model)
                .unwrap_or(Type::unknown())
        });
    let constraints = ConstraintSetBuilder::new();

    func_type
        .bindings(db)
        .match_parameters(db, &call_arguments)
        .check_types(
            db,
            &constraints,
            &call_arguments,
            TypeContext::default(),
            &[],
        )
        .unwrap_or_else(|CallError(_, bindings)| *bindings)
}

/// Returns the form for a single argument from a successful binding.
fn argument_form_from_successful_binding(
    binding: &crate::types::call::Binding<'_>,
    argument_index: usize,
) -> CallArgumentForm {
    if let Some(argument_match) = binding.argument_matches().get(argument_index)
        && argument_match.matched
        && let [parameter_index] = argument_match.parameters.as_slice()
    {
        return match binding.signature.parameters()[*parameter_index].form {
            ParameterForm::Value => CallArgumentForm::Value,
            ParameterForm::Type => CallArgumentForm::Type,
        };
    }

    CallArgumentForm::Unknown
}

/// Returns the form of each call-site argument in source order.
///
/// `CallArgumentForm::Unknown` indicates that an argument is unmatched or its form cannot be
/// determined unambiguously, for example because a variadic argument maps to multiple parameters.
pub fn call_argument_forms(
    model: &SemanticModel<'_>,
    call_expr: &ast::ExprCall,
) -> Vec<CallArgumentForm> {
    let Some(func_type) = call_expr.func.inferred_type(model) else {
        return Vec::new();
    };

    let argument_count = call_expr.arguments.len();

    // If the function doesn't contain any type forms, for any overloads, short-circuit.
    if !func_type.bindings(model.db()).iter_flat().any(|binding| {
        binding.overloads().iter().any(|overload| {
            overload
                .signature
                .parameters()
                .into_iter()
                .any(|parameter| parameter.form == ParameterForm::Type)
        })
    }) {
        return vec![CallArgumentForm::Value; argument_count];
    }

    let bindings = full_type_bindings_for_call(model, func_type, call_expr);

    let mut argument_forms = vec![CallArgumentForm::Unknown; argument_count];

    // If any bindings are successful, limit analysis to those bindings.
    let successful_bindings: Vec<_> = bindings
        .iter_flat()
        .flatten()
        .filter(|binding| binding.errors().is_empty())
        .collect();

    let Some((first_binding, remaining_bindings)) = successful_bindings.split_first() else {
        // If no binding succeeds, fall back to the merged non-conflicting forms from the full
        // binding result so callers still get the best conservative answer available.
        for (arg_index, form) in bindings.non_conflicting_argument_forms().enumerate() {
            let Some(argument_form) = argument_forms.get_mut(arg_index) else {
                break;
            };
            *argument_form = form.map_or(CallArgumentForm::Unknown, |form| match form {
                ParameterForm::Value => CallArgumentForm::Value,
                ParameterForm::Type => CallArgumentForm::Type,
            });
        }
        return argument_forms;
    };

    // If all successful bindings agree on the argument form, use the agreed-upon form; otherwise,
    // fall back to `CallArgumentForm::Unknown`.
    for (arg_index, resolved_argument_form) in argument_forms.iter_mut().enumerate() {
        let argument_form = argument_form_from_successful_binding(first_binding, arg_index);
        if argument_form == CallArgumentForm::Unknown {
            continue;
        }
        if remaining_bindings.iter().all(|binding| {
            argument_form_from_successful_binding(binding, arg_index) == argument_form
        }) {
            *resolved_argument_form = argument_form;
        }
    }

    argument_forms
}

/// Given a call expression that has overloads, and whose overload is resolved to a
/// single option by its arguments, return the type of the Signature.
///
/// This is only used for simplifying complex call types, so if we ever detect that
/// the given callable type *is* simple, or that our answer *won't* be simple, we
/// bail at out and return None, so that the original type can be used.
///
/// We do this because `Type::Signature` intentionally loses a lot of context, and
/// so it has a "worse" display than say `Type::FunctionLiteral` or `Type::BoundMethod`,
/// which this analysis would naturally wipe away. The contexts this function
/// succeeds in are those where we would print a complicated/ugly type anyway.
pub fn call_type_simplified_by_overloads(
    model: &SemanticModel,
    call_expr: &ast::ExprCall,
) -> Option<String> {
    let db = model.db();
    let func_type = call_expr.func.inferred_type(model)?;

    let callable_type = func_type.try_upcast_to_callable(db)?.into_type(db);

    // If the callable is trivial this analysis is useless, bail out
    if let Some(binding) = callable_type.bindings(db).single_element()
        && binding.overloads().len() < 2
    {
        return None;
    }

    let signature = resolve_single_overload(model, callable_type, call_expr)?;
    Some(
        signature
            .display_with(db, DisplaySettings::default().multiline())
            .to_string(),
    )
}

/// Returns the definitions of the binary operation along with its callable type.
pub fn definitions_for_bin_op<'db>(
    model: &SemanticModel<'db>,
    binary_op: &ast::ExprBinOp,
) -> Option<(Vec<ResolvedDefinition<'db>>, Type<'db>)> {
    let left_ty = binary_op.left.inferred_type(model)?;
    let right_ty = binary_op.right.inferred_type(model)?;

    let Ok(bindings) = Type::try_call_bin_op(model.db(), left_ty, binary_op.op, right_ty) else {
        return None;
    };

    let callable_type = promote_for_self(model.db(), bindings.callable_type());

    let definitions: Vec<_> = bindings
        .iter_flat()
        .flatten()
        .filter_map(|binding| {
            Some(ResolvedDefinition::Definition(
                binding.signature.definition?,
            ))
        })
        .collect();

    Some((definitions, callable_type))
}

/// Returns the definitions for an unary operator along with their callable types.
pub fn definitions_for_unary_op<'db>(
    model: &SemanticModel<'db>,
    unary_op: &ast::ExprUnaryOp,
) -> Option<(Vec<ResolvedDefinition<'db>>, Type<'db>)> {
    let operand_ty = unary_op.operand.inferred_type(model)?;

    let unary_dunder_method = match unary_op.op {
        ast::UnaryOp::Invert => "__invert__",
        ast::UnaryOp::UAdd => "__pos__",
        ast::UnaryOp::USub => "__neg__",
        ast::UnaryOp::Not => "__bool__",
    };

    let bindings = match operand_ty.try_call_dunder(
        model.db(),
        unary_dunder_method,
        CallArguments::none(),
        TypeContext::default(),
    ) {
        Ok(bindings) => bindings,
        Err(CallDunderError::MethodNotAvailable) if unary_op.op == ast::UnaryOp::Not => {
            // The runtime falls back to `__len__` for `not` if `__bool__` is not defined.
            match operand_ty.try_call_dunder(
                model.db(),
                "__len__",
                CallArguments::none(),
                TypeContext::default(),
            ) {
                Ok(bindings) => bindings,
                Err(CallDunderError::MethodNotAvailable) => return None,
                Err(
                    CallDunderError::PossiblyUnbound { bindings, .. }
                    | CallDunderError::CallError(_, bindings),
                ) => *bindings,
            }
        }
        Err(CallDunderError::MethodNotAvailable) => return None,
        Err(
            CallDunderError::PossiblyUnbound { bindings, .. }
            | CallDunderError::CallError(_, bindings),
        ) => *bindings,
    };

    let callable_type = promote_for_self(model.db(), bindings.callable_type());

    let definitions = bindings
        .iter_flat()
        .flatten()
        .filter_map(|binding| {
            Some(ResolvedDefinition::Definition(
                binding.signature.definition?,
            ))
        })
        .collect();

    Some((definitions, callable_type))
}

/// Promotes types in `self` positions.
///
/// This is so that we show e.g. `int.__add__` instead of `Literal[4].__add__`.
fn promote_for_self<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
    match ty {
        Type::BoundMethod(method) => Type::BoundMethod(method.map_self_type(db, |self_ty| {
            self_ty.literal_fallback_instance(db).unwrap_or(self_ty)
        })),
        Type::Union(elements) => elements.map(db, |ty| match ty {
            Type::BoundMethod(method) => Type::BoundMethod(method.map_self_type(db, |self_ty| {
                self_ty.literal_fallback_instance(db).unwrap_or(self_ty)
            })),
            _ => *ty,
        }),
        ty => ty,
    }
}

/// Find the active signature index from `CallSignatureDetails`.
/// The active signature is the first signature where all arguments present in the call
/// have valid mappings to parameters (i.e., none of the mappings are None).
pub fn find_active_signature_from_details(
    signature_details: &[CallSignatureDetails],
) -> Option<usize> {
    let first = signature_details.first()?;

    // If there are no arguments in the mapping, just return the first signature.
    if first.argument_to_parameter_mapping.is_empty() {
        return Some(0);
    }

    // First, try to find a signature where all arguments have valid parameter mappings.
    let perfect_match = signature_details.iter().position(|details| {
        // Check if all arguments have valid parameter mappings.
        details
            .argument_to_parameter_mapping
            .iter()
            .all(|mapping| mapping.matched)
    });

    if let Some(index) = perfect_match {
        return Some(index);
    }

    // If no perfect match, find the signature with the most valid argument mappings.
    let (best_index, _) = signature_details
        .iter()
        .enumerate()
        .max_by_key(|(_, details)| {
            details
                .argument_to_parameter_mapping
                .iter()
                .filter(|mapping| mapping.matched)
                .count()
        })?;

    Some(best_index)
}

/// Resolve a call expression to its matching overload's signature details,
/// using full type checking (not just arity matching) for overload resolution.
///
/// Falls back to arity-based matching if type-based resolution fails.
pub fn resolved_call_signature<'db>(
    model: &SemanticModel<'db>,
    call_expr: &ast::ExprCall,
) -> Option<CallSignatureDetails<'db>> {
    let db = model.db();
    let func_type = call_expr.func.inferred_type(model)?;
    let callable_type = func_type.try_upcast_to_callable(db)?.into_type(db);

    let args = CallArguments::from_arguments_typed(&call_expr.arguments, |splatted_value| {
        splatted_value
            .inferred_type(model)
            .unwrap_or(Type::unknown())
    });

    // Extract the `Bindings` regardless of whether type checking succeeded or failed.
    let constraints = ConstraintSetBuilder::new();
    let bindings = callable_type
        .bindings(db)
        .match_parameters(db, &args)
        .check_types(db, &constraints, &args, TypeContext::default(), &[])
        .unwrap_or_else(|CallError(_, bindings)| *bindings);

    // First, try to find the matching overload after full type checking.
    let type_checked_details: Vec<_> = bindings
        .iter_flat()
        .flat_map(|binding| binding.matching_overloads().map(|(_, overload)| overload))
        .map(|binding| CallSignatureDetails::from_binding(db, binding))
        .collect();

    if !type_checked_details.is_empty() {
        let active = find_active_signature_from_details(&type_checked_details)?;
        return type_checked_details.into_iter().nth(active);
    }

    // If all overloads have type-checking errors (e.g., `InvalidArgumentType`),
    // `matching_overloads()` returns empty. Fall back to arity-based matching
    // across all overloads to pick the best candidate for showing hints.
    let all_details: Vec<_> = bindings
        .iter_flat()
        .flatten()
        .map(|binding| CallSignatureDetails::from_binding(db, binding))
        .collect();

    if all_details.is_empty() {
        return None;
    }

    let active = find_active_signature_from_details(&all_details)?;
    all_details.into_iter().nth(active)
}

#[derive(Default)]
pub struct InlayHintCallArgumentDetails {
    /// The position of the arguments mapped to their name and the range of the argument definition in the signature.
    pub argument_names: HashMap<usize, (String, Option<FileRange>)>,
}

pub fn inlay_hint_call_argument_details<'db>(
    db: &'db dyn Db,
    model: &SemanticModel<'db>,
    call_expr: &ast::ExprCall,
) -> Option<InlayHintCallArgumentDetails> {
    let resolved = resolved_call_signature(model, call_expr)?;

    let parameters = resolved.signature.parameters();

    let mut argument_names = HashMap::new();

    for arg_index in 0..call_expr.arguments.args.len() {
        let Some(arg_mapping) = resolved.argument_to_parameter_mapping.get(arg_index) else {
            continue;
        };

        if !arg_mapping.matched {
            continue;
        }

        // Skip if this argument maps to multiple parameters (e.g., unpacked tuple filling
        // multiple slots). Showing a single parameter name would be misleading.
        if arg_mapping.parameters.len() > 1 {
            continue;
        }

        let Some(param_index) = arg_mapping.parameters.first() else {
            continue;
        };

        let Some(param) = parameters.get(*param_index) else {
            continue;
        };

        let parameter_label_offset = param.definition().map(|definition| {
            let param_file = definition.file(db);
            let module = parsed_module(db, param_file).load(db);
            definition.focus_range(db, &module)
        });

        // Only add hints for parameters that can be specified by name
        if !param.is_positional_only() && !param.is_variadic() && !param.is_keyword_variadic() {
            let Some(name) = param.name() else {
                continue;
            };
            argument_names.insert(arg_index, (name.to_string(), parameter_label_offset));
        }
    }

    Some(InlayHintCallArgumentDetails { argument_names })
}

mod resolve_definition {
    //! Resolves an Import, `ImportFrom` or `StarImport` definition to one or more
    //! "resolved definitions". This is done recursively to find the original
    //! definition targeted by the import.

    /// Controls whether local import aliases should be resolved to their targets or returned as-is.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ImportAliasResolution {
        /// Resolve import aliases to their original definitions
        ResolveAliases,
        /// Keep import aliases as-is, don't resolve to original definitions
        PreserveAliases,
    }

    use indexmap::IndexSet;
    use ruff_db::files::{File, FileRange, vendored_path_to_file};
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use ruff_db::system::SystemPath;
    use ruff_db::vendored::VendoredPathBuf;
    use ruff_python_ast as ast;
    use ruff_python_stdlib::sys::is_builtin_module;
    use rustc_hash::FxHashSet;
    use tracing::trace;
    use ty_module_resolver::{ModuleName, file_to_module, resolve_module, resolve_real_module};

    use crate::Db;
    use crate::module_docstring;
    use crate::types::binding_type;
    use ty_python_core::definition::{Definition, DefinitionKind};
    use ty_python_core::scope::{NodeWithScopeKind, ScopeId};
    use ty_python_core::{global_scope, place_table, semantic_index, use_def_map};

    /// Represents the result of resolving an import to either a specific definition or
    /// a specific range within a file.
    /// This enum helps distinguish between cases where an import resolves to:
    /// - A specific definition within a module (e.g., `from os import path` -> definition of `path`)
    /// - A specific range within a file, sometimes an empty range at the top of the file
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ResolvedDefinition<'db> {
        /// The import resolved to a specific definition within a module
        Definition(Definition<'db>),
        /// The import resolved to an entire module
        Module(File),
        /// The import resolved to a file with a specific range
        FileWithRange(FileRange),
    }

    impl<'db> ResolvedDefinition<'db> {
        pub fn definition(&self) -> Option<Definition<'db>> {
            match self {
                ResolvedDefinition::Definition(definition) => Some(*definition),
                ResolvedDefinition::Module(_) => None,
                ResolvedDefinition::FileWithRange(_) => None,
            }
        }

        fn file(&self, db: &'db dyn Db) -> File {
            match self {
                ResolvedDefinition::Definition(definition) => definition.file(db),
                ResolvedDefinition::Module(file) => *file,
                ResolvedDefinition::FileWithRange(file_range) => file_range.file(),
            }
        }

        pub fn docstring(&self, db: &'db dyn Db) -> Option<String> {
            match self {
                ResolvedDefinition::Definition(definition) => definition.docstring(db),
                ResolvedDefinition::Module(file) => module_docstring(db, *file),
                ResolvedDefinition::FileWithRange(_) => None,
            }
        }

        pub fn implementation_docstring(&self, db: &'db dyn Db) -> Option<String> {
            match self {
                ResolvedDefinition::Definition(definition) => {
                    implementation_docstring(db, *definition)
                }
                ResolvedDefinition::Module(_) | ResolvedDefinition::FileWithRange(_) => None,
            }
        }
    }

    // Overload declarations often omit docstrings, while the runtime
    // implementation appears as the last sibling binding for the same symbol.
    // Fall back to that binding's docstring when the resolved overload has none.
    //
    // Uses type-aware matching: resolves each end-of-scope binding's type to a
    // function literal, then checks whether that function's overloads contain the
    // current definition. This correctly handles version-conditional branches and
    // avoids picking up unrelated reassignments of the same name.
    fn implementation_docstring<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Option<String> {
        let DefinitionKind::Function(_) = definition.kind(db) else {
            return None;
        };

        let name = definition.name(db)?;
        let scope = definition.scope(db);
        let symbol_id = place_table(db, scope).symbol_id(&name)?;
        let use_def = use_def_map(db, scope);

        let current_overload = binding_type(db, definition)
            .as_function_literal()?
            .literal(db)
            .last_definition;

        // Find the last end-of-scope binding whose function type contains this overload.
        let implementation = use_def
            .end_of_scope_symbol_bindings(symbol_id)
            .filter_map(|binding| {
                let ty = binding_type(db, binding.binding.definition()?).as_function_literal()?;
                ty.iter_overloads_and_implementation(db)
                    .any(|overload| overload == current_overload)
                    .then_some(ty)
            })
            .last()?;

        implementation.definition(db).docstring(db)
    }

    /// Resolve import definitions to their targets.
    /// Returns resolved definitions which can be either specific definitions or module files.
    /// For non-import definitions, returns the definition wrapped in `ResolvedDefinition::Definition`.
    /// Always returns at least the original definition as a fallback if resolution fails.
    pub(crate) fn resolve_definition<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        symbol_name: Option<&str>,
        alias_resolution: ImportAliasResolution,
    ) -> Vec<ResolvedDefinition<'db>> {
        let mut visited = FxHashSet::default();
        let resolved = resolve_definition_recursive(
            db,
            definition,
            &mut visited,
            symbol_name,
            alias_resolution,
        );

        // If resolution failed, return the original definition as fallback
        if resolved.is_empty() {
            vec![ResolvedDefinition::Definition(definition)]
        } else {
            resolved
        }
    }

    /// Helper function to resolve import definitions recursively.
    fn resolve_definition_recursive<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        visited: &mut FxHashSet<Definition<'db>>,
        symbol_name: Option<&str>,
        alias_resolution: ImportAliasResolution,
    ) -> Vec<ResolvedDefinition<'db>> {
        // Prevent infinite recursion if there are circular imports
        if visited.contains(&definition) {
            return Vec::new(); // Return empty list for circular imports
        }
        visited.insert(definition);

        let kind = definition.kind(db);

        match kind {
            DefinitionKind::Import(import_def) => {
                let file = definition.file(db);
                let module = parsed_module(db, file).load(db);
                let alias = import_def.alias(&module);

                if alias.asname.is_some()
                    && alias_resolution == ImportAliasResolution::PreserveAliases
                {
                    return vec![ResolvedDefinition::Definition(definition)];
                }

                // Get the full module name being imported
                let Some(module_name) = ModuleName::new(&alias.name) else {
                    return Vec::new(); // Invalid module name, return empty list
                };

                // Resolve the module to its file
                let Some(resolved_module) = resolve_module(db, file, &module_name) else {
                    return Vec::new(); // Module not found, return empty list
                };

                let Some(module_file) = resolved_module.file(db) else {
                    return Vec::new(); // No file for module, return empty list
                };

                // For simple imports like "import os", we want to navigate to the module itself.
                // Return the module file directly instead of trying to find definitions within it.
                vec![ResolvedDefinition::Module(module_file)]
            }

            DefinitionKind::ImportFrom(import_from_def) => {
                let file = definition.file(db);
                let module = parsed_module(db, file).load(db);
                let import_node = import_from_def.import(&module);
                let alias = import_from_def.alias(&module);

                if alias.asname.is_some()
                    && alias_resolution == ImportAliasResolution::PreserveAliases
                {
                    return vec![ResolvedDefinition::Definition(definition)];
                }

                // For `ImportFrom`, we need to resolve the original imported symbol name
                // (alias.name), not the local alias (symbol_name)
                resolve_from_import_definitions(
                    db,
                    file,
                    import_node,
                    &alias.name,
                    visited,
                    alias_resolution,
                )
            }

            // For star imports, try to resolve to the specific symbol being accessed
            DefinitionKind::StarImport(star_import_def) => {
                let file = definition.file(db);
                let module = parsed_module(db, file).load(db);
                let import_node = star_import_def.import(&module);

                // If we have a symbol name, use the helper to resolve it in the target module
                if let Some(symbol_name) = symbol_name {
                    resolve_from_import_definitions(
                        db,
                        file,
                        import_node,
                        symbol_name,
                        visited,
                        alias_resolution,
                    )
                } else {
                    // No symbol context provided, can't resolve star import
                    Vec::new()
                }
            }

            // For non-import definitions, return the definition as is
            _ => vec![ResolvedDefinition::Definition(definition)],
        }
    }

    /// Helper function to resolve import definitions for `ImportFrom` and `StarImport` cases.
    pub(crate) fn resolve_from_import_definitions<'db>(
        db: &'db dyn Db,
        file: File,
        import_node: &ast::StmtImportFrom,
        symbol_name: &str,
        visited: &mut FxHashSet<Definition<'db>>,
        alias_resolution: ImportAliasResolution,
    ) -> Vec<ResolvedDefinition<'db>> {
        if alias_resolution == ImportAliasResolution::PreserveAliases {
            for alias in &import_node.names {
                if let Some(asname) = &alias.asname {
                    if asname.as_str() == symbol_name {
                        return vec![ResolvedDefinition::FileWithRange(FileRange::new(
                            file,
                            asname.range,
                        ))];
                    }
                }
            }
        }

        // Resolve the module being imported from (handles both relative and absolute imports)
        let Some(module_name) = ModuleName::from_import_statement(db, file, import_node).ok()
        else {
            return Vec::new();
        };
        let Some(resolved_module) = resolve_module(db, file, &module_name) else {
            return Vec::new();
        };

        // Resolve the target module file
        let module_file = resolved_module.file(db);

        let Some(module_file) = module_file else {
            // No file means this is a namespace package, try to import the submodule
            return Vec::from_iter(resolve_from_import_submodule_definitions(
                db,
                file,
                symbol_name,
                module_name,
            ));
        };

        // Find the definition of this symbol in the imported module's global scope
        let global_scope = global_scope(db, module_file);
        let definitions_in_module = find_symbol_in_scope(db, global_scope, symbol_name);

        // Recursively resolve any import definitions found in the target module
        let mut resolved_definitions = Vec::new();
        for def in definitions_in_module {
            let resolved =
                resolve_definition_recursive(db, def, visited, Some(symbol_name), alias_resolution);
            resolved_definitions.extend(resolved);
        }

        if resolved_definitions.is_empty() {
            // In `pkg/__init__.py`, `from . import child` resolves `.` to
            // `pkg/__init__.py`. Looking up `child` there can find an import definition
            // that recursively resolves back here (possibly through `from . import *`),
            // so recursive resolution bottoms out before reaching the `pkg.child`
            // submodule target. Fall back to the same submodule candidate we use when
            // `child` has no binding in `pkg/__init__.py`.
            Vec::from_iter(resolve_from_import_submodule_definitions(
                db,
                file,
                symbol_name,
                module_name,
            ))
        } else {
            resolved_definitions
        }
    }

    // Helper to resolve `from x.y import z` assuming `x.y.z` is a module.
    fn resolve_from_import_submodule_definitions<'db>(
        db: &'db dyn Db,
        file: File,
        symbol_name: &str,
        module_name: ModuleName,
    ) -> Option<ResolvedDefinition<'db>> {
        let submodule_name = ModuleName::new(symbol_name)?;
        let mut full_submodule_name = module_name;
        full_submodule_name.extend(&submodule_name);
        let module = resolve_module(db, file, &full_submodule_name)?;
        let file = module.file(db)?;

        Some(ResolvedDefinition::Module(file))
    }

    /// Find definitions for a symbol name in a specific scope.
    pub(crate) fn find_symbol_in_scope<'db>(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        symbol_name: &str,
    ) -> IndexSet<Definition<'db>> {
        let place_table = place_table(db, scope);
        let Some(symbol_id) = place_table.symbol_id(symbol_name) else {
            return IndexSet::new();
        };

        let use_def_map = use_def_map(db, scope);
        let mut definitions = IndexSet::new();

        // Get all definitions (both bindings and declarations) for this place
        let bindings = use_def_map.reachable_symbol_bindings(symbol_id);
        let declarations = use_def_map.reachable_symbol_declarations(symbol_id);

        for binding in bindings {
            if let Some(def) = binding.binding.definition() {
                definitions.insert(def);
            }
        }

        for declaration in declarations {
            if let Some(def) = declaration.declaration.definition() {
                definitions.insert(def);
            }
        }

        definitions
    }

    /// Given a definition that may be in a stub file, find the "real" definition in a non-stub.
    #[tracing::instrument(skip_all)]
    pub fn map_stub_definition<'db>(
        db: &'db dyn Db,
        def: &ResolvedDefinition<'db>,
        cached_vendored_typeshed: Option<&SystemPath>,
    ) -> Option<Vec<ResolvedDefinition<'db>>> {
        // If the file isn't a stub, this is presumably the real definition
        let stub_file = def.file(db);
        trace!("Stub mapping definition in: {}", stub_file.path(db));
        if !stub_file.is_stub(db) {
            trace!("File isn't a stub, no stub mapping to do");
            return None;
        }

        // We write vendored typeshed stubs to disk in the cache, and consequently "forget"
        // that they're typeshed when an IDE hands those paths back to us later. For most
        // purposes this seemingly doesn't matter at all, and avoids issues with someone
        // editing the cache by hand in their IDE and us getting confused about the contents
        // of the file (hello and welcome to anyone who has found Bigger Issues this causes).
        //
        // The major exception is in exactly stub-mapping, where we need to "remember" that
        // we're in typeshed to successfully stub-map to the Real Stdlib. So here we attempt
        // to do just that. The resulting file must not be used for anything other than
        // this module lookup, as the `ResolvedDefinition` we're handling isn't for that file.
        let mut stub_file_for_module_lookup = stub_file;
        if let Some(vendored_typeshed) = cached_vendored_typeshed
            && let Some(stub_path) = stub_file.path(db).as_system_path()
            && let Ok(rel_path) = stub_path.strip_prefix(vendored_typeshed)
            && let Ok(typeshed_file) =
                vendored_path_to_file(db, VendoredPathBuf::from(rel_path.as_str()))
        {
            trace!(
                "Stub is cached vendored typeshed: {}",
                typeshed_file.path(db)
            );
            stub_file_for_module_lookup = typeshed_file;
        }

        // It's definitely a stub, so now rerun module resolution but with stubs disabled.
        let stub_module = file_to_module(db, stub_file_for_module_lookup)?;
        trace!("Found stub module: {}", stub_module.name(db));
        // We need to pass an importing file to `resolve_real_module` which is a bit odd
        // here because there isn't really an importing file. However this `resolve_real_module`
        // can be understood as essentially `import .`, which is also what `file_to_module` is,
        // so this is in fact exactly the file we want to consider the importer.
        //
        // ... unless we have a builtin module. i.e., A module embedded
        // into the interpreter. In which case, all we have are stubs.
        // `resolve_real_module` will always return `None` for this case, but
        // it will emit false positive logs. And this saves us some work.
        if is_builtin_module(db.python_version().minor, stub_module.name(db)) {
            return None;
        }
        let real_module =
            resolve_real_module(db, stub_file_for_module_lookup, stub_module.name(db))?;
        trace!("Found real module: {}", real_module.name(db));
        let real_file = real_module.file(db)?;
        trace!("Found real file: {}", real_file.path(db));

        // A definition has a "Definition Path" in a file made of nested definitions (~scopes):
        //
        // ```
        // class myclass:  # ./myclass
        //     def some_func(args: bool):  # ./myclass/some_func
        //                 # ^~~~ ./myclass/other_func/args/
        // ```
        //
        // So our heuristic goal here is to compute a Definition Path in the stub file
        // and then resolve the same Definition Path in the real file.
        //
        // NOTE: currently a path component is just a str, but in the future additional
        // disambiguators (like "is a class def") could be added if needed.
        let mut path = Vec::new();
        let stub_parsed;
        let stub_ref;
        match *def {
            ResolvedDefinition::Definition(definition) => {
                stub_parsed = parsed_module(db, stub_file);
                stub_ref = stub_parsed.load(db);

                // Get the leaf of the path (the definition itself)
                let leaf = definition_path_component_for_leaf(db, &stub_ref, definition)
                    .map_err(|()| {
                        trace!("Found unsupported DefinitionKind while stub mapping, giving up");
                    })
                    .ok()?;
                path.push(leaf);

                // Get the ancestors of the path (all the definitions we're nested under)
                let index = semantic_index(db, stub_file);
                for (_scope_id, scope) in index.ancestor_scopes(definition.file_scope(db)) {
                    let node = scope.node();
                    let component = definition_path_component_for_node(&stub_ref, node)
                        .map_err(|()| {
                            trace!("Found unsupported NodeScopeKind while stub mapping, giving up");
                        })
                        .ok()?;
                    if let Some(component) = component {
                        path.push(component);
                    }
                }
                trace!("Built Definition Path: {path:?}");
            }
            ResolvedDefinition::Module(_) => {
                trace!(
                    "Found module mapping: {} => {}",
                    stub_file.path(db),
                    real_file.path(db)
                );
                return Some(vec![ResolvedDefinition::Module(real_file)]);
            }
            ResolvedDefinition::FileWithRange(_) => {
                // Not yet implemented -- in this case we want to recover something like a Definition
                // and build a Definition Path, but this input is a bit too abstract for now.
                trace!("Found arbitrary FileWithRange while stub mapping, giving up");
                return None;
            }
        }

        // Walk down the Definition Path in the real file
        let mut definitions = Vec::new();
        let index = semantic_index(db, real_file);
        let real_parsed = parsed_module(db, real_file);
        let real_ref = real_parsed.load(db);
        // Start our search in the module (global) scope
        let mut scopes = vec![global_scope(db, real_file)];
        while let Some(component) = path.pop() {
            trace!("Traversing definition path component: {}", component);
            // We're doing essentially a breadth-first traversal of the definitions.
            // If ever we find multiple matching scopes for a component, we need to continue
            // walking down each of them to try to resolve the path. Here we loop over
            // all the scopes at the current level of search.
            for scope in std::mem::take(&mut scopes) {
                if path.is_empty() {
                    // We're at the end of the path, everything we find here is the final result
                    definitions.extend(
                        find_symbol_in_scope(db, scope, component)
                            .into_iter()
                            .map(ResolvedDefinition::Definition),
                    );
                } else {
                    // We're in the middle of the path, look for scopes that match the current component
                    for (child_scope_id, child_scope) in index.child_scopes(scope.file_scope_id(db))
                    {
                        let scope_node = child_scope.node();
                        if let Ok(Some(real_component)) =
                            definition_path_component_for_node(&real_ref, scope_node)
                        {
                            if real_component == component {
                                scopes.push(child_scope_id.to_scope_id(db, real_file));
                            }
                        }
                        scope.node(db);
                    }
                }
            }
            trace!(
                "Found {} scopes and {} definitions",
                scopes.len(),
                definitions.len()
            );
        }
        if definitions.is_empty() {
            trace!("No definitions found in real file, stub mapping failed");
            None
        } else {
            trace!("Found {} definitions from stub mapping", definitions.len());
            Some(definitions)
        }
    }

    /// Computes a "Definition Path" component for an internal node of the definition path.
    ///
    /// See [`map_stub_definition`][] for details.
    fn definition_path_component_for_node<'parse>(
        parsed: &'parse ParsedModuleRef,
        node: &NodeWithScopeKind,
    ) -> Result<Option<&'parse str>, ()> {
        let component = match node {
            NodeWithScopeKind::Module => {
                // This is just implicit, so has no component
                return Ok(None);
            }
            NodeWithScopeKind::Class(class) => class.node(parsed).name.as_str(),
            NodeWithScopeKind::Function(func) => func.node(parsed).name.as_str(),
            NodeWithScopeKind::TypeAlias(_)
            | NodeWithScopeKind::ClassTypeParameters(_)
            | NodeWithScopeKind::FunctionTypeParameters(_)
            | NodeWithScopeKind::TypeAliasTypeParameters(_)
            | NodeWithScopeKind::Lambda(_)
            | NodeWithScopeKind::ListComprehension(_)
            | NodeWithScopeKind::SetComprehension(_)
            | NodeWithScopeKind::DictComprehension(_)
            | NodeWithScopeKind::GeneratorExpression(_) => {
                // Not yet implemented
                return Err(());
            }
        };
        Ok(Some(component))
    }

    /// Computes a "Definition Path" component for a leaf node of the definition path.
    ///
    /// See [`map_stub_definition`][] for details.
    fn definition_path_component_for_leaf<'parse>(
        db: &dyn Db,
        parsed: &'parse ParsedModuleRef,
        definition: Definition,
    ) -> Result<&'parse str, ()> {
        let component = match definition.kind(db) {
            DefinitionKind::Function(func) => func.node(parsed).name.as_str(),
            DefinitionKind::Class(class) => class.node(parsed).name.as_str(),
            DefinitionKind::TypeAlias(_)
            | DefinitionKind::Import(_)
            | DefinitionKind::ImportFrom(_)
            | DefinitionKind::ImportFromSubmodule(_)
            | DefinitionKind::StarImport(_)
            | DefinitionKind::NamedExpression(_)
            | DefinitionKind::Assignment(_)
            | DefinitionKind::AnnotatedAssignment(_)
            | DefinitionKind::AugmentedAssignment(_)
            | DefinitionKind::DictKeyAssignment(_)
            | DefinitionKind::For(_)
            | DefinitionKind::Comprehension(_)
            | DefinitionKind::Parameter(_)
            | DefinitionKind::LambdaParameter { .. }
            | DefinitionKind::WithItem(_)
            | DefinitionKind::MatchPattern(_)
            | DefinitionKind::ExceptHandler(_)
            | DefinitionKind::TypeVar(_)
            | DefinitionKind::ParamSpec(_)
            | DefinitionKind::TypeVarTuple(_)
            | DefinitionKind::LoopHeader(_) => {
                // Not yet implemented
                return Err(());
            }
        };

        Ok(component)
    }
}

/// Information about a class in the type hierarchy.
#[derive(Debug, Clone)]
pub struct TypeHierarchyClass {
    /// The name of the class.
    pub name: Name,
    /// The file containing the class definition.
    pub file: ruff_db::files::File,
    /// The range covering the full class definition header.
    pub full_range: TextRange,
    /// The range of the class name (for selection/focus).
    pub selection_range: TextRange,
}

/// Return a type hierarchy item for the class type given.
///
/// When the type given doesn't correspond to a class literal, then this always
/// returns `None`.
///
/// This is meant to be used to "prepare" for a subtype or supertype request.
/// That is, this effectively validates whether the given type can be used in
/// subsequent requests for supertypes or subtypes.
pub fn type_hierarchy_prepare(db: &dyn Db, ty: Type<'_>) -> Option<TypeHierarchyClass> {
    let class_literal = extract_class_literal(db, ty)?;
    Some(class_literal_to_hierarchy_info(db, class_literal))
}

/// Get the direct base classes for the class type given.
///
/// When the type given doesn't correspond to a class literal, then this always
/// returns an empty sequence.
///
/// This includes `object` when the given class has no direct base classes.
pub fn type_hierarchy_supertypes(db: &dyn Db, ty: Type<'_>) -> Vec<TypeHierarchyClass> {
    let Some(class_literal) = extract_class_literal(db, ty) else {
        return vec![];
    };
    if class_literal.is_known(db, KnownClass::Object) {
        return vec![];
    }

    let mut supertypes: Vec<TypeHierarchyClass> = class_literal
        .explicit_bases(db)
        .into_iter()
        .filter_map(|base| extract_class_literal(db, base))
        .map(|class_literal| class_literal_to_hierarchy_info(db, class_literal))
        .collect();
    // Every class implicitly inherits from `object` when no explicit
    // bases are declared.
    if supertypes.is_empty() {
        supertypes.push(class_literal_to_hierarchy_info(
            db,
            ClassLiteral::object(db),
        ));
    }
    supertypes
}

/// Get the direct subtypes of the class given.
///
/// When the type given doesn't correspond to a class literal, then this always
/// returns an empty sequence.
///
/// Note that this scans all modules in `db` to find classes that directly
/// inherit from the given class. This could be quite expensive in large
/// projects.
pub fn type_hierarchy_subtypes(db: &dyn Db, ty: Type<'_>) -> Vec<TypeHierarchyClass> {
    let Some(target_class) = extract_class_literal(db, ty) else {
        return vec![];
    };
    let target_name = target_class.name(db);
    let target_is_object = target_class.is_known(db, KnownClass::Object);
    let mut subtypes = vec![];

    // Scan all modules in the workspace
    for module in ty_module_resolver::all_modules(db) {
        let Some(file) = module.file(db) else {
            continue;
        };

        // Note that this will always consider namespace
        // packages to be "not firsty party." This isn't
        // necessarily correct, and we can probably improve
        // on this in response to user feedback.
        let is_non_first_party = module.search_path(db).is_none_or(|sp| !sp.is_first_party());
        let name = module.name(db);
        // Filter out non-first-party modules that are conventionally
        // regarded as private or tests.
        if is_non_first_party && (name.is_private() || name.is_test_module()) {
            continue;
        }

        // Skip files that don't contain the class name. This avoids expensive
        // semantic analysis for files that can't possibly contain a subclass
        // of the target. We can't do this when looking for subtypes of
        // `object` since `object` can be implicit.
        if !target_is_object && !source_text(db, file).contains(target_name.as_str()) {
            continue;
        }

        let index = semantic_index(db, file);
        for scope_id in index.scope_ids() {
            let scope = scope_id.node(db);
            let Some(class_node) = scope.as_class() else {
                continue;
            };

            let def = index.expect_single_definition(class_node);
            if !matches!(def.kind(db), DefinitionKind::Class(_)) {
                continue;
            }

            let file_scope_id = scope_id.file_scope_id(db);
            let parsed = parsed_module(db, file).load(db);
            if !is_range_reachable(db, index, file_scope_id, class_node.node(&parsed).range()) {
                continue;
            }

            let ty = crate::types::binding_type(db, def);
            let Some(class_ty) = extract_class_literal(db, ty) else {
                continue;
            };

            let bases = class_ty.explicit_bases(db);
            let is_subtype = if target_is_object
                && bases.is_empty()
                && !class_ty.is_known(db, KnownClass::Object)
            {
                true
            } else {
                bases.iter().any(|base| {
                    extract_class_literal(db, *base)
                        .is_some_and(|base_literal| base_literal == target_class)
                })
            };
            if is_subtype {
                subtypes.push(class_literal_to_hierarchy_info(db, class_ty));
            }
        }
    }
    subtypes
}

/// Extract a `ClassLiteral` from a `Type`, handling various type forms.
fn extract_class_literal<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<ClassLiteral<'db>> {
    match ty {
        Type::ClassLiteral(class_literal) => Some(class_literal),
        Type::SubclassOf(subclass_of) => {
            let inner = subclass_of.subclass_of();
            match inner {
                crate::types::SubclassOfInner::Class(class_type) => {
                    Some(class_type.class_literal(db))
                }
                crate::types::SubclassOfInner::Dynamic(_)
                | crate::types::SubclassOfInner::TypeVar(_) => None,
            }
        }
        Type::GenericAlias(generic_alias) => Some(ClassLiteral::Static(generic_alias.origin(db))),
        Type::NominalInstance(instance) => Some(instance.class(db).class_literal(db)),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .find_map(|elem| extract_class_literal(db, *elem)),

        _ => None,
    }
}

/// Convert a `ClassLiteral` to `TypeHierarchyClass` info.
///
/// For the most part, this is about extracting the right
/// text ranges.
fn class_literal_to_hierarchy_info(
    db: &dyn Db,
    class_literal: ClassLiteral<'_>,
) -> TypeHierarchyClass {
    let name = class_literal.name(db).clone();
    let file = class_literal.file(db);

    let (full_range, selection_range) = match class_literal {
        ClassLiteral::Static(static_class) => {
            let parsed = parsed_module(db, file).load(db);
            let header_range = static_class.header_range(db);
            let body_scope = static_class.body_scope(db);

            let selection_range = body_scope
                .node(db)
                .as_class()
                .map(|c| c.node(&parsed))
                .map(|class_def| class_def.name.range())
                .unwrap_or(header_range);
            (header_range, selection_range)
        }
        // For the dynamic cases, we special case a variable definition
        // like this:
        //
        //     Dynamic = type("Dynamic", (object,), {})
        //
        // In this case, the range for the element we return will correspond to
        // the left hand side of the variable assignment. This works better as
        // an "anchor" point because it avoids ambiguity with asking for the
        // type hierarchy of `type` itself.
        //
        // If there is not a variable definition, then we fall back to the
        // class definition's "header" range, which will be the `type` (or
        // `namedtuple`) call. Subsequent type hierarchy requests will then
        // (likely incorrectly) return the type hierarchy for `type` itself.
        ClassLiteral::Dynamic(dynamic_class) => {
            if let DynamicClassAnchor::Definition(definition) = dynamic_class.anchor(db) {
                let parsed = parsed_module(db, file).load(db);
                let kind = definition.kind(db);
                (kind.full_range(&parsed), kind.target_range(&parsed))
            } else {
                let header_range = dynamic_class.header_range(db);
                (header_range, header_range)
            }
        }
        ClassLiteral::DynamicNamedTuple(namedtuple) => {
            if let DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
            | DynamicNamedTupleAnchor::TypingDefinition(definition) = namedtuple.anchor(db)
            {
                let parsed = parsed_module(db, file).load(db);
                let kind = definition.kind(db);
                (kind.full_range(&parsed), kind.target_range(&parsed))
            } else {
                let header_range = namedtuple.header_range(db);
                (header_range, header_range)
            }
        }
        ClassLiteral::DynamicTypedDict(typeddict) => {
            let header_range = typeddict.header_range(db);
            (header_range, header_range)
        }
        ClassLiteral::DynamicEnum(dynamic_enum) => {
            if let DynamicEnumAnchor::Definition { definition, .. } = dynamic_enum.anchor(db) {
                let parsed = parsed_module(db, file).load(db);
                let kind = definition.kind(db);
                (kind.full_range(&parsed), kind.target_range(&parsed))
            } else {
                let header_range = dynamic_enum.header_range(db);
                (header_range, header_range)
            }
        }
    };

    TypeHierarchyClass {
        name,
        file,
        full_range,
        selection_range,
    }
}

pub fn constructor_signature(model: &SemanticModel, call_expr: &ast::ExprCall) -> Option<String> {
    let function_ty = call_expr.func.inferred_type(model)?;
    let db = model.db();
    let class_name = function_ty.as_class_literal()?.name(db);
    let display_sig = |signature: &Signature| {
        let params = signature
            .display_with(
                db,
                DisplaySettings::default()
                    .multiline()
                    .disallow_signature_name()
                    .hide_return_type(),
            )
            .to_string();

        format!("class {class_name}{params}")
    };
    let callable_type = function_ty.try_upcast_to_callable(db)?.into_type(db);
    let bindings = callable_type.bindings(db);

    if let Some(binding) = bindings.single_element()
        && binding.overloads().len() == 1
    {
        return binding
            .overloads()
            .first()
            .map(|overload| display_sig(&overload.signature));
    }

    if let Some(signature) = resolve_single_overload(model, callable_type, call_expr) {
        return Some(display_sig(&signature));
    }

    let all_sigs: Vec<String> = bindings
        .iter_flat()
        .flatten()
        .map(|binding| display_sig(&binding.signature))
        .collect();

    if all_sigs.is_empty() {
        None
    } else {
        Some(all_sigs.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::{CallArgumentForm, call_argument_forms};
    use crate::SemanticModel;
    use crate::db::tests::TestDbBuilder;
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;

    #[test]
    fn keyword_call_argument_forms_follow_source_order() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing import cast

cast(val="", typ=int)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let call = parsed
            .suite()
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let model = SemanticModel::new(&db, file);

        assert_eq!(
            call_argument_forms(&model, call),
            [CallArgumentForm::Value, CallArgumentForm::Type]
        );

        Ok(())
    }

    #[test]
    fn overloaded_call_argument_forms_follow_source_order() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing import overload

@overload
def f(x: int, y: str) -> None: ...
@overload
def f(x: str) -> None: ...
def f(*args, **kwargs): ...

f(y="", x=1)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let call = parsed
            .suite()
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let model = SemanticModel::new(&db, file);

        assert_eq!(
            call_argument_forms(&model, call),
            [CallArgumentForm::Value, CallArgumentForm::Value]
        );

        Ok(())
    }

    #[test]
    fn conditional_special_forms_preserve_type_form_information() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing_extensions import assert_type, cast

flag = bool(input())
f = cast if flag else assert_type
f(val="", typ=int)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let call = parsed
            .suite()
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let model = SemanticModel::new(&db, file);

        assert_eq!(
            call_argument_forms(&model, call),
            [CallArgumentForm::Value, CallArgumentForm::Type]
        );

        Ok(())
    }

    #[test]
    fn conditional_special_forms_degrade_to_unknown_for_positional_arguments() -> anyhow::Result<()>
    {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing_extensions import assert_type, cast

flag = bool(input())
f = cast if flag else assert_type
f("", int)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let call = parsed
            .suite()
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let model = SemanticModel::new(&db, file);

        assert_eq!(
            call_argument_forms(&model, call),
            [CallArgumentForm::Unknown, CallArgumentForm::Unknown]
        );

        Ok(())
    }

    #[test]
    fn successful_call_argument_forms_ignore_failed_bindings() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing import cast

flag = bool(input())
def g(x):
    return x

x = ""
f = cast if flag else g
f(int, x)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let call = parsed
            .suite()
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let model = SemanticModel::new(&db, file);

        assert_eq!(
            call_argument_forms(&model, call),
            [CallArgumentForm::Type, CallArgumentForm::Value]
        );

        Ok(())
    }

    #[test]
    fn call_argument_forms_fast_path_value_only_signatures() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing import cast

def f(x: type[int], y: int) -> None:
    pass

cast(int, 1)
f(int, 1)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let calls: Vec<_> = parsed
            .suite()
            .iter()
            .filter_map(|stmt| stmt.as_expr_stmt()?.value.as_call_expr())
            .collect();
        let model = SemanticModel::new(&db, file);

        assert_eq!(calls.len(), 2);
        assert_eq!(
            call_argument_forms(&model, calls[0]),
            [CallArgumentForm::Type, CallArgumentForm::Value]
        );
        assert_eq!(
            call_argument_forms(&model, calls[1]),
            [CallArgumentForm::Value, CallArgumentForm::Value]
        );

        Ok(())
    }

    #[test]
    fn variadic_call_argument_forms_are_unknown_when_matched_to_multiple_parameters()
    -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_file(
                "/src/foo.py",
                r#"
from typing import cast

args: tuple[str, type[int]] = ("", int)
cast(*args)
"#,
            )
            .build()?;

        let file = system_path_to_file(&db, "/src/foo.py").unwrap();
        let parsed = parsed_module(&db, file).load(&db);
        let call = parsed
            .suite()
            .last()
            .unwrap()
            .as_expr_stmt()
            .unwrap()
            .value
            .as_call_expr()
            .unwrap();
        let model = SemanticModel::new(&db, file);

        assert_eq!(
            call_argument_forms(&model, call),
            [CallArgumentForm::Unknown]
        );

        Ok(())
    }
}
