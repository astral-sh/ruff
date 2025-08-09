use std::cmp::Ordering;

use crate::place::{
    Place, builtins_module_scope, imported_symbol, place_from_bindings, place_from_declarations,
};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::definition::DefinitionKind;
use crate::semantic_index::scope::ScopeId;
use crate::semantic_index::{
    attribute_scopes, global_scope, place_table, semantic_index, use_def_map,
};
use crate::types::call::{CallArguments, MatchedArgument};
use crate::types::signatures::Signature;
use crate::types::{ClassBase, ClassLiteral, DynamicType, KnownClass, KnownInstanceType, Type};
use crate::{Db, HasType, NameKind, SemanticModel};
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

pub use resolve_definition::{ImportAliasResolution, ResolvedDefinition, map_stub_definition};
use resolve_definition::{find_symbol_in_scope, resolve_definition};

pub(crate) fn all_declarations_and_bindings<'db>(
    db: &'db dyn Db,
    scope_id: ScopeId<'db>,
) -> impl Iterator<Item = Member<'db>> + 'db {
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    use_def_map
        .all_end_of_scope_symbol_declarations()
        .filter_map(move |(symbol_id, declarations)| {
            place_from_declarations(db, declarations)
                .ok()
                .and_then(|result| {
                    result.place.ignore_possibly_unbound().map(|ty| {
                        let symbol = table.symbol(symbol_id);
                        Member {
                            name: symbol.name().clone(),
                            ty,
                        }
                    })
                })
        })
        .chain(use_def_map.all_end_of_scope_symbol_bindings().filter_map(
            move |(symbol_id, bindings)| {
                place_from_bindings(db, bindings)
                    .ignore_possibly_unbound()
                    .map(|ty| {
                        let symbol = table.symbol(symbol_id);
                        Member {
                            name: symbol.name().clone(),
                            ty,
                        }
                    })
            },
        ))
}

struct AllMembers<'db> {
    members: FxHashSet<Member<'db>>,
}

impl<'db> AllMembers<'db> {
    fn of(db: &'db dyn Db, ty: Type<'db>) -> Self {
        let mut all_members = Self {
            members: FxHashSet::default(),
        };
        all_members.extend_with_type(db, ty);
        all_members
    }

