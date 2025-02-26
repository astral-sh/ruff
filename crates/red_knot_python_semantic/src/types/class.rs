use crate::{
    module_resolver::file_to_module,
    semantic_index::{
        attribute_assignment::AttributeAssignment, attribute_assignments, semantic_index,
        symbol::ScopeId, symbol_table, use_def_map,
    },
    symbol::{
        class_symbol, known_module_symbol, symbol_from_bindings, symbol_from_declarations,
        Boundness, LookupError, LookupResult, Symbol, SymbolAndQualifiers,
    },
    types::{
        definition_expression_type, CallArguments, CallError, MetaclassCandidate, TupleType,
        UnionBuilder, UnionCallError,
    },
    Db, KnownModule, Program,
};
use indexmap::IndexSet;
use itertools::Itertools as _;
use ruff_db::files::File;
use ruff_python_ast::{self as ast, PythonVersion};

use super::{
    class_base::ClassBase, infer_expression_type, infer_unpack_types, IntersectionBuilder,
    KnownFunction, Mro, MroError, MroIterator, SubclassOfType, Truthiness, Type, TypeAliasType,
    TypeQualifiers, TypeVarInstance,
};

/// Representation of a runtime class object.
///
/// Does not in itself represent a type,
/// but is used as the inner data for several structs that *do* represent types.
#[salsa::interned]
pub struct Class<'db> {
    /// Name of the class at definition
    #[return_ref]
    pub(crate) name: ast::name::Name,

    body_scope: ScopeId<'db>,

    pub(crate) known: Option<KnownClass>,
}

