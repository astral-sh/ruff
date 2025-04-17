use std::hash::BuildHasherDefault;
use std::sync::{LazyLock, Mutex};

use super::{
    class_base::ClassBase, infer_expression_type, infer_unpack_types, IntersectionBuilder,
    KnownFunction, MemberLookupPolicy, Mro, MroError, MroIterator, SubclassOfType, Truthiness,
    Type, TypeAliasType, TypeQualifiers, TypeVarInstance,
};
use crate::semantic_index::definition::Definition;
use crate::semantic_index::DeclarationWithConstraint;
use crate::types::generics::{GenericContext, Specialization};
use crate::types::signatures::{Parameter, Parameters};
use crate::types::{CallableType, DataclassMetadata, Signature};
use crate::{
    module_resolver::file_to_module,
    semantic_index::{
        ast_ids::HasScopedExpressionId,
        attribute_assignments,
        definition::{DefinitionKind, TargetKind},
        semantic_index,
        symbol::ScopeId,
        symbol_table, use_def_map,
    },
    symbol::{
        class_symbol, known_module_symbol, symbol_from_bindings, symbol_from_declarations,
        Boundness, LookupError, LookupResult, Symbol, SymbolAndQualifiers,
    },
    types::{
        definition_expression_type, CallArgumentTypes, CallError, CallErrorKind, DynamicType,
        MetaclassCandidate, TupleType, UnionBuilder, UnionType,
    },
    Db, KnownModule, Program,
};
use indexmap::IndexSet;
use itertools::Itertools as _;
use ruff_db::files::File;
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, PythonVersion};
use rustc_hash::{FxHashSet, FxHasher};

type FxOrderMap<K, V> = ordermap::map::OrderMap<K, V, BuildHasherDefault<FxHasher>>;

fn explicit_bases_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &[Type<'db>],
    _count: u32,
    _self: ClassLiteralType<'db>,
) -> salsa::CycleRecoveryAction<Box<[Type<'db>]>> {
    salsa::CycleRecoveryAction::Iterate
}

fn explicit_bases_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: ClassLiteralType<'db>,
) -> Box<[Type<'db>]> {
    Box::default()
}

fn try_mro_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Result<Mro<'db>, MroError<'db>>,
    _count: u32,
    _self: ClassLiteralType<'db>,
    _specialization: Option<Specialization<'db>>,
) -> salsa::CycleRecoveryAction<Result<Mro<'db>, MroError<'db>>> {
    salsa::CycleRecoveryAction::Iterate
}

#[allow(clippy::unnecessary_wraps)]
fn try_mro_cycle_initial<'db>(
    db: &'db dyn Db,
    self_: ClassLiteralType<'db>,
    specialization: Option<Specialization<'db>>,
) -> Result<Mro<'db>, MroError<'db>> {
    Ok(Mro::from_error(
        db,
        self_.apply_optional_specialization(db, specialization),
    ))
}

#[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
fn inheritance_cycle_recover<'db>(
    _db: &'db dyn Db,
    _value: &Option<InheritanceCycle>,
    _count: u32,
    _self: ClassLiteralType<'db>,
) -> salsa::CycleRecoveryAction<Option<InheritanceCycle>> {
    salsa::CycleRecoveryAction::Iterate
}

fn inheritance_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: ClassLiteralType<'db>,
) -> Option<InheritanceCycle> {
    None
}

/// Representation of a class definition statement in the AST. This does not in itself represent a
/// type, but is used as the inner data for several structs that *do* represent types.
#[derive(Clone, Debug, Eq, Hash, PartialEq, salsa::Update)]
pub struct Class<'db> {
    /// Name of the class at definition
    pub(crate) name: ast::name::Name,

    pub(crate) body_scope: ScopeId<'db>,

    pub(crate) known: Option<KnownClass>,

    pub(crate) dataclass_metadata: Option<DataclassMetadata>,
}

impl<'db> Class<'db> {
    fn file(&self, db: &dyn Db) -> File {
        self.body_scope.file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node(&self, db: &'db dyn Db) -> &'db ast::StmtClassDef {
        self.body_scope.node(db).expect_class()
    }

    fn definition(&self, db: &'db dyn Db) -> Definition<'db> {
        let index = semantic_index(db, self.body_scope.file(db));
        index.expect_single_definition(self.body_scope.node(db).expect_class())
    }
}

/// A [`Class`] that is not generic.
#[salsa::interned(debug)]
pub struct NonGenericClass<'db> {
    #[return_ref]
    pub(crate) class: Class<'db>,
}

impl<'db> From<NonGenericClass<'db>> for Type<'db> {
    fn from(class: NonGenericClass<'db>) -> Type<'db> {
        Type::ClassLiteral(ClassLiteralType::NonGeneric(class))
    }
}

/// A [`Class`] that is generic.
#[salsa::interned(debug)]
pub struct GenericClass<'db> {
    #[return_ref]
    pub(crate) class: Class<'db>,
    pub(crate) generic_context: GenericContext<'db>,
}

impl<'db> From<GenericClass<'db>> for Type<'db> {
    fn from(class: GenericClass<'db>) -> Type<'db> {
        Type::ClassLiteral(ClassLiteralType::Generic(class))
    }
}

/// A specialization of a generic class with a particular assignment of types to typevars.
#[salsa::interned(debug)]
pub struct GenericAlias<'db> {
    pub(crate) origin: GenericClass<'db>,
    pub(crate) specialization: Specialization<'db>,
}

impl<'db> GenericAlias<'db> {
    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.origin(db).class(db).definition(db)
    }
}

impl<'db> From<GenericAlias<'db>> for Type<'db> {
    fn from(alias: GenericAlias<'db>) -> Type<'db> {
        Type::GenericAlias(alias)
    }
}