    fn extend_with_type(&mut self, db: &'db dyn Db, ty: Type<'db>) {
        match ty {
            Type::Union(union) => self.members.extend(
                union
                    .elements(db)
                    .iter()
                    .map(|ty| AllMembers::of(db, *ty).members)
                    .reduce(|acc, members| acc.intersection(&members).cloned().collect())
                    .unwrap_or_default(),
            ),

            Type::Intersection(intersection) => self.members.extend(
                intersection
                    .positive(db)
                    .iter()
                    .map(|ty| AllMembers::of(db, *ty).members)
                    .reduce(|acc, members| acc.union(&members).cloned().collect())
                    .unwrap_or_default(),
            ),

            Type::NominalInstance(instance) => {
                let (class_literal, _specialization) = instance.class.class_literal(db);
                self.extend_with_instance_members(db, ty, class_literal);
            }

            Type::ClassLiteral(class_literal) if class_literal.is_typed_dict(db) => {
                self.extend_with_type(db, KnownClass::TypedDictFallback.to_class_literal(db));
            }

            Type::GenericAlias(generic_alias) if generic_alias.is_typed_dict(db) => {
                self.extend_with_type(db, KnownClass::TypedDictFallback.to_class_literal(db));
            }

            Type::SubclassOf(subclass_of_type) if subclass_of_type.is_typed_dict(db) => {
                self.extend_with_type(db, KnownClass::TypedDictFallback.to_class_literal(db));
            }

            Type::ClassLiteral(class_literal) => {
                self.extend_with_class_members(db, ty, class_literal);

                if let Type::ClassLiteral(meta_class_literal) = ty.to_meta_type(db) {
                    self.extend_with_class_members(db, ty, meta_class_literal);
                }
            }

            Type::GenericAlias(generic_alias) => {
                let class_literal = generic_alias.origin(db);
                self.extend_with_class_members(db, ty, class_literal);
            }

            Type::SubclassOf(subclass_of_type) => {
                if let Some(class_literal) = subclass_of_type.subclass_of().into_class() {
                    self.extend_with_class_members(db, ty, class_literal.class_literal(db).0);
                }
            }

            Type::Dynamic(_) | Type::Never | Type::AlwaysTruthy | Type::AlwaysFalsy => {}

            Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::LiteralString
            | Type::Tuple(_)
            | Type::PropertyInstance(_)
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::MethodWrapper(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_)
            | Type::ProtocolInstance(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::TypeVar(_)
            | Type::BoundSuper(_)
            | Type::TypeIs(_) => match ty.to_meta_type(db) {
                Type::ClassLiteral(class_literal) => {
                    self.extend_with_class_members(db, ty, class_literal);
                }
                Type::SubclassOf(subclass_of) => {
                    if let Some(class) = subclass_of.subclass_of().into_class() {
                        self.extend_with_class_members(db, ty, class.class_literal(db).0);
                    }
                }
                Type::GenericAlias(generic_alias) => {
                    let class_literal = generic_alias.origin(db);
                    self.extend_with_class_members(db, ty, class_literal);
                }
                _ => {}
            },

            Type::TypedDict(_) => {
                if let Type::ClassLiteral(class_literal) = ty.to_meta_type(db) {
                    self.extend_with_class_members(db, ty, class_literal);
                }

                if let Type::ClassLiteral(class) =
                    KnownClass::TypedDictFallback.to_class_literal(db)
                {
                    self.extend_with_instance_members(db, ty, class);
                }
            }

            Type::ModuleLiteral(literal) => {
                self.extend_with_type(db, KnownClass::ModuleType.to_instance(db));
                let module = literal.module(db);

                let Some(file) = module.file(db) else {
                    return;
                };

                let module_scope = global_scope(db, file);
                let use_def_map = use_def_map(db, module_scope);
                let place_table = place_table(db, module_scope);

                for (symbol_id, _) in use_def_map.all_end_of_scope_symbol_declarations() {
                    let symbol_name = place_table.symbol(symbol_id).name();
                    let Place::Type(ty, _) = imported_symbol(db, file, symbol_name, None).place
                    else {
                        continue;
                    };

                    // Filter private symbols from stubs if they appear to be internal types
                    let is_stub_file = file.path(db).extension() == Some("pyi");
                    let is_private_symbol = match NameKind::classify(symbol_name) {
                        NameKind::Dunder | NameKind::Normal => false,
                        NameKind::Sunder => true,
                    };
                    if is_private_symbol && is_stub_file {
                        match ty {
                            Type::NominalInstance(instance)
                                if matches!(
                                    instance.class.known(db),
                                    Some(
                                        KnownClass::TypeVar
                                            | KnownClass::TypeVarTuple
                                            | KnownClass::ParamSpec
                                            | KnownClass::UnionType
                                    )
                                ) =>
                            {
                                continue;
                            }
                            Type::ClassLiteral(class) if class.is_protocol(db) => continue,
                            Type::KnownInstance(
                                KnownInstanceType::TypeVar(_) | KnownInstanceType::TypeAliasType(_),
                            ) => continue,
                            Type::Dynamic(DynamicType::TodoTypeAlias) => continue,
                            _ => {}
                        }
                    }

                    self.members.insert(Member {
                        name: symbol_name.clone(),
                        ty,
                    });
                }

                self.members
                    .extend(literal.available_submodule_attributes(db).filter_map(
                        |submodule_name| {
                            let ty = literal.resolve_submodule(db, &submodule_name)?;
                            let name = submodule_name.clone();
                            Some(Member { name, ty })
                        },
                    ));
            }
        }
    }

    /// Add members from `class_literal` (including following its
    /// parent classes).
    ///
    /// `ty` should be the original type that we're adding members for.
    /// For example, in:
    ///
    /// ```text
    /// class Meta(type):
    ///     @property
    ///     def meta_attr(self) -> int:
    ///         return 0
    ///
    /// class C(metaclass=Meta): ...
    ///
    /// C.<CURSOR>
    /// ```
    ///
    /// then `class_literal` might be `Meta`, but `ty` should be the
    /// type of `C`. This ensures that the descriptor protocol is
    /// correctly used (or not used) to get the type of each member of
    /// `C`.
    fn extend_with_class_members(
        &mut self,
        db: &'db dyn Db,
        ty: Type<'db>,
        class_literal: ClassLiteral<'db>,
    ) {
        for parent in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|class| class.class_literal(db).0)
        {
            let parent_scope = parent.body_scope(db);
            for Member { name, .. } in all_declarations_and_bindings(db, parent_scope) {
                let result = ty.member(db, name.as_str());
                let Some(ty) = result.place.ignore_possibly_unbound() else {
                    continue;
                };
                self.members.insert(Member { name, ty });
            }
        }
    }