#[salsa::tracked]
impl<'db> Class<'db> {
    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    /// Return `true` if this class represents the builtin class `object`
    pub(crate) fn is_object(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Object)
    }

    /// Return an iterator over the inferred types of this class's *explicit* bases.
    ///
    /// Note that any class (except for `object`) that has no explicit
    /// bases will implicitly inherit from `object` at runtime. Nonetheless,
    /// this method does *not* include `object` in the bases it iterates over.
    ///
    /// ## Why is this a salsa query?
    ///
    /// This is a salsa query to short-circuit the invalidation
    /// when the class's AST node changes.
    ///
    /// Were this not a salsa query, then the calling query
    /// would depend on the class's AST and rerun for every change in that file.
    pub(super) fn explicit_bases(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        self.explicit_bases_query(db)
    }

    /// Iterate over this class's explicit bases, filtering out any bases that are not class objects.
    fn fully_static_explicit_bases(self, db: &'db dyn Db) -> impl Iterator<Item = Class<'db>> {
        self.explicit_bases(db)
            .iter()
            .copied()
            .filter_map(Type::into_class_literal)
            .map(|ClassLiteralType { class }| class)
    }

    #[salsa::tracked(return_ref)]
    fn explicit_bases_query(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let class_stmt = self.node(db);

        let class_definition = semantic_index(db, self.file(db)).definition(class_stmt);

        class_stmt
            .bases()
            .iter()
            .map(|base_node| definition_expression_type(db, class_definition, base_node))
            .collect()
    }

    fn file(self, db: &dyn Db) -> File {
        self.body_scope(db).file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node(self, db: &'db dyn Db) -> &'db ast::StmtClassDef {
        self.body_scope(db).node(db).expect_class()
    }

    /// Return the types of the decorators on this class
    #[salsa::tracked(return_ref)]
    fn decorators(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let class_stmt = self.node(db);
        if class_stmt.decorator_list.is_empty() {
            return Box::new([]);
        }
        let class_definition = semantic_index(db, self.file(db)).definition(class_stmt);
        class_stmt
            .decorator_list
            .iter()
            .map(|decorator_node| {
                definition_expression_type(db, class_definition, &decorator_node.expression)
            })
            .collect()
    }

    /// Is this class final?
    pub(super) fn is_final(self, db: &'db dyn Db) -> bool {
        self.decorators(db)
            .iter()
            .filter_map(|deco| deco.into_function_literal())
            .any(|decorator| decorator.is_known(db, KnownFunction::Final))
    }

    /// Attempt to resolve the [method resolution order] ("MRO") for this class.
    /// If the MRO is unresolvable, return an error indicating why the class's MRO
    /// cannot be accurately determined. The error returned contains a fallback MRO
    /// that will be used instead for the purposes of type inference.
    ///
    /// The MRO is the tuple of classes that can be retrieved as the `__mro__`
    /// attribute on a class at runtime.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    #[salsa::tracked(return_ref)]
    pub(super) fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, MroError<'db>> {
        Mro::of_class(db, self)
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`Class::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(super) fn iter_mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        MroIterator::new(db, self)
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub(super) fn is_subclass_of(self, db: &'db dyn Db, other: Class) -> bool {
        // `is_subclass_of` is checking the subtype relation, in which gradual types do not
        // participate, so we should not return `True` if we find `Any/Unknown` in the MRO.
        self.iter_mro(db).contains(&ClassBase::Class(other))
    }

    /// Return the explicit `metaclass` of this class, if one is defined.
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn explicit_metaclass(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let class_stmt = self.node(db);
        let metaclass_node = &class_stmt
            .arguments
            .as_ref()?
            .find_keyword("metaclass")?
            .value;
        let class_definition = semantic_index(db, self.file(db)).definition(class_stmt);
        let metaclass_ty = definition_expression_type(db, class_definition, metaclass_node);
        Some(metaclass_ty)
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(super) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        self.try_metaclass(db)
            .unwrap_or_else(|_| SubclassOfType::subclass_of_unknown())
    }

    /// Return the metaclass of this class, or an error if the metaclass cannot be inferred.
    #[salsa::tracked]
    pub(super) fn try_metaclass(self, db: &'db dyn Db) -> Result<Type<'db>, MetaclassError<'db>> {
        // Identify the class's own metaclass (or take the first base class's metaclass).
        let mut base_classes = self.fully_static_explicit_bases(db).peekable();

        if base_classes.peek().is_some() && self.inheritance_cycle(db).is_some() {
            // We emit diagnostics for cyclic class definitions elsewhere.
            // Avoid attempting to infer the metaclass if the class is cyclically defined:
            // it would be easy to enter an infinite loop.
            return Ok(SubclassOfType::subclass_of_unknown());
        }

        let explicit_metaclass = self.explicit_metaclass(db);
        let (metaclass, class_metaclass_was_from) = if let Some(metaclass) = explicit_metaclass {
            (metaclass, self)
        } else if let Some(base_class) = base_classes.next() {
            (base_class.metaclass(db), base_class)
        } else {
            (KnownClass::Type.to_class_literal(db), self)
        };

        let mut candidate = if let Type::ClassLiteral(metaclass_ty) = metaclass {
            MetaclassCandidate {
                metaclass: metaclass_ty.class,
                explicit_metaclass_of: class_metaclass_was_from,
            }
        } else {
            let name = Type::string_literal(db, self.name(db));
            let bases = TupleType::from_elements(db, self.explicit_bases(db));
            // TODO: Should be `dict[str, Any]`
            let namespace = KnownClass::Dict.to_instance(db);

            // TODO: Other keyword arguments?
            let arguments = CallArguments::positional([name, bases, namespace]);

            let return_ty_result = match metaclass.try_call(db, &arguments) {
                Ok(outcome) => Ok(outcome.return_type(db)),

                Err(CallError::NotCallable { not_callable_type }) => Err(MetaclassError {
                    kind: MetaclassErrorKind::NotCallable(not_callable_type),
                }),

                Err(CallError::Union(UnionCallError {
                    called_type,
                    errors,
                    bindings,
                })) => {
                    let mut partly_not_callable = false;

                    let return_ty = errors
                        .iter()
                        .fold(None, |acc, error| {
                            let ty = error.return_type(db);

                            match (acc, ty) {
                                (acc, None) => {
                                    partly_not_callable = true;
                                    acc
                                }
                                (None, Some(ty)) => Some(UnionBuilder::new(db).add(ty)),
                                (Some(builder), Some(ty)) => Some(builder.add(ty)),
                            }
                        })
                        .map(|mut builder| {
                            for binding in bindings {
                                builder = builder.add(binding.return_type());
                            }

                            builder.build()
                        });

                    if partly_not_callable {
                        Err(MetaclassError {
                            kind: MetaclassErrorKind::PartlyNotCallable(called_type),
                        })
                    } else {
                        Ok(return_ty.unwrap_or(Type::unknown()))
                    }
                }

                Err(CallError::PossiblyUnboundDunderCall { .. }) => Err(MetaclassError {
                    kind: MetaclassErrorKind::PartlyNotCallable(metaclass),
                }),

                // TODO we should also check for binding errors that would indicate the metaclass
                // does not accept the right arguments
                Err(CallError::BindingError { binding }) => Ok(binding.return_type()),
            };

            return return_ty_result.map(|ty| ty.to_meta_type(db));
        };

        // Reconcile all base classes' metaclasses with the candidate metaclass.
        //
        // See:
        // - https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass
        // - https://github.com/python/cpython/blob/83ba8c2bba834c0b92de669cac16fcda17485e0e/Objects/typeobject.c#L3629-L3663
        for base_class in base_classes {
            let metaclass = base_class.metaclass(db);
            let Type::ClassLiteral(metaclass) = metaclass else {
                continue;
            };
            if metaclass.class.is_subclass_of(db, candidate.metaclass) {
                candidate = MetaclassCandidate {
                    metaclass: metaclass.class,
                    explicit_metaclass_of: base_class,
                };
                continue;
            }
            if candidate.metaclass.is_subclass_of(db, metaclass.class) {
                continue;
            }
            return Err(MetaclassError {
                kind: MetaclassErrorKind::Conflict {
                    candidate1: candidate,
                    candidate2: MetaclassCandidate {
                        metaclass: metaclass.class,
                        explicit_metaclass_of: base_class,
                    },
                    candidate1_is_base_class: explicit_metaclass.is_none(),
                },
            });
        }

        Ok(Type::class_literal(candidate.metaclass))
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        if name == "__mro__" {
            let tuple_elements = self.iter_mro(db).map(Type::from);
            return Symbol::bound(TupleType::from_elements(db, tuple_elements));
        }

        // If we encounter a dynamic type in this class's MRO, we'll save that dynamic type
        // in this variable. After we've traversed the MRO, we'll either:
        // (1) Use that dynamic type as the type for this attribute,
        //     if no other classes in the MRO define the attribute; or,
        // (2) Intersect that dynamic type with the type of the attribute
        //     from the non-dynamic members of the class's MRO.
        let mut dynamic_type_to_intersect_with: Option<Type<'db>> = None;

        let mut lookup_result: LookupResult<'db> = Err(LookupError::Unbound);

        for superclass in self.iter_mro(db) {
            match superclass {
                ClassBase::Dynamic(_) => {
                    // Note: calling `Type::from(superclass).member()` would be incorrect here.
                    // What we'd really want is a `Type::Any.own_class_member()` method,
                    // but adding such a method wouldn't make much sense -- it would always return `Any`!
                    dynamic_type_to_intersect_with.get_or_insert(Type::from(superclass));
                }
                ClassBase::Class(class) => {
                    lookup_result = lookup_result.or_else(|lookup_error| {
                        lookup_error.or_fall_back_to(db, class.own_class_member(db, name))
                    });
                }
            }
            if lookup_result.is_ok() {
                break;
            }
        }

        match (Symbol::from(lookup_result), dynamic_type_to_intersect_with) {
            (symbol, None) => symbol,
            (Symbol::Type(ty, _), Some(dynamic_type)) => Symbol::bound(
                IntersectionBuilder::new(db)
                    .add_positive(ty)
                    .add_positive(dynamic_type)
                    .build(),
            ),
            (Symbol::Unbound, Some(dynamic_type)) => Symbol::bound(dynamic_type),
        }
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as ClassVars are considered.
    ///
    /// Returns [`Symbol::Unbound`] if `name` cannot be found in this class's scope
    /// directly. Use [`Class::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        let body_scope = self.body_scope(db);
        class_symbol(db, body_scope, name)
    }

    /// Returns the `name` attribute of an instance of this class.
    ///
    /// The attribute could be defined in the class body, but it could also be an implicitly
    /// defined attribute that is only present in a method (typically `__init__`).
    ///
    /// The attribute might also be defined in a superclass of this class.
    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        let mut union = UnionBuilder::new(db);
        let mut union_qualifiers = TypeQualifiers::empty();

        for superclass in self.iter_mro(db) {
            match superclass {
                ClassBase::Dynamic(_) => {
                    return SymbolAndQualifiers::todo(
                        "instance attribute on class with dynamic base",
                    );
                }
                ClassBase::Class(class) => {
                    if let member @ SymbolAndQualifiers(Symbol::Type(ty, boundness), qualifiers) =
                        class.own_instance_member(db, name)
                    {
                        // TODO: We could raise a diagnostic here if there are conflicting type qualifiers
                        union_qualifiers = union_qualifiers.union(qualifiers);

                        if boundness == Boundness::Bound {
                            if union.is_empty() {
                                // Short-circuit, no need to allocate inside the union builder
                                return member;
                            }

                            return SymbolAndQualifiers(
                                Symbol::bound(union.add(ty).build()),
                                union_qualifiers,
                            );
                        }

                        // If we see a possibly-unbound symbol, we need to keep looking
                        // higher up in the MRO.
                        union = union.add(ty);
                    }
                }
            }
        }

        if union.is_empty() {
            SymbolAndQualifiers(Symbol::Unbound, TypeQualifiers::empty())
        } else {
            // If we have reached this point, we know that we have only seen possibly-unbound symbols.
            // This means that the final result is still possibly-unbound.

            SymbolAndQualifiers(
                Symbol::Type(union.build(), Boundness::PossiblyUnbound),
                union_qualifiers,
            )
        }
    }

    /// Tries to find declarations/bindings of an instance attribute named `name` that are only
    /// "implicitly" defined in a method of the class that corresponds to `class_body_scope`.
    fn implicit_instance_attribute(
        db: &'db dyn Db,
        class_body_scope: ScopeId<'db>,
        name: &str,
        inferred_from_class_body: &Symbol<'db>,
    ) -> Symbol<'db> {
        // If we do not see any declarations of an attribute, neither in the class body nor in
        // any method, we build a union of `Unknown` with the inferred types of all bindings of
        // that attribute. We include `Unknown` in that union to account for the fact that the
        // attribute might be externally modified.
        let mut union_of_inferred_types = UnionBuilder::new(db).add(Type::unknown());
        let mut union_boundness = Boundness::Bound;

        if let Symbol::Type(ty, boundness) = inferred_from_class_body {
            union_of_inferred_types = union_of_inferred_types.add(*ty);
            union_boundness = *boundness;
        }

        let attribute_assignments = attribute_assignments(db, class_body_scope);

        let Some(attribute_assignments) = attribute_assignments
            .as_deref()
            .and_then(|assignments| assignments.get(name))
        else {
            if inferred_from_class_body.is_unbound() {
                return Symbol::Unbound;
            }
            return Symbol::Type(union_of_inferred_types.build(), union_boundness);
        };

        for attribute_assignment in attribute_assignments {
            match attribute_assignment {
                AttributeAssignment::Annotated { annotation } => {
                    // We found an annotated assignment of one of the following forms (using 'self' in these
                    // examples, but we support arbitrary names for the first parameters of methods):
                    //
                    //     self.name: <annotation>
                    //     self.name: <annotation> = …

                    let annotation_ty = infer_expression_type(db, *annotation);

                    // TODO: check if there are conflicting declarations
                    return Symbol::bound(annotation_ty);
                }
                AttributeAssignment::Unannotated { value } => {
                    // We found an un-annotated attribute assignment of the form:
                    //
                    //     self.name = <value>

                    let inferred_ty = infer_expression_type(db, *value);

                    union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                }
                AttributeAssignment::Iterable { iterable } => {
                    // We found an attribute assignment like:
                    //
                    //     for self.name in <iterable>:

                    let iterable_ty = infer_expression_type(db, *iterable);
                    // TODO: Potential diagnostics resulting from the iterable are currently not reported.
                    let inferred_ty = iterable_ty.iterate(db);

                    union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                }
                AttributeAssignment::Unpack {
                    attribute_expression_id,
                    unpack,
                } => {
                    // We found an unpacking assignment like:
                    //
                    //     .., self.name, .. = <value>
                    //     (.., self.name, ..) = <value>
                    //     [.., self.name, ..] = <value>

                    let inferred_ty =
                        infer_unpack_types(db, *unpack).expression_type(*attribute_expression_id);
                    union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                }
            }
        }

        Symbol::Type(union_of_inferred_types.build(), union_boundness)
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    fn own_instance_member(self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        // TODO: There are many things that are not yet implemented here:
        // - `typing.Final`
        // - Proper diagnostics

        let body_scope = self.body_scope(db);
        let table = symbol_table(db, body_scope);

        if let Some(symbol_id) = table.symbol_id_by_name(name) {
            let use_def = use_def_map(db, body_scope);

            let declarations = use_def.public_declarations(symbol_id);

            match symbol_from_declarations(db, declarations) {
                Ok(SymbolAndQualifiers(declared @ Symbol::Type(declared_ty, _), qualifiers)) => {
                    // The attribute is declared in the class body.

                    if let Some(function) = declared_ty.into_function_literal() {
                        // TODO: Eventually, we are going to process all decorators correctly. This is
                        // just a temporary heuristic to provide a broad categorization

                        if function.has_known_class_decorator(db, KnownClass::Classmethod)
                            && function.decorators(db).len() == 1
                        {
                            SymbolAndQualifiers(declared, qualifiers)
                        } else if function.has_known_class_decorator(db, KnownClass::Property) {
                            SymbolAndQualifiers::todo("@property")
                        } else if function.has_known_function_decorator(db, KnownFunction::Overload)
                        {
                            SymbolAndQualifiers::todo("overloaded method")
                        } else if !function.decorators(db).is_empty() {
                            SymbolAndQualifiers::todo("decorated method")
                        } else {
                            SymbolAndQualifiers(declared, qualifiers)
                        }
                    } else {
                        SymbolAndQualifiers(declared, qualifiers)
                    }
                }
                Ok(SymbolAndQualifiers(Symbol::Unbound, _)) => {
                    // The attribute is not *declared* in the class body. It could still be declared
                    // in a method, and it could also be *bound* in the class body (and/or in a method).

                    let bindings = use_def.public_bindings(symbol_id);
                    let inferred = symbol_from_bindings(db, bindings);

                    Self::implicit_instance_attribute(db, body_scope, name, &inferred).into()
                }
                Err((declared_ty, _conflicting_declarations)) => {
                    // There are conflicting declarations for this attribute in the class body.
                    SymbolAndQualifiers(
                        Symbol::bound(declared_ty.inner_type()),
                        declared_ty.qualifiers(),
                    )
                }
            }
        } else {
            // This attribute is neither declared nor bound in the class body.
            // It could still be implicitly defined in a method.

            Self::implicit_instance_attribute(db, body_scope, name, &Symbol::Unbound).into()
        }
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    #[salsa::tracked]
    pub(super) fn inheritance_cycle(self, db: &'db dyn Db) -> Option<InheritanceCycle> {
        /// Return `true` if the class is cyclically defined.
        ///
        /// Also, populates `visited_classes` with all base classes of `self`.
        fn is_cyclically_defined_recursive<'db>(
            db: &'db dyn Db,
            class: Class<'db>,
            classes_on_stack: &mut IndexSet<Class<'db>>,
            visited_classes: &mut IndexSet<Class<'db>>,
        ) -> bool {
            let mut result = false;
            for explicit_base_class in class.fully_static_explicit_bases(db) {
                if !classes_on_stack.insert(explicit_base_class) {
                    return true;
                }

                if visited_classes.insert(explicit_base_class) {
                    // If we find a cycle, keep searching to check if we can reach the starting class.
                    result |= is_cyclically_defined_recursive(
                        db,
                        explicit_base_class,
                        classes_on_stack,
                        visited_classes,
                    );
                }

                classes_on_stack.pop();
            }
            result
        }

        let visited_classes = &mut IndexSet::new();
        if !is_cyclically_defined_recursive(db, self, &mut IndexSet::new(), visited_classes) {
            None
        } else if visited_classes.contains(&self) {
            Some(InheritanceCycle::Participant)
        } else {
            Some(InheritanceCycle::Inherited)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) enum InheritanceCycle {
    /// The class is cyclically defined and is a participant in the cycle.
    /// i.e., it inherits either directly or indirectly from itself.
    Participant,
    /// The class inherits from a class that is a `Participant` in an inheritance cycle,
    /// but is not itself a participant.
    Inherited,
}

impl InheritanceCycle {
    pub(super) const fn is_participant(self) -> bool {
        matches!(self, InheritanceCycle::Participant)
    }
}

/// A singleton type representing a single class object at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct ClassLiteralType<'db> {
    pub(super) class: Class<'db>,
}

impl<'db> ClassLiteralType<'db> {
    pub(crate) fn class(self) -> Class<'db> {
        self.class
    }

    pub(crate) fn body_scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.class.body_scope(db)
    }

    pub(super) fn static_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        self.class.class_member(db, name)
    }
}

