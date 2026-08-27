use crate::ProgramEnvironment;
use itertools::{Either, Itertools};
use ruff_db::{
    PythonFile,
    diagnostic::Span,
    files::File,
    parsed::{ParsedModuleRef, parsed_module},
};
use ruff_python_ast as ast;
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_text_size::{Ranged, TextRange};
use std::cell::RefCell;

use super::implicit_attributes::implicit_attribute_names;
use crate::{
    Db, FxIndexMap, FxIndexSet, TypeQualifiers,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, PublicTypePolicy, TypeOrigin,
        place_from_bindings, place_from_declarations,
    },
    reachability::{DeclarationsIteratorExtension, ReachabilityConstraintsExtension},
    types::{
        ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, CallArguments,
        CallableType, ClassBase, ClassLiteral, ClassType, DATACLASS_FLAGS, DataclassFlags,
        DataclassParams, GenericAlias, GenericContext, KnownClass, KnownInstanceType,
        MaterializationKind, MemberLookupPolicy, MetaclassCandidate, MetaclassTransformInfo,
        Parameter, Parameters, PropertyInstanceType, Signature, SpecialFormType, StaticMroError,
        SubclassOfType, Type, TypeContext, TypeMapping, TypeVarVariance, TypingModule,
        UnionBuilder, UnionType,
        bound_super::BoundSuperType,
        call::{CallError, CallErrorKind},
        callable::{CallableFunctionProvenance, CallableTypeKind},
        class::{
            ClassInstanceFlags, ClassMemberResult, ClassMetaclass, CodeGeneratorKind, DisjointBase,
            DynamicTypedDictLiteral, Field, FieldKind, InstanceMemberResult, MetaclassError,
            MetaclassErrorKind, MethodDecorator, MroLookup, NamedTupleField,
            synthesize_namedtuple_class_member,
            typed_dict::{TypedDictFields, synthesize_typed_dict_method, typed_dict_class_member},
        },
        context::InferContext,
        dedicated::pydantic,
        definition_expression_type, determine_upper_bound,
        diagnostic::INVALID_DATACLASS_OVERRIDE,
        enums::{enum_metadata, is_enum_class_by_inheritance, try_unwrap_nonmember_value},
        function::{DataclassTransformerParams, KnownFunction},
        generics::Specialization,
        inferred_declaration,
        known_instance::DeprecatedInstance,
        member::{Member, class_member},
        mro::{Mro, MroIterator},
        signatures::CallableSignature,
        tuple::{FixedLengthTuple, Tuple},
        typed_dict::{TypedDictParams, TypedDictType, typed_dict_params_from_class_def},
        variance::VarianceInferable,
        visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard},
    },
};
use ty_python_core::{
    ProgramFile, attribute_scopes,
    definition::{Definition, DefinitionKind, DefinitionState},
    place_table,
    scope::ScopeId,
    semantic_index,
    symbol::Symbol,
    use_def_map,
};

/// Representation of a class definition statement in the AST: either a non-generic class, or a
/// generic class that has not been specialized.
///
/// This does not in itself represent a type, but can be transformed into a [`ClassType`] that
/// does. (For generic classes, this requires specializing its generic context.)
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct StaticClassLiteral<'db> {
    /// Name of the class at definition
    #[returns(ref)]
    pub(crate) name: Name,

    #[returns(copy)]
    pub(crate) body_scope: ScopeId<'db>,

    #[returns(copy)]
    pub(crate) known: Option<KnownClass>,

    /// If this class is deprecated, this holds the deprecation message.
    #[returns(copy)]
    pub(crate) deprecated: Option<DeprecatedInstance<'db>>,

    #[returns(copy)]
    pub(crate) type_check_only: bool,

    #[returns(copy)]
    pub(crate) dataclass_params: Option<DataclassParams<'db>>,
    #[returns(copy)]
    pub(crate) dataclass_transformer_params: Option<DataclassTransformerParams<'db>>,

    /// Whether this class is decorated with `@functools.total_ordering`
    #[returns(copy)]
    pub(crate) total_ordering: bool,

    /// Whether this class has any decorators.
    #[returns(copy)]
    pub(crate) has_decorators: bool,

    /// Whether this class has PEP 695 type parameters.
    #[returns(copy)]
    pub(crate) has_type_params: bool,

    /// Whether this class has any explicit base classes.
    #[returns(copy)]
    pub(crate) has_explicit_bases: bool,

    /// Whether this class has an explicit `metaclass` keyword argument.
    #[returns(copy)]
    pub(crate) has_explicit_metaclass: bool,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for StaticClassLiteral<'_> {}

/// The result of [`StaticClassLiteral::inherited_frozen_dataclass_dispatch`].
///
/// See that method for details on how generated frozen-dataclass methods handle fields and
/// non-fields on subclass instances.
#[derive(Clone, Copy)]
pub(crate) enum FrozenDataclassDispatch<'db> {
    /// A reachable frozen dataclass rejects assignment to or deletion of one of its fields.
    FrozenField,
    /// Every reachable frozen method delegates, with lookup resuming after this base.
    Delegate(StaticClassLiteral<'db>),
}

impl<'db> FrozenDataclassDispatch<'db> {
    /// Returns the receiver for the next step of assignment or deletion validation.
    ///
    /// Validation stays on `object_ty` for a frozen field because the generated method rejects the
    /// mutation. For a non-field, the generated method calls `super(frozen_base, object_ty)`, so
    /// lookup must resume after the last frozen base. For example, assigning `Child().y` for
    /// `class Child(Frozen, Later)` uses `super(Frozen, child)` when `y` is not a field of `Frozen`;
    /// this preserves a later `__setattr__` or a descriptor for `y`.
    pub(crate) fn receiver(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        object_ty: Type<'db>,
    ) -> Type<'db> {
        match self {
            Self::FrozenField => object_ty,
            Self::Delegate(frozen_base) => BoundSuperType::build(
                db,
                env,
                Type::ClassLiteral(ClassLiteral::Static(frozen_base)),
                object_ty,
            )
            .unwrap_or(object_ty),
        }
    }
}

/// A method synthesized for a frozen dataclass.
#[derive(Clone, Copy)]
enum FrozenDataclassMethod {
    SetAttr,
    DelAttr,
}

impl FrozenDataclassMethod {
    /// Returns the frozen-dataclass method for `name`, if it is `__setattr__` or `__delattr__`.
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "__setattr__" => Some(Self::SetAttr),
            "__delattr__" => Some(Self::DelAttr),
            _ => None,
        }
    }

    /// Returns the corresponding Python special-method name.
    const fn name(self) -> &'static str {
        match self {
            Self::SetAttr => "__setattr__",
            Self::DelAttr => "__delattr__",
        }
    }
}

/// Fields protected by reachable frozen-dataclass methods.
struct InheritedFrozenDataclassFields<'db> {
    names: Box<[Name]>,
    /// The final frozen dataclass whose generated method participates in dispatch.
    ///
    /// For a non-field, mutation validation resumes after this class in the MRO.
    last_frozen_base: StaticClassLiteral<'db>,
}