    fn extend_with_instance_members(
        &mut self,
        db: &'db dyn Db,
        ty: Type<'db>,
        class_literal: ClassLiteral<'db>,
    ) {
        for parent in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|class| class.class_literal(db).0)
        {
            let class_body_scope = parent.body_scope(db);
            let file = class_body_scope.file(db);
            let index = semantic_index(db, file);
            for function_scope_id in attribute_scopes(db, class_body_scope) {
                let place_table = index.place_table(function_scope_id);
                for place_expr in place_table.members() {
                    let Some(name) = place_expr.as_instance_attribute() else {
                        continue;
                    };
                    let result = ty.member(db, name);
                    let Some(ty) = result.place.ignore_possibly_unbound() else {
                        continue;
                    };
                    self.members.insert(Member {
                        name: Name::new(name),
                        ty,
                    });
                }
            }

            // This is very similar to `extend_with_class_members`,
            // but uses the type of the class instance to query the
            // class member. This gets us the right type for each
            // member, e.g., `SomeClass.__delattr__` is not a bound
            // method, but `instance_of_SomeClass.__delattr__` is.
            for Member { name, .. } in all_declarations_and_bindings(db, class_body_scope) {
                let result = ty.member(db, name.as_str());
                let Some(ty) = result.place.ignore_possibly_unbound() else {
                    continue;
                };
                self.members.insert(Member { name, ty });
            }
        }
    }
}

/// A member of a type.
///
/// This represents a single item in (ideally) the list returned by
/// `dir(object)`.
///
/// The equality, comparison and hashing traits implemented for
/// this type are done so by taking only the name into account. At
/// present, this is because we assume the name is enough to uniquely
/// identify each attribute on an object. This is perhaps complicated
/// by overloads, but they only get represented by one member for
/// now. Moreover, it is convenient to be able to sort collections of
/// members, and a `Type` currently (as of 2025-07-09) has no way to do
/// ordered comparisons.
#[derive(Clone, Debug)]
pub struct Member<'db> {
    pub name: Name,
    pub ty: Type<'db>,
}

impl std::hash::Hash for Member<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Eq for Member<'_> {}

impl<'db> PartialEq for Member<'db> {
    fn eq(&self, rhs: &Member<'db>) -> bool {
        self.name == rhs.name
    }
}

impl<'db> Ord for Member<'db> {
    fn cmp(&self, rhs: &Member<'db>) -> Ordering {
        self.name.cmp(&rhs.name)
    }
}

impl<'db> PartialOrd for Member<'db> {
    fn partial_cmp(&self, rhs: &Member<'db>) -> Option<Ordering> {
        Some(self.cmp(rhs))
    }
}

/// List all members of a given type: anything that would be valid when accessed
/// as an attribute on an object of the given type.
pub fn all_members<'db>(db: &'db dyn Db, ty: Type<'db>) -> FxHashSet<Member<'db>> {
    AllMembers::of(db, ty).members
}