impl<'db> From<ClassLiteralType<'db>> for Type<'db> {
    fn from(value: ClassLiteralType<'db>) -> Self {
        Self::ClassLiteral(value)
    }
}

/// A type representing the set of runtime objects which are instances of a certain class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update)]
pub struct InstanceType<'db> {
    pub(super) class: Class<'db>,
}

impl<'db> InstanceType<'db> {
    pub(super) fn class(self) -> Class<'db> {
        self.class
    }

    pub(super) fn is_subtype_of(self, db: &'db dyn Db, other: InstanceType<'db>) -> bool {
        // N.B. The subclass relation is fully static
        self.class.is_subclass_of(db, other.class)
    }
}

impl<'db> From<InstanceType<'db>> for Type<'db> {
    fn from(value: InstanceType<'db>) -> Self {
        Self::Instance(value)
    }
}
/// Non-exhaustive enumeration of known classes (e.g. `builtins.int`, `typing.Any`, ...) to allow
/// for easier syntax when interacting with very common classes.
///
/// Feel free to expand this enum if you ever find yourself using the same class in multiple
/// places.
/// Note: good candidates are any classes in `[crate::module_resolver::module::KnownModule]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(strum_macros::EnumIter))]
pub enum KnownClass {
    // To figure out where an stdlib symbol is defined, you can go into `crates/red_knot_vendored`
    // and grep for the symbol name in any `.pyi` file.