/// Annotated fields and class-variable declarations collected from one class body.
///
/// Class variables are not constructor parameters, but they can mask inherited dataclass fields:
///
/// ```python
/// @dataclass
/// class Child(Base):
///     value: ClassVar[int]
///     required: int
/// ```
///
/// Here, `required` is a constructor field and `value` masks an inherited `Base.value` field.
#[derive(Debug, Default, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
struct OwnClassFields<'db> {
    fields: FxIndexMap<Name, Field<'db>>,
    class_variables: Box<[Name]>,
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    /// Return `true` if this class represents `known_class`
    pub(crate) fn is_known(self, db: &'db dyn Db, known_class: KnownClass) -> bool {
        self.known(db) == Some(known_class)
    }

    pub(crate) fn is_tuple(self, db: &'db dyn Db) -> bool {
        self.is_known(db, KnownClass::Tuple)
    }

    /// Returns `true` if this class inherits from a functional namedtuple
    /// (`DynamicNamedTupleLiteral`) that has unknown fields.
    ///
    /// When the base namedtuple's fields were determined dynamically (e.g., from a variable),
    /// we can't synthesize precise method signatures and should fall back to `NamedTupleFallback`.
    fn namedtuple_base_has_unknown_fields(self, db: &'db dyn Db) -> bool {
        self.explicit_bases(db).iter().any(|base| match base {
            Type::ClassLiteral(ClassLiteral::DynamicNamedTuple(namedtuple)) => {
                !namedtuple.has_known_fields(db)
            }
            _ => false,
        })
    }

    /// Returns `true` if this class is a dataclass-like class.
    ///
    /// This covers `@dataclass`-decorated classes, as well as classes created via
    /// `dataclass_transform` (function-based, metaclass-based, and base-class-based).
    /// This specifically excludes Pydantic models, even though their metaclass also uses
    /// `dataclass_transform`.
    pub(crate) fn is_dataclass_like(self, db: &'db dyn Db) -> bool {
        CodeGeneratorKind::from_class(db, ClassLiteral::Static(self))
            .is_some_and(CodeGeneratorKind::is_dataclass_like)
    }

    /// Returns `true` if this class is decorated with `@dataclass(order=True)`.
    pub(crate) fn is_ordered_dataclass(self, db: &'db dyn Db) -> bool {
        self.find_dataclass_decorator_position(db).is_some()
            && self
                .dataclass_params(db)
                .is_some_and(|params| params.flags(db).contains(DataclassFlags::ORDER))
    }

    /// Returns a new [`StaticClassLiteral`] with the given dataclass params, preserving all other fields.
    pub(crate) fn with_dataclass_params(
        self,
        db: &'db dyn Db,
        dataclass_params: Option<DataclassParams<'db>>,
    ) -> Self {
        Self::new(
            db,
            self.name(db),
            self.body_scope(db),
            self.known(db),
            self.deprecated(db),
            self.type_check_only(db),
            dataclass_params,
            self.dataclass_transformer_params(db),
            self.total_ordering(db),
            self.has_decorators(db),
            self.has_type_params(db),
            self.has_explicit_bases(db),
            self.has_explicit_metaclass(db),
        )
    }

    /// Returns `true` if this class defines any ordering method (`__lt__`, `__le__`, `__gt__`,
    /// `__ge__`) in its own body (not inherited). Used by `@total_ordering` to determine if
    /// synthesis is valid.
    #[salsa::tracked(returns(copy))]
    pub(crate) fn has_own_ordering_method(self, db: &'db dyn Db) -> bool {
        let body_scope = self.body_scope(db);
        ["__lt__", "__le__", "__gt__", "__ge__"]
            .iter()
            .any(|method| !class_member(db, body_scope, method).is_undefined())
    }

    #[salsa::tracked(returns(copy))]
    pub(crate) fn has_own_comparison_methods(self, db: &'db dyn Db) -> bool {
        let body_scope = self.body_scope(db);
        ["__lt__", "__le__", "__gt__", "__ge__"]
            .iter()
            .all(|method| !class_member(db, body_scope, method).is_undefined())
    }

    /// Returns `true` if any class in this class's MRO (excluding `object`) defines an ordering
    /// method (`__lt__`, `__le__`, `__gt__`, `__ge__`). Used by `@total_ordering` validation.
    pub(crate) fn has_ordering_method_in_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> bool {
        self.total_ordering_root_method(db, specialization)
            .is_some()
    }

    /// Returns the type of the ordering method used by `@total_ordering`, if any.
    ///
    /// Following `functools.total_ordering` precedence, we prefer `__lt__` > `__le__` > `__gt__` >
    /// `__ge__`, regardless of whether the method is defined locally or inherited.
    ///
    /// Note: We use direct scope lookups here to avoid infinite recursion
    /// through `own_class_member` -> `own_synthesized_member`.
    fn total_ordering_root_method(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Option<Type<'db>> {
        const ORDERING_METHODS: [&str; 4] = ["__lt__", "__le__", "__gt__", "__ge__"];

        for name in ORDERING_METHODS {
            for base in self.iter_mro(db, specialization) {
                let Some(base_class) = base.into_class() else {
                    continue;
                };
                match base_class.class_literal(db) {
                    ClassLiteral::Static(base_literal) => {
                        if base_literal.is_known(db, KnownClass::Object) {
                            continue;
                        }
                        let member = class_member(db, base_literal.body_scope(db), name);
                        if let Some(ty) = member.ignore_possibly_undefined() {
                            let base_specialization = base_class
                                .static_class_literal(db)
                                .and_then(|(_, spec)| spec);
                            return Some(ty.apply_optional_specialization(db, base_specialization));
                        }
                    }
                    ClassLiteral::Dynamic(dynamic) => {
                        // Dynamic classes (created with `type()`) can also define ordering methods
                        // in their namespace dict.
                        let member = dynamic.own_class_member(db, name);
                        if let Some(ty) = member.ignore_possibly_undefined() {
                            return Some(ty);
                        }
                    }
                    ClassLiteral::DynamicNamedTuple(_)
                    | ClassLiteral::DynamicTypedDict(_)
                    | ClassLiteral::DynamicEnum(_) => {}
                }
            }
        }

        None
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size,
    )]
    pub(crate) fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        // Several typeshed definitions examine `sys.version_info`. To break cycles, we hard-code
        // the knowledge that this class is not generic.
        if self.is_known(db, KnownClass::VersionInfo) {
            return None;
        }

        // We've already verified that the class literal does not contain both a PEP-695 generic
        // scope and a `typing.Generic` base class.
        //
        // Note that if a class has an explicit legacy generic context (by inheriting from
        // `typing.Generic`), and also an implicit one (by inheriting from other generic classes,
        // specialized by typevars), the explicit one takes precedence.
        self.pep695_generic_context(db)
            .or_else(|| self.legacy_generic_context(db))
            .or_else(|| self.inherited_legacy_generic_context(db))
    }

    pub(crate) fn has_pep_695_type_params(self, db: &'db dyn Db) -> bool {
        self.pep695_generic_context(db).is_some()
    }

    pub(crate) fn pep695_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        if !self.has_type_params(db) {
            return None;
        }
        self.pep695_generic_context_inner(db)
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size,
    )]
    fn pep695_generic_context_inner(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        let scope = self.body_scope(db);
        let program_file = scope.program_file(db);
        let python_file = program_file.python_file(db);
        let parsed = parsed_module(db, python_file).load(db);
        let class_def_node = scope.node(db).expect_class().node(&parsed);
        class_def_node.type_params.as_ref().map(|type_params| {
            let index = semantic_index(db, program_file);
            let definition = index.expect_single_definition(class_def_node);
            GenericContext::from_type_params(db, index, definition, type_params)
        })
    }

    pub(crate) fn legacy_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.explicit_bases(db).iter().find_map(|base| match base {
            Type::KnownInstance(
                KnownInstanceType::SubscriptedGeneric(generic_context)
                | KnownInstanceType::SubscriptedProtocol(generic_context),
            ) => Some(*generic_context),
            _ => None,
        })
    }

    pub(crate) fn inherited_legacy_generic_context(
        self,
        db: &'db dyn Db,
    ) -> Option<GenericContext<'db>> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, _, _| None,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn inherited_legacy_generic_context_inner<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
        ) -> Option<GenericContext<'db>> {
            GenericContext::from_base_classes(
                db,
                class.definition(db),
                class
                    .explicit_bases(db)
                    .iter()
                    .copied()
                    .filter(|ty| matches!(ty, Type::GenericAlias(_))),
            )
        }

        if !self.has_explicit_bases(db) {
            return None;
        }
        inherited_legacy_generic_context_inner(db, self)
    }

    /// Returns all of the typevars that are referenced in this class's base class list.
    /// (This is used to ensure that classes do not reference typevars from enclosing
    /// generic contexts.)
    pub(crate) fn typevars_referenced_in_bases(
        self,
        db: &'db dyn Db,
    ) -> FxIndexSet<BoundTypeVarInstance<'db>> {
        struct CollectTypeVars<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            typevars: RefCell<FxIndexSet<BoundTypeVarInstance<'db>>>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for CollectTypeVars<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_bound_type_var_type(
                &self,
                _db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                self.typevars.borrow_mut().insert(bound_typevar);
            }

            fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
                // The generic context contains the base class's formal type parameters, not type
                // variables referenced by this class's base expression.
                for ty in alias.specialization(db).types(db) {
                    self.visit_type(db, *ty);
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        let env = ProgramEnvironment::from_scope(self.body_scope(db));
        let visitor = CollectTypeVars {
            env: &env,
            typevars: RefCell::default(),
            recursion_guard: TypeCollector::default(),
        };
        for base in self.explicit_bases(db) {
            visitor.visit_type(db, *base);
        }
        visitor.typevars.into_inner()
    }

    /// Returns the generic context that should be inherited by any constructor methods of this class.
    fn inherited_generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.generic_context(db)
    }

    pub(crate) fn file(self, db: &dyn Db) -> File {
        self.body_scope(db).file(db)
    }

    pub(crate) fn python_file(self, db: &'db dyn Db) -> PythonFile<'db> {
        self.body_scope(db).python_file(db)
    }

    pub(crate) fn program_file(self, db: &'db dyn Db) -> ProgramFile<'db> {
        self.body_scope(db).program_file(db)
    }

    /// Return the original [`ast::StmtClassDef`] node associated with this class
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn node<'ast>(self, db: &'db dyn Db, module: &'ast ParsedModuleRef) -> &'ast ast::StmtClassDef {
        self.body_scope(db).node(db).expect_class().node(module)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        let body_scope = self.body_scope(db);
        let index = semantic_index(db, body_scope.program_file(db));
        index.expect_single_definition(body_scope.node(db).expect_class())
    }

    pub(crate) fn apply_specialization(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(GenericContext<'db>) -> Specialization<'db>,
    ) -> ClassType<'db> {
        match self.generic_context(db) {
            None => ClassType::NonGeneric(self.into()),
            Some(generic_context) => {
                let specialization = f(generic_context);

                ClassType::Generic(GenericAlias::new(db, self, specialization))
            }
        }
    }

    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            specialization
                .unwrap_or_else(|| generic_context.default_specialization(db, self.known(db)))
        })
    }

    pub(crate) fn top_materialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            let env = ProgramEnvironment::from_program(generic_context.program(db));
            generic_context
                .unknown_specialization(db, self.known(db))
                .materialize_impl(
                    db,
                    MaterializationKind::Top,
                    &ApplyTypeMappingVisitor::new(&env),
                )
        })
    }

    /// Returns the default specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// applies the default specialization to the class's typevars.
    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.default_specialization(db, self.known(db))
        })
    }

    /// Returns the unknown specialization of this class. For non-generic classes, the class is
    /// returned unchanged. For a non-specialized generic class, we return a generic alias that
    /// maps each of the class's typevars to `Unknown`.
    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.unknown_specialization(db, self.known(db))
        })
    }

    /// Returns a specialization of this class where each typevar is mapped to itself.
    pub(crate) fn identity_specialization(self, db: &'db dyn Db) -> ClassType<'db> {
        self.apply_specialization(db, |generic_context| {
            generic_context.identity_specialization(db)
        })
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
    pub(crate) fn explicit_bases(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        #[salsa::tracked(returns(deref), cycle_initial=explicit_bases_cycle_initial, cycle_fn=explicit_bases_cycle_fn, heap_size=ruff_memory_usage::heap_size)]
        fn explicit_bases_inner<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
        ) -> Box<[Type<'db>]> {
            tracing::trace!(
                "StaticClassLiteral::explicit_bases_query: {}",
                class.name(db)
            );

            let program_file = class.program_file(db);
            let python_file = program_file.python_file(db);
            let module = parsed_module(db, python_file).load(db);
            let class_stmt = class.node(db, &module);

            let class_definition =
                semantic_index(db, program_file).expect_single_definition(class_stmt);
            expanded_class_base_entries(db, class.known(db), class_stmt, class_definition)
                .into_iter()
                .map(ExpandedClassBaseEntry::ty)
                .collect()
        }

        if !self.has_explicit_bases(db) {
            return &[];
        }
        explicit_bases_inner(db, self)
    }

    /// Return `Some()` if this class is known to be a [`DisjointBase`], or `None` if it is not.
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        if self
            .known_function_decorators(db)
            .contains(&KnownFunction::DisjointBase)
            && !self.is_typed_dict(db)
            && !self.is_protocol(db)
        {
            Some(DisjointBase::due_to_decorator(self))
        } else if self.has_nonempty_slots(db) {
            Some(DisjointBase::due_to_dunder_slots(ClassLiteral::Static(
                self,
            )))
        } else {
            None
        }
    }

    /// Iterate over the explicit bases that contribute to metaclass selection.
    fn metaclass_bases(self, db: &'db dyn Db) -> impl Iterator<Item = ClassBase<'db>> {
        let env = ProgramEnvironment::from_scope(self.body_scope(db));
        self.explicit_bases(db)
            .iter()
            .copied()
            .filter_map(move |ty| {
                ClassBase::try_from_type(db, &env, ty, Some(ClassLiteral::Static(self)))
            })
            .filter(|base| matches!(base, ClassBase::Class(_) | ClassBase::Protocol))
    }

    /// Determine if this class is a protocol.
    ///
    /// This method relies on the accuracy of the [`KnownClass::is_protocol`] method,
    /// which hardcodes knowledge about certain special-cased classes. See the docs on
    /// that method for why we do this rather than relying on generalised logic for all
    /// classes, including the special-cased ones that are included in the [`KnownClass`]
    /// enum.
    pub(crate) fn is_protocol(self, db: &'db dyn Db) -> bool {
        self.known(db)
            .map(KnownClass::is_protocol)
            .unwrap_or_else(|| {
                // Iterate through the last three bases of the class
                // searching for `Protocol` or `Protocol[]` in the bases list.
                //
                // If `Protocol` is present in the bases list of a valid protocol class, it must either:
                //
                // - be the last base
                // - OR be the last-but-one base (with the final base being `Generic[]` or `object`)
                // - OR be the last-but-two base (with the penultimate base being `Generic[]`
                //                                and the final base being `object`)
                self.explicit_bases(db).iter().rev().take(3).any(|base| {
                    matches!(
                        base,
                        Type::SpecialForm(SpecialFormType::Protocol)
                            | Type::KnownInstance(KnownInstanceType::SubscriptedProtocol(_))
                    )
                })
            })
    }

    /// Return the types of the decorators on this class
    fn decorators(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        if !self.has_decorators(db) {
            return &[];
        }
        self.decorators_inner(db)
    }

    #[salsa::tracked(returns(deref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    fn decorators_inner(self, db: &'db dyn Db) -> Box<[Type<'db>]> {
        tracing::trace!("StaticClassLiteral::decorators: {}", self.name(db));

        let program_file = self.program_file(db);
        let python_file = program_file.python_file(db);
        let module = parsed_module(db, python_file).load(db);

        let class_stmt = self.node(db, &module);
        if class_stmt.decorator_list.is_empty() {
            return Box::new([]);
        }

        let class_definition =
            semantic_index(db, self.program_file(db)).expect_single_definition(class_stmt);

        class_stmt
            .decorator_list
            .iter()
            .map(|decorator_node| {
                definition_expression_type(db, class_definition, &decorator_node.expression)
            })
            .collect()
    }

    pub(crate) fn known_function_decorators(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = KnownFunction> + 'db {
        self.decorators(db)
            .iter()
            .filter_map(|deco| deco.as_function_literal())
            .filter_map(|decorator| decorator.known(db))
    }

    /// Iterate through the decorators on this class, returning the index of the first one
    /// that is either `@dataclass` or `@dataclass(...)`.
    pub(crate) fn find_dataclass_decorator_position(self, db: &'db dyn Db) -> Option<usize> {
        let program_file = self.program_file(db);
        let python_file = program_file.python_file(db);
        let module = parsed_module(db, python_file).load(db);
        let class_stmt = self.node(db, &module);
        let class_definition =
            semantic_index(db, program_file).expect_single_definition(class_stmt);

        class_stmt.decorator_list.iter().position(|decorator| {
            let decorator_callable = decorator
                .expression
                .as_call_expr()
                .map_or(&decorator.expression, |call| &call.func);

            definition_expression_type(db, class_definition, decorator_callable)
                .as_function_literal()
                .is_some_and(|function| function.is_known(db, KnownFunction::Dataclass))
        })
    }

    /// Is this class final?
    pub(crate) fn is_final(self, db: &'db dyn Db) -> bool {
        self.known_function_decorators(db)
            .contains(&KnownFunction::Final)
            || enum_metadata(db, ClassLiteral::Static(self)).is_some()
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
    pub(in crate::types) fn try_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Result<&'db Mro<'db>, &'db StaticMroError<'db>> {
        match specialization {
            None => self.try_mro_unspecialized(db),
            Some(specialization) => self.try_mro_specialized(db, specialization),
        }
    }

    #[salsa::tracked(
        returns(as_ref),
        cycle_initial=|db, _, self_: StaticClassLiteral<'db>| {
            let env = ProgramEnvironment::from_scope(self_.body_scope(db));
            Err(StaticMroError::cycle(
                db, &env,
                self_.apply_optional_specialization(db, None),
            ))
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn try_mro_unspecialized(self, db: &'db dyn Db) -> Result<Mro<'db>, StaticMroError<'db>> {
        tracing::trace!("StaticClassLiteral::try_mro: {}", self.name(db));
        Mro::of_static_class(db, self, None)
    }

    #[salsa::tracked(
        returns(as_ref),
        cycle_initial=|db, _, self_: StaticClassLiteral<'db>, specialization| {
            let env = ProgramEnvironment::from_scope(self_.body_scope(db));
            Err(StaticMroError::cycle(
                db, &env,
                self_.apply_optional_specialization(db, Some(specialization)),
            ))
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn try_mro_specialized(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Result<Mro<'db>, StaticMroError<'db>> {
        tracing::trace!("StaticClassLiteral::try_mro: {}", self.name(db));
        Mro::of_static_class(db, self, Some(specialization))
    }

    /// Iterate over the [method resolution order] ("MRO") of the class.
    ///
    /// If the MRO could not be accurately resolved, this method falls back to iterating
    /// over an MRO that has the class directly inheriting from `Unknown`. Use
    /// [`StaticClassLiteral::try_mro`] if you need to distinguish between the success and failure
    /// cases rather than simply iterating over the inferred resolution order for the class.
    ///
    /// [method resolution order]: https://docs.python.org/3/glossary.html#term-method-resolution-order
    pub(crate) fn iter_mro(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> MroIterator<'db> {
        MroIterator::new(db, ClassLiteral::Static(self), specialization)
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

    /// Return whether this class defines its own non-default `__getattribute__`.
    ///
    /// An explicit metaclass can install the method even when the class body does not define it:
    ///
    /// ```python
    /// def interceptor(self, name): ...
    ///
    /// class Meta(type):
    ///     def __init__(cls, *args):
    ///         cls.__getattribute__ = interceptor
    ///
    /// class Example(metaclass=Meta): ...
    /// ```
    fn has_own_custom_getattribute(self, db: &'db dyn Db) -> bool {
        if matches!(self.known(db), Some(KnownClass::Object | KnownClass::Type)) {
            return false;
        }

        if place_table(db, self.body_scope(db))
            .symbol_id("__getattribute__")
            .is_some()
        {
            return true;
        }

        if !self.has_explicit_metaclass(db) {
            return false;
        }

        let Some(metaclass) = self.metaclass(db).to_class_type(db) else {
            return true;
        };

        metaclass.iter_mro(db).any(|base| match base {
            ClassBase::Any | ClassBase::Dynamic(_) | ClassBase::Divergent(_) => true,
            ClassBase::Class(base) => base.static_class_literal(db).is_none_or(|(base, _)| {
                implicit_attribute_names(db, base.body_scope(db))
                    .binary_search(&Name::new_static("__getattribute__"))
                    .is_ok()
            }),
            ClassBase::Generic | ClassBase::Protocol | ClassBase::TypedDict(_) => false,
        })
    }

    /// Return the properties shared by all instances of this class.
    pub(super) fn instance_flags(self, db: &'db dyn Db) -> ClassInstanceFlags {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, _, _| ClassInstanceFlags::empty(),
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn instance_flags_inner<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
        ) -> ClassInstanceFlags {
            let mut flags = ClassInstanceFlags::empty();
            for base in class.iter_mro(db, None) {
                match base {
                    ClassBase::Any => flags.insert(
                        ClassInstanceFlags::INHERITS_FROM_EXPLICIT_ANY
                            | ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE,
                    ),
                    ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {
                        flags.insert(ClassInstanceFlags::HAS_DYNAMIC_GETATTRIBUTE);
                    }
                    ClassBase::TypedDict(_) => flags.insert(ClassInstanceFlags::TYPED_DICT),
                    ClassBase::Class(class)
                        if class
                            .static_class_literal(db)
                            .is_none_or(|(class, _)| class.has_own_custom_getattribute(db)) =>
                    {
                        flags.insert(ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE);
                    }
                    ClassBase::Class(_) | ClassBase::Generic | ClassBase::Protocol => {}
                }
            }
            flags
        }

        let mut flags = if let Some(known) = self.known(db) {
            if known.is_typed_dict_subclass() {
                ClassInstanceFlags::TYPED_DICT
            } else {
                ClassInstanceFlags::empty()
            }
        } else if self.has_explicit_bases(db) {
            return instance_flags_inner(db, self);
        } else {
            ClassInstanceFlags::empty()
        };

        flags.set(
            ClassInstanceFlags::HAS_CUSTOM_GETATTRIBUTE,
            self.has_own_custom_getattribute(db),
        );
        flags
    }

    /// Return the module defining the `TypedDict` base of this class.
    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn typed_dict_module(self, db: &'db dyn Db) -> Option<TypingModule> {
        self.iter_mro(db, None)
            .find_map(ClassBase::typed_dict_module)
    }

    /// Return `true` if this class constitutes a typed dict specification (inherits from
    /// `typing.TypedDict` or `typing_extensions.TypedDict`, either directly or indirectly).
    pub fn is_typed_dict(self, db: &'db dyn Db) -> bool {
        if let Some(known) = self.known(db) {
            return known.is_typed_dict_subclass();
        }

        self.has_explicit_bases(db)
            && self
                .instance_flags(db)
                .contains(ClassInstanceFlags::TYPED_DICT)
    }

    /// Return `true` if this class is, or inherits from, a `NamedTuple` (inherits from
    /// `typing.NamedTuple`, either directly or indirectly, including functional forms like
    /// `NamedTuple("X", ...)`).
    pub(crate) fn has_named_tuple_class_in_mro(self, db: &'db dyn Db) -> bool {
        self.iter_mro(db, None)
            .filter_map(ClassBase::into_class)
            .any(|base| match base.class_literal(db) {
                ClassLiteral::DynamicNamedTuple(_) => true,
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicTypedDict(_)
                | ClassLiteral::DynamicEnum(_) => false,
                ClassLiteral::Static(class) => class
                    .explicit_bases(db)
                    .contains(&Type::SpecialForm(SpecialFormType::NamedTuple)),
            })
    }

    /// Compute `TypedDict` parameters dynamically based on MRO detection and AST parsing.
    fn typed_dict_params(self, db: &'db dyn Db) -> Option<TypedDictParams> {
        if !self.is_typed_dict(db) {
            return None;
        }

        let module = parsed_module(db, self.python_file(db)).load(db);
        let class_stmt = self.node(db, &module);
        Some(typed_dict_params_from_class_def(class_stmt))
    }

    /// Returns dataclass params for this class, sourced from both dataclass params and dataclass
    /// transform params
    fn merged_dataclass_params(
        self,
        db: &'db dyn Db,
        field_policy: CodeGeneratorKind<'db>,
    ) -> (Option<DataclassParams<'db>>, Option<DataclassParams<'db>>) {
        let dataclass_params = self.dataclass_params(db);

        let mut transformer_params =
            field_policy
                .dataclass_transformer_params()
                .map(|transformer_params| {
                    DataclassParams::from_transformer_params(db, transformer_params)
                });

        // Dataclass transformer flags can be overwritten using class arguments.
        if let Some(transformer_params) = transformer_params.as_mut()
            && let Some(class_def) = self.definition(db).kind(db).as_class()
        {
            let module = parsed_module(db, self.python_file(db)).load(db);

            if let Some(arguments) = &class_def.node(&module).arguments {
                let mut flags = transformer_params.flags(db);

                for ast::Keyword { arg, value, .. } in &arguments.keywords {
                    if let Some(arg_name) = arg
                        && let ast::Expr::BooleanLiteral(is_set) = value
                    {
                        for (flag_name, flag) in DATACLASS_FLAGS {
                            if arg_name == *flag_name {
                                flags.set(*flag, is_set.value);
                            }
                        }
                    }
                }

                *transformer_params =
                    DataclassParams::new(db, flags, transformer_params.field_specifiers(db));
            }
        }

        (dataclass_params, transformer_params)
    }

    /// Returns the effective frozen status of this class if it's a dataclass-like class.
    ///
    /// Returns `Some(true)` for a frozen dataclass-like class, `Some(false)` for a non-frozen one,
    /// and `None` if the class is not a dataclass-like class, or if the dataclass is neither frozen
    /// nor non-frozen.
    pub(crate) fn is_frozen_dataclass(self, db: &'db dyn Db) -> Option<bool> {
        // Check if this is a base-class-based transformer that has dataclass_transformer_params directly
        // attached to it (because it is itself decorated with `@dataclass_transform`), or if this class
        // has an explicit metaclass that is decorated with `@dataclass_transform`.
        //
        // In both cases, this signifies that this class is neither frozen nor non-frozen.
        //
        // See <https://typing.python.org/en/latest/spec/dataclasses.html#dataclass-semantics> for details.
        if self.dataclass_transformer_params(db).is_some()
            || self
                .try_metaclass(db)
                .is_ok_and(|(_, info)| info.is_some_and(|i| i.from_explicit_metaclass))
        {
            return None;
        }

        if let field_policy @ CodeGeneratorKind::DataclassLike(_) =
            CodeGeneratorKind::from_class(db, self.into())?
        {
            // Otherwise, if this class is a dataclass-like class, determine its frozen status based on
            // dataclass params and dataclass transformer params.
            Some(self.has_dataclass_param(db, field_policy, DataclassFlags::FROZEN))
        } else {
            None
        }
    }

    /// Return `true` if Pydantic's effective model configuration marks this model as frozen.
    fn is_frozen_pydantic_model(db: &'db dyn Db, field_policy: CodeGeneratorKind<'db>) -> bool {
        field_policy
            .pydantic_metadata()
            .is_some_and(|metadata| metadata.is_frozen(db))
    }

    /// Checks if the given dataclass parameter flag is set for this class.
    /// This checks both the `dataclass_params` and `transformer_params`.
    pub(crate) fn has_dataclass_param(
        self,
        db: &'db dyn Db,
        field_policy: CodeGeneratorKind<'db>,
        param: DataclassFlags,
    ) -> bool {
        let (dataclass_params, transformer_params) = self.merged_dataclass_params(db, field_policy);
        dataclass_params.is_some_and(|params| params.flags(db).contains(param))
            || transformer_params.is_some_and(|params| params.flags(db).contains(param))
    }

    /// Returns the nearest `@dataclass_transform` parameters for this class or its MRO.
    ///
    /// This is used for metaclass-based transforms because `__dataclass_transform__` is inherited,
    /// so a metaclass subclass should preserve the transform metadata of its decorated base class
    /// unless it provides its own.
    fn inherited_dataclass_transformer_params(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
    ) -> Option<DataclassTransformerParams<'db>> {
        self.dataclass_transformer_params(db).or_else(|| {
            self.iter_mro(db, specialization).skip(1).find_map(|base| {
                base.into_class().and_then(|class| {
                    class
                        .static_class_literal(db)
                        .and_then(|(lit, _)| lit.dataclass_transformer_params(db))
                })
            })
        })
    }

    /// Return the explicit `metaclass` of this class, if one is defined.
    ///
    /// ## Note
    /// Only call this function from queries in the same file or your
    /// query depends on the AST of another file (bad!).
    fn explicit_metaclass(self, db: &'db dyn Db, module: &ParsedModuleRef) -> Option<Type<'db>> {
        let class_stmt = self.node(db, module);
        let metaclass_node = &class_stmt
            .arguments
            .as_ref()?
            .find_keyword("metaclass")?
            .value;

        let class_definition = self.definition(db);

        Some(definition_expression_type(
            db,
            class_definition,
            metaclass_node,
        ))
    }

    /// Return the metaclass of this class, or `type[Unknown]` if the metaclass cannot be inferred.
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        let env = ProgramEnvironment::from_scope(self.body_scope(db));
        self.inferred_metaclass(db).to_type(db, &env)
    }

    pub(in crate::types) fn inferred_metaclass(self, db: &'db dyn Db) -> ClassMetaclass<'db> {
        self.try_metaclass(db)
            .map(|(metaclass, _)| metaclass)
            .unwrap_or_else(|_| ClassMetaclass::Selected(SubclassOfType::subclass_of_unknown()))
    }

    /// Return the selected metaclass or protocol fallback, or an error if it cannot be inferred.
    pub(in crate::types) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<(ClassMetaclass<'db>, Option<MetaclassTransformInfo<'db>>), MetaclassError<'db>>
    {
        #[salsa::tracked(
            returns(clone),
            cycle_initial=|_, _, _| Err(MetaclassError {
                kind: MetaclassErrorKind::Cycle,
            }),
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn try_metaclass_inner<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
        ) -> Result<(ClassMetaclass<'db>, Option<MetaclassTransformInfo<'db>>), MetaclassError<'db>>
        {
            let program_file = class.program_file(db);
            let python_file = program_file.python_file(db);
            let env = ProgramEnvironment::from_file(program_file);
            tracing::trace!("StaticClassLiteral::try_metaclass: {}", class.name(db));

            // Identify the class's own metaclass (or take the first base class's metaclass).
            let mut base_classes = class.metaclass_bases(db).peekable();

            if (base_classes.peek().is_some() && class.inheritance_cycle(db).is_some())
                || class.try_mro(db, None).is_err_and(StaticMroError::is_cycle)
            {
                // We emit diagnostics for cyclic class definitions elsewhere.
                // Avoid attempting to infer the metaclass if the class is cyclically defined.
                return Ok((
                    ClassMetaclass::Selected(SubclassOfType::subclass_of_unknown()),
                    None,
                ));
            }

            let module = parsed_module(db, python_file).load(db);

            let explicit_metaclass = class.explicit_metaclass(db, &module);

            // Generic metaclasses parameterized by type variables are not supported.
            // `metaclass=Meta[int]` is fine, but `metaclass=Meta[T]` is not.
            // See: https://typing.python.org/en/latest/spec/generics.html#generic-metaclasses
            if let Some(Type::GenericAlias(alias)) = explicit_metaclass {
                let specialization_has_typevars = alias
                    .specialization(db)
                    .types(db)
                    .iter()
                    .any(|ty| ty.has_typevar_or_typevar_instance(db, &env));
                if specialization_has_typevars {
                    return Err(MetaclassError {
                        kind: MetaclassErrorKind::GenericMetaclass,
                    });
                }
            }

            let mut has_protocol_fallback = false;
            let mut base_metaclasses = base_classes.filter_map(|base| {
                match base.inferred_metaclass(db, &env, ClassLiteral::Static(class)) {
                    ClassMetaclass::Selected(metaclass) => Some((base, metaclass)),
                    ClassMetaclass::ProtocolFallback => {
                        has_protocol_fallback = true;
                        None
                    }
                }
            });
            let (metaclass, base) = if let Some(metaclass) = explicit_metaclass {
                (metaclass, None)
            } else if let Some((base_class, metaclass)) = base_metaclasses.next() {
                (metaclass, Some(base_class))
            } else {
                (KnownClass::Type.to_class_literal(db, &env), None)
            };

            let mut candidate = if let Some(metaclass_ty) = metaclass.to_class_type(db) {
                MetaclassCandidate {
                    metaclass: metaclass_ty,
                    base,
                }
            } else {
                let name = Type::string_literal(db, class.name(db));
                let bases = Type::heterogeneous_tuple(db, &env, class.explicit_bases(db));
                let namespace = KnownClass::Dict.to_specialized_instance(
                    db,
                    &env,
                    &[KnownClass::Str.to_instance(db, &env), Type::any()],
                );

                // TODO: Other keyword arguments?
                let arguments = CallArguments::positional([name, bases, namespace]);

                let return_ty_result = match metaclass.try_call(db, &env, &arguments) {
                    Ok(bindings) => Ok(bindings.return_type(db, &env)),

                    Err(CallError(CallErrorKind::NotCallable, bindings)) => Err(MetaclassError {
                        kind: MetaclassErrorKind::NotCallable(bindings.callable_type()),
                    }),

                    // TODO we should also check for binding errors that would indicate the metaclass
                    // does not accept the right arguments
                    Err(CallError(CallErrorKind::BindingError, bindings)) => {
                        Ok(bindings.return_type(db, &env))
                    }

                    Err(CallError(CallErrorKind::PossiblyNotCallable, _)) => Err(MetaclassError {
                        kind: MetaclassErrorKind::PartlyNotCallable(metaclass),
                    }),
                };

                return return_ty_result
                    .map(|ty| (ClassMetaclass::Selected(ty.to_meta_type(db, &env)), None));
            };

            // Reconcile all base classes' metaclasses with the candidate metaclass.
            //
            // See:
            // - https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass
            // - https://github.com/python/cpython/blob/83ba8c2bba834c0b92de669cac16fcda17485e0e/Objects/typeobject.c#L3629-L3663
            for (base_class, metaclass) in base_metaclasses {
                let Some(metaclass) = metaclass.to_class_type(db) else {
                    continue;
                };
                if candidate.metaclass.is_subclass_of(db, &env, metaclass) {
                    continue;
                }
                if metaclass.is_subclass_of(db, &env, candidate.metaclass) {
                    candidate = MetaclassCandidate {
                        metaclass,
                        base: Some(base_class),
                    };
                    continue;
                }
                return Err(MetaclassError {
                    kind: MetaclassErrorKind::Conflict {
                        candidate,
                        base_metaclass: metaclass,
                        base: base_class,
                    },
                });
            }

            let transform_info = candidate
                .metaclass
                .static_class_literal(db)
                .and_then(|(metaclass_literal, specialization)| {
                    metaclass_literal.inherited_dataclass_transformer_params(db, specialization)
                })
                .map(|params| MetaclassTransformInfo {
                    params,
                    from_explicit_metaclass: candidate.base.is_none(),
                });
            let use_protocol_fallback = has_protocol_fallback
                && !class
                    .known(db)
                    .is_some_and(|known| known.has_known_type_metaclass(env.python_version(db)));
            Ok((
                ClassMetaclass::with_protocol_fallback(
                    db,
                    candidate.metaclass.into(),
                    use_protocol_fallback,
                ),
                transform_info,
            ))
        }

        if !self.has_explicit_bases(db) && !self.has_explicit_metaclass(db) {
            let env = ProgramEnvironment::from_scope(self.body_scope(db));
            return Ok((
                ClassMetaclass::Selected(KnownClass::Type.to_class_literal(db, &env)),
                None,
            ));
        }
        try_metaclass_inner(db, self)
    }

    /// Returns the class member of this class named `name`.
    ///
    /// The member resolves to a member on the class itself or any of its proper superclasses.
    ///
    /// TODO: Should this be made private...?
    pub(super) fn class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        self.class_member_inner(db, env, None, name, policy)
    }

    pub(super) fn class_member_inner(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        // An unspecialized MRO retains mappings such as `Parent[T@Child]`, so ordinary members
        // accessed through `Child` must use its default arguments. Constructor methods are different:
        // we add their class's type variables to the callable's generic context, so those variables
        // are genuinely inferable and must remain generic instead of using the default arguments.
        if specialization.is_none()
            && let Some(generic_context) = self.generic_context(db)
        {
            match name {
                "__new__" | "__init__" => {
                    // Specifically apply the identity specialization; otherwise `iter_mro` will
                    // apply the default specialization for us.
                    let specialization = generic_context.identity_specialization(db);
                    self.class_member_from_mro(
                        db,
                        env,
                        name,
                        policy,
                        self.iter_mro(db, Some(specialization)),
                    )
                }
                _ => {
                    let member =
                        self.class_member_from_mro(db, env, name, policy, self.iter_mro(db, None));
                    let specialization = generic_context.default_specialization(db, self.known(db));
                    // An inherited method's `Self` bound can still contain this class's type
                    // variables, so the default arguments must also specialize that bound.
                    member.map_type(|ty| {
                        ty.apply_optional_owner_specialization_to_member(db, Some(specialization))
                    })
                }
            }
        } else {
            self.class_member_from_mro(db, env, name, policy, self.iter_mro(db, specialization))
        }
    }

    pub(crate) fn class_member_from_mro(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        mro_iter: impl Iterator<Item = ClassBase<'db>>,
    ) -> PlaceAndQualifiers<'db> {
        fn into_function_like_callable<'d>(
            db: &'d dyn Db,
            env: &ProgramEnvironment<'d>,
            ty: Type<'d>,
        ) -> Type<'d> {
            match ty {
                Type::Callable(callable_ty)
                    if callable_ty.is_regular(db)
                        && callable_ty.signatures(db).has_parameters() =>
                {
                    Type::Callable(callable_ty.into_function_like(db))
                }
                Type::Union(union) => union.map(db, env, |element| {
                    into_function_like_callable(db, env, *element)
                }),
                Type::Intersection(intersection) => intersection.map_positive(db, env, |element| {
                    into_function_like_callable(db, env, *element)
                }),
                _ => ty,
            }
        }

        let result = MroLookup::new(db, env, mro_iter).class_member(
            name,
            policy,
            self.inherited_generic_context(db),
            self.is_known(db, KnownClass::Object),
        );

        let mut member = match result {
            ClassMemberResult::Done(result) => result.finalize(db, env),
            ClassMemberResult::TypedDict(module) => typed_dict_class_member(
                db,
                env,
                self.identity_specialization(db),
                module,
                policy,
                name,
            ),
        };

        // We generally treat dunder attributes with `Callable` types as function-like callables.
        // See `callables_as_descriptors.md` for more details.
        if name.starts_with("__") && name.ends_with("__") {
            member = member.map_type(|ty| into_function_like_callable(db, env, ty));
        }

        member
    }

    /// Returns the inferred type of the class member named `name`. Only bound members
    /// or those marked as `ClassVars` are considered.
    ///
    /// Returns [`Place::Undefined`] if `name` cannot be found in this class's scope
    /// directly. Use [`StaticClassLiteral::class_member`] if you require a method that will
    /// traverse through the MRO until it finds the member.
    pub(super) fn own_class_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        inherited_generic_context: Option<GenericContext<'db>>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Member<'db> {
        fn into_dunder_paramspec_callable<'d>(
            db: &'d dyn Db,
            env: &ProgramEnvironment<'d>,
            ty: Type<'d>,
        ) -> Type<'d> {
            match ty {
                Type::Callable(callable_ty)
                    if callable_ty.is_regular(db)
                        && callable_ty.signatures(db).is_single_paramspec().is_some() =>
                {
                    Type::Callable(callable_ty.into_dunder_paramspec(db))
                }
                Type::Union(union) => union.map(db, env, |element| {
                    into_dunder_paramspec_callable(db, env, *element)
                }),
                Type::Intersection(intersection) => intersection.map_positive(db, env, |element| {
                    into_dunder_paramspec_callable(db, env, *element)
                }),
                _ => ty,
            }
        }

        // Check if this class is dataclass-like (either via @dataclass or via dataclass_transform)
        if CodeGeneratorKind::from_class(db, self.into())
            .is_some_and(CodeGeneratorKind::is_dataclass_like)
        {
            if name == "__dataclass_fields__" {
                // Make this class look like a subclass of the `DataClassInstance` protocol
                return Member {
                    inner: Place::declared(KnownClass::Dict.to_specialized_instance(
                        db,
                        env,
                        &[
                            KnownClass::Str.to_instance(db, env),
                            KnownClass::Field.to_specialized_instance(db, env, &[Type::any()]),
                        ],
                    ))
                    .with_qualifiers(TypeQualifiers::CLASS_VAR),
                };
            } else if name == "__dataclass_params__" {
                // There is no typeshed class for this. For now, we model it as `Any`.
                return Member {
                    inner: Place::declared(Type::any()).with_qualifiers(TypeQualifiers::CLASS_VAR),
                };
            }
        }

        if CodeGeneratorKind::NamedTuple.matches(db, self.into()) {
            if let Some(field) = self
                .own_fields(db, specialization, CodeGeneratorKind::NamedTuple)
                .get(name)
            {
                let property_getter_signature = Signature::new(
                    Parameters::standard([Parameter::positional_only(Some(Name::new_static(
                        "self",
                    )))]),
                    field.declared_ty,
                );
                let property_getter = Type::single_callable(db, property_getter_signature);
                let property = PropertyInstanceType::new(db, Some(property_getter), None, None);
                return Member::definitely_declared(Type::PropertyInstance(property));
            }
        }

        let body_scope = self.body_scope(db);
        let member = class_member(db, body_scope, name).map_type(|ty| {
            let ty = if name.starts_with("__") && name.ends_with("__") {
                into_dunder_paramspec_callable(db, env, ty)
            } else {
                ty
            };

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
            match (inherited_generic_context, ty, specialization, name) {
                (
                    Some(generic_context),
                    Type::FunctionLiteral(function),
                    Some(_),
                    "__new__" | "__init__",
                ) => Type::FunctionLiteral(
                    function.with_inherited_generic_context(db, generic_context),
                ),
                _ => ty,
            }
        });

        // The inherited `object.__dict__` annotation already describes dictionary access. A
        // synthesized slot descriptor would incorrectly replace the class's own namespace.
        if name != "__dict__"
            && self
                .slot_names(db)
                .is_some_and(|slots| slots.iter().any(|slot| slot == name))
            && (self.has_generated_slots(db)
                || !self.has_own_class_binding(db, name)
                || self.file(db).is_stub(db) && self.has_instance_slot(db, name))
        {
            return Member::definitely_declared(self.own_slot_descriptor(
                db,
                env,
                specialization,
                name,
            ));
        }

        if member.is_undefined()
            || name == "__slots__" && self.has_generated_slots(db) && !self.has_explicit_slots(db)
        {
            if let Some(synthesized_member) = self.own_synthesized_member(
                db,
                env,
                specialization,
                inherited_generic_context,
                name,
            ) {
                return Member::definitely_declared(synthesized_member);
            }
            // The symbol was not found in the class scope. It might still be implicitly defined in `@classmethod`s.
            return self.implicit_attribute(db, name, MethodDecorator::ClassMethod);
        }

        // For dataclass-like classes, `KW_ONLY` sentinel fields are not real
        // class attributes; they are markers used by the dataclass decorator to
        // indicate that subsequent fields are keyword-only. Treat them as
        // undefined so the MRO falls through to parent classes.
        if member
            .inner
            .place
            .raw_type()
            .is_some_and(|ty| ty.is_instance_of(db, KnownClass::KwOnly))
            && CodeGeneratorKind::from_static_class(db, self)
                .is_some_and(CodeGeneratorKind::is_dataclass_like)
        {
            return Member::unbound();
        }

        // For enum classes, `nonmember(value)` creates a non-member attribute.
        // At runtime, the enum metaclass unwraps the value, so accessing the attribute
        // returns the inner value, not the `nonmember` wrapper.
        if let Some(ty) = member.inner.place.raw_type()
            && let Some(value_ty) = try_unwrap_nonmember_value(db, env, ty)
            && is_enum_class_by_inheritance(db, env, self)
        {
            return Member::definitely_declared(value_ty);
        }

        member
    }

    /// Returns the type of a synthesized dataclass member like `__init__` or `__lt__`, or
    /// a synthesized `__new__` method for a `NamedTuple`.
    pub(crate) fn own_synthesized_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        inherited_generic_context: Option<GenericContext<'db>>,
        name: &str,
    ) -> Option<Type<'db>> {
        // Handle `@functools.total_ordering`: synthesize comparison methods
        // for classes that have `@total_ordering` and define at least one
        // ordering method. The decorator requires at least one of __lt__,
        // __le__, __gt__, or __ge__ to be defined (either in this class or
        // inherited from a superclass, excluding `object`).
        //
        // Only synthesize methods that are not already defined in the MRO.
        // Note: We use direct scope lookups here to avoid infinite recursion
        // through `own_class_member` -> `own_synthesized_member`.
        if self.total_ordering(db)
            && matches!(name, "__lt__" | "__le__" | "__gt__" | "__ge__")
            && !self
                .iter_mro(db, specialization)
                .filter_map(ClassBase::into_class)
                .filter_map(|class| class.static_class_literal(db))
                .filter(|(class, _)| !class.is_known(db, KnownClass::Object))
                .any(|(class, _)| {
                    class_member(db, class.body_scope(db), name)
                        .ignore_possibly_undefined()
                        .is_some()
                })
            && self.has_ordering_method_in_mro(db, specialization)
            && let Some(root_method_ty) = self.total_ordering_root_method(db, specialization)
            && let Some(callables) = root_method_ty.try_upcast_to_callable(db, env)
        {
            let bool_ty = KnownClass::Bool.to_instance(db, env);
            let synthesized_callables = callables.map(|callable| {
                let signatures = CallableSignature::from_overloads(
                    callable.signatures(db).iter().map(|signature| {
                        // The generated methods return a union of the root method's return type
                        // and `bool`. This is because `@total_ordering` synthesizes methods like:
                        //     def __gt__(self, other): return not (self == other or self < other)
                        // If `__lt__` returns `int`, then `__gt__` could return `int | bool`.
                        let return_ty =
                            UnionType::from_two_elements(db, env, signature.return_ty, bool_ty);
                        Signature::new_generic(
                            signature.generic_context,
                            signature.parameters().clone(),
                            return_ty,
                        )
                    }),
                );
                CallableType::new(
                    db,
                    signatures,
                    CallableTypeKind::FunctionLike,
                    CallableFunctionProvenance::None,
                )
            });

            return Some(synthesized_callables.into_type(db, env));
        }

        // An ordinary subclass of a frozen dataclass is not itself dataclass-like, so the
        // `CodeGeneratorKind::from_class` check below would return `None` before dataclass-like
        // synthesis runs. Still, an instance of such a subclass inherits the frozen dataclass's
        // generated `__setattr__` and `__delattr__`, which reject assignments and deletions of
        // frozen base fields.
        if let Some(method) = FrozenDataclassMethod::from_name(name)
            && let Some(synthesized_method) =
                self.own_frozen_dataclass_subclass_method(db, env, specialization, method)
        {
            return Some(synthesized_method);
        }

        let field_policy = CodeGeneratorKind::from_class(db, self.into())?;
        let pydantic_constructor_fields_are_keyword_only =
            field_policy.is_pydantic() && pydantic::constructor_fields_are_keyword_only(db, self);
        let pydantic_constructor_fields_are_optional = name == "__init__"
            && field_policy.is_pydantic()
            && pydantic::constructor_fields_are_optional(db, self);

        let instance_ty = Type::instance(
            db,
            env,
            self.apply_optional_specialization(db, specialization),
        );

        let signature_from_fields = |mut parameters: Vec<_>, return_ty: Type<'db>| {
            if name == "__init__" && field_policy.is_pydantic() {
                pydantic::extend_settings_constructor_parameters(db, self, &mut parameters);
            }

            for (field_name, field) in self.fields(db, specialization, field_policy) {
                let (init, mut default_ty, kw_only, alias, converter, strict) = match &field.kind {
                    FieldKind::NamedTuple { default_ty } => (
                        true,
                        *default_ty,
                        None,
                        None,
                        None,
                        pydantic::ConfigBoolean::Unspecified,
                    ),
                    FieldKind::Dataclass {
                        init,
                        default_ty,
                        kw_only,
                        alias,
                        converter,
                        ..
                    } => (
                        *init,
                        *default_ty,
                        *kw_only,
                        alias.as_ref(),
                        *converter,
                        pydantic::ConfigBoolean::Unspecified,
                    ),
                    FieldKind::Pydantic {
                        init,
                        default_ty,
                        alias,
                        strict,
                    } => (*init, *default_ty, None, alias.as_ref(), None, *strict),
                    FieldKind::TypedDict { .. } => continue,
                };
                let mut field_ty = field.declared_ty;

                if !init && (name == "__init__" || field_policy.is_pydantic()) {
                    // Fields with `init=False` are excluded from constructors. Pydantic's private
                    // and internal fields are also excluded from replacement.
                    continue;
                }

                if field.is_kw_only_sentinel(db) {
                    // Attributes annotated with `dataclass.KW_ONLY` are not present in the synthesized
                    // `__init__` method; they are used to indicate that the following parameters are
                    // keyword-only.
                    continue;
                }

                let dunder_set = field_ty.class_member(db, env, "__set__");
                if let Place::Defined(DefinedPlace {
                    ty: dunder_set,
                    definedness: Definedness::AlwaysDefined,
                    ..
                }) = dunder_set.place
                {
                    // The descriptor handling below is guarded by this not-dynamic check, because
                    // dynamic types like `Any` are valid (data) descriptors: since they have all
                    // possible attributes, they also have a (callable) `__set__` method. The
                    // problem is that we can't determine the type of the value parameter this way.
                    // Instead, we want to use the dynamic type itself in this case, so we skip the
                    // special descriptor handling.
                    if !dunder_set.is_dynamic() {
                        // This type of this attribute is a data descriptor. Instead of overwriting the
                        // descriptor attribute, data-classes will (implicitly) call the `__set__` method
                        // of the descriptor. This means that the synthesized `__init__` parameter for
                        // this attribute is determined by possible `value` parameter types with which
                        // the `__set__` method can be called.
                        //
                        // We union parameter types across overloads of a single callable, intersect
                        // callable bindings inside an intersection element, and union outer elements.
                        field_ty = dunder_set.bindings(db, env).map_types(db, env, |binding| {
                            let mut value_types = UnionBuilder::new(db, env);
                            let mut has_value_type = false;
                            for overload in binding {
                                if let Some(value_param) =
                                    overload.signature.parameters().get_positional(2)
                                {
                                    value_types = value_types.add(value_param.annotated_type());
                                    has_value_type = true;
                                } else if overload.signature.parameters().is_gradual() {
                                    value_types = value_types.add(Type::unknown());
                                    has_value_type = true;
                                }
                            }
                            has_value_type.then(|| value_types.build())
                        });

                        // The default value of the attribute is *not* determined by the right hand side
                        // of the class-body assignment. Instead, the runtime invokes `__get__` on the
                        // descriptor, as if it had been called on the class itself, i.e. it passes `None`
                        // for the `instance` argument.

                        if let Some(ref mut default_ty) = default_ty {
                            *default_ty = default_ty
                                .try_call_dunder_get(db, env, None, Type::from(self))
                                .unwrap_or_else(|error| Some(error.fallback()))
                                .map(|result| result.return_type)
                                .unwrap_or_else(Type::unknown);
                        }
                    }
                }

                if let Some((converter_input_ty, _)) = converter {
                    field_ty = converter_input_ty;
                }

                if name == "__init__"
                    && let Some(metadata) = field_policy.pydantic_metadata()
                {
                    field_ty = pydantic::constructor_parameter_type(
                        db, self, field_name, field_ty, strict, metadata,
                    );
                }

                if pydantic_constructor_fields_are_optional && default_ty.is_none() {
                    default_ty = Some(Type::unknown());
                }

                let is_kw_only = matches!(name, "__replace__" | "_replace")
                    || pydantic_constructor_fields_are_keyword_only
                    || kw_only.unwrap_or(false);

                let mut add_parameter_with_name = |parameter_name, default_ty| {
                    let mut parameter = if is_kw_only {
                        Parameter::keyword_only(parameter_name)
                    } else {
                        Parameter::positional_or_keyword(parameter_name)
                    }
                    .with_annotated_type(field_ty)
                    .with_definition(field.first_declaration);

                    parameter = if matches!(name, "__replace__" | "_replace") {
                        // When replacing, we know there is a default value for the field
                        // (the value that is currently assigned to the field)
                        // assume this to be the declared type of the field
                        parameter.with_default_type(field_ty)
                    } else {
                        parameter.with_optional_default_type(default_ty)
                    };

                    parameters.push(parameter);
                };

                if name == "__init__"
                    && let Some(metadata) = field_policy.pydantic_metadata()
                    && let Some(alias) = alias
                {
                    match (
                        metadata.validates_by_alias(db),
                        metadata.validates_by_name(db),
                    ) {
                        (true, true) => {
                            let alias = Name::new(&**alias);
                            if alias == *field_name {
                                add_parameter_with_name(field_name.clone(), default_ty);
                            } else {
                                // A normal signature cannot express that at least one of two
                                // differently named parameters is required. We could solve
                                // this with overloads, but the number of overloads would grow
                                // exponentially in the number of parameters. So for now, we
                                // treat both the alias and the field name as optional
                                // parameters, which leads to false negatives if none of them
                                // is provided.
                                let default_ty = Some(default_ty.unwrap_or_else(Type::unknown));
                                add_parameter_with_name(alias, default_ty);
                                add_parameter_with_name(field_name.clone(), default_ty);
                            }
                        }
                        (true, false) => {
                            add_parameter_with_name(Name::new(&**alias), default_ty);
                        }
                        (false, true) => {
                            add_parameter_with_name(field_name.clone(), default_ty);
                        }
                        (false, false) => {}
                    }
                } else if name == "__replace__" && field_policy.is_pydantic() {
                    // Pydantic updates model fields by name rather than by initialization alias.
                    add_parameter_with_name(field_name.clone(), default_ty);
                } else {
                    // Use the alias name if provided, otherwise use the field name.
                    let parameter_name =
                        Name::new(alias.map(|alias| &**alias).unwrap_or(&**field_name));
                    add_parameter_with_name(parameter_name, default_ty);
                }
            }

            // In the event that we have a mix of keyword-only and positional parameters, we need to sort them
            // so that the keyword-only parameters appear after positional parameters.
            parameters.sort_by_key(Parameter::is_keyword_only);

            if name == "__init__"
                && field_policy
                    .pydantic_metadata()
                    .is_some_and(|metadata| pydantic::model_init_accepts_extra(db, self, metadata))
            {
                let extra = pydantic::extra_parameter(&parameters);
                parameters.push(extra);
            }

            let signature = match name {
                "__new__" | "__init__" => Signature::new_generic(
                    inherited_generic_context.or_else(|| self.inherited_generic_context(db)),
                    Parameters::standard(parameters),
                    return_ty,
                ),
                _ => Signature::new(Parameters::standard(parameters), return_ty),
            };
            Some(Type::function_like_callable(db, signature))
        };

        match (field_policy, name) {
            (field_policy, "__init__")
                if field_policy.synthesizes_constructor_signature_from_fields(db, self) =>
            {
                if field_policy.is_dataclass_like()
                    && !self.has_dataclass_param(db, field_policy, DataclassFlags::INIT)
                {
                    return None;
                }

                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    // TODO: could be `Self`.
                    .with_annotated_type(instance_ty);
                signature_from_fields(vec![self_parameter], Type::none(db, env))
            }
            (
                CodeGeneratorKind::NamedTuple,
                "__new__" | "__init__" | "__match_args__" | "_replace" | "__replace__" | "_fields",
            ) if self.namedtuple_base_has_unknown_fields(db) => {
                // When the namedtuple base has unknown fields, fall back to NamedTupleFallback
                // which has generic signatures that accept any arguments.
                KnownClass::NamedTupleFallback
                    .to_class_literal(db, env)
                    .as_class_literal()?
                    .as_static()?
                    .own_class_member(db, env, inherited_generic_context, None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            env,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: instance_ty,
                            },
                            TypeContext::default(),
                        )
                    })
            }
            (
                CodeGeneratorKind::NamedTuple,
                "__match_args__" | "__new__" | "_replace" | "__replace__" | "_fields" | "__slots__",
            ) => {
                let fields = self.fields(db, specialization, field_policy);
                let fields_iter = fields.iter().map(|(name, field)| {
                    let default_ty = match &field.kind {
                        FieldKind::NamedTuple { default_ty } => *default_ty,
                        _ => None,
                    };
                    NamedTupleField {
                        name: name.clone(),
                        ty: field.declared_ty,
                        default: default_ty,
                        definition: field.first_declaration,
                    }
                });
                synthesize_namedtuple_class_member(
                    db,
                    env,
                    name,
                    instance_ty,
                    fields_iter,
                    specialization.map(|s| s.generic_context(db)),
                )
            }
            (
                field_policy @ CodeGeneratorKind::DataclassLike(_),
                "__lt__" | "__le__" | "__gt__" | "__ge__",
            ) => {
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::ORDER) {
                    return None;
                }

                let signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            // TODO: could be `Self`.
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("other"))
                            // TODO: could be `Self`.
                            .with_annotated_type(instance_ty),
                    ]),
                    KnownClass::Bool.to_instance(db, env),
                );

                Some(Type::function_like_callable(db, signature))
            }
            (field_policy @ CodeGeneratorKind::DataclassLike(_), "__hash__") => {
                let unsafe_hash =
                    self.has_dataclass_param(db, field_policy, DataclassFlags::UNSAFE_HASH);
                let frozen = self.has_dataclass_param(db, field_policy, DataclassFlags::FROZEN);
                let eq = self.has_dataclass_param(db, field_policy, DataclassFlags::EQ);

                if unsafe_hash || (frozen && eq) {
                    let signature = Signature::new(
                        Parameters::standard([Parameter::positional_or_keyword(Name::new_static(
                            "self",
                        ))
                        .with_annotated_type(instance_ty)]),
                        KnownClass::Int.to_instance(db, env),
                    );

                    Some(Type::function_like_callable(db, signature))
                } else if eq && !frozen {
                    Some(Type::none(db, env))
                } else {
                    // No `__hash__` is generated, fall back to `object.__hash__`
                    None
                }
            }
            (field_policy @ CodeGeneratorKind::DataclassLike(_), "__match_args__")
                if env.python_version(db) >= PythonVersion::PY310 =>
            {
                if !self.has_dataclass_param(db, field_policy, DataclassFlags::MATCH_ARGS) {
                    return None;
                }

                let kw_only_default =
                    self.has_dataclass_param(db, field_policy, DataclassFlags::KW_ONLY);

                let fields = self.fields(db, specialization, field_policy);
                let match_args = fields
                    .iter()
                    .filter(|(_, field)| {
                        if let FieldKind::Dataclass { init, kw_only, .. } = &field.kind {
                            *init && !kw_only.unwrap_or(kw_only_default)
                        } else {
                            false
                        }
                    })
                    .map(|(name, _)| Type::string_literal(db, name));
                Some(Type::heterogeneous_tuple(db, env, match_args))
            }
            (CodeGeneratorKind::NamedTuple, name) if name != "__init__" => {
                KnownClass::NamedTupleFallback
                    .to_class_literal(db, env)
                    .as_class_literal()?
                    .as_static()?
                    .own_class_member(db, env, self.inherited_generic_context(db), None, name)
                    .ignore_possibly_undefined()
                    .map(|ty| {
                        ty.apply_type_mapping(
                            db,
                            env,
                            &TypeMapping::ReplaceSelf {
                                new_upper_bound: determine_upper_bound(
                                    db,
                                    env,
                                    ClassLiteral::Static(self),
                                    |base| {
                                        base.into_class()
                                            .is_some_and(|c| c.is_known(db, KnownClass::Tuple))
                                    },
                                ),
                            },
                            TypeContext::default(),
                        )
                    })
            }
            (
                CodeGeneratorKind::DataclassLike(_) | CodeGeneratorKind::Pydantic(_),
                "__replace__",
            ) if env.python_version(db) >= PythonVersion::PY313 => {
                let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                    .with_annotated_type(instance_ty);

                signature_from_fields(vec![self_parameter], instance_ty)
            }
            (
                field_policy @ (CodeGeneratorKind::DataclassLike(_)
                | CodeGeneratorKind::Pydantic(_)),
                "__setattr__",
            ) => {
                if self.is_frozen_dataclass(db) == Some(true)
                    || Self::is_frozen_pydantic_model(db, field_policy)
                {
                    let signature = Signature::new(
                        Parameters::standard([
                            Parameter::positional_or_keyword(Name::new_static("self"))
                                .with_annotated_type(instance_ty),
                            Parameter::positional_or_keyword(Name::new_static("name")),
                            Parameter::positional_or_keyword(Name::new_static("value")),
                        ]),
                        Type::Never,
                    );

                    return Some(Type::function_like_callable(db, signature));
                }
                None
            }
            (CodeGeneratorKind::DataclassLike(_), "__delattr__")
                if self.is_frozen_dataclass(db) == Some(true) =>
            {
                let signature = Signature::new(
                    Parameters::standard([
                        Parameter::positional_or_keyword(Name::new_static("self"))
                            .with_annotated_type(instance_ty),
                        Parameter::positional_or_keyword(Name::new_static("name")),
                    ]),
                    Type::Never,
                );

                Some(Type::function_like_callable(db, signature))
            }
            (field_policy @ CodeGeneratorKind::DataclassLike(_), "__slots__")
                if env.python_version(db) >= PythonVersion::PY310 =>
            {
                self.has_dataclass_param(db, field_policy, DataclassFlags::SLOTS)
                    .then(|| {
                        if let Some(slots) = self.slot_names(db) {
                            return Type::heterogeneous_tuple(
                                db,
                                env,
                                slots.iter().map(|name| Type::string_literal(db, name)),
                            );
                        }

                        let fields = self.fields(db, specialization, field_policy);
                        let slots = fields.keys().map(|name| Type::string_literal(db, name));
                        Type::heterogeneous_tuple(db, env, slots)
                    })
            }
            (CodeGeneratorKind::TypedDict, name) => synthesize_typed_dict_method(
                db,
                env,
                instance_ty
                    .as_typed_dict()
                    .expect("TypedDict code generation should use a TypedDict instance"),
                name,
                || TypedDictFields::Static(self.fields(db, specialization, field_policy)),
            ),
            _ => None,
        }
    }

    /// Synthesize a `__setattr__` or `__delattr__` view for an ordinary subclass of a frozen
    /// dataclass.
    ///
    /// CPython's generated frozen-dataclass `__setattr__` and `__delattr__` reject all assignments
    /// and deletions on exact instances of the frozen dataclass, but on subclass instances they
    /// only reject assignments and deletions of that dataclass's fields before delegating to the
    /// next method in the MRO.
    fn own_frozen_dataclass_subclass_method(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        method: FrozenDataclassMethod,
    ) -> Option<Type<'db>> {
        if CodeGeneratorKind::from_static_class(db, self).is_some() {
            return None;
        }

        let frozen_base_fields =
            self.inherited_non_slotted_frozen_dataclass_fields(db, specialization, method.name())?;

        let instance_ty = Type::instance(
            db,
            env,
            self.apply_optional_specialization(db, specialization),
        );
        let method_signature = |name_ty, return_ty| {
            let self_parameter = Parameter::positional_or_keyword(Name::new_static("self"))
                .with_annotated_type(instance_ty);
            let name_parameter = Parameter::positional_or_keyword(Name::new_static("name"))
                .with_annotated_type(name_ty);
            let parameters = match method {
                FrozenDataclassMethod::SetAttr => Parameters::standard([
                    self_parameter,
                    name_parameter,
                    Parameter::positional_or_keyword(Name::new_static("value")),
                ]),
                FrozenDataclassMethod::DelAttr => {
                    Parameters::standard([self_parameter, name_parameter])
                }
            };
            Signature::new(parameters, return_ty)
        };

        let overloads = frozen_base_fields
            .names
            .iter()
            .map(|field| method_signature(Type::string_literal(db, field), Type::Never))
            .chain([method_signature(
                KnownClass::Str.to_instance(db, env),
                Type::none(db, env),
            )]);

        Some(Type::Callable(CallableType::new(
            db,
            CallableSignature::from_overloads(overloads),
            CallableTypeKind::FunctionLike,
            CallableFunctionProvenance::None,
        )))
    }

    /// Determines how an inherited generated frozen-dataclass `method` handles `name`.
    ///
    /// CPython's generated `__setattr__` and `__delattr__` reject every mutation when called on an
    /// instance of the exact frozen class. On an ordinary subclass instance, they reject only
    /// dataclass fields and delegate other names with `super(frozen_class, instance)`.
    ///
    /// If multiple frozen dataclasses are reachable before an explicit implementation of
    /// `method`, a non-field delegates past each generated method.
    /// [`FrozenDataclassDispatch::Delegate`] stores the last frozen base so the caller can perform
    /// the equivalent lookup once, after all of them.
    pub(crate) fn inherited_frozen_dataclass_dispatch(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        method: &str,
        name: &str,
    ) -> Option<FrozenDataclassDispatch<'db>> {
        if CodeGeneratorKind::from_static_class(db, self).is_some()
            || class_member(db, self.body_scope(db), method)
                .ignore_possibly_undefined()
                .is_some()
        {
            return None;
        }

        let frozen_base_fields =
            self.inherited_non_slotted_frozen_dataclass_fields(db, specialization, method)?;

        if frozen_base_fields
            .names
            .iter()
            .any(|field| field.as_str() == name)
        {
            Some(FrozenDataclassDispatch::FrozenField)
        } else {
            Some(FrozenDataclassDispatch::Delegate(
                frozen_base_fields.last_frozen_base,
            ))
        }
    }

    /// Returns the inherited fields whose generated `__setattr__` or `__delattr__` still applies.
    fn inherited_non_slotted_frozen_dataclass_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        method: &str,
    ) -> Option<InheritedFrozenDataclassFields<'db>> {
        let mut names = FxIndexSet::default();
        let mut last_frozen_base = None;

        for base in self.iter_mro(db, specialization).skip(1) {
            let Some(base_class_type) = base.into_class() else {
                break;
            };
            let Some((base_class, base_specialization)) = base_class_type.static_class_literal(db)
            else {
                break;
            };

            // Stop if another class in the MRO replaces the relevant generated frozen method:
            //
            //   @dataclass(frozen=True)
            //   class Frozen: x: int
            //
            //   class Mutable(Frozen):
            //       def __setattr__(self, name: str, value: object) -> None: ...
            //       def __delattr__(self, name: str) -> None: ...
            //
            //   class Child(Mutable): ...
            //
            // Writes and deletions of `Child().x` dispatch to the corresponding `Mutable` method,
            // not to the synthesized `Frozen` method.
            if class_member(db, base_class.body_scope(db), method)
                .ignore_possibly_undefined()
                .is_some()
            {
                break;
            }

            if base_class.is_frozen_dataclass(db) == Some(true) {
                let field_policy @ CodeGeneratorKind::DataclassLike(_) =
                    CodeGeneratorKind::from_static_class(db, base_class)?
                else {
                    break;
                };

                if base_class.has_dataclass_param(db, field_policy, DataclassFlags::SLOTS) {
                    break;
                }

                names.extend(
                    base_class
                        .fields(db, base_specialization, field_policy)
                        .iter()
                        .filter(|(_, field)| {
                            !matches!(
                                field.kind,
                                FieldKind::Dataclass {
                                    init_only: true,
                                    ..
                                }
                            )
                        })
                        .map(|(name, _)| name.clone()),
                );
                last_frozen_base = Some(base_class);
            }
        }

        Some(InheritedFrozenDataclassFields {
            names: names.into_iter().collect(),
            last_frozen_base: last_frozen_base?,
        })
    }

    /// Member lookup for classes that inherit from `typing.TypedDict`.
    ///
    /// This is implemented as a separate method because the item definitions on a `TypedDict`-based
    /// class are *not* accessible as class members. Instead, this mostly defers to `TypedDictFallback`,
    /// unless `name` corresponds to one of the specialized synthetic members like `__getitem__`.
    pub(crate) fn typed_dict_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        if let Some(member) = self.own_synthesized_member(db, env, specialization, None, name) {
            Place::bound(member).into()
        } else {
            let class = match specialization {
                Some(specialization) => {
                    ClassType::Generic(GenericAlias::new(db, self, specialization))
                }
                None => self.identity_specialization(db),
            };
            let Some(module) = self.typed_dict_module(db) else {
                return Place::Undefined.into();
            };
            typed_dict_class_member(db, env, class, module, policy, name)
        }
    }

    /// Returns a list of all annotated attributes defined in this class, or any of its superclasses.
    ///
    /// See [`StaticClassLiteral::own_fields`] for more details.
    pub(crate) fn fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> &'db FxIndexMap<Name, Field<'db>> {
        if field_policy == CodeGeneratorKind::NamedTuple {
            // NamedTuples do not allow multiple inheritance, so it is sufficient to enumerate the
            // fields of this class only.
            return self.own_fields(db, specialization, field_policy);
        }

        self.fields_inner(db, specialization, field_policy)
    }

    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _, _, _| FxIndexMap::default(),
        heap_size=get_size2::GetSize::get_heap_size
    )]
    fn fields_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> FxIndexMap<Name, Field<'db>> {
        enum FieldSource<'db> {
            Static(StaticClassLiteral<'db>, Option<Specialization<'db>>),
            DynamicTypedDict(DynamicTypedDictLiteral<'db>),
        }

        debug_assert_ne!(
            field_policy,
            CodeGeneratorKind::NamedTuple,
            "Collecting `fields` for NamedTuples should short-circuit in `fields()`"
        );

        let mut class_variables = FxIndexSet::default();
        let mut map: FxIndexMap<_, _> = self
            .iter_mro(db, specialization)
            .rev()
            .filter_map(|superclass| {
                let class = superclass.into_class()?;

                if let Some((class_literal, specialization)) = class.static_class_literal(db) {
                    // Pydantic collects annotated attributes from every class in the model's MRO,
                    // including ordinary classes that are not themselves Pydantic models.
                    if field_policy.is_pydantic() || field_policy.matches(db, class_literal.into())
                    {
                        return Some(FieldSource::Static(class_literal, specialization));
                    }
                }

                if field_policy == CodeGeneratorKind::TypedDict
                    && let ClassLiteral::DynamicTypedDict(typeddict) = class.class_literal(db)
                {
                    return Some(FieldSource::DynamicTypedDict(typeddict));
                }

                None
            })
            .flat_map(|source| match source {
                FieldSource::Static(class, specialization) => {
                    let own_fields =
                        class.own_fields_with_class_variables(db, specialization, field_policy);

                    if field_policy.is_dataclass_like() {
                        class_variables.extend(own_fields.class_variables.iter().cloned());
                        for name in own_fields.fields.keys() {
                            class_variables.swap_remove(name);
                        }
                    }

                    Either::Left(
                        own_fields
                            .fields
                            .iter()
                            .map(|(name, field)| (name.clone(), field.clone())),
                    )
                }
                FieldSource::DynamicTypedDict(typeddict) => {
                    Either::Right(typeddict.items(db).iter().map(|(name, td_field)| {
                        (
                            name.clone(),
                            Field {
                                declared_ty: td_field.declared_ty,
                                kind: FieldKind::TypedDict {
                                    is_required: td_field.is_required(),
                                    is_read_only: td_field.is_read_only(),
                                },
                                first_declaration: td_field.first_declaration(),
                            },
                        )
                    }))
                }
            })
            // KW_ONLY sentinels are markers, not real fields. Exclude them so
            // they cannot shadow an inherited field with the same name.
            .filter(|(_, field)| !field.is_kw_only_sentinel(db))
            // We collect into a FxOrderMap here to deduplicate attributes
            .collect();

        if field_policy.is_dataclass_like() {
            // `own_fields` excludes class variables, but their declarations can still mask
            // inherited fields. Delay removal so restoring a field preserves its original slot.
            map.retain(|name, _| !class_variables.contains(name));
        }

        map.shrink_to_fit();
        map
    }

    pub(crate) fn validate_members(self, context: &InferContext<'db, '_>) {
        let db = context.db();
        let env = context.program_environment();
        let Some(field_policy) = CodeGeneratorKind::from_static_class(db, self) else {
            return;
        };
        let class_body_scope = self.body_scope(db);
        let table = place_table(db, class_body_scope);
        let use_def = use_def_map(db, class_body_scope);
        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            let result = place_from_declarations(db, env, declarations.clone());
            let attr = result.ignore_conflicting_declarations();
            let symbol = table.symbol(symbol_id);
            let name = symbol.name();

            let Some(Type::FunctionLiteral(literal)) = attr.place.ignore_possibly_undefined()
            else {
                continue;
            };

            match name.as_str() {
                "__setattr__" | "__delattr__" => {
                    if field_policy.is_dataclass_like()
                        && self.is_frozen_dataclass(db) == Some(true)
                    {
                        if let Some(builder) = context.report_lint(
                            &INVALID_DATACLASS_OVERRIDE,
                            literal.node(db, context.file(), context.module()),
                        ) {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot overwrite attribute `{}` in frozen dataclass `{}`",
                                name,
                                self.name(db)
                            ));
                            diagnostic.info(name);
                        }
                    }
                }
                "__lt__" | "__le__" | "__gt__" | "__ge__" => {
                    if field_policy.is_dataclass_like()
                        && self.has_dataclass_param(db, field_policy, DataclassFlags::ORDER)
                    {
                        if let Some(builder) = context.report_lint(
                            &INVALID_DATACLASS_OVERRIDE,
                            literal.node(db, context.file(), context.module()),
                        ) {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Cannot overwrite attribute `{}` in dataclass `{}` with `order=True`",
                                name,
                                self.name(db)
                            ));
                            diagnostic.info(name);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Returns a map of all annotated attributes defined in the body of this class.
    /// This extends the `__annotations__` attribute at runtime by also including default values
    /// and computed field properties.
    ///
    /// For a class body like
    /// ```py
    /// @dataclass(kw_only=True)
    /// class C:
    ///     x: int
    ///     y: str = "hello"
    ///     z: float = field(kw_only=False, default=1.0)
    /// ```
    /// we return a map `{"x": Field, "y": Field, "z": Field}` in class-body declaration order,
    /// where each `Field` contains the annotated type, default value (if any), and field
    /// properties.
    ///
    /// **Important**: The returned `Field` objects represent our full understanding of the fields,
    /// including properties inherited from class-level dataclass parameters (like `kw_only=True`)
    /// and dataclass-transform parameters (like `kw_only_default=True`). They do not represent
    /// only what is explicitly specified in each field definition.
    pub(crate) fn own_fields(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> &'db FxIndexMap<Name, Field<'db>> {
        &self
            .own_fields_with_class_variables(db, specialization, field_policy)
            .fields
    }

    fn own_fields_with_class_variables(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> &'db OwnClassFields<'db> {
        self.own_fields_inner(db, specialization, field_policy)
    }

    /// Collects ordered constructor fields and `ClassVar` masks in one pass over a class body.
    ///
    /// Keeping both together avoids reinterpreting declarations while merging inherited fields.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _, _, _| OwnClassFields::default(),
        heap_size=get_size2::GetSize::get_heap_size
    )]
    fn own_fields_inner(
        self,
        db: &'db dyn Db,
        specialization: Option<Specialization<'db>>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> OwnClassFields<'db> {
        let class_body_scope = self.body_scope(db);
        let env = ProgramEnvironment::from_scope(class_body_scope);
        let table = place_table(db, class_body_scope);

        let use_def = use_def_map(db, class_body_scope);

        // `own_fields(..., NamedTuple)` is called while constructing the class's MRO because the
        // field types determine the synthesized tuple base. `typed_dict_params` also queries the
        // class's MRO, so only read the `total` default when collecting `TypedDict` fields.
        let typed_dict_fields_are_required_by_default =
            if field_policy == CodeGeneratorKind::TypedDict {
                self.typed_dict_params(db)
                    .expect("TypedDictParams should be available for CodeGeneratorKind::TypedDict")
                    .contains(TypedDictParams::TOTAL)
            } else {
                false
            };
        let dataclass_kw_only_default = field_policy.is_dataclass_like().then(|| {
            let own_field_policy =
                CodeGeneratorKind::from_class(db, self.into()).unwrap_or(field_policy);
            self.has_dataclass_param(db, own_field_policy, DataclassFlags::KW_ONLY)
        });
        let mut kw_only_sentinel_field_seen = false;
        let mut field_declarations = Vec::new();

        for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
            // Here, we exclude all declarations that are not annotated assignments. We need this because
            // things like function definitions and nested classes would otherwise be considered dataclass
            // fields. The check is too broad in the sense that it also excludes (weird) constructs where
            // a symbol would have multiple declarations, one of which is an annotated assignment. If we
            // want to improve this, we could instead pass a definition-kind filter to the use-def map
            // query, or to the `symbol_from_declarations` call below. Doing so would potentially require
            // us to generate a union of `__init__` methods.
            if declarations.clone().any_reachable(db, |declaration| {
                declaration.is_defined_and(|declaration| {
                    !matches!(
                        declaration.kind(db),
                        DefinitionKind::AnnotatedAssignment(..)
                    )
                })
            }) {
                continue;
            }

            // Field contents come from the declarations live at end of scope, but field order is
            // anchored to the first reachable annotated declaration in the class body.
            let Some(first_declaration_order) = use_def
                .reachable_symbol_declarations(symbol_id)
                .first_reachable_declaration_order(db, |declaration| {
                    declaration.is_defined_and(|declaration| {
                        matches!(
                            declaration.kind(db),
                            DefinitionKind::AnnotatedAssignment(..)
                        )
                    })
                })
            else {
                continue;
            };

            let result = place_from_declarations(db, &env, declarations.clone());
            field_declarations.push((first_declaration_order, symbol_id, result));
        }

        field_declarations
            .sort_unstable_by_key(|(first_declaration_order, _, _)| *first_declaration_order);

        let mut attributes = FxIndexMap::default();
        let mut class_variables = Vec::new();
        for (_, symbol_id, result) in field_declarations {
            let symbol = table.symbol(symbol_id);
            let first_declaration = result.first_declaration;
            let attr = result.ignore_conflicting_declarations();
            if attr.is_class_var() {
                if field_policy.is_dataclass_like() {
                    class_variables.push(symbol.name().clone());
                }
                continue;
            }

            if let Some(attr_ty) = attr.place.ignore_possibly_undefined() {
                // Annotation-only declarations in stubs also act as bindings for attribute
                // lookup, but they do not supply field defaults.
                let mut default_ty = if field_policy == CodeGeneratorKind::TypedDict
                    || (self.file(db).is_stub(db)
                        && !first_declaration.is_some_and(|definition| {
                            matches!(
                                definition.kind(db),
                                DefinitionKind::AnnotatedAssignment(annotation)
                                    if annotation.has_value()
                            )
                        })) {
                    None
                } else {
                    place_from_bindings(db, &env, use_def.end_of_scope_symbol_bindings(symbol_id))
                        .place
                        .ignore_possibly_undefined()
                };

                default_ty =
                    default_ty.map(|ty| ty.apply_optional_specialization(db, specialization));

                let mut init = true;
                let mut kw_only = None;
                let mut alias = None;
                let mut converter = None;
                let mut strict = pydantic::ConfigBoolean::Unspecified;
                if field_policy.is_pydantic() {
                    let metadata =
                        pydantic::field_metadata(db, first_declaration, default_ty, specialization);
                    default_ty = metadata.default_ty;
                    init = metadata.init;
                    alias = metadata.alias;
                    strict = metadata.strict;
                } else if let Some(Type::KnownInstance(KnownInstanceType::Field(field))) =
                    default_ty
                {
                    default_ty = field.default_type(db);
                    init = field.init(db);
                    kw_only = field.kw_only(db);
                    alias.clone_from(field.alias(db));
                    converter = field.converter(db);
                }

                let kind = match field_policy {
                    CodeGeneratorKind::NamedTuple => FieldKind::NamedTuple { default_ty },
                    CodeGeneratorKind::DataclassLike(_) => FieldKind::Dataclass {
                        default_ty,
                        init_only: attr.is_init_var(),
                        init,
                        kw_only,
                        alias,
                        converter,
                    },
                    CodeGeneratorKind::Pydantic(_) => FieldKind::Pydantic {
                        default_ty,
                        // Private attributes are instance attributes but never constructor parameters.
                        init: init && !pydantic::is_private_attribute(symbol.name()),
                        alias,
                        strict,
                    },
                    CodeGeneratorKind::TypedDict => {
                        let is_required = if attr.is_required() {
                            // Explicit Required[T] annotation - always required
                            true
                        } else if attr.is_not_required() {
                            // Explicit NotRequired[T] annotation - never required
                            false
                        } else {
                            // No explicit qualifier - use class default (`total` parameter)
                            typed_dict_fields_are_required_by_default
                        };

                        FieldKind::TypedDict {
                            is_required,
                            is_read_only: attr.is_read_only(),
                        }
                    }
                };

                let mut field = Field {
                    declared_ty: attr_ty.apply_optional_specialization(db, specialization),
                    kind,
                    first_declaration,
                };

                // Check if this is a KW_ONLY sentinel and mark subsequent fields as keyword-only
                if field_policy.is_dataclass_like() && field.is_kw_only_sentinel(db) {
                    kw_only_sentinel_field_seen = true;
                }

                // If no explicit kw_only setting and we've seen KW_ONLY sentinel, mark as keyword-only
                if kw_only_sentinel_field_seen {
                    if let FieldKind::Dataclass {
                        kw_only: ref mut kw @ None,
                        ..
                    } = field.kind
                    {
                        *kw = Some(true);
                    }
                }

                // Resolve the kw_only to the class-level default. This ensures that when fields
                // are inherited by child classes, they use their defining class's kw_only default.
                if let FieldKind::Dataclass {
                    kw_only: ref mut kw @ None,
                    ..
                } = field.kind
                {
                    *kw = dataclass_kw_only_default;
                }

                attributes.insert(symbol.name().clone(), field);
            }
        }

        attributes.shrink_to_fit();

        OwnClassFields {
            fields: attributes,
            class_variables: class_variables.into_boxed_slice(),
        }
    }

    /// Return the type qualifiers attached to each reachable annotated assignment in source order.
    ///
    /// This uses the declaration history rather than [`StaticClassLiteral::own_fields`], because a
    /// later method or nested class can replace the symbol's binding while leaving its entry in
    /// `__annotations__`:
    ///
    /// ```python
    /// class Example(NamedTuple):
    ///     value: Final[int]
    ///     def value(self) -> int: ...
    /// ```
    ///
    /// Each qualifier remains paired with its own definition so diagnostics can point to the
    /// annotation that introduced it, including when declarations occur in different branches.
    pub(crate) fn own_annotated_qualifiers(
        self,
        db: &'db dyn Db,
    ) -> Vec<(Name, TypeQualifiers, Definition<'db>)> {
        let body_scope = self.body_scope(db);
        let table = place_table(db, body_scope);
        let use_def = use_def_map(db, body_scope);
        let mut annotated_qualifiers = Vec::new();

        for (symbol_id, _) in use_def.all_end_of_scope_symbol_declarations() {
            let declarations = use_def.reachable_symbol_declarations(symbol_id);
            let predicates = declarations.predicates();
            let reachability_constraints = declarations.reachability_constraints();

            for declaration in declarations {
                if reachability_constraints
                    .evaluate(db, predicates, declaration.reachability_constraint)
                    .is_always_false()
                {
                    continue;
                }

                let DefinitionState::Defined(definition) = declaration.declaration else {
                    continue;
                };
                if !matches!(definition.kind(db), DefinitionKind::AnnotatedAssignment(..)) {
                    continue;
                }

                let Some(declared) = inferred_declaration(db, definition).declared() else {
                    continue;
                };
                annotated_qualifiers.push((
                    declaration.declaration_order,
                    table.symbol(symbol_id).name().clone(),
                    declared.qualifiers(),
                    definition,
                ));
            }
        }

        annotated_qualifiers
            .sort_unstable_by_key(|(declaration_order, _, _, _)| *declaration_order);
        annotated_qualifiers
            .into_iter()
            .map(|(_, name, qualifiers, definition)| (name, qualifiers, definition))
            .collect()
    }

    /// Look up an instance attribute (available in `__dict__`) of the given name.
    ///
    /// See [`Type::instance_member`] for more details.
    pub(super) fn instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> PlaceAndQualifiers<'db> {
        if self.is_typed_dict(db) || self.lacks_instance_storage(db, name) {
            return Place::Undefined.into();
        }

        match MroLookup::new(db, env, self.iter_mro(db, specialization)).instance_member(name) {
            InstanceMemberResult::Done(result) => result,
            InstanceMemberResult::TypedDict => KnownClass::TypedDictFallback
                .to_instance(db, env)
                .instance_member(db, env, name)
                .map_type(|ty| {
                    ty.apply_type_mapping(
                        db,
                        env,
                        &TypeMapping::ReplaceSelf {
                            new_upper_bound: Type::instance(
                                db,
                                env,
                                self.unknown_specialization(db),
                            ),
                        },
                        TypeContext::default(),
                    )
                }),
        }
    }

    /// A helper function for `instance_member` that looks up the `name` attribute only on
    /// this class, not on its superclasses.
    pub(super) fn own_instance_member(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> Member<'db> {
        // TODO: There are many things that are not yet implemented here:
        // - `typing.Final`
        // - Proper diagnostics

        // NamedTuple fields are modeled via synthesized descriptors on the class. Treating them
        // as instance attributes here causes inherited fields to leak through after a subclass
        // shadows the name with a normal class attribute.
        if CodeGeneratorKind::NamedTuple.matches(db, self.into())
            && self
                .own_fields(db, None, CodeGeneratorKind::NamedTuple)
                .contains_key(name)
        {
            return Member::unbound();
        }

        let body_scope = self.body_scope(db);
        let table = place_table(db, body_scope);

        if let Some(symbol_id) = table.symbol_id(name) {
            let use_def = use_def_map(db, body_scope);

            let declarations = use_def.end_of_scope_symbol_declarations(symbol_id);
            let declared_and_qualifiers =
                place_from_declarations(db, env, declarations).ignore_conflicting_declarations();

            match declared_and_qualifiers {
                PlaceAndQualifiers {
                    place:
                        mut declared @ Place::Defined(DefinedPlace {
                            ty: declared_ty,
                            definedness: declaredness,
                            provenance: declared_provenance,
                            ..
                        }),
                    qualifiers,
                } => {
                    // For the purpose of finding instance attributes, ignore `ClassVar`
                    // declarations:
                    if qualifiers.contains(TypeQualifiers::CLASS_VAR) {
                        declared = Place::Undefined;
                    }

                    if qualifiers.contains(TypeQualifiers::INIT_VAR) {
                        // We ignore `InitVar` declarations on the class body, unless that attribute is overwritten
                        // by an implicit assignment in a method
                        if self
                            .implicit_attribute(db, name, MethodDecorator::None)
                            .is_undefined()
                        {
                            return Member::unbound();
                        }
                    }

                    // `KW_ONLY` sentinels are markers, not real instance attributes.
                    if declared_ty.is_instance_of(db, KnownClass::KwOnly)
                        && CodeGeneratorKind::from_static_class(db, self)
                            .is_some_and(CodeGeneratorKind::is_dataclass_like)
                    {
                        return Member::unbound();
                    }

                    // The attribute is declared in the class body.

                    let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
                    let inferred = place_from_bindings(db, env, bindings).place;
                    // Stub assignments to slots describe instance storage, not runtime class
                    // attributes.
                    let has_binding = !(inferred.is_undefined()
                        || self.file(db).is_stub(db) && self.has_instance_slot(db, name));

                    if has_binding {
                        // The attribute is declared and bound in the class body.

                        let implicit = self.implicit_attribute(db, name, MethodDecorator::None);
                        if let Place::Defined(DefinedPlace {
                            ty: implicit_ty,
                            provenance: implicit_provenance,
                            ..
                        }) = implicit.inner.place
                        {
                            if declaredness == Definedness::AlwaysDefined {
                                // If a symbol is definitely declared, and we see
                                // attribute assignments in methods of the class,
                                // we trust the declared type.
                                Member {
                                    inner: declared.with_qualifiers(qualifiers),
                                }
                            } else {
                                Member {
                                    inner: Place::Defined(DefinedPlace {
                                        ty: UnionType::from_two_elements(
                                            db,
                                            env,
                                            declared_ty,
                                            implicit_ty,
                                        ),
                                        origin: TypeOrigin::Declared,
                                        definedness: declaredness,
                                        public_type_policy: PublicTypePolicy::Raw,
                                        provenance: implicit_provenance.or(declared_provenance),
                                    })
                                    .with_qualifiers(qualifiers),
                                }
                            }
                        } else if self.is_own_dataclass_instance_field(db, name)
                            && declared_ty
                                .class_member(db, env, "__get__")
                                .place
                                .is_undefined()
                        {
                            // For dataclass-like classes, declared fields are assigned
                            // by the synthesized `__init__`, so they are instance
                            // attributes even without an explicit `self.x = ...`
                            // assignment in a method body.
                            //
                            // However, if the declared type is a descriptor (has
                            // `__get__`), we return unbound so that the descriptor
                            // protocol in `member_lookup_with_policy` can resolve
                            // the attribute type through `__get__`.
                            Member {
                                inner: declared.with_qualifiers(qualifiers),
                            }
                        } else {
                            // The symbol is declared and bound in the class body,
                            // but we did not find any attribute assignments in
                            // methods of the class. This means that the attribute
                            // has a class-level default value, but it would not be
                            // found in a `__dict__` lookup.

                            Member::unbound()
                        }
                    } else {
                        // The attribute is declared but not bound in the class body.
                        // We take this as a sign that this is intended to be a pure
                        // instance attribute, and we trust the declared type, unless
                        // it is possibly-undeclared. In the latter case, we also
                        // union with the inferred type from attribute assignments.

                        if declaredness == Definedness::AlwaysDefined {
                            Member {
                                inner: declared.with_qualifiers(qualifiers),
                            }
                        } else {
                            if let Place::Defined(DefinedPlace {
                                ty: implicit_ty,
                                provenance: implicit_provenance,
                                ..
                            }) = self
                                .implicit_attribute(db, name, MethodDecorator::None)
                                .inner
                                .place
                            {
                                Member {
                                    inner: Place::Defined(DefinedPlace {
                                        ty: UnionType::from_two_elements(
                                            db,
                                            env,
                                            declared_ty,
                                            implicit_ty,
                                        ),
                                        origin: TypeOrigin::Declared,
                                        definedness: declaredness,
                                        public_type_policy: PublicTypePolicy::Raw,
                                        provenance: implicit_provenance.or(declared_provenance),
                                    })
                                    .with_qualifiers(qualifiers),
                                }
                            } else {
                                Member {
                                    inner: declared.with_qualifiers(qualifiers),
                                }
                            }
                        }
                    }
                }

                PlaceAndQualifiers {
                    place: Place::Undefined,
                    qualifiers: _,
                } => {
                    // The attribute is not *declared* in the class body. It could still be declared/bound
                    // in a method.

                    self.implicit_attribute(db, name, MethodDecorator::None)
                }
            }
        } else {
            // This attribute is neither declared nor bound in the class body.
            // It could still be implicitly defined in a method.

            self.implicit_attribute(db, name, MethodDecorator::None)
        }
    }

    /// Returns `true` if `name` is a non-init-only field directly declared on this
    /// dataclass (i.e., a field that corresponds to an instance attribute).
    ///
    /// This is used to decide whether a bare class-body annotation like `x: int`
    /// should be treated as defining an instance attribute: dataclass fields are
    /// implicitly assigned in `__init__`, so they behave as instance attributes
    /// even though no explicit binding exists in the class body.
    fn is_own_dataclass_instance_field(self, db: &'db dyn Db, name: &str) -> bool {
        let Some(field_policy) = CodeGeneratorKind::from_static_class(db, self) else {
            return false;
        };
        if !field_policy.treats_fields_as_instance_attributes() {
            return false;
        }

        let fields = self.own_fields(db, None, field_policy);
        let Some(field) = fields.get(name) else {
            return false;
        };
        matches!(
            field.kind,
            FieldKind::Dataclass {
                init_only: false,
                ..
            } | FieldKind::Pydantic { .. }
        )
    }

    /// Returns the converter's input type (i.e., the type of its first positional parameter) for a
    /// dataclass field, if the field has a converter function specified.
    pub(super) fn converter_input_type_for_field(
        self,
        db: &'db dyn Db,
        name: &str,
    ) -> Option<Type<'db>> {
        let field_policy @ CodeGeneratorKind::DataclassLike(_) =
            CodeGeneratorKind::from_static_class(db, self)?
        else {
            return None;
        };
        let fields = self.fields(db, None, field_policy);
        let field = fields.get(name)?;
        if let FieldKind::Dataclass { converter, .. } = field.kind {
            converter.map(|(input_ty, _)| input_ty)
        } else {
            None
        }
    }

    pub(super) fn to_non_generic_instance(self, db: &'db dyn Db) -> Type<'db> {
        let env = ProgramEnvironment::from_scope(self.body_scope(db));
        Type::instance(db, &env, ClassType::NonGeneric(self.into()))
    }

    /// Return this class' involvement in an inheritance cycle, if any.
    ///
    /// A class definition like this will fail at runtime,
    /// but we must be resilient to it or we could panic.
    pub(crate) fn inheritance_cycle(self, db: &'db dyn Db) -> Option<InheritanceCycle> {
        if !self.has_explicit_bases(db) {
            return None;
        }

        #[salsa::tracked(returns(copy), cycle_initial=|_, _, _| None, heap_size=ruff_memory_usage::heap_size)]
        fn inheritance_cycle_inner<'db>(
            db: &'db dyn Db,
            class: StaticClassLiteral<'db>,
        ) -> Option<InheritanceCycle> {
            /// Return `true` if the class is cyclically defined.
            ///
            /// Also, populates `visited_classes` with all base classes of `class`.
            fn is_cyclically_defined_recursive<'db>(
                db: &'db dyn Db,
                class: StaticClassLiteral<'db>,
                classes_on_stack: &mut FxIndexSet<StaticClassLiteral<'db>>,
                visited_classes: &mut FxIndexSet<StaticClassLiteral<'db>>,
            ) -> bool {
                let mut result = false;
                for explicit_base in class.explicit_bases(db) {
                    let explicit_base_class_literal = match explicit_base {
                        Type::ClassLiteral(class_literal) => class_literal.as_static(),
                        Type::GenericAlias(generic_alias) => Some(generic_alias.origin(db)),
                        _ => continue,
                    };
                    let Some(explicit_base_class_literal) = explicit_base_class_literal else {
                        continue;
                    };
                    if !classes_on_stack.insert(explicit_base_class_literal) {
                        return true;
                    }

                    if visited_classes.insert(explicit_base_class_literal) {
                        // If we find a cycle, keep searching to check if we can reach the starting
                        // class.
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

            tracing::trace!("Class::inheritance_cycle: {}", class.name(db));
            let visited_classes = &mut FxIndexSet::default();
            if !is_cyclically_defined_recursive(
                db,
                class,
                &mut FxIndexSet::default(),
                visited_classes,
            ) {
                None
            } else if visited_classes.contains(&class) {
                Some(InheritanceCycle::Participant)
            } else {
                Some(InheritanceCycle::Inherited)
            }
        }

        inheritance_cycle_inner(db, self)
    }

    /// Returns a [`Span`] with the range of the class's header.
    ///
    /// See [`Self::header_range`] for more details.
    pub(crate) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.file(db)).with_range(self.header_range(db))
    }

    /// Returns the range of the class's "header": the class name
    /// and any arguments passed to the `class` statement. E.g.
    ///
    /// ```ignore
    /// class Foo(Bar, metaclass=Baz): ...
    ///       ^^^^^^^^^^^^^^^^^^^^^^^
    /// ```
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let class_scope = self.body_scope(db);
        let module = parsed_module(db, class_scope.python_file(db)).load(db);
        let class_node = self.node(db, &module);
        let class_name = &class_node.name;
        TextRange::new(
            class_name.start(),
            class_node
                .arguments
                .as_deref()
                .map(Ranged::end)
                .unwrap_or_else(|| class_name.end()),
        )
    }

    /// Returns the range of the class's name
    pub(crate) fn focus_range(self, db: &'db dyn Db) -> TextRange {
        let class_scope = self.body_scope(db);
        let module = parsed_module(db, class_scope.python_file(db)).load(db);
        let class_node = self.node(db, &module);
        class_node.name.range()
    }
}

