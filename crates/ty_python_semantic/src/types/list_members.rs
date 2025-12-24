//! Routines and types to list all members present on a given type or in a given scope.
//!
//! These two concepts are closely related, since listing all members of a given
//! module-literal type requires listing all members in the module's scope, and
//! listing all members on a nominal-instance type or a class-literal type requires
//! listing all members in the class's body scope.

use std::cmp::Ordering;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use crate::{
    Db, NameKind,
    place::{
        Place, PlaceWithDefinition, imported_symbol, place_from_bindings, place_from_declarations,
    },
    semantic_index::{
        attribute_scopes, definition::Definition, global_scope, place_table, scope::ScopeId,
        semantic_index, use_def_map,
    },
    types::{
        ClassBase, ClassLiteral, KnownClass, KnownInstanceType, SubclassOfInner, Type,
        TypeVarBoundOrConstraints, class::CodeGeneratorKind, generics::Specialization,
    },
};

/// Iterate over all declarations and bindings that exist at the end
/// of the given scope.
pub(crate) fn all_end_of_scope_members<'db>(
    db: &'db dyn Db,
    scope_id: ScopeId<'db>,
) -> impl Iterator<Item = MemberWithDefinition<'db>> + 'db {
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    use_def_map
        .all_end_of_scope_symbol_declarations()
        .filter_map(move |(symbol_id, declarations)| {
            let place_result = place_from_declarations(db, declarations);
            let first_reachable_definition = place_result.first_declaration?;
            let ty = place_result
                .ignore_conflicting_declarations()
                .place
                .ignore_possibly_undefined()?;
            let symbol = table.symbol(symbol_id);
            let member = Member {
                name: symbol.name().clone(),
                ty,
            };
            Some(MemberWithDefinition {
                member,
                first_reachable_definition,
            })
        })
        .chain(use_def_map.all_end_of_scope_symbol_bindings().filter_map(
            move |(symbol_id, bindings)| {
                let PlaceWithDefinition {
                    place,
                    first_definition,
                } = place_from_bindings(db, bindings);

                let first_reachable_definition = first_definition?;
                let ty = place.ignore_possibly_undefined()?;

                let symbol = table.symbol(symbol_id);
                let member = Member {
                    name: symbol.name().clone(),
                    ty,
                };
                Some(MemberWithDefinition {
                    member,
                    first_reachable_definition,
                })
            },
        ))
}

/// Iterate over all declarations and bindings that are reachable anywhere
/// in the given scope.
pub(crate) fn all_reachable_members<'db>(
    db: &'db dyn Db,
    scope_id: ScopeId<'db>,
) -> impl Iterator<Item = MemberWithDefinition<'db>> + 'db {
    let use_def_map = use_def_map(db, scope_id);
    let table = place_table(db, scope_id);

    use_def_map
        .all_reachable_symbols()
        .flat_map(move |(symbol_id, declarations, bindings)| {
            let symbol = table.symbol(symbol_id);

            let declaration_place_result = place_from_declarations(db, declarations);
            let declaration =
                declaration_place_result
                    .first_declaration
                    .and_then(|first_reachable_definition| {
                        let ty = declaration_place_result
                            .ignore_conflicting_declarations()
                            .place
                            .ignore_possibly_undefined()?;
                        let member = Member {
                            name: symbol.name().clone(),
                            ty,
                        };
                        Some(MemberWithDefinition {
                            member,
                            first_reachable_definition,
                        })
                    });

            let place_with_definition = place_from_bindings(db, bindings);
            let binding =
                place_with_definition
                    .first_definition
                    .and_then(|first_reachable_definition| {
                        let ty = place_with_definition.place.ignore_possibly_undefined()?;
                        let member = Member {
                            name: symbol.name().clone(),
                            ty,
                        };
                        Some(MemberWithDefinition {
                            member,
                            first_reachable_definition,
                        })
                    });

            [declaration, binding]
        })
        .flatten()
}