    // Builtins
    Bool,
    Object,
    Bytes,
    Type,
    Int,
    Float,
    Complex,
    Str,
    List,
    Tuple,
    Set,
    FrozenSet,
    Dict,
    Slice,
    Range,
    Property,
    BaseException,
    BaseExceptionGroup,
    Classmethod,
    // Types
    GenericAlias,
    ModuleType,
    FunctionType,
    MethodType,
    MethodWrapperType,
    WrapperDescriptorType,
    // Typeshed
    NoneType, // Part of `types` for Python >= 3.10
    // Typing
    StdlibAlias,
    SpecialForm,
    TypeVar,
    TypeAliasType,
    NoDefaultType,
    // TODO: This can probably be removed when we have support for protocols
    SupportsIndex,
    // Collections
    ChainMap,
    Counter,
    DefaultDict,
    Deque,
    OrderedDict,
    // sys
    VersionInfo,
    // Exposed as `types.EllipsisType` on Python >=3.10;
    // backported as `builtins.ellipsis` by typeshed on Python <=3.9
    EllipsisType,
}

impl<'db> KnownClass {
    pub(crate) const fn is_bool(self) -> bool {
        matches!(self, Self::Bool)
    }

    /// Determine whether instances of this class are always truthy, always falsy,
    /// or have an ambiguous truthiness.
    pub(crate) const fn bool(self) -> Truthiness {
        match self {
            // N.B. It's only generally safe to infer `Truthiness::AlwaysTrue` for a `KnownClass`
            // variant if the class's `__bool__` method always returns the same thing *and* the
            // class is `@final`.
            //
            // E.g. `ModuleType.__bool__` always returns `True`, but `ModuleType` is not `@final`.
            // Equally, `range` is `@final`, but its `__bool__` method can return `False`.
            Self::EllipsisType
            | Self::NoDefaultType
            | Self::MethodType
            | Self::Slice
            | Self::FunctionType
            | Self::VersionInfo
            | Self::TypeAliasType
            | Self::TypeVar
            | Self::WrapperDescriptorType
            | Self::MethodWrapperType => Truthiness::AlwaysTrue,

            Self::NoneType => Truthiness::AlwaysFalse,

            Self::BaseException
            | Self::Object
            | Self::OrderedDict
            | Self::BaseExceptionGroup
            | Self::Bool
            | Self::Str
            | Self::List
            | Self::GenericAlias
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::Set
            | Self::Tuple
            | Self::Int
            | Self::Type
            | Self::Bytes
            | Self::FrozenSet
            | Self::Range
            | Self::Property
            | Self::SpecialForm
            | Self::Dict
            | Self::ModuleType
            | Self::ChainMap
            | Self::Complex
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::Float
            | Self::Classmethod => Truthiness::Ambiguous,
        }
    }

    pub(crate) fn as_str(self, db: &'db dyn Db) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::Object => "object",
            Self::Bytes => "bytes",
            Self::Tuple => "tuple",
            Self::Int => "int",
            Self::Float => "float",
            Self::Complex => "complex",
            Self::FrozenSet => "frozenset",
            Self::Str => "str",
            Self::Set => "set",
            Self::Dict => "dict",
            Self::List => "list",
            Self::Type => "type",
            Self::Slice => "slice",
            Self::Range => "range",
            Self::Property => "property",
            Self::BaseException => "BaseException",
            Self::BaseExceptionGroup => "BaseExceptionGroup",
            Self::Classmethod => "classmethod",
            Self::GenericAlias => "GenericAlias",
            Self::ModuleType => "ModuleType",
            Self::FunctionType => "FunctionType",
            Self::MethodType => "MethodType",
            Self::MethodWrapperType => "MethodWrapperType",
            Self::WrapperDescriptorType => "WrapperDescriptorType",
            Self::NoneType => "NoneType",
            Self::SpecialForm => "_SpecialForm",
            Self::TypeVar => "TypeVar",
            Self::TypeAliasType => "TypeAliasType",
            Self::NoDefaultType => "_NoDefaultType",
            Self::SupportsIndex => "SupportsIndex",
            Self::ChainMap => "ChainMap",
            Self::Counter => "Counter",
            Self::DefaultDict => "defaultdict",
            Self::Deque => "deque",
            Self::OrderedDict => "OrderedDict",
            // For example, `typing.List` is defined as `List = _Alias()` in typeshed
            Self::StdlibAlias => "_Alias",
            // This is the name the type of `sys.version_info` has in typeshed,
            // which is different to what `type(sys.version_info).__name__` is at runtime.
            // (At runtime, `type(sys.version_info).__name__ == "version_info"`,
            // which is impossible to replicate in the stubs since the sole instance of the class
            // also has that name in the `sys` module.)
            Self::VersionInfo => "_version_info",
            Self::EllipsisType => {
                // Exposed as `types.EllipsisType` on Python >=3.10;
                // backported as `builtins.ellipsis` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    "EllipsisType"
                } else {
                    "ellipsis"
                }
            }
        }
    }

    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        self.to_class_literal(db).to_instance(db)
    }

    pub(crate) fn to_class_literal(self, db: &'db dyn Db) -> Type<'db> {
        known_module_symbol(db, self.canonical_module(db), self.as_str(db))
            .ignore_possibly_unbound()
            .unwrap_or(Type::unknown())
    }

    pub(crate) fn to_subclass_of(self, db: &'db dyn Db) -> Type<'db> {
        self.to_class_literal(db)
            .into_class_literal()
            .map(|ClassLiteralType { class }| SubclassOfType::from(db, class))
            .unwrap_or_else(SubclassOfType::subclass_of_unknown)
    }

    /// Return `true` if this symbol can be resolved to a class definition `class` in typeshed,
    /// *and* `class` is a subclass of `other`.
    pub(super) fn is_subclass_of(self, db: &'db dyn Db, other: Class<'db>) -> bool {
        known_module_symbol(db, self.canonical_module(db), self.as_str(db))
            .ignore_possibly_unbound()
            .and_then(Type::into_class_literal)
            .is_some_and(|ClassLiteralType { class }| class.is_subclass_of(db, other))
    }

    /// Return the module in which we should look up the definition for this class
    fn canonical_module(self, db: &'db dyn Db) -> KnownModule {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Classmethod
            | Self::Slice
            | Self::Range
            | Self::Property => KnownModule::Builtins,
            Self::VersionInfo => KnownModule::Sys,
            Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType => KnownModule::Types,
            Self::NoneType => KnownModule::Typeshed,
            Self::SpecialForm
            | Self::TypeVar
            | Self::TypeAliasType
            | Self::StdlibAlias
            | Self::SupportsIndex => KnownModule::Typing,
            Self::NoDefaultType => {
                let python_version = Program::get(db).python_version(db);

                // typing_extensions has a 3.13+ re-export for the `typing.NoDefault`
                // singleton, but not for `typing._NoDefaultType`. So we need to switch
                // to `typing._NoDefaultType` for newer versions:
                if python_version >= PythonVersion::PY313 {
                    KnownModule::Typing
                } else {
                    KnownModule::TypingExtensions
                }
            }
            Self::EllipsisType => {
                // Exposed as `types.EllipsisType` on Python >=3.10;
                // backported as `builtins.ellipsis` by typeshed on Python <=3.9
                if Program::get(db).python_version(db) >= PythonVersion::PY310 {
                    KnownModule::Types
                } else {
                    KnownModule::Builtins
                }
            }
            Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict => KnownModule::Collections,
        }
    }

    /// Return true if all instances of this `KnownClass` compare equal.
    pub(super) const fn is_single_valued(self) -> bool {
        match self {
            KnownClass::NoneType
            | KnownClass::NoDefaultType
            | KnownClass::VersionInfo
            | KnownClass::EllipsisType
            | KnownClass::TypeAliasType => true,

            KnownClass::Bool
            | KnownClass::Object
            | KnownClass::Bytes
            | KnownClass::Type
            | KnownClass::Int
            | KnownClass::Float
            | KnownClass::Complex
            | KnownClass::Str
            | KnownClass::List
            | KnownClass::Tuple
            | KnownClass::Set
            | KnownClass::FrozenSet
            | KnownClass::Dict
            | KnownClass::Slice
            | KnownClass::Range
            | KnownClass::Property
            | KnownClass::BaseException
            | KnownClass::BaseExceptionGroup
            | KnownClass::Classmethod
            | KnownClass::GenericAlias
            | KnownClass::ModuleType
            | KnownClass::FunctionType
            | KnownClass::MethodType
            | KnownClass::MethodWrapperType
            | KnownClass::WrapperDescriptorType
            | KnownClass::SpecialForm
            | KnownClass::ChainMap
            | KnownClass::Counter
            | KnownClass::DefaultDict
            | KnownClass::Deque
            | KnownClass::OrderedDict
            | KnownClass::SupportsIndex
            | KnownClass::StdlibAlias
            | KnownClass::TypeVar => false,
        }
    }

    /// Is this class a singleton class?
    ///
    /// A singleton class is a class where it is known that only one instance can ever exist at runtime.
    pub(super) const fn is_singleton(self) -> bool {
        // TODO there are other singleton types (NotImplementedType -- any others?)
        match self {
            Self::NoneType
            | Self::EllipsisType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::TypeAliasType => true,

            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Tuple
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::List
            | Self::Type
            | Self::Slice
            | Self::Range
            | Self::Property
            | Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType
            | Self::SpecialForm
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Classmethod
            | Self::TypeVar => false,
        }
    }

    pub(super) fn try_from_file_and_name(
        db: &dyn Db,
        file: File,
        class_name: &str,
    ) -> Option<Self> {
        // We assert that this match is exhaustive over the right-hand side in the unit test
        // `known_class_roundtrip_from_str()`
        let candidate = match class_name {
            "bool" => Self::Bool,
            "object" => Self::Object,
            "bytes" => Self::Bytes,
            "tuple" => Self::Tuple,
            "type" => Self::Type,
            "int" => Self::Int,
            "float" => Self::Float,
            "complex" => Self::Complex,
            "str" => Self::Str,
            "set" => Self::Set,
            "frozenset" => Self::FrozenSet,
            "dict" => Self::Dict,
            "list" => Self::List,
            "slice" => Self::Slice,
            "range" => Self::Range,
            "property" => Self::Property,
            "BaseException" => Self::BaseException,
            "BaseExceptionGroup" => Self::BaseExceptionGroup,
            "classmethod" => Self::Classmethod,
            "GenericAlias" => Self::GenericAlias,
            "NoneType" => Self::NoneType,
            "ModuleType" => Self::ModuleType,
            "FunctionType" => Self::FunctionType,
            "MethodType" => Self::MethodType,
            "MethodWrapperType" => Self::MethodWrapperType,
            "WrapperDescriptorType" => Self::WrapperDescriptorType,
            "TypeAliasType" => Self::TypeAliasType,
            "TypeVar" => Self::TypeVar,
            "ChainMap" => Self::ChainMap,
            "Counter" => Self::Counter,
            "defaultdict" => Self::DefaultDict,
            "deque" => Self::Deque,
            "OrderedDict" => Self::OrderedDict,
            "_Alias" => Self::StdlibAlias,
            "_SpecialForm" => Self::SpecialForm,
            "_NoDefaultType" => Self::NoDefaultType,
            "SupportsIndex" => Self::SupportsIndex,
            "_version_info" => Self::VersionInfo,
            "ellipsis" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                Self::EllipsisType
            }
            "EllipsisType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                Self::EllipsisType
            }
            _ => return None,
        };

        candidate
            .check_module(db, file_to_module(db, file)?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if the module of `self` matches `module`
    fn check_module(self, db: &'db dyn Db, module: KnownModule) -> bool {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Type
            | Self::Int
            | Self::Float
            | Self::Complex
            | Self::Str
            | Self::List
            | Self::Tuple
            | Self::Set
            | Self::FrozenSet
            | Self::Dict
            | Self::Slice
            | Self::Range
            | Self::Property
            | Self::GenericAlias
            | Self::ChainMap
            | Self::Counter
            | Self::DefaultDict
            | Self::Deque
            | Self::OrderedDict
            | Self::StdlibAlias  // no equivalent class exists in typing_extensions, nor ever will
            | Self::ModuleType
            | Self::VersionInfo
            | Self::BaseException
            | Self::EllipsisType
            | Self::BaseExceptionGroup
            | Self::Classmethod
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::WrapperDescriptorType => module == self.canonical_module(db),
            Self::NoneType => matches!(module, KnownModule::Typeshed | KnownModule::Types),
            Self::SpecialForm | Self::TypeVar | Self::TypeAliasType | Self::NoDefaultType | Self::SupportsIndex => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
        }
    }
}