/// A single semantic class-base entry after expanding starred tuple bases.
#[derive(Clone, Copy)]
pub(crate) struct ExpandedClassBaseEntry<'a, 'db> {
    source_node: &'a ast::Expr,
    ty: Type<'db>,
}

impl<'a, 'db> ExpandedClassBaseEntry<'a, 'db> {
    /// Returns the source expression for this base entry.
    pub(crate) const fn source_node(self) -> &'a ast::Expr {
        self.source_node
    }

    /// Returns the semantic type of this base entry.
    pub(crate) const fn ty(self) -> Type<'db> {
        self.ty
    }
}

/// Expands a class's bases into the semantic entries used by [`StaticClassLiteral::explicit_bases`].
pub(crate) fn expanded_class_base_entries<'a, 'db>(
    db: &'db dyn Db,
    known_class: Option<KnownClass>,
    class_stmt: &'a ast::StmtClassDef,
    class_definition: Definition<'db>,
) -> Vec<ExpandedClassBaseEntry<'a, 'db>> {
    match known_class {
        // Special-case `NotImplementedType`: typeshed says that it inherits from `Any`,
        // but this causes more problems than it fixes.
        Some(KnownClass::NotImplementedType) => vec![],
        _ => {
            let mut expanded_bases = Vec::with_capacity(class_stmt.bases().len());

            for base_node in class_stmt.bases() {
                if let Some(tuple) =
                    expanded_fixed_length_starred_class_base_tuple(db, class_definition, base_node)
                {
                    if let ast::Expr::Starred(starred) = base_node
                        && let Some(tuple_literal) = starred.value.as_tuple_expr()
                        && tuple_literal.len() == tuple.len()
                        && tuple_literal
                            .iter()
                            .all(|element| !element.is_starred_expr())
                    {
                        expanded_bases.extend(
                            tuple_literal
                                .iter()
                                .zip(tuple.owned_elements().into_vec())
                                .map(|(source_node, ty)| ExpandedClassBaseEntry {
                                    source_node,
                                    ty,
                                }),
                        );
                        continue;
                    }

                    expanded_bases.extend(tuple.owned_elements().into_vec().into_iter().map(
                        |ty| ExpandedClassBaseEntry {
                            source_node: base_node,
                            ty,
                        },
                    ));
                    continue;
                }

                let ty = if matches!(base_node, ast::Expr::Starred(_)) {
                    Type::unknown()
                } else {
                    definition_expression_type(db, class_definition, base_node)
                };
                expanded_bases.push(ExpandedClassBaseEntry {
                    source_node: base_node,
                    ty,
                });
            }

            expanded_bases
        }
    }
}

