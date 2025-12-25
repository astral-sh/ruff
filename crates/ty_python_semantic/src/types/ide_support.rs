use std::collections::HashMap;

use crate::FxIndexSet;
use crate::place::builtins_module_scope;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::definition::DefinitionKind;
use crate::semantic_index::{attribute_scopes, global_scope, semantic_index, use_def_map};
use crate::types::call::{CallArguments, MatchedArgument};
use crate::types::signatures::{ParameterKind, Signature};
use crate::types::{
    CallDunderError, CallableTypes, ClassBase, KnownUnion, Type, TypeContext, UnionType,
};
use crate::{Db, DisplaySettings, HasType, SemanticModel};
use ruff_db::files::FileRange;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

pub use resolve_definition::{ImportAliasResolution, ResolvedDefinition, map_stub_definition};
use resolve_definition::{find_symbol_in_scope, resolve_definition};

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

        // TODO: The current algorithm doesn't return definintions or bindings
        // for other scopes that are outside of this scope hierarchy that target
        // this name using a nonlocal or global binding. The semantic analyzer
        // doesn't appear to track these in a way that we can easily access
        // them from here without walking all scopes in the module.

        // If marked as global, skip to global scope
        if is_global {
            let global_scope_id = global_scope(db, file);
            let global_place_table = crate::semantic_index::place_table(db, global_scope_id);

            if let Some(global_symbol_id) = global_place_table.symbol_id(name_str) {
                let global_use_def_map = crate::semantic_index::use_def_map(db, global_scope_id);
                let global_bindings =
                    global_use_def_map.reachable_symbol_bindings(global_symbol_id);
                let global_declarations =
                    global_use_def_map.reachable_symbol_declarations(global_symbol_id);

                for binding in global_bindings {
                    if let Some(def) = binding.binding.definition() {
                        all_definitions.insert(def);
                    }
                }

                for declaration in global_declarations {
                    if let Some(def) = declaration.declaration.definition() {
                        all_definitions.insert(def);
                    }
                }
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
        let bindings = use_def_map.reachable_symbol_bindings(symbol_id);
        let declarations = use_def_map.reachable_symbol_declarations(symbol_id);

        for binding in bindings {
            if let Some(def) = binding.binding.definition() {
                all_definitions.insert(def);
            }
        }

        for declaration in declarations {
            if let Some(def) = declaration.declaration.definition() {
                all_definitions.insert(def);
            }
        }

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
                .map(|instance| {
                    let definition = instance.class_literal(db).definition(db);
                    ResolvedDefinition::Definition(definition)
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
        Type::Union(union) => union.elements(model.db()).to_vec(),
        _ => vec![lhs_ty],
    };

    // Expand intersections for each subtype into their components
    let expanded_tys = tys
        .into_iter()
        .flat_map(|ty| match ty {
            Type::Intersection(intersection) => intersection.positive(db).iter().copied().collect(),
            _ => vec![ty],
        })
        .collect::<Vec<_>>();

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

        // First, transform the type to its meta type, unless it's already a class-like type.
        let meta_type = match ty {
            Type::ClassLiteral(_) | Type::SubclassOf(_) | Type::GenericAlias(_) => ty,
            _ => ty.to_meta_type(db),
        };
        let class_literal = match meta_type {
            Type::ClassLiteral(class_literal) => class_literal,
            Type::SubclassOf(subclass) => match subclass.subclass_of().into_class(db) {
                Some(cls) => cls.class_literal(db).0,
                None => continue,
            },
            _ => continue,
        };

        // Walk the MRO: include class and its ancestors, but stop when we find a match
        'scopes: for ancestor in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|cls| cls.class_literal(db).0)
        {
            let class_scope = ancestor.body_scope(db);
            let class_place_table = crate::semantic_index::place_table(db, class_scope);

            // Look for class-level declarations and bindings
            if let Some(place_id) = class_place_table.symbol_id(name_str) {
                let use_def = use_def_map(db, class_scope);

                // Check declarations first
                for decl in use_def.reachable_symbol_declarations(place_id) {
                    if let Some(def) = decl.declaration.definition() {
                        resolved.extend(resolve_definition(
                            db,
                            def,
                            Some(name_str),
                            ImportAliasResolution::ResolveAliases,
                        ));
                        break 'scopes;
                    }
                }

                // If no declarations found, check bindings
                for binding in use_def.reachable_symbol_bindings(place_id) {
                    if let Some(def) = binding.binding.definition() {
                        resolved.extend(resolve_definition(
                            db,
                            def,
                            Some(name_str),
                            ImportAliasResolution::ResolveAliases,
                        ));
                        break 'scopes;
                    }
                }
            }

            // Look for instance attributes in method scopes (e.g., self.x = 1)
            let file = class_scope.file(db);
            let index = semantic_index(db, file);

            for function_scope_id in attribute_scopes(db, class_scope) {
                if let Some(place_id) = index
                    .place_table(function_scope_id)
                    .member_id_by_instance_attribute_name(name_str)
                {
                    let use_def = index.use_def_map(function_scope_id);

                    // Check declarations first
                    for decl in use_def.reachable_member_declarations(place_id) {
                        if let Some(def) = decl.declaration.definition() {
                            resolved.extend(resolve_definition(
                                db,
                                def,
                                Some(name_str),
                                ImportAliasResolution::ResolveAliases,
                            ));
                            break 'scopes;
                        }
                    }

                    // If no declarations found, check bindings
                    for binding in use_def.reachable_member_bindings(place_id) {
                        if let Some(def) = binding.binding.definition() {
                            resolved.extend(resolve_definition(
                                db,
                                def,
                                Some(name_str),
                                ImportAliasResolution::ResolveAliases,
                            ));
                            break 'scopes;
                        }
                    }
                }
            }

            // TODO: Add support for metaclass attribute lookups
        }
    }

    resolved
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
            if let Some((_param_index, _param)) =
                signature.parameters().keyword_by_name(keyword_name_str)
            {
                if let Some(function_definition) = signature.definition() {
                    let function_file = function_definition.file(db);
                    let module = parsed_module(db, function_file).load(db);
                    let def_kind = function_definition.kind(db);

                    if let DefinitionKind::Function(function_ast_ref) = def_kind {
                        let function_node = function_ast_ref.node(&module);

                        if let Some(parameter_range) =
                            find_parameter_range(&function_node.parameters, keyword_name_str)
                        {
                            resolved_definitions.push(ResolvedDefinition::FileWithRange(
                                FileRange::new(function_file, parameter_range),
                            ));
                        }
                    }
                }
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

/// Details about a callable signature for IDE support.
#[derive(Debug, Clone)]
pub struct CallSignatureDetails<'db> {
    /// The signature itself
    pub signature: Signature<'db>,

    /// The display label for this signature (e.g., "(param1: str, param2: int) -> str")
    pub label: String,

    /// Label offsets for each parameter in the signature string.
    /// Each range specifies the start position and length of a parameter label
    /// within the full signature string.
    pub parameter_label_offsets: Vec<TextRange>,

    /// The names of the parameters in the signature, in order.
    /// This provides easy access to parameter names for documentation lookup.
    pub parameter_names: Vec<String>,

    /// Parameter kinds, useful to determine correct autocomplete suggestions.
    pub parameter_kinds: Vec<ParameterKind<'db>>,

    /// Parameter kinds, useful to determine correct autocomplete suggestions.
    pub parameter_types: Vec<Option<Type<'db>>>,

    /// The definition where this callable was originally defined (useful for
    /// extracting docstrings).
    pub definition: Option<Definition<'db>>,

    /// Mapping from argument indices to parameter indices. This helps
    /// determine which parameter corresponds to which argument position.
    pub argument_to_parameter_mapping: Vec<MatchedArgument<'db>>,
}

impl CallSignatureDetails<'_> {
    fn get_definition_parameter_range(&self, db: &dyn Db, name: &str) -> Option<FileRange> {
        let definition = self.signature.definition()?;
        let file = definition.file(db);
        let module_ref = parsed_module(db, file).load(db);

        let parameters = match definition.kind(db) {
            DefinitionKind::Function(node) => &node.node(&module_ref).parameters,
            // TODO: lambda functions
            _ => return None,
        };

        Some(FileRange::new(file, parameters.find(name)?.name().range))
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

    // Use into_callable to handle all the complex type conversions
    if let Some(callable_type) = func_type
        .try_upcast_to_callable(model.db())
        .map(|callables| callables.into_type(model.db()))
    {
        let call_arguments =
            CallArguments::from_arguments(&call_expr.arguments, |_, splatted_value| {
                splatted_value
                    .inferred_type(model)
                    .unwrap_or(Type::unknown())
            });
        let bindings = callable_type
            .bindings(model.db())
            .match_parameters(model.db(), &call_arguments);

        // Extract signature details from all callable bindings
        bindings
            .into_iter()
            .flatten()
            .map(|binding| {
                let argument_to_parameter_mapping = binding.argument_matches().to_vec();
                let signature = binding.signature;
                let display_details = signature.display(model.db()).to_string_parts();
                let parameter_label_offsets = display_details.parameter_ranges;
                let parameter_names = display_details.parameter_names;
                let (parameter_kinds, parameter_types): (Vec<ParameterKind>, Vec<Option<Type>>) =
                    signature
                        .parameters()
                        .iter()
                        .map(|param| (param.kind().clone(), param.annotated_type()))
                        .unzip();

                CallSignatureDetails {
                    definition: signature.definition(),
                    signature,
                    label: display_details.label,
                    parameter_label_offsets,
                    parameter_names,
                    parameter_kinds,
                    parameter_types,
                    argument_to_parameter_mapping,
                }
            })
            .collect()
    } else {
        // Type is not callable, return empty signatures
        vec![]
    }
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

    // Use into_callable to handle all the complex type conversions
    let callable_type = func_type.try_upcast_to_callable(db)?.into_type(db);
    let bindings = callable_type.bindings(db);

    // If the callable is trivial this analysis is useless, bail out
    if let Some(binding) = bindings.single_element()
        && binding.overloads().len() < 2
    {
        return None;
    }

    // Hand the overload resolution system as much type info as we have
    let args = CallArguments::from_arguments_typed(&call_expr.arguments, |_, splatted_value| {
        splatted_value
            .inferred_type(model)
            .unwrap_or(Type::unknown())
    });

    // Try to resolve overloads with the arguments/types we have
    let mut resolved = bindings
        .match_parameters(db, &args)
        .check_types(db, &args, TypeContext::default(), &[])
        // Only use the Ok
        .iter()
        .flatten()
        .flat_map(|binding| {
            binding.matching_overloads().map(|(_, overload)| {
                overload
                    .signature
                    .display_with(db, DisplaySettings::default().multiline())
                    .to_string()
            })
        })
        .collect::<Vec<_>>();

    // If at the end of this we still got multiple signatures (or no signatures), give up
    if resolved.len() != 1 {
        return None;
    }

    resolved.pop()
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

    let callable_type = promote_literals_for_self(model.db(), bindings.callable_type());

    let definitions: Vec<_> = bindings
        .into_iter()
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
                    CallDunderError::PossiblyUnbound(bindings)
                    | CallDunderError::CallError(_, bindings),
                ) => *bindings,
            }
        }
        Err(CallDunderError::MethodNotAvailable) => return None,
        Err(
            CallDunderError::PossiblyUnbound(bindings) | CallDunderError::CallError(_, bindings),
        ) => *bindings,
    };

    let callable_type = promote_literals_for_self(model.db(), bindings.callable_type());

    let definitions = bindings
        .into_iter()
        .flatten()
        .filter_map(|binding| {
            Some(ResolvedDefinition::Definition(
                binding.signature.definition?,
            ))
        })
        .collect();

    Some((definitions, callable_type))
}