/// Enumeration of specific runtime that are special enough to be considered their own type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub enum KnownInstanceType<'db> {
    /// The symbol `typing.Annotated` (which can also be found as `typing_extensions.Annotated`)
    Annotated,
    /// The symbol `typing.Literal` (which can also be found as `typing_extensions.Literal`)
    Literal,
    /// The symbol `typing.LiteralString` (which can also be found as `typing_extensions.LiteralString`)
    LiteralString,
    /// The symbol `typing.Optional` (which can also be found as `typing_extensions.Optional`)
    Optional,
    /// The symbol `typing.Union` (which can also be found as `typing_extensions.Union`)
    Union,
    /// The symbol `typing.NoReturn` (which can also be found as `typing_extensions.NoReturn`)
    NoReturn,
    /// The symbol `typing.Never` available since 3.11 (which can also be found as `typing_extensions.Never`)
    Never,
    /// The symbol `typing.Any` (which can also be found as `typing_extensions.Any`)
    Any,
    /// The symbol `typing.Tuple` (which can also be found as `typing_extensions.Tuple`)
    Tuple,
    /// The symbol `typing.List` (which can also be found as `typing_extensions.List`)
    List,
    /// The symbol `typing.Dict` (which can also be found as `typing_extensions.Dict`)
    Dict,
    /// The symbol `typing.Set` (which can also be found as `typing_extensions.Set`)
    Set,
    /// The symbol `typing.FrozenSet` (which can also be found as `typing_extensions.FrozenSet`)
    FrozenSet,
    /// The symbol `typing.ChainMap` (which can also be found as `typing_extensions.ChainMap`)
    ChainMap,
    /// The symbol `typing.Counter` (which can also be found as `typing_extensions.Counter`)
    Counter,
    /// The symbol `typing.DefaultDict` (which can also be found as `typing_extensions.DefaultDict`)
    DefaultDict,
    /// The symbol `typing.Deque` (which can also be found as `typing_extensions.Deque`)
    Deque,
    /// The symbol `typing.OrderedDict` (which can also be found as `typing_extensions.OrderedDict`)
    OrderedDict,
    /// The symbol `typing.Type` (which can also be found as `typing_extensions.Type`)
    Type,
    /// A single instance of `typing.TypeVar`
    TypeVar(TypeVarInstance<'db>),
    /// A single instance of `typing.TypeAliasType` (PEP 695 type alias)
    TypeAliasType(TypeAliasType<'db>),
    /// The symbol `knot_extensions.Unknown`
    Unknown,
    /// The symbol `knot_extensions.AlwaysTruthy`
    AlwaysTruthy,
    /// The symbol `knot_extensions.AlwaysFalsy`
    AlwaysFalsy,
    /// The symbol `knot_extensions.Not`
    Not,
    /// The symbol `knot_extensions.Intersection`
    Intersection,
    /// The symbol `knot_extensions.TypeOf`
    TypeOf,

    // Various special forms, special aliases and type qualifiers that we don't yet understand
    // (all currently inferred as TODO in most contexts):
    TypingSelf,
    Final,
    ClassVar,
    Callable,
    Concatenate,
    Unpack,
    Required,
    NotRequired,
    TypeAlias,
    TypeGuard,
    TypeIs,
    ReadOnly,
    // TODO: fill this enum out with more special forms, etc.
}

impl<'db> KnownInstanceType<'db> {
    /// Evaluate the known instance in boolean context
    pub(crate) const fn bool(self) -> Truthiness {
        match self {
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Optional
            | Self::TypeVar(_)
            | Self::Union
            | Self::NoReturn
            | Self::Never
            | Self::Any
            | Self::Tuple
            | Self::Type
            | Self::TypingSelf
            | Self::Final
            | Self::ClassVar
            | Self::Callable
            | Self::Concatenate
            | Self::Unpack
            | Self::Required
            | Self::NotRequired
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypeIs
            | Self::List
            | Self::Dict
            | Self::DefaultDict
            | Self::Set
            | Self::FrozenSet
            | Self::Counter
            | Self::Deque
            | Self::ChainMap
            | Self::OrderedDict
            | Self::ReadOnly
            | Self::TypeAliasType(_)
            | Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf => Truthiness::AlwaysTrue,
        }
    }

    /// Return the repr of the symbol at runtime
    pub(crate) fn repr(self, db: &'db dyn Db) -> &'db str {
        match self {
            Self::Annotated => "typing.Annotated",
            Self::Literal => "typing.Literal",
            Self::LiteralString => "typing.LiteralString",
            Self::Optional => "typing.Optional",
            Self::Union => "typing.Union",
            Self::NoReturn => "typing.NoReturn",
            Self::Never => "typing.Never",
            Self::Any => "typing.Any",
            Self::Tuple => "typing.Tuple",
            Self::Type => "typing.Type",
            Self::TypingSelf => "typing.Self",
            Self::Final => "typing.Final",
            Self::ClassVar => "typing.ClassVar",
            Self::Callable => "typing.Callable",
            Self::Concatenate => "typing.Concatenate",
            Self::Unpack => "typing.Unpack",
            Self::Required => "typing.Required",
            Self::NotRequired => "typing.NotRequired",
            Self::TypeAlias => "typing.TypeAlias",
            Self::TypeGuard => "typing.TypeGuard",
            Self::TypeIs => "typing.TypeIs",
            Self::List => "typing.List",
            Self::Dict => "typing.Dict",
            Self::DefaultDict => "typing.DefaultDict",
            Self::Set => "typing.Set",
            Self::FrozenSet => "typing.FrozenSet",
            Self::Counter => "typing.Counter",
            Self::Deque => "typing.Deque",
            Self::ChainMap => "typing.ChainMap",
            Self::OrderedDict => "typing.OrderedDict",
            Self::ReadOnly => "typing.ReadOnly",
            Self::TypeVar(typevar) => typevar.name(db),
            Self::TypeAliasType(_) => "typing.TypeAliasType",
            Self::Unknown => "knot_extensions.Unknown",
            Self::AlwaysTruthy => "knot_extensions.AlwaysTruthy",
            Self::AlwaysFalsy => "knot_extensions.AlwaysFalsy",
            Self::Not => "knot_extensions.Not",
            Self::Intersection => "knot_extensions.Intersection",
            Self::TypeOf => "knot_extensions.TypeOf",
        }
    }

    /// Return the [`KnownClass`] which this symbol is an instance of
    pub(crate) const fn class(self) -> KnownClass {
        match self {
            Self::Annotated => KnownClass::SpecialForm,
            Self::Literal => KnownClass::SpecialForm,
            Self::LiteralString => KnownClass::SpecialForm,
            Self::Optional => KnownClass::SpecialForm,
            Self::Union => KnownClass::SpecialForm,
            Self::NoReturn => KnownClass::SpecialForm,
            Self::Never => KnownClass::SpecialForm,
            Self::Any => KnownClass::Object,
            Self::Tuple => KnownClass::SpecialForm,
            Self::Type => KnownClass::SpecialForm,
            Self::TypingSelf => KnownClass::SpecialForm,
            Self::Final => KnownClass::SpecialForm,
            Self::ClassVar => KnownClass::SpecialForm,
            Self::Callable => KnownClass::SpecialForm,
            Self::Concatenate => KnownClass::SpecialForm,
            Self::Unpack => KnownClass::SpecialForm,
            Self::Required => KnownClass::SpecialForm,
            Self::NotRequired => KnownClass::SpecialForm,
            Self::TypeAlias => KnownClass::SpecialForm,
            Self::TypeGuard => KnownClass::SpecialForm,
            Self::TypeIs => KnownClass::SpecialForm,
            Self::ReadOnly => KnownClass::SpecialForm,
            Self::List => KnownClass::StdlibAlias,
            Self::Dict => KnownClass::StdlibAlias,
            Self::DefaultDict => KnownClass::StdlibAlias,
            Self::Set => KnownClass::StdlibAlias,
            Self::FrozenSet => KnownClass::StdlibAlias,
            Self::Counter => KnownClass::StdlibAlias,
            Self::Deque => KnownClass::StdlibAlias,
            Self::ChainMap => KnownClass::StdlibAlias,
            Self::OrderedDict => KnownClass::StdlibAlias,
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
            Self::TypeOf => KnownClass::SpecialForm,
            Self::Not => KnownClass::SpecialForm,
            Self::Intersection => KnownClass::SpecialForm,
            Self::Unknown => KnownClass::Object,
            Self::AlwaysTruthy => KnownClass::Object,
            Self::AlwaysFalsy => KnownClass::Object,
        }
    }

    /// Return the instance type which this type is a subtype of.
    ///
    /// For example, the symbol `typing.Literal` is an instance of `typing._SpecialForm`,
    /// so `KnownInstanceType::Literal.instance_fallback(db)`
    /// returns `Type::Instance(InstanceType { class: <typing._SpecialForm> })`.
    pub(super) fn instance_fallback(self, db: &dyn Db) -> Type {
        self.class().to_instance(db)
    }

    /// Return `true` if this symbol is an instance of `class`.
    pub(super) fn is_instance_of(self, db: &'db dyn Db, class: Class<'db>) -> bool {
        self.class().is_subclass_of(db, class)
    }

    pub(super) fn try_from_file_and_name(
        db: &'db dyn Db,
        file: File,
        symbol_name: &str,
    ) -> Option<Self> {
        let candidate = match symbol_name {
            "Any" => Self::Any,
            "ClassVar" => Self::ClassVar,
            "Deque" => Self::Deque,
            "List" => Self::List,
            "Dict" => Self::Dict,
            "DefaultDict" => Self::DefaultDict,
            "Set" => Self::Set,
            "FrozenSet" => Self::FrozenSet,
            "Counter" => Self::Counter,
            "ChainMap" => Self::ChainMap,
            "OrderedDict" => Self::OrderedDict,
            "Optional" => Self::Optional,
            "Union" => Self::Union,
            "NoReturn" => Self::NoReturn,
            "Tuple" => Self::Tuple,
            "Type" => Self::Type,
            "Callable" => Self::Callable,
            "Annotated" => Self::Annotated,
            "Literal" => Self::Literal,
            "Never" => Self::Never,
            "Self" => Self::TypingSelf,
            "Final" => Self::Final,
            "Unpack" => Self::Unpack,
            "Required" => Self::Required,
            "TypeAlias" => Self::TypeAlias,
            "TypeGuard" => Self::TypeGuard,
            "TypeIs" => Self::TypeIs,
            "ReadOnly" => Self::ReadOnly,
            "Concatenate" => Self::Concatenate,
            "NotRequired" => Self::NotRequired,
            "LiteralString" => Self::LiteralString,
            "Unknown" => Self::Unknown,
            "AlwaysTruthy" => Self::AlwaysTruthy,
            "AlwaysFalsy" => Self::AlwaysFalsy,
            "Not" => Self::Not,
            "Intersection" => Self::Intersection,
            "TypeOf" => Self::TypeOf,
            _ => return None,
        };

        candidate
            .check_module(file_to_module(db, file)?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if `module` is a module from which this `KnownInstance` variant can validly originate.
    ///
    /// Most variants can only exist in one module, which is the same as `self.class().canonical_module()`.
    /// Some variants could validly be defined in either `typing` or `typing_extensions`, however.
    fn check_module(self, module: KnownModule) -> bool {
        match self {
            Self::Any
            | Self::ClassVar
            | Self::Deque
            | Self::List
            | Self::Dict
            | Self::DefaultDict
            | Self::Set
            | Self::FrozenSet
            | Self::Counter
            | Self::ChainMap
            | Self::OrderedDict
            | Self::Optional
            | Self::Union
            | Self::NoReturn
            | Self::Tuple
            | Self::Type
            | Self::Callable => module.is_typing(),
            Self::Annotated
            | Self::Literal
            | Self::LiteralString
            | Self::Never
            | Self::TypingSelf
            | Self::Final
            | Self::Concatenate
            | Self::Unpack
            | Self::Required
            | Self::NotRequired
            | Self::TypeAlias
            | Self::TypeGuard
            | Self::TypeIs
            | Self::ReadOnly
            | Self::TypeAliasType(_)
            | Self::TypeVar(_) => {
                matches!(module, KnownModule::Typing | KnownModule::TypingExtensions)
            }
            Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf => module.is_knot_extensions(),
        }
    }

    pub(super) fn static_member(self, db: &'db dyn Db, name: &str) -> Symbol<'db> {
        let ty = match (self, name) {
            (Self::TypeVar(typevar), "__name__") => Type::string_literal(db, typevar.name(db)),
            (Self::TypeAliasType(alias), "__name__") => Type::string_literal(db, alias.name(db)),
            _ => return self.instance_fallback(db).static_member(db, name),
        };
        Symbol::bound(ty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(super) struct MetaclassError<'db> {
    kind: MetaclassErrorKind<'db>,
}

impl<'db> MetaclassError<'db> {
    /// Return an [`MetaclassErrorKind`] variant describing why we could not resolve the metaclass for this class.
    pub(super) fn reason(&self) -> &MetaclassErrorKind<'db> {
        &self.kind
    }
}

#[derive(Debug, Clone, PartialEq, Eq, salsa::Update)]
pub(super) enum MetaclassErrorKind<'db> {
    /// The class has incompatible metaclasses in its inheritance hierarchy.
    ///
    /// The metaclass of a derived class must be a (non-strict) subclass of the metaclasses of all
    /// its bases.
    Conflict {
        /// `candidate1` will either be the explicit `metaclass=` keyword in the class definition,
        /// or the inferred metaclass of a base class
        candidate1: MetaclassCandidate<'db>,

        /// `candidate2` will always be the inferred metaclass of a base class
        candidate2: MetaclassCandidate<'db>,

        /// Flag to indicate whether `candidate1` is the explicit `metaclass=` keyword or the
        /// inferred metaclass of a base class. This helps us give better error messages in diagnostics.
        candidate1_is_base_class: bool,
    },
    /// The metaclass is not callable
    NotCallable(Type<'db>),
    /// The metaclass is of a union type whose some members are not callable
    PartlyNotCallable(Type<'db>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::setup_db;
    use crate::module_resolver::resolve_module;
    use strum::IntoEnumIterator;

    #[test]
    fn known_class_roundtrip_from_str() {
        let db = setup_db();
        for class in KnownClass::iter() {
            let class_name = class.as_str(&db);
            let class_module = resolve_module(&db, &class.canonical_module(&db).name()).unwrap();

            assert_eq!(
                KnownClass::try_from_file_and_name(&db, class_module.file(), class_name),
                Some(class),
                "`KnownClass::candidate_from_str` appears to be missing a case for `{class_name}`"
            );
        }
    }
}
