use std::cmp::Ordering;

use crate::place::{
    Place, builtins_module_scope, imported_symbol, place_from_bindings, place_from_declarations,
};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::definition::DefinitionKind;
use crate::semantic_index::place::ScopeId;
use crate::semantic_index::{
    attribute_scopes, global_scope, place_table, semantic_index, use_def_map,
};
use crate::types::call::CallArguments;
use crate::types::signatures::Signature;
use crate::types::{ClassBase, ClassLiteral, DynamicType, KnownClass, KnownInstanceType, Type};
use crate::{Db, HasType, NameKind, SemanticModel};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;

pub use resolve_definition::ResolvedDefinition;
use resolve_definition::{find_symbol_in_scope, resolve_definition};

pub(crate) fn all_declarations_and_bindings<'db>(
    db: &'db dyn Db,
    scope_id: ScopeId<'db>,
) -> impl Iterator<Item = Member<'db>> + 'db {
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    use_def_map
        .all_end_of_scope_declarations()
        .filter_map(move |(symbol_id, declarations)| {
            place_from_declarations(db, declarations)
                .ok()
                .and_then(|result| {
                    result.place.ignore_possibly_unbound().and_then(|ty| {
                        table
                            .place_expr(symbol_id)
                            .as_name()
                            .cloned()
                            .map(|name| Member { name, ty })
                    })
                })
        })
        .chain(
            use_def_map
                .all_end_of_scope_bindings()
                .filter_map(move |(symbol_id, bindings)| {
                    place_from_bindings(db, bindings)
                        .ignore_possibly_unbound()
                        .and_then(|ty| {
                            table
                                .place_expr(symbol_id)
                                .as_name()
                                .cloned()
                                .map(|name| Member { name, ty })
                        })
                }),
        )
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
                self.extend_with_instance_members(db, class_literal);
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
                Type::GenericAlias(generic_alias) => {
                    let class_literal = generic_alias.origin(db);
                    self.extend_with_class_members(db, ty, class_literal);
                }
                _ => {}
            },

            Type::ModuleLiteral(literal) => {
                self.extend_with_type(db, KnownClass::ModuleType.to_instance(db));
                let module = literal.module(db);

                let Some(file) = module.file() else {
                    return;
                };

                let module_scope = global_scope(db, file);
                let use_def_map = use_def_map(db, module_scope);
                let place_table = place_table(db, module_scope);

                for (symbol_id, _) in use_def_map.all_end_of_scope_declarations() {
                    let Some(symbol_name) = place_table.place_expr(symbol_id).as_name() else {
                        continue;
                    };
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
                        name: place_table.place_expr(symbol_id).expect_name().clone(),
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

    fn extend_with_instance_members(&mut self, db: &'db dyn Db, class_literal: ClassLiteral<'db>) {
        for parent in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|class| class.class_literal(db).0)
        {
            let parent_instance = Type::instance(db, parent.default_specialization(db));
            let class_body_scope = parent.body_scope(db);
            let file = class_body_scope.file(db);
            let index = semantic_index(db, file);
            for function_scope_id in attribute_scopes(db, class_body_scope) {
                let place_table = index.place_table(function_scope_id);
                for place_expr in place_table.places() {
                    let Some(name) = place_expr.as_instance_attribute() else {
                        continue;
                    };
                    let result = parent_instance.member(db, name.as_str());
                    let Some(ty) = result.place.ignore_possibly_unbound() else {
                        continue;
                    };
                    self.members.insert(Member {
                        name: name.clone(),
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
                let result = parent_instance.member(db, name.as_str());
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
    let place_id = place_table.place_id_by_name(name_str)?;

    // Get the use-def map and look up definitions for this place
    let use_def_map = index.use_def_map(file_scope);
    let declarations = use_def_map.all_reachable_declarations(place_id);

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

        let Some(place_id) = place_table.place_id_by_name(name_str) else {
            continue; // Name not found in this scope, try parent scope
        };

        // Check if this place is marked as global or nonlocal
        let place_expr = place_table.place_expr(place_id);
        let is_global = place_expr.is_marked_global();
        let is_nonlocal = place_expr.is_marked_nonlocal();

        // TODO: The current algorithm doesn't return definintions or bindings
        // for other scopes that are outside of this scope hierarchy that target
        // this name using a nonlocal or global binding. The semantic analyzer
        // doesn't appear to track these in a way that we can easily access
        // them from here without walking all scopes in the module.

        // If marked as global, skip to global scope
        if is_global {
            let global_scope_id = global_scope(db, file);
            let global_place_table = crate::semantic_index::place_table(db, global_scope_id);

            if let Some(global_place_id) = global_place_table.place_id_by_name(name_str) {
                let global_use_def_map = crate::semantic_index::use_def_map(db, global_scope_id);
                let global_bindings = global_use_def_map.all_reachable_bindings(global_place_id);
                let global_declarations =
                    global_use_def_map.all_reachable_declarations(global_place_id);

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
        let bindings = use_def_map.all_reachable_bindings(place_id);
        let declarations = use_def_map.all_reachable_declarations(place_id);

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
        let resolved = resolve_definition(db, *definition, Some(name_str));
        resolved_definitions.extend(resolved);
    }

    // If we didn't find any definitions in scopes, fallback to builtins
    if resolved_definitions.is_empty() {
        let Some(builtins_scope) = builtins_module_scope(db) else {
            return Vec::new();
        };
        find_symbol_in_scope(db, builtins_scope, name_str)
            .into_iter()
            .flat_map(|def| resolve_definition(db, def, Some(name_str)))
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

    for ty in tys {
        // Handle modules
        if let Type::ModuleLiteral(module_literal) = ty {
            if let Some(module_file) = module_literal.module(db).file() {
                let module_scope = global_scope(db, module_file);
                for def in find_symbol_in_scope(db, module_scope, name_str) {
                    resolved.extend(resolve_definition(db, def, Some(name_str)));
                }
            }
            continue;
        }

        // Determine the class literal for this type, if any
        let class_literal = match ty {
            Type::NominalInstance(instance) => instance.class.class_literal(db).0,
            Type::ClassLiteral(class_literal) => class_literal,
            Type::GenericAlias(alias) => alias.origin(db),
            Type::SubclassOf(subclass) => match subclass.subclass_of().into_class() {
                Some(cls) => cls.class_literal(db).0,
                None => continue,
            },
            // Handle additional types that have class-based lookups
            Type::FunctionLiteral(_) => {
                if let Type::ClassLiteral(class_literal) =
                    KnownClass::FunctionType.to_class_literal(db)
                {
                    class_literal
                } else {
                    continue;
                }
            }
            Type::BoundMethod(_) => {
                if let Type::ClassLiteral(class_literal) =
                    KnownClass::MethodType.to_class_literal(db)
                {
                    class_literal
                } else {
                    continue;
                }
            }
            Type::MethodWrapper(_) => {
                if let Type::ClassLiteral(class_literal) =
                    KnownClass::MethodWrapperType.to_class_literal(db)
                {
                    class_literal
                } else {
                    continue;
                }
            }
            Type::PropertyInstance(_) => {
                if let Type::ClassLiteral(class_literal) = KnownClass::Property.to_class_literal(db)
                {
                    class_literal
                } else {
                    continue;
                }
            }
            Type::Tuple(_) => {
                if let Type::ClassLiteral(class_literal) = KnownClass::Tuple.to_class_literal(db) {
                    class_literal
                } else {
                    continue;
                }
            }
            Type::SpecialForm(_) => {
                if let Type::ClassLiteral(class_literal) = KnownClass::Type.to_class_literal(db) {
                    class_literal
                } else {
                    continue;
                }
            }
            Type::ProtocolInstance(protocol) => {
                match protocol.inner {
                    super::instance::Protocol::FromClass(class) => class.class_literal(db).0,
                    super::instance::Protocol::Synthesized(_) => {
                        // For synthesized protocols, we can't navigate to a specific class,
                        // but we could potentially look up the interface members
                        // For now, skip synthesized protocols
                        continue;
                    }
                }
            }
            _ => continue,
        };

        let mut found_attr = false;

        // Walk the MRO: include class and its ancestors, but stop when we find a match
        for ancestor in class_literal
            .iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .map(|cls| cls.class_literal(db).0)
        {
            let class_scope = ancestor.body_scope(db);
            let class_place_table = crate::semantic_index::place_table(db, class_scope);

            // Look for class-level declarations and bindings
            if let Some(place_id) = class_place_table.place_id_by_name(name_str) {
                let use_def = use_def_map(db, class_scope);

                // Check declarations first
                for decl in use_def.all_reachable_declarations(place_id) {
                    if let Some(def) = decl.declaration.definition() {
                        resolved.extend(resolve_definition(db, def, Some(name_str)));
                        found_attr = true;
                        break;
                    }
                }

                // If no declarations found, check bindings
                if !found_attr {
                    for binding in use_def.all_reachable_bindings(place_id) {
                        if let Some(def) = binding.binding.definition() {
                            resolved.extend(resolve_definition(db, def, Some(name_str)));
                            found_attr = true;
                            break;
                        }
                    }
                }
            }

            // Look for instance attributes in method scopes (e.g., self.x = 1)
            if !found_attr {
                let file = class_scope.file(db);
                let index = semantic_index(db, file);

                for function_scope_id in attribute_scopes(db, class_scope) {
                    let place_table = index.place_table(function_scope_id);

                    if let Some(place_id) =
                        place_table.place_id_by_instance_attribute_name(name_str)
                    {
                        let use_def = index.use_def_map(function_scope_id);

                        // Check declarations first
                        for decl in use_def.all_reachable_declarations(place_id) {
                            if let Some(def) = decl.declaration.definition() {
                                resolved.extend(resolve_definition(db, def, Some(name_str)));
                                found_attr = true;
                                break;
                            }
                        }

                        // If no declarations found, check bindings
                        if !found_attr {
                            for binding in use_def.all_reachable_bindings(place_id) {
                                if let Some(def) = binding.binding.definition() {
                                    resolved.extend(resolve_definition(db, def, Some(name_str)));
                                    found_attr = true;
                                    break;
                                }
                            }
                        }

                        if found_attr {
                            break;
                        }
                    }
                }
            }

            // TODO: Add support for metaclass attribute lookups

            if found_attr {
                break;
            }
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
                                function_file,
                                parameter_range,
                            ));
                        }
                    }
                }
            }
        }
    }

    resolved_definitions
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
    /// identify which argument corresponds to which parameter.
    pub argument_to_parameter_mapping: Vec<Option<usize>>,
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
        let call_arguments = CallArguments::from_arguments(&call_expr.arguments);
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
                    argument_to_parameter_mapping: binding.argument_to_parameter_mapping().to_vec(),
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
    // Check regular positional parameters
    for param in &parameters.args {
        if param.parameter.name.as_str() == parameter_name {
            return Some(param.parameter.name.range());
        }
    }

    // Check keyword-only parameters
    for param in &parameters.kwonlyargs {
        if param.parameter.name.as_str() == parameter_name {
            return Some(param.parameter.name.range());
        }
    }

    None
}

mod resolve_definition {
    //! Resolves an Import, `ImportFrom` or `StarImport` definition to one or more
    //! "resolved definitions". This is done recursively to find the original
    //! definition targeted by the import.

    use ruff_db::files::File;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast as ast;
    use ruff_text_size::TextRange;
    use rustc_hash::FxHashSet;

    use crate::semantic_index::definition::{Definition, DefinitionKind};
    use crate::semantic_index::place::ScopeId;
    use crate::semantic_index::{global_scope, place_table, use_def_map};
    use crate::{Db, ModuleName, resolve_module};

    /// Represents the result of resolving an import to either a specific definition or
    /// a specific range within a file.
    /// This enum helps distinguish between cases where an import resolves to:
    /// - A specific definition within a module (e.g., `from os import path` -> definition of `path`)
    /// - A specific range within a file, sometimes an empty range at the top of the file
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum ResolvedDefinition<'db> {
        /// The import resolved to a specific definition within a module
        Definition(Definition<'db>),
        /// The import resolved to a file with an optional specific range
        FileWithRange(File, TextRange),
    }

    /// Resolve import definitions to their targets.
    /// Returns resolved definitions which can be either specific definitions or module files.
    /// For non-import definitions, returns the definition wrapped in `ResolvedDefinition::Definition`.
    /// Always returns at least the original definition as a fallback if resolution fails.
    pub(crate) fn resolve_definition<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        symbol_name: Option<&str>,
    ) -> Vec<ResolvedDefinition<'db>> {
        let mut visited = FxHashSet::default();
        let resolved = resolve_definition_recursive(db, definition, &mut visited, symbol_name);

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

                let Some(module_file) = resolved_module.file() else {
                    return Vec::new(); // No file for module, return empty list
                };

                // For simple imports like "import os", we want to navigate to the module itself.
                // Return the module file directly instead of trying to find definitions within it.
                vec![ResolvedDefinition::FileWithRange(
                    module_file,
                    TextRange::default(),
                )]
            }

            DefinitionKind::ImportFrom(import_from_def) => {
                let file = definition.file(db);
                let module = parsed_module(db, file).load(db);
                let import_node = import_from_def.import(&module);
                let alias = import_from_def.alias(&module);

                // For `ImportFrom`, we need to resolve the original imported symbol name
                // (alias.name), not the local alias (symbol_name)
                resolve_from_import_definitions(db, file, import_node, &alias.name, visited)
            }

            // For star imports, try to resolve to the specific symbol being accessed
            DefinitionKind::StarImport(star_import_def) => {
                let file = definition.file(db);
                let module = parsed_module(db, file).load(db);
                let import_node = star_import_def.import(&module);

                // If we have a symbol name, use the helper to resolve it in the target module
                if let Some(symbol_name) = symbol_name {
                    resolve_from_import_definitions(db, file, import_node, symbol_name, visited)
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
    fn resolve_from_import_definitions<'db>(
        db: &'db dyn Db,
        file: File,
        import_node: &ast::StmtImportFrom,
        symbol_name: &str,
        visited: &mut FxHashSet<Definition<'db>>,
    ) -> Vec<ResolvedDefinition<'db>> {
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
            resolved_module.file()
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
                let resolved = resolve_definition_recursive(db, def, visited, Some(symbol_name));
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
    ) -> Vec<Definition<'db>> {
        let place_table = place_table(db, scope);
        let Some(place_id) = place_table.place_id_by_name(symbol_name) else {
            return Vec::new();
        };

        let use_def_map = use_def_map(db, scope);
        let mut definitions = Vec::new();

        // Get all definitions (both bindings and declarations) for this place
        let bindings = use_def_map.all_reachable_bindings(place_id);
        let declarations = use_def_map.all_reachable_declarations(place_id);

        for binding in bindings {
            if let Some(def) = binding.binding.definition() {
                definitions.push(def);
            }
        }

        for declaration in declarations {
            if let Some(def) = declaration.declaration.definition() {
                definitions.push(def);
            }
        }

        definitions
    }
}