// `__init__`, `__repr__`, `__eq__`, `__ne__` and `__hash__` are always included via `object`,
// so we don't need to list them here.
const SYNTHETIC_DATACLASS_ATTRIBUTES: &[&str] = &[
    "__lt__",
    "__le__",
    "__gt__",
    "__ge__",
    "__replace__",
    "__setattr__",
    "__delattr__",
    "__slots__",
    "__weakref__",
    "__match_args__",
    "__dataclass_fields__",
    "__dataclass_params__",
];

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
                let (class_literal, specialization) = instance.class(db).class_literal(db);
                self.extend_with_instance_members(db, ty, class_literal);
                self.extend_with_synthetic_members(db, ty, class_literal, specialization);
            }

            Type::NewTypeInstance(newtype) => {
                self.extend_with_type(db, newtype.concrete_base_type(db));
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
                self.extend_with_synthetic_members(db, ty, class_literal, None);
                if let Type::ClassLiteral(metaclass) = class_literal.metaclass(db) {
                    self.extend_with_class_members(db, ty, metaclass);
                }
            }

            Type::GenericAlias(generic_alias) => {
                let class_literal = generic_alias.origin(db);
                self.extend_with_class_members(db, ty, class_literal);
                self.extend_with_synthetic_members(db, ty, class_literal, None);
                if let Type::ClassLiteral(metaclass) = class_literal.metaclass(db) {
                    self.extend_with_class_members(db, ty, metaclass);
                }
            }

            Type::SubclassOf(subclass_of_type) => match subclass_of_type.subclass_of() {
                SubclassOfInner::Dynamic(_) => {
                    self.extend_with_type(db, KnownClass::Type.to_instance(db));
                }
                _ => {
                    if let Some(class_type) = subclass_of_type.subclass_of().into_class(db) {
                        let (class_literal, specialization) = class_type.class_literal(db);
                        self.extend_with_class_members(db, ty, class_literal);
                        self.extend_with_synthetic_members(db, ty, class_literal, specialization);
                        if let Type::ClassLiteral(metaclass) = class_literal.metaclass(db) {
                            self.extend_with_class_members(db, ty, metaclass);
                        }
                    }
                }
            },

            Type::Dynamic(_) | Type::Never | Type::AlwaysTruthy | Type::AlwaysFalsy => {
                self.extend_with_type(db, Type::object());
            }

            Type::TypeAlias(alias) => self.extend_with_type(db, alias.value_type(db)),

            Type::TypeVar(bound_typevar) => {
                match bound_typevar.typevar(db).bound_or_constraints(db) {
                    None => {
                        self.extend_with_type(db, Type::object());
                    }
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        self.extend_with_type(db, bound);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        self.members.extend(
                            constraints
                                .elements(db)
                                .iter()
                                .map(|ty| AllMembers::of(db, *ty).members)
                                .reduce(|acc, members| {
                                    acc.intersection(&members).cloned().collect()
                                })
                                .unwrap_or_default(),
                        );
                    }
                }
            }

            Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_)
            | Type::LiteralString
            | Type::PropertyInstance(_)
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::Callable(_)
            | Type::ProtocolInstance(_)
            | Type::SpecialForm(_)
            | Type::KnownInstance(_)
            | Type::BoundSuper(_)
            | Type::TypeIs(_) => match ty.to_meta_type(db) {
                Type::ClassLiteral(class_literal) => {
                    self.extend_with_class_members(db, ty, class_literal);
                }
                Type::SubclassOf(subclass_of) => {
                    if let Some(class) = subclass_of.subclass_of().into_class(db) {
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
                    let Place::Defined(ty, _, _, _) =
                        imported_symbol(db, file, symbol_name, None).place
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
                                    instance.known_class(db),
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
                                KnownInstanceType::TypeVar(_)
                                | KnownInstanceType::TypeAliasType(_)
                                | KnownInstanceType::UnionType(_)
                                | KnownInstanceType::Literal(_)
                                | KnownInstanceType::Annotated(_),
                            ) => continue,
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
            for memberdef in all_end_of_scope_members(db, parent_scope) {
                let result = ty.member(db, memberdef.member.name.as_str());
                let Some(ty) = result.place.ignore_possibly_undefined() else {
                    continue;
                };
                self.members.insert(Member {
                    name: memberdef.member.name,
                    ty,
                });
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
                for place_expr in index.place_table(function_scope_id).members() {
                    let Some(name) = place_expr.as_instance_attribute() else {
                        continue;
                    };
                    let result = ty.member(db, name);
                    let Some(ty) = result.place.ignore_possibly_undefined() else {
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
            for memberdef in all_end_of_scope_members(db, class_body_scope) {
                let result = ty.member(db, memberdef.member.name.as_str());
                let Some(ty) = result.place.ignore_possibly_undefined() else {
                    continue;
                };
                self.members.insert(Member {
                    name: memberdef.member.name,
                    ty,
                });
            }
        }
    }

    fn extend_with_synthetic_members(
        &mut self,
        db: &'db dyn Db,
        ty: Type<'db>,
        class_literal: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) {
        match CodeGeneratorKind::from_class(db, class_literal, specialization) {
            Some(CodeGeneratorKind::NamedTuple) => {
                if ty.is_nominal_instance() {
                    self.extend_with_type(db, KnownClass::NamedTupleFallback.to_instance(db));
                } else {
                    self.extend_with_type(db, KnownClass::NamedTupleFallback.to_class_literal(db));
                }
            }
            Some(CodeGeneratorKind::TypedDict) => {}
            Some(CodeGeneratorKind::DataclassLike(_)) => {
                for attr in SYNTHETIC_DATACLASS_ATTRIBUTES {
                    if let Place::Defined(synthetic_member, _, _, _) = ty.member(db, attr).place {
                        self.members.insert(Member {
                            name: Name::from(*attr),
                            ty: synthetic_member,
                        });
                    }
                }
            }
            None => {}
        }
    }
}

/// A member of a type or scope, with the first reachable definition of that member.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MemberWithDefinition<'db> {
    pub member: Member<'db>,
    pub first_reachable_definition: Definition<'db>,
}

/// A member of a type or scope.
///
/// In the context of the [`all_members`] routine, this represents
/// a single item in (ideally) the list returned by `dir(object)`.
///
/// The equality, comparison and hashing traits implemented for
/// this type are done so by taking only the name into account. At
/// present, this is because we assume the name is enough to uniquely
/// identify each attribute on an object. This is perhaps complicated
/// by overloads, but they only get represented by one member for
/// now. Moreover, it is convenient to be able to sort collections of
/// members, and a [`Type`] currently (as of 2025-07-09) has no way to do
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