/// Promotes literal types in `self` positions to their fallback instance types.
///
/// This is so that we show e.g. `int.__add__` instead of `Literal[4].__add__`.
fn promote_literals_for_self<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
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
    let signature_details = call_signature_details(model, call_expr);

    if signature_details.is_empty() {
        return None;
    }

    let active_signature_index = find_active_signature_from_details(&signature_details)?;

    let call_signature_details = signature_details.get(active_signature_index)?;

    let parameters = call_signature_details.signature.parameters();

    let mut argument_names = HashMap::new();

    for arg_index in 0..call_expr.arguments.args.len() {
        let Some(arg_mapping) = call_signature_details
            .argument_to_parameter_mapping
            .get(arg_index)
        else {
            continue;
        };

        if !arg_mapping.matched {
            continue;
        }

        let Some(param_index) = arg_mapping.parameters.first() else {
            continue;
        };

        let Some(param) = parameters.get(*param_index) else {
            continue;
        };

        let parameter_label_offset =
            call_signature_details.get_definition_parameter_range(db, param.name()?);

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

/// Find the text range of a specific parameter in function parameters by name.
/// Only searches for parameters that can be addressed by name in keyword arguments.
fn find_parameter_range(parameters: &ast::Parameters, parameter_name: &str) -> Option<TextRange> {
    // Check regular positional and keyword-only parameters
    parameters
        .args
        .iter()
        .chain(&parameters.kwonlyargs)
        .find(|param| param.parameter.name.as_str() == parameter_name)
        .map(|param| param.parameter.name.range())
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
    use crate::semantic_index::definition::{Definition, DefinitionKind, module_docstring};
    use crate::semantic_index::scope::{NodeWithScopeKind, ScopeId};
    use crate::semantic_index::{global_scope, place_table, semantic_index, use_def_map};

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
        pub(crate) fn definition(&self) -> Option<Definition<'db>> {
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
        if definitions_in_module.is_empty() {
            // This might be importing a submodule, try that
            return Vec::from_iter(resolve_from_import_submodule_definitions(
                db,
                file,
                symbol_name,
                module_name,
            ));
        }

        let mut resolved_definitions = Vec::new();
        for def in definitions_in_module {
            let resolved =
                resolve_definition_recursive(db, def, visited, Some(symbol_name), alias_resolution);
            resolved_definitions.extend(resolved);
        }
        resolved_definitions
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
            | DefinitionKind::For(_)
            | DefinitionKind::Comprehension(_)
            | DefinitionKind::VariadicPositionalParameter(_)
            | DefinitionKind::VariadicKeywordParameter(_)
            | DefinitionKind::Parameter(_)
            | DefinitionKind::WithItem(_)
            | DefinitionKind::MatchPattern(_)
            | DefinitionKind::ExceptHandler(_)
            | DefinitionKind::TypeVar(_)
            | DefinitionKind::ParamSpec(_)
            | DefinitionKind::TypeVarTuple(_) => {
                // Not yet implemented
                return Err(());
            }
        };

        Ok(component)
    }
}