/// Represents a class type, which might be a non-generic class, or a specialization of a generic
/// class.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, salsa::Supertype, salsa::Update,
)]
pub enum ClassType<'db> {
    NonGeneric(NonGenericClass<'db>),
    Generic(GenericAlias<'db>),
}

#[salsa::tracked]
impl<'db> ClassType<'db> {
    fn class(self, db: &'db dyn Db) -> &'db Class<'db> {
        match self {
            Self::NonGeneric(non_generic) => non_generic.class(db),
            Self::Generic(generic) => generic.origin(db).class(db),
        }
    }

    /// Returns the class literal and specialization for this class. For a non-generic class, this
    /// is the class itself. For a generic alias, this is the alias's origin.
    pub(crate) fn class_literal(
        self,
        db: &'db dyn Db,
    ) -> (ClassLiteralType<'db>, Option<Specialization<'db>>) {
        match self {
            Self::NonGeneric(non_generic) => (ClassLiteralType::NonGeneric(non_generic), None),
            Self::Generic(generic) => (
                ClassLiteralType::Generic(generic.origin(db)),
                Some(generic.specialization(db)),
            ),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        &self.class(db).name
    }

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownClass> {
        self.class(db).known
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.class(db).definition(db)
    }

    fn specialize_type(self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        match self {
            Self::NonGeneric(_) => ty,
            Self::Generic(generic) => ty.apply_specialization(db, generic.specialization(db)),
        }
    }

    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.class(db).known == Some(known_class)
    }

    /// Return `true` if this class represents the builtin class `object`
    pub(crate) fn is_object(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Object)
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`ClassLiteralType::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(super) fn iter_mro(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal.iter_mro(db, specialization)
    }

    /// Is this class final?
    pub(super) fn is_final(self, db: &'db dyn Db) -> bool {
        let (class_literal, _) = self.class_literal(db);
        class_literal.is_final(db)
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub(super) fn is_subclass_of(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        // `is_subclass_of` is checking the subtype relation, in which gradual types do not
        // participate, so we should not return `True` if we find `Any/Unknown` in the MRO.
        self.iter_mro(db).contains(&ClassBase::Class(other))
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(super) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        let (class_literal, _) = self.class_literal(db);
        self.specialize_type(db, class_literal.metaclass(db))
    }

    /// Return a type representing "the set of all instances of the metaclass of this class".
    pub(super) fn metaclass_instance_type(self, db: &'db dyn Db) -> Type<'db> {
        self
            .metaclass(db)
            .to_instance(db)
            .expect("`Type::to_instance()` should always return `Some()` when called on the type of a metaclass")
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> SymbolAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .class_member_inner(db, specialization, name, policy)
            .map_type(|ty| self.specialize_type(db, ty))
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as ClassVars are considered.
    ///
    /// Returns [`Symbol::Unbound`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassType::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .own_class_member(db, specialization, name)
            .map_type(|ty| self.specialize_type(db, ty))
    }

    /// Returns the `name` attribute of an instance of this class.
    ///
    /// The attribute could be defined in the class body, but it could also be an implicitly
    /// defined attribute that is only present in a method (typically `__init__`).
    ///
    /// The attribute might also be defined in a superclass of this class.
    pub(super) fn instance_member(self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        let (class_literal, specialization) = self.class_literal(db);
        class_literal
            .instance_member(db, specialization, name)
            .map_type(|ty| self.specialize_type(db, ty))
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    fn own_instance_member(self, db: &'db dyn Db, name: &str) -> SymbolAndQualifiers<'db> {
        let (class_literal, _) = self.class_literal(db);
        class_literal
            .own_instance_member(db, name)
            .map_type(|ty| self.specialize_type(db, ty))
    }
}

impl<'db> From<GenericAlias<'db>> for ClassType<'db> {
    fn from(generic: GenericAlias<'db>) -> ClassType<'db> {
        ClassType::Generic(generic)
    }
}

impl<'db> From<ClassType<'db>> for Type<'db> {
    fn from(class: ClassType<'db>) -> Type<'db> {
        match class {
            ClassType::NonGeneric(non_generic) => non_generic.into(),
            ClassType::Generic(generic) => generic.into(),
        }
    }
}

/// Represents a single class object at runtime, which might be a non-generic class, or a generic
/// class that has not been specialized.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, salsa::Supertype, salsa::Update,
)]
pub enum ClassLiteralType<'db> {
    NonGeneric(NonGenericClass<'db>),
    Generic(GenericClass<'db>),
}

#[salsa::tracked]
impl<'db> ClassLiteralType<'db> {
    fn class(self, db: &'db dyn Db) -> &'db Class<'db> {
        match self {
            Self::NonGeneric(non_generic) => non_generic.class(db),
            Self::Generic(generic) => generic.class(db),
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db ast::name::Name {
        &self.class(db).name
    }

    pub(crate) fn known(self, db: &'db dyn Db) -> Option<KnownClass> {
        self.class(db).known
    }

    pub(crate) fn dataclass_metadata(self, db: &'db dyn Db) -> Option<DataclassMetadata> {
        self.class(db).dataclass_metadata
    }

    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.class(db).known == Some(known_class)
    }

    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match self {
            Self::NonGeneric(_) => None,
            Self::Generic(generic) => Some(generic.generic_context(db)),
        }
    }

    /// Return `true` if this class represents the builtin class `object`
    pub(crate) fn is_object(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Object)
    }

    pub(crate) fn body_scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        self.class(db).body_scope
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.class(db).definition(db)
    }

    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> ClassType<'db> {
        match (self, specialization) {
            (Self::NonGeneric(non_generic), _) => ClassType::NonGeneric(non_generic),
            (Self::Generic(generic), None) => {
                let specialization = generic.generic_context(db).default_specialization(db);
                ClassType::Generic(GenericAlias::new(db, generic, specialization))
            }
            (Self::Generic(generic), Some(specialization)) => {
                ClassType::Generic(GenericAlias::new(db, generic, specialization))
            }
        }
    }

    /// Returns the default specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// applies the default specialization to the class's typevars.
    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::NonGeneric(non_generic) => ClassType::NonGeneric(non_generic),
            Self::Generic(generic) => {
                let specialization = generic.generic_context(db).default_specialization(db);
                ClassType::Generic(GenericAlias::new(db, generic, specialization))
            }
        }
    }

    /// Returns the unknown specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// maps each of the class's typevars to `Unknown`.
    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        match self {
            Self::NonGeneric(non_generic) => ClassType::NonGeneric(non_generic),
            Self::Generic(generic) => {
                let specialization = generic.generic_context(db).unknown_specialization(db);
                ClassType::Generic(GenericAlias::new(db, generic, specialization))
            }
        }
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
    fn fully_static_explicit_bases(self, db: &'db dyn Db) -> impl Iterator<Item = ClassType<'db>> {
        self.explicit_bases(db)
            .iter()
            .copied()
            .filter_map(Type::into_class_type)
    }

    #[salsa::tracked(return_ref, cycle_fn=explicit_bases_cycle_recover, cycle_initial=explicit_bases_cycle_initial)]
    fn explicit_bases_query(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let class = self.class(db);
        tracing::trace!("ClassLiteralType::explicit_bases_query: {}", class.name);

        let class_stmt = class.node(db);
        let class_definition =
            semantic_index(db, class.file(db)).expect_single_definition(class_stmt);

        class_stmt
            .bases()
            .iter()
            .map(|base_node| definition_expression_type(db, class_definition, base_node))
            .collect()
    }

    /// Return the types of the decorators on this class
    #[salsa::tracked(return_ref)]
    fn decorators(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        let class = self.class(db);
        tracing::trace!("ClassLiteralType::decorators: {}", class.name);

        let class_stmt = class.node(db);
        if class_stmt.decorator_list.is_empty() {
            return Box::new([]);
        }

        let class_definition =
            semantic_index(db, class.file(db)).expect_single_definition(class_stmt);

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
    #[salsa::tracked(return_ref, cycle_fn=try_mro_cycle_recover, cycle_initial=try_mro_cycle_initial)]
    pub(super) fn try_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, MroError<'db>> {
        let class = self.class(db);
        tracing::trace!("ClassLiteralType::try_mro: {}", class.name);
        Mro::of_class(db, self, specialization)
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`ClassLiteralType::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(super) fn iter_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> impl Iterator<Item = ClassBase<'db>> {
        MroIterator::new(db, self, specialization)
    }

    /// Return `true` if `other` is present in this class's MRO.
    pub(super) fn is_subclass_of(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        other: ClassType<'db>,
    ) -> bool {
        // `is_subclass_of` is checking the subtype relation, in which gradual types do not
        // participate, so we should not return `True` if we find `Any/Unknown` in the MRO.
        self.iter_mro(db, specialization)
            .contains(&ClassBase::Class(other))
    }

    /// Return the explicit `metaclass` of this class, if one is defined.
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn explicit_metaclass(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let class = self.class(db);
        let class_stmt = class.node(db);
        let metaclass_node = &class_stmt
            .arguments
            .as_ref()?
            .find_keyword("metaclass")?
            .value;

        let class_definition = class.definition(db);

        Some(definition_expression_type(
            db,
            class_definition,
            metaclass_node,
        ))
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(super) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        self.try_metaclass(db)
            .unwrap_or_else(|_| SubclassOfType::subclass_of_unknown())
    }

    /// Return a type representing "the set of all instances of the metaclass of this class".
    pub(super) fn metaclass_instance_type(self, db: &'db dyn Db) -> Type<'db> {
        self
            .metaclass(db)
            .to_instance(db)
            .expect("`Type::to_instance()` should always return `Some()` when called on the type of a metaclass")
    }

    /// Return the metaclass of this class, or an error if the metaclass cannot be inferred.
    #[salsa::tracked]
    pub(super) fn try_metaclass(self, db: &'db dyn Db) -> Result<Type<'db>, MetaclassError<'db>> {
        let class = self.class(db);
        tracing::trace!("ClassLiteralType::try_metaclass: {}", class.name);

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
            let (base_class_literal, _) = base_class.class_literal(db);
            (base_class.metaclass(db), base_class_literal)
        } else {
            (KnownClass::Type.to_class_literal(db), self)
        };

        let mut candidate = if let Some(metaclass_ty) = metaclass.into_class_type() {
            MetaclassCandidate {
                metaclass: metaclass_ty,
                explicit_metaclass_of: class_metaclass_was_from,
            }
        } else {
            let name = Type::string_literal(db, &class.name);
            let bases = TupleType::from_elements(db, self.explicit_bases(db));
            // TODO: Should be `dict[str, Any]`
            let namespace = KnownClass::Dict.to_instance(db);

            // TODO: Other keyword arguments?
            let arguments = CallArgumentTypes::positional([name, bases, namespace]);

            let return_ty_result = match metaclass.try_call(db, arguments) {
                Ok(bindings) => Ok(bindings.return_type(db)),

                Err(CallError(CallErrorKind::NotCallable, bindings)) => Err(MetaclassError {
                    kind: MetaclassErrorKind::NotCallable(bindings.callable_type()),
                }),

                // TODO we should also check for binding errors that would indicate the metaclass
                // does not accept the right arguments
                Err(CallError(CallErrorKind::BindingError, bindings)) => {
                    Ok(bindings.return_type(db))
                }

                Err(CallError(CallErrorKind::PossiblyNotCallable, _)) => Err(MetaclassError {
                    kind: MetaclassErrorKind::PartlyNotCallable(metaclass),
                }),
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
            let Some(metaclass) = metaclass.into_class_type() else {
                continue;
            };
            if metaclass.is_subclass_of(db, candidate.metaclass) {
                let (base_class_literal, _) = base_class.class_literal(db);
                candidate = MetaclassCandidate {
                    metaclass,
                    explicit_metaclass_of: base_class_literal,
                };
                continue;
            }
            if candidate.metaclass.is_subclass_of(db, metaclass) {
                continue;
            }
            let (base_class_literal, _) = base_class.class_literal(db);
            return Err(MetaclassError {
                kind: MetaclassErrorKind::Conflict {
                    candidate1: candidate,
                    candidate2: MetaclassCandidate {
                        metaclass,
                        explicit_metaclass_of: base_class_literal,
                    },
                    candidate1_is_base_class: explicit_metaclass.is_none(),
                },
            });
        }

        Ok(candidate.metaclass.into())
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> SymbolAndQualifiers<'db> {
        self.class_member_inner(db, None, name, policy)
    }

    fn class_member_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> SymbolAndQualifiers<'db> {
        if name == "__mro__" {
            let tuple_elements = self.iter_mro(db, specialization).map(Type::from);
            return Symbol::bound(TupleType::from_elements(db, tuple_elements)).into();
        }

        self.class_member_from_mro(db, name, policy, self.iter_mro(db, specialization))
    }

    pub(super) fn class_member_from_mro(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> SymbolAndQualifiers<'db> {
        // If we encounter a dynamic type in this class's MRO, we'll save that dynamic type
        // in this variable. After we've traversed the MRO, we'll either:
        // (1) Use that dynamic type as the type for this attribute,
        //     if no other classes in the MRO define the attribute; or,
        // (2) Intersect that dynamic type with the type of the attribute
        //     from the non-dynamic members of the class's MRO.
        let mut dynamic_type_to_intersect_with: Option<Type<'db>> = None;

        let mut lookup_result: LookupResult<'db> =
            Err(LookupError::Unbound(TypeQualifiers::empty()));

        for superclass in mro_iter {
            match superclass {
                ClassBase::Dynamic(DynamicType::TodoProtocol) => {
                    // TODO: We currently skip `Protocol` when looking up class members, in order to
                    // avoid creating many dynamic types in our test suite that would otherwise
                    // result from looking up attributes on builtin types like `str`, `list`, `tuple`
                }
                ClassBase::Dynamic(_) => {
                    // Note: calling `Type::from(superclass).member()` would be incorrect here.
                    // What we'd really want is a `Type::Any.own_class_member()` method,
                    // but adding such a method wouldn't make much sense -- it would always return `Any`!
                    dynamic_type_to_intersect_with.get_or_insert(Type::from(superclass));
                }
                ClassBase::Class(class) => {
                    if class.is_known(db, KnownClass::Object)
                        // Only exclude `object` members if this is not an `object` class itself
                        && (policy.mro_no_object_fallback() && !self.is_known(db, KnownClass::Object))
                    {
                        continue;
                    }

                    if class.is_known(db, KnownClass::Type) && policy.meta_class_no_type_fallback()
                    {
                        continue;
                    }

                    lookup_result = lookup_result.or_else(|lookup_error| {
                        lookup_error.or_fall_back_to(db, class.own_class_member(db, name))
                    });
                }
            }
            if lookup_result.is_ok() {
                break;
            }
        }

        match (
            SymbolAndQualifiers::from(lookup_result),
            dynamic_type_to_intersect_with,
        ) {
            (symbol_and_qualifiers, None) => symbol_and_qualifiers,

            (
                SymbolAndQualifiers {
                    symbol: Symbol::Type(ty, _),
                    qualifiers,
                },
                Some(dynamic_type),
            ) => Symbol::bound(
                IntersectionBuilder::new(db)
                    .add_positive(ty)
                    .add_positive(dynamic_type)
                    .build(),
            )
            .with_qualifiers(qualifiers),

            (
                SymbolAndQualifiers {
                    symbol: Symbol::Unbound,
                    qualifiers,
                },
                Some(dynamic_type),
            ) => Symbol::bound(dynamic_type).with_qualifiers(qualifiers),
        }
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as ClassVars are considered.
    ///
    /// Returns [`Symbol::Unbound`] if `name` cannot be found in this class's scope
    /// directly. Use [`ClassLiteralType::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> SymbolAndQualifiers<'db> {
        let body_scope = self.body_scope(db);
        let symbol = class_symbol(db, body_scope, name).map_type(|ty| {
            // The `__new__` and `__init__` members of a non-specialized generic class are handled
            // specially: they inherit the generic context of their class. That lets us treat them
            // as generic functions when constructing the class, and infer the specialization of
            // the class from the arguments that are passed in.
            //
            // We might decide to handle other class methods the same way, having them inherit the
            // class's generic context, and performing type inference on calls to them to determine
            // the specialization of the class. If we do that, we would update this to also apply
            // to any method with a `@classmethod` decorator. (`__init__` would remain a special
            // case, since it's an _instance_ method where we don't yet know the generic class's
            // specialization.)
            match (self, ty, specialization, name) {
                (
                    ClassLiteralType::Generic(origin),
                    Type::FunctionLiteral(function),
                    Some(_),
                    "__new__" | "__init__",
                ) => Type::FunctionLiteral(
                    function.with_generic_context(db, origin.generic_context(db)),
                ),
                _ => ty,
            }
        });

        if symbol.symbol.is_unbound() {
            if let Some(metadata) = self.dataclass_metadata(db) {
                if let Some(dataclass_member) =
                    self.own_dataclass_member(db, specialization, metadata, name)
                {
                    return Symbol::bound(dataclass_member).into();
                }
            }
        }

        symbol
    }

    /// Returns the type of a synthesized dataclass member like `__init__` or `__lt__`.
    fn own_dataclass_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        metadata: DataclassMetadata,
        name: &str,
    ) -> Option<Type<'db>> {
        if name == "__init__" && metadata.contains(DataclassMetadata::INIT) {
            let mut parameters = vec![];

            for (name, (mut attr_ty, mut default_ty)) in self.dataclass_fields(db, specialization) {
                // The descriptor handling below is guarded by this fully-static check, because dynamic
                // types like `Any` are valid (data) descriptors: since they have all possible attributes,
                // they also have a (callable) `__set__` method. The problem is that we can't determine
                // the type of the value parameter this way. Instead, we want to use the dynamic type
                // itself in this case, so we skip the special descriptor handling.
                if attr_ty.is_fully_static(db) {
                    let dunder_set = attr_ty.class_member(db, "__set__".into());
                    if let Some(dunder_set) = dunder_set.symbol.ignore_possibly_unbound() {
                        // This type of this attribute is a data descriptor. Instead of overwriting the
                        // descriptor attribute, data-classes will (implicitly) call the `__set__` method
                        // of the descriptor. This means that the synthesized `__init__` parameter for
                        // this attribute is determined by possible `value` parameter types with which
                        // the `__set__` method can be called. We build a union of all possible options
                        // to account for possible overloads.
                        let mut value_types = UnionBuilder::new(db);
                        for signature in &dunder_set.signatures(db) {
                            for overload in signature {
                                if let Some(value_param) = overload.parameters().get_positional(2) {
                                    value_types = value_types.add(
                                        value_param.annotated_type().unwrap_or_else(Type::unknown),
                                    );
                                } else if overload.parameters().is_gradual() {
                                    value_types = value_types.add(Type::unknown());
                                }
                            }
                        }
                        attr_ty = value_types.build();

                        // The default value of the attribute is *not* determined by the right hand side
                        // of the class-body assignment. Instead, the runtime invokes `__get__` on the
                        // descriptor, as if it had been called on the class itself, i.e. it passes `None`
                        // for the `instance` argument.

                        if let Some(ref mut default_ty) = default_ty {
                            *default_ty = default_ty
                                .try_call_dunder_get(db, Type::none(db), Type::ClassLiteral(self))
                                .map(|(return_ty, _)| return_ty)
                                .unwrap_or_else(Type::unknown);
                        }
                    }
                }

                let mut parameter =
                    Parameter::positional_or_keyword(name).with_annotated_type(attr_ty);

                if let Some(default_ty) = default_ty {
                    parameter = parameter.with_default_type(default_ty);
                }

                parameters.push(parameter);
            }

            let init_signature = Signature::new(Parameters::new(parameters), Some(Type::none(db)));

            return Some(Type::Callable(CallableType::new(db, init_signature)));
        } else if matches!(name, "__lt__" | "__le__" | "__gt__" | "__ge__") {
            if metadata.contains(DataclassMetadata::ORDER) {
                let signature = Signature::new(
                    Parameters::new([Parameter::positional_or_keyword(Name::new_static("other"))
                        // TODO: could be `Self`.
                        .with_annotated_type(Type::instance(
                            self.apply_optional_specialization(db, specialization),
                        ))]),
                    Some(KnownClass::Bool.to_instance(db)),
                );

                return Some(Type::Callable(CallableType::new(db, signature)));
            }
        }

        None
    }

    /// Returns a list of all annotated attributes defined in this class, or any of its superclasses.
    ///
    /// See [`ClassLiteralType::own_dataclass_fields`] for more details.
    fn dataclass_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> FxOrderMap<Name, (Type<'db>, Option<Type<'db>>)> {
        let dataclasses_in_mro: Vec<_> = self
            .iter_mro(db, specialization)
            .filter_map(|superclass| {
                if let Some(class) = superclass.into_class() {
                    let class_literal = class.class_literal(db).0;
                    if class_literal.dataclass_metadata(db).is_some() {
                        Some(class_literal)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            // We need to collect into a `Vec` here because we iterate the MRO in reverse order
            .collect();

        dataclasses_in_mro
            .into_iter()
            .rev()
            .flat_map(|class| class.own_dataclass_fields(db))
            // We collect into a FxOrderMap here to deduplicate attributes
            .collect()
    }

    /// Returns a list of all annotated attributes defined in the body of this class. This is similar
    /// to the `__annotations__` attribute at runtime, but also contains default values.
    ///
    /// For a class body like
    /// ```py
    /// @dataclass
    /// class C:
    ///     x: int
    ///     y: str = "a"
    /// ```
    /// we return a map `{"x": (int, None), "y": (str, Some(Literal["a"]))}`.
    fn own_dataclass_fields(
        self,
        db: &'db dyn Db,
    ) -> FxOrderMap<Name, (Type<'db>, Option<Type<'db>>)> {
        let mut attributes = FxOrderMap::default();

        let class_body_scope = self.body_scope(db);
        let table = symbol_table(db, class_body_scope);

        let use_def = use_def_map(db, class_body_scope);
        for (symbol_id, declarations) in use_def.all_public_declarations() {
            // Here, we exclude all declarations that are not annotated assignments. We need this because
            // things like function definitions and nested classes would otherwise be considered dataclass
            // fields. The check is too broad in the sense that it also excludes (weird) constructs where
            // a symbol would have multiple declarations, one of which is an annotated assignment. If we
            // want to improve this, we could instead pass a definition-kind filter to the use-def map
            // query, or to the `symbol_from_declarations` call below. Doing so would potentially require
            // us to generate a union of `__init__` methods.
            if !declarations
                .clone()
                .all(|DeclarationWithConstraint { declaration, .. }| {
                    declaration.is_some_and(|declaration| {
                        matches!(
                            declaration.kind(db),
                            DefinitionKind::AnnotatedAssignment(..)
                        )
                    })
                })
            {
                continue;
            }

            let symbol = table.symbol(symbol_id);

            if let Ok(attr) = symbol_from_declarations(db, declarations) {
                if attr.is_class_var() {
                    continue;
                }

                if let Some(attr_ty) = attr.symbol.ignore_possibly_unbound() {
                    let bindings = use_def.public_bindings(symbol_id);
                    let default_ty = symbol_from_bindings(db, bindings).ignore_possibly_unbound();

                    attributes.insert(symbol.name().clone(), (attr_ty, default_ty));
                }
            }
        }

        attributes
    }

    /// Returns the `name` attribute of an instance of this class.
    ///
    /// The attribute could be defined in the class body, but it could also be an implicitly
    /// defined attribute that is only present in a method (typically `__init__`).
    ///
    /// The attribute might also be defined in a superclass of this class.
    pub(super) fn instance_member(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> SymbolAndQualifiers<'db> {
        let mut union = UnionBuilder::new(db);
        let mut union_qualifiers = TypeQualifiers::empty();

        for superclass in self.iter_mro(db, specialization) {
            match superclass {
                ClassBase::Dynamic(DynamicType::TodoProtocol) => {
                    // TODO: We currently skip `Protocol` when looking up instance members, in order to
                    // avoid creating many dynamic types in our test suite that would otherwise
                    // result from looking up attributes on builtin types like `str`, `list`, `tuple`
                }
                ClassBase::Dynamic(_) => {
                    return SymbolAndQualifiers::todo(
                        "instance attribute on class with dynamic base",
                    );
                }
                ClassBase::Class(class) => {
                    if let member @ SymbolAndQualifiers {
                        symbol: Symbol::Type(ty, boundness),
                        qualifiers,
                    } = class.own_instance_member(db, name)
                    {
                        // TODO: We could raise a diagnostic here if there are conflicting type qualifiers
                        union_qualifiers |= qualifiers;

                        if boundness == Boundness::Bound {
                            if union.is_empty() {
                                // Short-circuit, no need to allocate inside the union builder
                                return member;
                            }

                            return Symbol::bound(union.add(ty).build())
                                .with_qualifiers(union_qualifiers);
                        }

                        // If we see a possibly-unbound symbol, we need to keep looking
                        // higher up in the MRO.
                        union = union.add(ty);
                    }
                }
            }
        }

        if union.is_empty() {
            Symbol::Unbound.with_qualifiers(TypeQualifiers::empty())
        } else {
            // If we have reached this point, we know that we have only seen possibly-unbound symbols.
            // This means that the final result is still possibly-unbound.

            Symbol::Type(union.build(), Boundness::PossiblyUnbound)
                .with_qualifiers(union_qualifiers)
        }
    }

    /// Tries to find declarations/bindings of an instance attribute named `name` that are only
    /// "implicitly" defined in a method of the class that corresponds to `class_body_scope`.
    fn implicit_instance_attribute(
        db: &'db dyn Db,
        class_body_scope: ScopeId<'db>,
        name: &str,
    ) -> Symbol<'db> {
        // If we do not see any declarations of an attribute, neither in the class body nor in
        // any method, we build a union of `Unknown` with the inferred types of all bindings of
        // that attribute. We include `Unknown` in that union to account for the fact that the
        // attribute might be externally modified.
        let mut union_of_inferred_types = UnionBuilder::new(db).add(Type::unknown());

        let mut is_attribute_bound = Truthiness::AlwaysFalse;

        let file = class_body_scope.file(db);
        let index = semantic_index(db, file);
        let class_map = use_def_map(db, class_body_scope);
        let class_table = symbol_table(db, class_body_scope);

        for (attribute_assignments, method_scope_id) in
            attribute_assignments(db, class_body_scope, name)
        {
            let method_scope = method_scope_id.to_scope_id(db, file);
            let method_map = use_def_map(db, method_scope);

            // The attribute assignment inherits the visibility of the method which contains it
            let is_method_visible = if let Some(method_def) = method_scope.node(db).as_function() {
                let method = index.expect_single_definition(method_def);
                let method_symbol = class_table.symbol_id_by_name(&method_def.name).unwrap();
                class_map
                    .public_bindings(method_symbol)
                    .find_map(|bind| {
                        (bind.binding == Some(method))
                            .then(|| class_map.is_binding_visible(db, &bind))
                    })
                    .unwrap_or(Truthiness::AlwaysFalse)
            } else {
                Truthiness::AlwaysFalse
            };
            if is_method_visible.is_always_false() {
                continue;
            }

            let mut attribute_assignments = attribute_assignments.peekable();
            let unbound_visibility = attribute_assignments
                .peek()
                .map(|attribute_assignment| {
                    if attribute_assignment.binding.is_none() {
                        method_map.is_binding_visible(db, attribute_assignment)
                    } else {
                        Truthiness::AlwaysFalse
                    }
                })
                .unwrap_or(Truthiness::AlwaysFalse);

            for attribute_assignment in attribute_assignments {
                let Some(binding) = attribute_assignment.binding else {
                    continue;
                };
                match method_map
                    .is_binding_visible(db, &attribute_assignment)
                    .and(is_method_visible)
                {
                    Truthiness::AlwaysTrue => {
                        is_attribute_bound = Truthiness::AlwaysTrue;
                    }
                    Truthiness::Ambiguous => {
                        if is_attribute_bound.is_always_false() {
                            is_attribute_bound = Truthiness::Ambiguous;
                        }
                    }
                    Truthiness::AlwaysFalse => {
                        continue;
                    }
                }

                // There is at least one attribute assignment that may be visible,
                // so if `unbound_visibility` is always false then this attribute is considered bound.
                // TODO: this is incomplete logic since the attributes bound after termination are considered visible.
                if unbound_visibility
                    .negate()
                    .and(is_method_visible)
                    .is_always_true()
                {
                    is_attribute_bound = Truthiness::AlwaysTrue;
                }

                match binding.kind(db) {
                    DefinitionKind::AnnotatedAssignment(ann_assign) => {
                        // We found an annotated assignment of one of the following forms (using 'self' in these
                        // examples, but we support arbitrary names for the first parameters of methods):
                        //
                        //     self.name: <annotation>
                        //     self.name: <annotation> = …

                        let annotation_ty =
                            infer_expression_type(db, index.expression(ann_assign.annotation()));

                        // TODO: check if there are conflicting declarations
                        match is_attribute_bound {
                            Truthiness::AlwaysTrue => {
                                return Symbol::bound(annotation_ty);
                            }
                            Truthiness::Ambiguous => {
                                return Symbol::possibly_unbound(annotation_ty);
                            }
                            Truthiness::AlwaysFalse => unreachable!("If the attribute assignments are all invisible, inference of their types should be skipped"),
                        }
                    }
                    DefinitionKind::Assignment(assign) => {
                        match assign.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     .., self.name, .. = <value>
                                //     (.., self.name, ..) = <value>
                                //     [.., self.name, ..] = <value>

                                let unpacked = infer_unpack_types(db, unpack);
                                let target_ast_id =
                                    assign.target().scoped_expression_id(db, method_scope);
                                let inferred_ty = unpacked.expression_type(target_ast_id);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::NameOrAttribute => {
                                // We found an un-annotated attribute assignment of the form:
                                //
                                //     self.name = <value>

                                let inferred_ty =
                                    infer_expression_type(db, index.expression(assign.value()));

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::For(for_stmt) => {
                        match for_stmt.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     for .., self.name, .. in <iterable>:

                                let unpacked = infer_unpack_types(db, unpack);
                                let target_ast_id =
                                    for_stmt.target().scoped_expression_id(db, method_scope);
                                let inferred_ty = unpacked.expression_type(target_ast_id);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::NameOrAttribute => {
                                // We found an attribute assignment like:
                                //
                                //     for self.name in <iterable>:

                                let iterable_ty = infer_expression_type(
                                    db,
                                    index.expression(for_stmt.iterable()),
                                );
                                // TODO: Potential diagnostics resulting from the iterable are currently not reported.
                                let inferred_ty = iterable_ty.iterate(db);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::WithItem(with_item) => {
                        match with_item.target_kind() {
                            TargetKind::Sequence(_, unpack) => {
                                // We found an unpacking assignment like:
                                //
                                //     with <context_manager> as .., self.name, ..:

                                let unpacked = infer_unpack_types(db, unpack);
                                let target_ast_id =
                                    with_item.target().scoped_expression_id(db, method_scope);
                                let inferred_ty = unpacked.expression_type(target_ast_id);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                            TargetKind::NameOrAttribute => {
                                // We found an attribute assignment like:
                                //
                                //     with <context_manager> as self.name:

                                let context_ty = infer_expression_type(
                                    db,
                                    index.expression(with_item.context_expr()),
                                );
                                let inferred_ty = context_ty.enter(db);

                                union_of_inferred_types = union_of_inferred_types.add(inferred_ty);
                            }
                        }
                    }
                    DefinitionKind::Comprehension(_) => {
                        // TODO:
                    }
                    DefinitionKind::AugmentedAssignment(_) => {
                        // TODO:
                    }
                    DefinitionKind::NamedExpression(_) => {
                        // TODO:
                    }
                    _ => {}
                }
            }
        }

        match is_attribute_bound {
            Truthiness::AlwaysTrue => Symbol::bound(union_of_inferred_types.build()),
            Truthiness::Ambiguous => Symbol::possibly_unbound(union_of_inferred_types.build()),
            Truthiness::AlwaysFalse => Symbol::Unbound,
        }
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
            let declared_and_qualifiers = symbol_from_declarations(db, declarations);
            match declared_and_qualifiers {
                Ok(SymbolAndQualifiers {
                    symbol: declared @ Symbol::Type(declared_ty, declaredness),
                    qualifiers,
                }) => {
                    // The attribute is declared in the class body.

                    let bindings = use_def.public_bindings(symbol_id);
                    let inferred = symbol_from_bindings(db, bindings);
                    let has_binding = !inferred.is_unbound();

                    if has_binding {
                        // The attribute is declared and bound in the class body.

                        if let Some(implicit_ty) =
                            Self::implicit_instance_attribute(db, body_scope, name)
                                .ignore_possibly_unbound()
                        {
                            if declaredness == Boundness::Bound {
                                // If a symbol is definitely declared, and we see
                                // attribute assignments in methods of the class,
                                // we trust the declared type.
                                declared.with_qualifiers(qualifiers)
                            } else {
                                Symbol::Type(
                                    UnionType::from_elements(db, [declared_ty, implicit_ty]),
                                    declaredness,
                                )
                                .with_qualifiers(qualifiers)
                            }
                        } else {
                            // The symbol is declared and bound in the class body,
                            // but we did not find any attribute assignments in
                            // methods of the class. This means that the attribute
                            // has a class-level default value, but it would not be
                            // found in a `__dict__` lookup.

                            Symbol::Unbound.into()
                        }
                    } else {
                        // The attribute is declared but not bound in the class body.
                        // We take this as a sign that this is intended to be a pure
                        // instance attribute, and we trust the declared type, unless
                        // it is possibly-undeclared. In the latter case, we also
                        // union with the inferred type from attribute assignments.

                        if declaredness == Boundness::Bound {
                            declared.with_qualifiers(qualifiers)
                        } else {
                            if let Some(implicit_ty) =
                                Self::implicit_instance_attribute(db, body_scope, name)
                                    .ignore_possibly_unbound()
                            {
                                Symbol::Type(
                                    UnionType::from_elements(db, [declared_ty, implicit_ty]),
                                    declaredness,
                                )
                                .with_qualifiers(qualifiers)
                            } else {
                                declared.with_qualifiers(qualifiers)
                            }
                        }
                    }
                }

                Ok(SymbolAndQualifiers {
                    symbol: Symbol::Unbound,
                    qualifiers: _,
                }) => {
                    // The attribute is not *declared* in the class body. It could still be declared/bound
                    // in a method.

                    Self::implicit_instance_attribute(db, body_scope, name).into()
                }
                Err((declared, _conflicting_declarations)) => {
                    // There are conflicting declarations for this attribute in the class body.
                    Symbol::bound(declared.inner_type()).with_qualifiers(declared.qualifiers())
                }
            }
        } else {
            // This attribute is neither declared nor bound in the class body.
            // It could still be implicitly defined in a method.

            Self::implicit_instance_attribute(db, body_scope, name).into()
        }
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    #[salsa::tracked(cycle_fn=inheritance_cycle_recover, cycle_initial=inheritance_cycle_initial)]
    pub(super) fn inheritance_cycle(self, db: &'db dyn Db) -> Option<InheritanceCycle> {
        /// Return `true` if the class is cyclically defined.
        ///
        /// Also, populates `visited_classes` with all base classes of `self`.
        fn is_cyclically_defined_recursive<'db>(
            db: &'db dyn Db,
            class: ClassLiteralType<'db>,
            classes_on_stack: &mut IndexSet<ClassLiteralType<'db>>,
            visited_classes: &mut IndexSet<ClassLiteralType<'db>>,
        ) -> bool {
            let mut result = false;
            for explicit_base_class in class.fully_static_explicit_bases(db) {
                let (explicit_base_class_literal, _) = explicit_base_class.class_literal(db);
                if !classes_on_stack.insert(explicit_base_class_literal) {
                    return true;
                }

                if visited_classes.insert(explicit_base_class_literal) {
                    // If we find a cycle, keep searching to check if we can reach the starting class.
                    result |= is_cyclically_defined_recursive(
                        db,
                        explicit_base_class_literal,
                        classes_on_stack,
                        visited_classes,
                    );
                }

                classes_on_stack.pop();
            }
            result
        }

        tracing::trace!("Class::inheritance_cycle: {}", self.name(db));

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

impl<'db> From<ClassLiteralType<'db>> for Type<'db> {
    fn from(class: ClassLiteralType<'db>) -> Type<'db> {
        match class {
            ClassLiteralType::NonGeneric(non_generic) => non_generic.into(),
            ClassLiteralType::Generic(generic) => generic.into(),
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

/// A type representing the set of runtime objects which are instances of a certain class.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, salsa::Update)]
pub struct InstanceType<'db> {
    pub class: ClassType<'db>,
}

impl<'db> InstanceType<'db> {
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
pub(crate) enum KnownClass {
    // To figure out where an stdlib symbol is defined, you can go into `crates/red_knot_vendored`
    // and grep for the symbol name in any `.pyi` file.

    // Builtins
    Bool,
    Object,
    Bytes,
    Bytearray,
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
    Super,
    // enum
    Enum,
    // Types
    GenericAlias,
    ModuleType,
    FunctionType,
    MethodType,
    MethodWrapperType,
    WrapperDescriptorType,
    UnionType,
    // Typeshed
    NoneType, // Part of `types` for Python >= 3.10
    // Typing
    Any,
    StdlibAlias,
    SpecialForm,
    TypeVar,
    ParamSpec,
    ParamSpecArgs,
    ParamSpecKwargs,
    TypeVarTuple,
    TypeAliasType,
    NoDefaultType,
    NewType,
    Sized,
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
    NotImplementedType,
}

impl<'db> KnownClass {
    pub(crate) const fn is_bool(self) -> bool {
        matches!(self, Self::Bool)
    }

    pub(crate) const fn is_special_form(self) -> bool {
        matches!(self, Self::SpecialForm)
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
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Super
            | Self::WrapperDescriptorType
            | Self::UnionType
            | Self::MethodWrapperType => Truthiness::AlwaysTrue,

            Self::NoneType => Truthiness::AlwaysFalse,

            Self::Any
            | Self::BaseException
            | Self::Object
            | Self::OrderedDict
            | Self::BaseExceptionGroup
            | Self::Bool
            | Self::Str
            | Self::List
            | Self::GenericAlias
            | Self::NewType
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::Set
            | Self::Tuple
            | Self::Int
            | Self::Type
            | Self::Bytes
            | Self::Bytearray
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
            | Self::Sized
            | Self::Enum
            // Evaluating `NotImplementedType` in a boolean context was deprecated in Python 3.9
            // and raises a `TypeError` in Python >=3.14
            // (see https://docs.python.org/3/library/constants.html#NotImplemented)
            | Self::NotImplementedType
            | Self::Classmethod => Truthiness::Ambiguous,
        }
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'static str {
        match self {
            Self::Any => "Any",
            Self::Bool => "bool",
            Self::Object => "object",
            Self::Bytes => "bytes",
            Self::Bytearray => "bytearray",
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
            Self::UnionType => "UnionType",
            Self::MethodWrapperType => "MethodWrapperType",
            Self::WrapperDescriptorType => "WrapperDescriptorType",
            Self::NoneType => "NoneType",
            Self::SpecialForm => "_SpecialForm",
            Self::TypeVar => "TypeVar",
            Self::ParamSpec => "ParamSpec",
            Self::ParamSpecArgs => "ParamSpecArgs",
            Self::ParamSpecKwargs => "ParamSpecKwargs",
            Self::TypeVarTuple => "TypeVarTuple",
            Self::TypeAliasType => "TypeAliasType",
            Self::NoDefaultType => "_NoDefaultType",
            Self::NewType => "NewType",
            Self::SupportsIndex => "SupportsIndex",
            Self::ChainMap => "ChainMap",
            Self::Counter => "Counter",
            Self::DefaultDict => "defaultdict",
            Self::Deque => "deque",
            Self::Sized => "Sized",
            Self::OrderedDict => "OrderedDict",
            Self::Enum => "Enum",
            Self::Super => "super",
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
            Self::NotImplementedType => "_NotImplementedType",
        }
    }

    fn display(self, db: &'db dyn Db) -> impl std::fmt::Display + 'db {
        struct KnownClassDisplay<'db> {
            db: &'db dyn Db,
            class: KnownClass,
        }

        impl std::fmt::Display for KnownClassDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let KnownClassDisplay {
                    class: known_class,
                    db,
                } = *self;
                write!(
                    f,
                    "{module}.{class}",
                    module = known_class.canonical_module(db),
                    class = known_class.name(db)
                )
            }
        }

        KnownClassDisplay { db, class: self }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing all possible instances of the class.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_instance(self, db: &'db dyn Db) -> Type<'db> {
        self.to_class_literal(db)
            .into_class_type()
            .map(Type::instance)
            .unwrap_or_else(Type::unknown)
    }

    /// Attempt to lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// Return an error if the symbol cannot be found in the expected typeshed module,
    /// or if the symbol is not a class definition, or if the symbol is possibly unbound.
    pub(crate) fn try_to_class_literal(
        self,
        db: &'db dyn Db,
    ) -> Result<ClassLiteralType<'db>, KnownClassLookupError<'db>> {
        let symbol = known_module_symbol(db, self.canonical_module(db), self.name(db)).symbol;
        match symbol {
            Symbol::Type(Type::ClassLiteral(class_literal), Boundness::Bound) => Ok(class_literal),
            Symbol::Type(Type::ClassLiteral(class_literal), Boundness::PossiblyUnbound) => {
                Err(KnownClassLookupError::ClassPossiblyUnbound { class_literal })
            }
            Symbol::Type(found_type, _) => {
                Err(KnownClassLookupError::SymbolNotAClass { found_type })
            }
            Symbol::Unbound => Err(KnownClassLookupError::ClassNotFound),
        }
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`] representing that class-literal.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_class_literal(self, db: &'db dyn Db) -> Type<'db> {
        // a cache of the `KnownClass`es that we have already failed to lookup in typeshed
        // (and therefore that we've already logged a warning for)
        static MESSAGES: LazyLock<Mutex<FxHashSet<KnownClass>>> = LazyLock::new(Mutex::default);

        self.try_to_class_literal(db)
            .map(Type::ClassLiteral)
            .unwrap_or_else(|lookup_error| {
                if MESSAGES.lock().unwrap().insert(self) {
                    if matches!(
                        lookup_error,
                        KnownClassLookupError::ClassPossiblyUnbound { .. }
                    ) {
                        tracing::info!("{}", lookup_error.display(db, self));
                    } else {
                        tracing::info!(
                            "{}. Falling back to `Unknown` for the symbol instead.",
                            lookup_error.display(db, self)
                        );
                    }
                }

                match lookup_error {
                    KnownClassLookupError::ClassPossiblyUnbound { class_literal, .. } => {
                        class_literal.into()
                    }
                    KnownClassLookupError::ClassNotFound { .. }
                    | KnownClassLookupError::SymbolNotAClass { .. } => Type::unknown(),
                }
            })
    }

    /// Lookup a [`KnownClass`] in typeshed and return a [`Type`]
    /// representing that class and all possible subclasses of the class.
    ///
    /// If the class cannot be found in typeshed, a debug-level log message will be emitted stating this.
    pub(crate) fn to_subclass_of(self, db: &'db dyn Db) -> Type<'db> {
        self.to_class_literal(db)
            .into_class_type()
            .map(|class| SubclassOfType::from(db, class))
            .unwrap_or_else(SubclassOfType::subclass_of_unknown)
    }

    /// Return `true` if this symbol can be resolved to a class definition `class` in typeshed,
    /// *and* `class` is a subclass of `other`.
    pub(super) fn is_subclass_of(self, db: &'db dyn Db, other: ClassType<'db>) -> bool {
        self.try_to_class_literal(db)
            .is_ok_and(|class| class.is_subclass_of(db, None, other))
    }

    /// Return the module in which we should look up the definition for this class
    fn canonical_module(self, db: &'db dyn Db) -> KnownModule {
        match self {
            Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
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
            | Self::Super
            | Self::Property => KnownModule::Builtins,
            Self::VersionInfo => KnownModule::Sys,
            Self::Enum => KnownModule::Enum,
            Self::GenericAlias
            | Self::ModuleType
            | Self::FunctionType
            | Self::MethodType
            | Self::MethodWrapperType
            | Self::UnionType
            | Self::WrapperDescriptorType => KnownModule::Types,
            Self::NoneType => KnownModule::Typeshed,
            Self::Any
            | Self::SpecialForm
            | Self::TypeVar
            | Self::StdlibAlias
            | Self::SupportsIndex
            | Self::Sized => KnownModule::Typing,
            Self::TypeAliasType
            | Self::TypeVarTuple
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::NewType => KnownModule::TypingExtensions,
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
            Self::NotImplementedType => KnownModule::Builtins,
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
            Self::NoneType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::EllipsisType
            | Self::TypeAliasType
            | Self::UnionType
            | Self::NotImplementedType => true,

            Self::Any
            | Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
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
            | Self::BaseException
            | Self::BaseExceptionGroup
            | Self::Classmethod
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
            | Self::SupportsIndex
            | Self::StdlibAlias
            | Self::TypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Sized
            | Self::Enum
            | Self::Super
            | Self::NewType => false,
        }
    }

    /// Is this class a singleton class?
    ///
    /// A singleton class is a class where it is known that only one instance can ever exist at runtime.
    pub(super) const fn is_singleton(self) -> bool {
        match self {
            Self::NoneType
            | Self::EllipsisType
            | Self::NoDefaultType
            | Self::VersionInfo
            | Self::TypeAliasType
            | Self::NotImplementedType => true,

            Self::Any
            | Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
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
            | Self::TypeVar
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Sized
            | Self::Enum
            | Self::Super
            | Self::UnionType
            | Self::NewType => false,
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
            "Any" => Self::Any,
            "bool" => Self::Bool,
            "object" => Self::Object,
            "bytes" => Self::Bytes,
            "bytearray" => Self::Bytearray,
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
            "UnionType" => Self::UnionType,
            "MethodWrapperType" => Self::MethodWrapperType,
            "WrapperDescriptorType" => Self::WrapperDescriptorType,
            "NewType" => Self::NewType,
            "TypeAliasType" => Self::TypeAliasType,
            "TypeVar" => Self::TypeVar,
            "ParamSpec" => Self::ParamSpec,
            "ParamSpecArgs" => Self::ParamSpecArgs,
            "ParamSpecKwargs" => Self::ParamSpecKwargs,
            "TypeVarTuple" => Self::TypeVarTuple,
            "ChainMap" => Self::ChainMap,
            "Counter" => Self::Counter,
            "defaultdict" => Self::DefaultDict,
            "deque" => Self::Deque,
            "OrderedDict" => Self::OrderedDict,
            "_Alias" => Self::StdlibAlias,
            "_SpecialForm" => Self::SpecialForm,
            "_NoDefaultType" => Self::NoDefaultType,
            "SupportsIndex" => Self::SupportsIndex,
            "Sized" => Self::Sized,
            "Enum" => Self::Enum,
            "super" => Self::Super,
            "_version_info" => Self::VersionInfo,
            "ellipsis" if Program::get(db).python_version(db) <= PythonVersion::PY39 => {
                Self::EllipsisType
            }
            "EllipsisType" if Program::get(db).python_version(db) >= PythonVersion::PY310 => {
                Self::EllipsisType
            }
            "_NotImplementedType" => Self::NotImplementedType,
            _ => return None,
        };

        candidate
            .check_module(db, file_to_module(db, file)?.known()?)
            .then_some(candidate)
    }

    /// Return `true` if the module of `self` matches `module`
    fn check_module(self, db: &'db dyn Db, module: KnownModule) -> bool {
        match self {
            Self::Any
            | Self::Bool
            | Self::Object
            | Self::Bytes
            | Self::Bytearray
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
            | Self::Enum
            | Self::Super
            | Self::NotImplementedType
            | Self::UnionType
            | Self::WrapperDescriptorType => module == self.canonical_module(db),
            Self::NoneType => matches!(module, KnownModule::Typeshed | KnownModule::Types),
            Self::SpecialForm
            | Self::TypeVar
            | Self::TypeAliasType
            | Self::NoDefaultType
            | Self::SupportsIndex
            | Self::ParamSpec
            | Self::ParamSpecArgs
            | Self::ParamSpecKwargs
            | Self::TypeVarTuple
            | Self::Sized
            | Self::NewType => matches!(module, KnownModule::Typing | KnownModule::TypingExtensions),
        }
    }
}

/// Enumeration of ways in which looking up a [`KnownClass`] in typeshed could fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KnownClassLookupError<'db> {
    /// There is no symbol by that name in the expected typeshed module.
    ClassNotFound,
    /// There is a symbol by that name in the expected typeshed module,
    /// but it's not a class.
    SymbolNotAClass { found_type: Type<'db> },
    /// There is a symbol by that name in the expected typeshed module,
    /// and it's a class definition, but it's possibly unbound.
    ClassPossiblyUnbound {
        class_literal: ClassLiteralType<'db>,
    },
}

impl<'db> KnownClassLookupError<'db> {
    fn display(&self, db: &'db dyn Db, class: KnownClass) -> impl std::fmt::Display + 'db {
        struct ErrorDisplay<'db> {
            db: &'db dyn Db,
            class: KnownClass,
            error: KnownClassLookupError<'db>,
        }

        impl std::fmt::Display for ErrorDisplay<'_> {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let ErrorDisplay { db, class, error } = *self;

                let class = class.display(db);
                let python_version = Program::get(db).python_version(db);

                match error {
                    KnownClassLookupError::ClassNotFound => write!(
                        f,
                        "Could not find class `{class}` in typeshed on Python {python_version}",
                    ),
                    KnownClassLookupError::SymbolNotAClass { found_type } => write!(
                        f,
                        "Error looking up `{class}` in typeshed: expected to find a class definition \
                        on Python {python_version}, but found a symbol of type `{found_type}` instead",
                        found_type = found_type.display(db),
                    ),
                    KnownClassLookupError::ClassPossiblyUnbound { .. } => write!(
                        f,
                        "Error looking up `{class}` in typeshed on Python {python_version}: \
                        expected to find a fully bound symbol, but found one that is possibly unbound",
                    )
                }
            }
        }

        ErrorDisplay {
            db,
            class,
            error: *self,
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
    /// This is not used since typeshed switched to representing `Any` as a class; now we use
    /// `KnownClass::Any` instead. But we still support the old `Any = object()` representation, at
    /// least for now. TODO maybe remove?
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
    /// The symbol `typing.Protocol` (which can also be found as `typing_extensions.Protocol`)
    Protocol,
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
    /// The symbol `knot_extensions.CallableTypeOf`
    CallableTypeOf,

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
            | Self::Protocol
            | Self::ReadOnly
            | Self::TypeAliasType(_)
            | Self::Unknown
            | Self::AlwaysTruthy
            | Self::AlwaysFalsy
            | Self::Not
            | Self::Intersection
            | Self::TypeOf
            | Self::CallableTypeOf => Truthiness::AlwaysTrue,
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
            Self::Protocol => "typing.Protocol",
            Self::ReadOnly => "typing.ReadOnly",
            Self::TypeVar(typevar) => typevar.name(db),
            Self::TypeAliasType(_) => "typing.TypeAliasType",
            Self::Unknown => "knot_extensions.Unknown",
            Self::AlwaysTruthy => "knot_extensions.AlwaysTruthy",
            Self::AlwaysFalsy => "knot_extensions.AlwaysFalsy",
            Self::Not => "knot_extensions.Not",
            Self::Intersection => "knot_extensions.Intersection",
            Self::TypeOf => "knot_extensions.TypeOf",
            Self::CallableTypeOf => "knot_extensions.CallableTypeOf",
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
            Self::Protocol => KnownClass::SpecialForm,
            Self::TypeVar(_) => KnownClass::TypeVar,
            Self::TypeAliasType(_) => KnownClass::TypeAliasType,
            Self::TypeOf => KnownClass::SpecialForm,
            Self::Not => KnownClass::SpecialForm,
            Self::Intersection => KnownClass::SpecialForm,
            Self::CallableTypeOf => KnownClass::SpecialForm,
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
    pub(super) fn is_instance_of(self, db: &'db dyn Db, class: ClassType<'db>) -> bool {
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
            "Protocol" => Self::Protocol,
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
            "CallableTypeOf" => Self::CallableTypeOf,
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
            | Self::Protocol
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
            | Self::TypeOf
            | Self::CallableTypeOf => module.is_knot_extensions(),
        }
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
    use salsa::Setter;
    use strum::IntoEnumIterator;

    #[test]
    fn known_class_roundtrip_from_str() {
        let db = setup_db();
        for class in KnownClass::iter() {
            let class_name = class.name(&db);
            let class_module = resolve_module(&db, &class.canonical_module(&db).name()).unwrap();

            assert_eq!(
                KnownClass::try_from_file_and_name(&db, class_module.file(), class_name),
                Some(class),
                "`KnownClass::candidate_from_str` appears to be missing a case for `{class_name}`"
            );
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_latest_version() {
        let mut db = setup_db();

        Program::get(&db)
            .set_python_version(&mut db)
            .to(PythonVersion::latest());

        for class in KnownClass::iter() {
            assert_ne!(
                class.to_instance(&db),
                Type::unknown(),
                "Unexpectedly fell back to `Unknown` for `{class:?}`"
            );
        }
    }

    #[test]
    fn known_class_doesnt_fallback_to_unknown_unexpectedly_on_low_python_version() {
        let mut db = setup_db();

        for class in KnownClass::iter() {
            let version_added = match class {
                KnownClass::UnionType => PythonVersion::PY310,
                KnownClass::BaseExceptionGroup => PythonVersion::PY311,
                KnownClass::GenericAlias => PythonVersion::PY39,
                _ => PythonVersion::PY37,
            };

            Program::get(&db)
                .set_python_version(&mut db)
                .to(version_added);

            assert_ne!(
                class.to_instance(&db),
                Type::unknown(),
                "Unexpectedly fell back to `Unknown` for `{class:?}` on Python {version_added}"
            );
        }
    }
}