/// If `base_node` is a starred class base whose value is inferred as a fixed-length tuple,
/// returns the unpacked tuple in source order.
fn expanded_fixed_length_starred_class_base_tuple<'db>(
    db: &'db dyn Db,
    class_definition: Definition<'db>,
    base_node: &ast::Expr,
) -> Option<FixedLengthTuple<Type<'db>>> {
    let ast::Expr::Starred(starred) = base_node else {
        return None;
    };

    let starred_ty = definition_expression_type(db, class_definition, &starred.value);
    let env = ProgramEnvironment::from_definition(class_definition);
    let Tuple::Fixed(tuple) = starred_ty.tuple_instance_spec(db, &env)?.into_owned() else {
        return None;
    };
    Some(tuple)
}

impl<'db> VarianceInferable<'db> for StaticClassLiteral<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        _: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        self.variance_of_owner(db, typevar)
    }
}

#[salsa::tracked]
impl<'db> StaticClassLiteral<'db> {
    #[salsa::tracked(returns(copy), cycle_initial=|_, _, _, _| TypeVarVariance::Bivariant, heap_size=ruff_memory_usage::heap_size)]
    fn variance_of_owner(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> TypeVarVariance {
        let env = ProgramEnvironment::from_scope(self.body_scope(db));

        if self.is_typed_dict(db) {
            return TypedDictType::new(self.identity_specialization(db))
                .variance_of_items(db, &env, typevar);
        }

        let typevar_in_generic_context = self
            .generic_context(db)
            .is_some_and(|generic_context| generic_context.contains(db, typevar));

        if !typevar_in_generic_context {
            return TypeVarVariance::Bivariant;
        }

        if self.is_protocol(db)
            && let Some(protocol) = self.identity_specialization(db).into_protocol_class(db)
            && protocol.supports_variance_inference(db)
        {
            return protocol.interface(db).variance_of(db, &env, typevar);
        }

        let class_body_scope = self.body_scope(db);
        let program_file = class_body_scope.program_file(db);
        let python_version = env.python_version(db);

        let index = semantic_index(db, program_file);

        let explicit_bases_variances = self
            .explicit_bases(db)
            .iter()
            .map(|class| class.variance_of(db, &env, typevar));

        let default_attribute_variance = {
            let is_namedtuple = CodeGeneratorKind::NamedTuple.matches(db, self.into());
            // Python 3.13 introduced a synthesized `__replace__` method on dataclasses which uses
            // their field types in contravariant position, thus meaning a frozen dataclass must
            // still be invariant in its field types. Other synthesized methods on dataclasses are
            // not considered here, since they don't use field types in their signatures. TODO:
            // ideally we'd have a single source of truth for information about synthesized
            // methods, so we just look them up normally and don't hardcode this knowledge here.
            let is_frozen_dataclass_prior_to_313 = python_version <= PythonVersion::PY312
                && CodeGeneratorKind::from_static_class(db, self)
                    .is_some_and(|kind| self.has_dataclass_param(db, kind, DataclassFlags::FROZEN));

            if is_namedtuple || is_frozen_dataclass_prior_to_313 {
                TypeVarVariance::Covariant
            } else {
                TypeVarVariance::Invariant
            }
        };

        let init_name: &Name = &"__init__".into();
        let new_name: &Name = &"__new__".into();

        let use_def_map = index.use_def_map(class_body_scope.file_scope_id(db));
        let table = place_table(db, class_body_scope);
        let attribute_places_and_qualifiers =
            use_def_map
                .all_end_of_scope_symbol_declarations()
                .map(|(symbol_id, declarations)| {
                    let place_and_qual = place_from_declarations(db, &env, declarations)
                        .ignore_conflicting_declarations();
                    (symbol_id, place_and_qual)
                })
                .chain(use_def_map.all_end_of_scope_symbol_bindings().map(
                    |(symbol_id, bindings)| {
                        (
                            symbol_id,
                            place_from_bindings(db, &env, bindings).place.into(),
                        )
                    },
                ))
                .filter_map(|(symbol_id, place_and_qual)| {
                    if let Some(name) = table.place(symbol_id).as_symbol().map(Symbol::name) {
                        (![init_name, new_name].contains(&name))
                            .then_some((name.to_string(), place_and_qual))
                    } else {
                        None
                    }
                });

        // Dataclasses can have some additional synthesized methods (`__eq__`, `__hash__`,
        // `__lt__`, etc.) but none of these will have field types type variables in their signatures, so we
        // don't need to consider them for variance.

        let attribute_names = attribute_scopes(db, self.body_scope(db))
            .flat_map(|function_scope_id| {
                index
                    .place_table(function_scope_id)
                    .members()
                    .filter_map(|member| member.as_instance_attribute())
                    .filter(|name| *name != init_name && *name != new_name)
                    .map(std::string::ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .dedup();

        let attribute_variances = attribute_names
            .map(|name| {
                let place_and_quals = self.own_instance_member(db, &env, &name).inner;
                (name, place_and_quals)
            })
            .chain(attribute_places_and_qualifiers)
            .dedup()
            .filter_map(|(name, place_and_qual)| {
                place_and_qual.ignore_possibly_undefined().map(|ty| {
                    let variance = if place_and_qual
                        .qualifiers
                        // None of these fields can be mutated through an instance.
                        .intersects(
                            TypeQualifiers::CLASS_VAR
                                | TypeQualifiers::FINAL
                                | TypeQualifiers::READ_ONLY,
                        )
                        // We don't allow mutation of methods or properties
                        || ty.is_function_literal()
                        || ty.is_property_instance()
                        // Underscore-prefixed attributes are assumed not to be externally mutated
                        || name.starts_with('_')
                    {
                        // CLASS_VAR: class vars generally shouldn't contain the
                        // type variable, but they could if it's a
                        // callable type. They can't be mutated on instances.
                        //
                        // FINAL and READ_ONLY: immutable fields are covariant.
                        TypeVarVariance::Covariant
                    } else {
                        default_attribute_variance
                    };
                    ty.with_polarity(variance).variance_of(db, &env, typevar)
                })
            });

        attribute_variances
            .chain(explicit_bases_variances)
            .collect()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, get_size2::GetSize)]
pub(crate) enum InheritanceCycle {
    /// The class is cyclically defined and is a participant in the cycle.
    /// i.e., it inherits either directly or indirectly from itself.
    Participant,
    /// The class inherits from a class that is a `Participant` in an inheritance cycle,
    /// but is not itself a participant.
    Inherited,
}

impl InheritanceCycle {
    pub(crate) const fn is_participant(self) -> bool {
        matches!(self, InheritanceCycle::Participant)
    }
}

fn explicit_bases_cycle_initial<'db>(
    db: &'db dyn Db,
    id: salsa::Id,
    literal: StaticClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    let module = parsed_module(db, literal.python_file(db)).load(db);
    let class_stmt = literal.node(db, &module);
    // Try to produce a list of `Divergent` types of the right length. However, if one or more of
    // the bases is a starred expression, we don't know how many entries that will eventually
    // expand to.
    vec![Type::divergent(id); class_stmt.bases().len()].into_boxed_slice()
}

fn explicit_bases_cycle_fn<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &[Type<'db>],
    current: Box<[Type<'db>]>,
    literal: StaticClassLiteral<'db>,
) -> Box<[Type<'db>]> {
    if previous.len() == current.len() {
        let env = ProgramEnvironment::from_scope(literal.body_scope(db));
        // As long as the length of bases hasn't changed, use the same "monotonic widening"
        // strategy that we use with most types, to avoid oscillations.
        current
            .iter()
            .zip(previous.iter())
            .map(|(curr, prev)| curr.cycle_normalized(db, &env, *prev, cycle))
            .collect()
    } else {
        // The length of bases has changed, presumably because we expanded a starred expression. We
        // don't do "monotonic widening" here, because we don't want to make assumptions about
        // which previous entries correspond to which current ones. An oscillation here would be
        // unfortunate, but maybe only pathological programs can trigger such a thing.
        current
    }
}
