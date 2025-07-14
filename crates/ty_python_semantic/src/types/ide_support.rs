use std::cmp::Ordering;

use crate::place::{Place, imported_symbol, place_from_bindings, place_from_declarations};
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
use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ruff_text_size::TextRange;
use rustc_hash::FxHashSet;

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
    let file_scope = index.try_expression_scope_id(&ast::Expr::Name(name.clone()))?;

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