/// Get the primary definition kind for a name expression within a specific file.
/// Returns the first definition kind that is reachable for this name in its scope.
/// This is useful for IDE features like semantic tokens.
pub fn definition_kind_for_name<'db>(
    db: &'db dyn Db,
    file: File,
    name: &ast::ExprName,
) -> Option<DefinitionKind<'db>> {
    let index = semantic_index(db, file);
    let name_str = name.id.as_str();

    // Get the scope for this name expression
    let file_scope = index.expression_scope_id(&ast::ExprRef::from(name));

    // Get the place table for this scope
    let place_table = index.place_table(file_scope);

    // Look up the place by name
    let symbol_id = place_table.symbol_id(name_str)?;

    // Get the use-def map and look up definitions for this place
    let use_def_map = index.use_def_map(file_scope);
    let declarations = use_def_map.all_reachable_symbol_declarations(symbol_id);

    // Find the first valid definition and return its kind
    for declaration in declarations {
        if let Some(def) = declaration.declaration.definition() {
            return Some(def.kind(db).clone());
        }
    }

    None
}

/// Returns all definitions for a name. If any definitions are imports, they
/// are resolved (recursively) to the original definitions or module files.
pub fn definitions_for_name<'db>(
    db: &'db dyn Db,
    file: File,
    name: &ast::ExprName,
) -> Vec<ResolvedDefinition<'db>> {
    let index = semantic_index(db, file);
    let name_str = name.id.as_str();

    // Get the scope for this name expression
    let file_scope = index.expression_scope_id(&ast::ExprRef::from(name));

    let mut all_definitions = Vec::new();

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
                    global_use_def_map.all_reachable_symbol_bindings(global_symbol_id);
                let global_declarations =
                    global_use_def_map.all_reachable_symbol_declarations(global_symbol_id);

                for binding in global_bindings {
                    if let Some(def) = binding.binding.definition() {
                        all_definitions.push(def);
                    }
                }

                for declaration in global_declarations {
                    if let Some(def) = declaration.declaration.definition() {
                        all_definitions.push(def);
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
        let bindings = use_def_map.all_reachable_symbol_bindings(symbol_id);
        let declarations = use_def_map.all_reachable_symbol_declarations(symbol_id);

        for binding in bindings {
            if let Some(def) = binding.binding.definition() {
                all_definitions.push(def);
            }
        }

        for declaration in declarations {
            if let Some(def) = declaration.declaration.definition() {
                all_definitions.push(def);
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
        let resolved = resolve_definition(
            db,
            *definition,
            Some(name_str),
            ImportAliasResolution::ResolveAliases,
        );
        resolved_definitions.extend(resolved);
    }

    // If we didn't find any definitions in scopes, fallback to builtins
    if resolved_definitions.is_empty() {
        let Some(builtins_scope) = builtins_module_scope(db) else {
            return Vec::new();
        };
        find_symbol_in_scope(db, builtins_scope, name_str)
            .into_iter()
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
    db: &'db dyn Db,
    file: File,
    attribute: &ast::ExprAttribute,
) -> Vec<ResolvedDefinition<'db>> {
    let name_str = attribute.attr.as_str();
    let model = SemanticModel::new(db, file);

    let mut resolved = Vec::new();

    // Determine the type of the LHS
    let lhs_ty = attribute.value.inferred_type(&model);
    let tys = match lhs_ty {
        Type::Union(union) => union.elements(db).to_vec(),
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
            Type::SubclassOf(subclass) => match subclass.subclass_of().into_class() {
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
                for decl in use_def.all_reachable_symbol_declarations(place_id) {
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
                for binding in use_def.all_reachable_symbol_bindings(place_id) {
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
                let place_table = index.place_table(function_scope_id);

                if let Some(place_id) = place_table.member_id_by_instance_attribute_name(name_str) {
                    let use_def = index.use_def_map(function_scope_id);

                    // Check declarations first
                    for decl in use_def.all_reachable_member_declarations(place_id) {
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
                    for binding in use_def.all_reachable_member_bindings(place_id) {
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
    db: &'db dyn Db,
    file: File,
    keyword: &ast::Keyword,
    call_expr: &ast::ExprCall,
) -> Vec<ResolvedDefinition<'db>> {
    let model = SemanticModel::new(db, file);
    let func_type = call_expr.func.inferred_type(&model);

    let Some(keyword_name) = keyword.arg.as_ref() else {
        return Vec::new();
    };
    let keyword_name_str = keyword_name.as_str();

    let mut resolved_definitions = Vec::new();

    if let Some(Type::Callable(callable_type)) = func_type.into_callable(db) {
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
    db: &'db dyn Db,
    file: File,
    import_node: &ast::StmtImportFrom,
    symbol_name: &str,
    alias_resolution: ImportAliasResolution,
) -> Vec<ResolvedDefinition<'db>> {
    let mut visited = FxHashSet::default();
    resolve_definition::resolve_from_import_definitions(
        db,
        file,
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

    /// The definition where this callable was originally defined (useful for
    /// extracting docstrings).
    pub definition: Option<Definition<'db>>,

    /// Mapping from argument indices to parameter indices. This helps
    /// determine which parameter corresponds to which argument position.
    pub argument_to_parameter_mapping: Vec<MatchedArgument>,
}

/// Extract signature details from a function call expression.
/// This function analyzes the callable being invoked and returns zero or more
/// `CallSignatureDetails` objects, each representing one possible signature
/// (in case of overloads or union types).
pub fn call_signature_details<'db>(
    db: &'db dyn Db,
    file: File,
    call_expr: &ast::ExprCall,
) -> Vec<CallSignatureDetails<'db>> {
    let model = SemanticModel::new(db, file);
    let func_type = call_expr.func.inferred_type(&model);

    // Use into_callable to handle all the complex type conversions
    if let Some(callable_type) = func_type.into_callable(db) {
        let call_arguments =
            CallArguments::from_arguments(db, &call_expr.arguments, |_, splatted_value| {
                splatted_value.inferred_type(&model)
            });
        let bindings = callable_type.bindings(db).match_parameters(&call_arguments);

        // Extract signature details from all callable bindings
        bindings
            .into_iter()
            .flat_map(std::iter::IntoIterator::into_iter)
            .map(|binding| {
                let signature = &binding.signature;
                let display_details = signature.display(db).to_string_parts();
                let parameter_label_offsets = display_details.parameter_ranges.clone();
                let parameter_names = display_details.parameter_names.clone();

                CallSignatureDetails {
                    signature: signature.clone(),
                    label: display_details.label,
                    parameter_label_offsets,
                    parameter_names,
                    definition: signature.definition(),
                    argument_to_parameter_mapping: binding.argument_matches().to_vec(),
                }
            })
            .collect()
    } else {
        // Type is not callable, return empty signatures
        vec![]
    }
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
    use ruff_db::files::{File, FileRange};
    use ruff_db::parsed::{ParsedModuleRef, parsed_module};
    use ruff_python_ast as ast;
    use ruff_text_size::{Ranged, TextRange};
    use rustc_hash::FxHashSet;
    use tracing::trace;

    use crate::module_resolver::file_to_module;
    use crate::semantic_index::definition::{Definition, DefinitionKind};
    use crate::semantic_index::scope::{NodeWithScopeKind, ScopeId};
    use crate::semantic_index::{global_scope, place_table, semantic_index, use_def_map};
    use crate::{Db, ModuleName, resolve_module, resolve_real_module};

    /// Represents the result of resolving an import to either a specific definition or
    /// a specific range within a file.
    /// This enum helps distinguish between cases where an import resolves to:
    /// - A specific definition within a module (e.g., `from os import path` -> definition of `path`)
    /// - A specific range within a file, sometimes an empty range at the top of the file
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ResolvedDefinition<'db> {
        /// The import resolved to a specific definition within a module
        Definition(Definition<'db>),
        /// The import resolved to a file with a specific range
        FileWithRange(FileRange),
    }

    impl<'db> ResolvedDefinition<'db> {
        fn file(&self, db: &'db dyn Db) -> File {
            match self {
                ResolvedDefinition::Definition(definition) => definition.file(db),
                ResolvedDefinition::FileWithRange(file_range) => file_range.file(),
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

                // Get the full module name being imported
                let Some(module_name) = ModuleName::new(&alias.name) else {
                    return Vec::new(); // Invalid module name, return empty list
                };

                // Resolve the module to its file
                let Some(resolved_module) = resolve_module(db, &module_name) else {
                    return Vec::new(); // Module not found, return empty list
                };

                let Some(module_file) = resolved_module.file(db) else {
                    return Vec::new(); // No file for module, return empty list
                };

                // For simple imports like "import os", we want to navigate to the module itself.
                // Return the module file directly instead of trying to find definitions within it.
                vec![ResolvedDefinition::FileWithRange(FileRange::new(
                    module_file,
                    TextRange::default(),
                ))]
            }

            DefinitionKind::ImportFrom(import_from_def) => {
                let file = definition.file(db);
                let module = parsed_module(db, file).load(db);
                let import_node = import_from_def.import(&module);
                let alias = import_from_def.alias(&module);

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

        // Resolve the target module file
        let module_file = {
            // Resolve the module being imported from (handles both relative and absolute imports)
            let Some(module_name) = ModuleName::from_import_statement(db, file, import_node).ok()
            else {
                return Vec::new();
            };
            let Some(resolved_module) = resolve_module(db, &module_name) else {
                return Vec::new();
            };
            resolved_module.file(db)
        };

        let Some(module_file) = module_file else {
            return Vec::new(); // Module resolution failed
        };

        // Find the definition of this symbol in the imported module's global scope
        let global_scope = global_scope(db, module_file);
        let definitions_in_module = find_symbol_in_scope(db, global_scope, symbol_name);

        // Recursively resolve any import definitions found in the target module
        if definitions_in_module.is_empty() {
            // If we can't find the specific symbol, return empty list
            Vec::new()
        } else {
            let mut resolved_definitions = Vec::new();
            for def in definitions_in_module {
                let resolved = resolve_definition_recursive(
                    db,
                    def,
                    visited,
                    Some(symbol_name),
                    alias_resolution,
                );
                resolved_definitions.extend(resolved);
            }
            resolved_definitions
        }
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
        let bindings = use_def_map.all_reachable_symbol_bindings(symbol_id);
        let declarations = use_def_map.all_reachable_symbol_declarations(symbol_id);

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
    ) -> Option<Vec<ResolvedDefinition<'db>>> {
        trace!("Stub mapping definition...");
        // If the file isn't a stub, this is presumably the real definition
        let stub_file = def.file(db);
        if !stub_file.is_stub(db) {
            trace!("File isn't a stub, no stub mapping to do");
            return None;
        }

        // It's definitely a stub, so now rerun module resolution but with stubs disabled.
        let stub_module = file_to_module(db, stub_file)?;
        trace!("Found stub module: {}", stub_module.name(db));
        let real_module = resolve_real_module(db, stub_module.name(db))?;
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
            ResolvedDefinition::FileWithRange(file_range) => {
                return if file_range.range() == TextRange::default() {
                    trace!(
                        "Found module mapping: {} => {}",
                        stub_file.path(db),
                        real_file.path(db)
                    );
                    // This is just a reference to a module, no need to do paths
                    Some(vec![ResolvedDefinition::FileWithRange(FileRange::new(
                        real_file,
                        TextRange::default(),
                    ))])
                } else {
                    // Not yet implemented -- in this case we want to recover something like a Definition
                    // and build a Definition Path, but this input is a bit too abstract for now.
                    trace!("Found arbitrary FileWithRange by stub mapping, giving up");
                    None
                };
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
