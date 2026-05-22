use ruff_db::{diagnostic::Span, parsed::parsed_module};
use ruff_python_ast::{self as ast, NodeIndex, name::Name};
use ruff_text_size::{Ranged, TextRange};

use crate::{
    Db, TypeQualifiers,
    place::{Place, PlaceAndQualifiers},
    types::{
        ClassBase, ClassLiteral, ClassType, DataclassParams, KnownClass, MemberLookupPolicy,
        SubclassOfType, Type,
        class::{
            ClassMemberResult, CodeGeneratorKind, DisjointBase, InstanceMemberResult, MroLookup,
        },
        definition_expression_type, extract_fixed_length_iterable_element_types,
        member::Member,
        mro::{DynamicMroError, Mro, MroIterator},
    },
};
use ty_python_core::{definition::Definition, scope::ScopeId};

/// A class created dynamically via a three-argument `type()` or `types.new_class()` call.
///
/// For example:
/// ```python
/// Foo = type("Foo", (Base,), {"attr": 1})
/// ```
///
/// The type of `Foo` would be `<class 'Foo'>` where `Foo` is a `DynamicClassLiteral` with:
/// - name: "Foo"
/// - members: [("attr", int)]
///
/// This is called "dynamic" because the class is created dynamically at runtime
/// via a function call rather than a class statement.
///
/// # Salsa interning
///
/// This is a Salsa-interned struct. Two different `type()` / `types.new_class()` calls
/// always produce distinct `DynamicClassLiteral` instances, even if they have the same
/// name and bases:
///
/// ```python
/// Foo1 = type("Foo", (Base,), {})
/// Foo2 = type("Foo", (Base,), {})
/// # Foo1 and Foo2 are distinct types
/// ```
///
/// The `anchor` field provides stable identity:
/// - For assigned calls, the `Definition` uniquely identifies the class.
/// - For dangling calls, a relative node offset anchored to the enclosing scope
///   provides stable identity that only changes when the scope itself changes.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct DynamicClassLiteral<'db> {
    /// The name of the class (from the first argument).
    #[returns(ref)]
    pub name: Name,

    /// The anchor for this dynamic class, providing stable identity.
    ///
    /// - `Definition`: The call is assigned to a variable. The definition
    ///   uniquely identifies this class and can be used to find the call expression.
    /// - `ScopeOffset`: The call is "dangling" (not assigned). The offset
    ///   is relative to the enclosing scope's anchor node index.
    #[returns(ref)]
    pub anchor: DynamicClassAnchor<'db>,

    /// The class members extracted from the namespace argument.
    /// Each entry is a (name, type) pair extracted from the dict literal.
    #[returns(deref)]
    pub members: Box<[(Name, Type<'db>)]>,

    /// Whether the namespace is dynamic (not a literal dict, or contains
    /// non-string-literal keys). When true, attribute lookups on this class
    /// and its instances return `Unknown` instead of failing.
    pub has_dynamic_namespace: bool,

    /// Dataclass parameters if this class has been wrapped with `@dataclass` decorator
    /// or passed to `dataclass()` as a function.
    pub dataclass_params: Option<DataclassParams<'db>>,
}

/// Anchor for identifying a dynamic class literal.
///
/// This enum provides stable identity for `DynamicClassLiteral`:
/// - For assigned calls, the `Definition` uniquely identifies the class.
/// - For dangling calls, a relative offset provides stable identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum DynamicClassAnchor<'db> {
    /// The call is assigned to a variable.
    ///
    /// The `Definition` uniquely identifies this class. The call expression
    /// is the `value` of the assignment, so we can get its range from the definition.
    Definition(Definition<'db>),

    /// The call is "dangling" (not assigned to a variable).
    ///
    /// The offset is relative to the enclosing scope's anchor node index.
    /// For module scope, this is equivalent to an absolute index (anchor is 0).
    ///
    /// The `explicit_bases` are computed eagerly at creation time since dangling
    /// calls cannot recursively reference the class being defined.
    ScopeOffset {
        scope: ScopeId<'db>,
        offset: u32,
        explicit_bases: Box<[Type<'db>]>,
    },
}

impl get_size2::GetSize for DynamicClassLiteral<'_> {}

/// Returns the `bases` argument for a dynamic class constructor call.
///
/// Dynamic class constructors accept `bases` either as the second positional argument or as a
/// `bases=` keyword argument.
pub(crate) fn dynamic_class_bases_argument(arguments: &ast::Arguments) -> Option<&ast::Expr> {
    arguments.args.get(1).or_else(|| {
        arguments
            .keywords
            .iter()
            .find(|kw| kw.arg.as_deref() == Some("bases"))
            .map(|kw| &kw.value)
    })
}

#[salsa::tracked]
impl<'db> DynamicClassLiteral<'db> {
    /// Returns the definition where this class is created, if it was assigned to a variable.
    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.anchor(db) {
            DynamicClassAnchor::Definition(definition) => Some(*definition),
            DynamicClassAnchor::ScopeOffset { .. } => None,
        }
    }

    /// Returns the scope in which this dynamic class was created.
    pub(crate) fn scope(self, db: &'db dyn Db) -> ScopeId<'db> {
        match self.anchor(db) {
            DynamicClassAnchor::Definition(definition) => definition.scope(db),
            DynamicClassAnchor::ScopeOffset { scope, .. } => *scope,
        }
    }

    /// Returns the explicit base classes of this dynamic class.
    ///
    /// For assigned calls, bases are computed lazily using deferred inference to handle
    /// forward references (e.g., `X = type("X", (tuple["X | None"],), {})`).
    ///
    /// For dangling calls, bases are computed eagerly at creation time and stored
    /// directly on the anchor, since dangling calls cannot recursively reference the
    /// class being defined.
    ///
    /// Returns an empty slice if the bases cannot be computed (e.g., due to a cycle)
    /// or if the bases argument cannot be extracted precisely.
    ///
    /// Returns `[Unknown]` if the bases iterable is variable-length.
    pub(crate) fn explicit_bases(self, db: &'db dyn Db) -> &'db [Type<'db>] {
        /// Inner cached function for deferred inference of bases.
        /// Only called for assigned calls where inference was deferred.
        #[salsa::tracked(returns(deref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
        fn deferred_explicit_bases<'db>(
            db: &'db dyn Db,
            definition: Definition<'db>,
        ) -> Box<[Type<'db>]> {
            let module = parsed_module(db, definition.file(db)).load(db);

            let value = definition
                .kind(db)
                .value(&module)
                .expect("DynamicClassAnchor::Definition should only be used for assignments");
            let call_expr = value
                .as_call_expr()
                .expect("Definition value should be a call expression");

            let Some(bases_arg) = dynamic_class_bases_argument(&call_expr.arguments) else {
                return Box::default();
            };

            // Use `definition_expression_type` for deferred inference support.
            extract_fixed_length_iterable_element_types(db, bases_arg, |expr| {
                definition_expression_type(db, definition, expr)
            })
            .unwrap_or_else(|| Box::from([Type::unknown()]))
        }

        match self.anchor(db) {
            // For dangling calls, bases are stored directly on the anchor.
            DynamicClassAnchor::ScopeOffset { explicit_bases, .. } => explicit_bases.as_ref(),
            // For assigned calls, use deferred inference.
            DynamicClassAnchor::Definition(definition) => deferred_explicit_bases(db, *definition),
        }
    }

    /// Returns a [`Span`] with the range of the `type()` call expression.
    ///
    /// See [`Self::header_range`] for more details.
    pub(super) fn header_span(self, db: &'db dyn Db) -> Span {
        Span::from(self.scope(db).file(db)).with_range(self.header_range(db))
    }

    /// Returns the range of the `type()` call expression that created this class.
    pub(crate) fn header_range(self, db: &'db dyn Db) -> TextRange {
        let scope = self.scope(db);
        let file = scope.file(db);
        let module = parsed_module(db, file).load(db);

        match self.anchor(db) {
            DynamicClassAnchor::Definition(definition) => {
                // For definitions, get the range from the definition's value.
                // The `type()` call is the value of the assignment.
                definition
                    .kind(db)
                    .value(&module)
                    .expect("DynamicClassAnchor::Definition should only be used for assignments")
                    .range()
            }
            DynamicClassAnchor::ScopeOffset { offset, .. } => {
                // For dangling `type()` calls, compute the absolute index from the offset.
                let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
                let anchor_u32 = scope_anchor
                    .as_u32()
                    .expect("anchor should not be NodeIndex::NONE");
                let absolute_index = NodeIndex::from(anchor_u32 + *offset);

                // Get the node and return its range.
                let node: &ast::ExprCall = module
                    .get_by_index(absolute_index)
                    .try_into()
                    .expect("scope offset should point to ExprCall");
                node.range()
            }
        }
    }

    /// Get the metaclass of this dynamic class.
    ///
    /// Derives the metaclass from base classes: finds the most derived metaclass
    /// that is a subclass of all other base metaclasses.
    ///
    /// See <https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass>
    pub(crate) fn metaclass(self, db: &'db dyn Db) -> Type<'db> {
        self.try_metaclass(db)
            .unwrap_or_else(|_| SubclassOfType::subclass_of_unknown())
    }

    /// Try to get the metaclass of this dynamic class.
    ///
    /// Returns `Err(DynamicMetaclassConflict)` if there's a metaclass conflict
    /// (i.e., two base classes have metaclasses that are not in a subclass relationship).
    ///
    /// See <https://docs.python.org/3/reference/datamodel.html#determining-the-appropriate-metaclass>
    pub(crate) fn try_metaclass(
        self,
        db: &'db dyn Db,
    ) -> Result<Type<'db>, DynamicMetaclassConflict<'db>> {
        let original_bases = self.explicit_bases(db);

        // If no bases, metaclass is `type`.
        // To dynamically create a class with no bases that has a custom metaclass,
        // you have to invoke that metaclass rather than `type()`.
        if original_bases.is_empty() {
            return Ok(KnownClass::Type.to_class_literal(db));
        }

        // If there's an MRO error, return unknown to avoid cascading errors.
        if self.try_mro(db).is_err() {
            return Ok(SubclassOfType::subclass_of_unknown());
        }

        // Convert Types to ClassBases for metaclass computation.
        // All bases should convert successfully here: `try_mro()` above would have
        // returned `Err(InvalidBases)` if any failed, causing us to return early.
        let bases: Vec<ClassBase<'db>> = original_bases
            .iter()
            .filter_map(|base_type| ClassBase::try_from_type(db, *base_type, None))
            .collect();

        // If all bases failed to convert, return type as the metaclass.
        if bases.is_empty() {
            return Ok(KnownClass::Type.to_class_literal(db));
        }

        // Start with the first base's metaclass as the candidate.
        let mut candidate = bases[0].metaclass(db);

        // Track which base the candidate metaclass came from.
        let (mut candidate_base, rest) = bases.split_first().unwrap();

        // Reconcile with other bases' metaclasses.
        for base in rest {
            let base_metaclass = base.metaclass(db);

            // Get the ClassType for comparison.
            let Some(candidate_class) = candidate.to_class_type(db) else {
                // If candidate isn't a class type, keep it as is.
                continue;
            };
            let Some(base_metaclass_class) = base_metaclass.to_class_type(db) else {
                continue;
            };

            // If base's metaclass is more derived, use it.
            if base_metaclass_class.is_subclass_of(db, candidate_class) {
                candidate = base_metaclass;
                candidate_base = base;
                continue;
            }

            // If candidate is already more derived, keep it.
            if candidate_class.is_subclass_of(db, base_metaclass_class) {
                continue;
            }

            // Conflict: neither metaclass is a subclass of the other.
            // Python raises `TypeError: metaclass conflict` at runtime.
            return Err(DynamicMetaclassConflict {
                metaclass1: candidate_class,
                base1: *candidate_base,
                metaclass2: base_metaclass_class,
                base2: *base,
            });
        }

        Ok(candidate)
    }

    /// Iterate over the MRO of this class using C3 linearization.
    ///
    /// The MRO includes the class itself as the first element, followed
    /// by the merged base class MROs (consistent with `ClassType::iter_mro`).
    ///
    /// If the MRO cannot be computed (e.g., due to inconsistent ordering), falls back
    /// to iterating over base MROs sequentially with deduplication.
    pub(crate) fn iter_mro(self, db: &'db dyn Db) -> MroIterator<'db> {
        MroIterator::new(db, ClassLiteral::Dynamic(self), None)
    }

    /// Look up an instance member by iterating through the MRO.
    pub(crate) fn instance_member(self, db: &'db dyn Db, name: &str) -> PlaceAndQualifiers<'db> {
        match MroLookup::new(db, self.iter_mro(db)).instance_member(name) {
            InstanceMemberResult::Done(result) => result,
            InstanceMemberResult::TypedDict => {
                // Simplified `TypedDict` handling without type mapping.
                KnownClass::TypedDictFallback
                    .to_instance(db)
                    .instance_member(db, name)
            }
        }
    }

    /// Look up a class-level member by iterating through the MRO.
    ///
    /// Uses `MroLookup` with:
    /// - No inherited generic context (dynamic classes aren't generic).
    /// - `is_self_object = false` (dynamic classes are never `object`).
    pub(crate) fn class_member(
        self,
        db: &'db dyn Db,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> PlaceAndQualifiers<'db> {
        // Check if this dynamic class is dataclass-like (via dataclass_transform inheritance).
        if matches!(
            CodeGeneratorKind::from_class(db, self.into(), None),
            Some(CodeGeneratorKind::DataclassLike(_))
        ) {
            if name == "__dataclass_fields__" {
                // Make this class look like a subclass of the `DataClassInstance` protocol.
                return Place::declared(KnownClass::Dict.to_specialized_instance(
                    db,
                    &[
                        KnownClass::Str.to_instance(db),
                        KnownClass::Field.to_specialized_instance(db, &[Type::any()]),
                    ],
                ))
                .with_qualifiers(TypeQualifiers::CLASS_VAR);
            } else if name == "__dataclass_params__" {
                // There is no typeshed class for this. For now, we model it as `Any`.
                return Place::declared(Type::any()).with_qualifiers(TypeQualifiers::CLASS_VAR);
            }
        }

        let result = MroLookup::new(db, self.iter_mro(db)).class_member(
            name, policy, None,  // No inherited generic context.
            false, // Dynamic classes are never `object`.
        );

        match result {
            ClassMemberResult::Done(result) => result.finalize(db),
            ClassMemberResult::TypedDict => {
                // Simplified `TypedDict` handling without type mapping.
                KnownClass::TypedDictFallback
                    .to_class_literal(db)
                    .find_name_in_mro_with_policy(db, name, policy)
                    .expect("Will return Some() when called on class literal")
            }
        }
    }

    /// Look up a class member defined directly on this class (not inherited).
    ///
    /// Returns [`Member::unbound`] if the member is not found in the namespace dict,
    /// unless the namespace is dynamic, in which case returns `Unknown`.
    pub(super) fn own_class_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        // If the namespace is dynamic (not a literal dict) and the name isn't in `self.members`,
        // return Unknown since we can't know what attributes might be defined.
        self.members(db)
            .iter()
            .find_map(|(member_name, ty)| (name == member_name).then_some(*ty))
            .or_else(|| self.has_dynamic_namespace(db).then(Type::unknown))
            .map(Member::definitely_declared)
            .unwrap_or_default()
    }

    /// Look up an instance member defined directly on this class (not inherited).
    ///
    /// For dynamic classes, instance members are the same as class members
    /// since they come from the namespace dict.
    pub(super) fn own_instance_member(self, db: &'db dyn Db, name: &str) -> Member<'db> {
        self.own_class_member(db, name)
    }

    /// Try to compute the MRO for this dynamic class.
    ///
    /// Returns `Ok(Mro)` if successful, or `Err(DynamicMroError)` if there's
    /// an error (duplicate bases or C3 linearization failure).
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|db, _, self_: DynamicClassLiteral<'db>| {
            Ok(Mro::from([
                ClassBase::Class(ClassType::NonGeneric(ClassLiteral::Dynamic(self_))),
                ClassBase::object(db),
            ]))
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    pub(crate) fn try_mro(self, db: &'db dyn Db) -> Result<Mro<'db>, DynamicMroError<'db>> {
        Mro::of_dynamic_class(db, self)
    }

    /// Return `Some()` if this dynamic class is known to be a [`DisjointBase`].
    ///
    /// A dynamic class is a disjoint base if `__slots__` is defined in the namespace
    /// dictionary and is non-empty. Example:
    /// ```python
    /// X = type("X", (), {"__slots__": ("a",)})
    /// ```
    pub(super) fn as_disjoint_base(self, db: &'db dyn Db) -> Option<DisjointBase<'db>> {
        // Check if __slots__ is in the members
        for (name, ty) in self.members(db) {
            if name.as_str() == "__slots__" {
                // Check if the slots are non-empty
                let is_non_empty = match ty {
                    // __slots__ = ("a", "b")
                    Type::NominalInstance(nominal) => nominal.tuple_spec(db).is_some_and(|spec| {
                        spec.len().into_fixed_length().is_some_and(|len| len > 0)
                    }),
                    // __slots__ = "abc"  # Same as ("abc",)
                    Type::LiteralValue(literal) if literal.is_string() => true,
                    // Other types are considered dynamic/unknown
                    _ => false,
                };
                if is_non_empty {
                    return Some(DisjointBase::due_to_dunder_slots(ClassLiteral::Dynamic(
                        self,
                    )));
                }
            }
        }
        None
    }

    /// Returns `true` if this dynamic class defines any ordering method (`__lt__`, `__le__`,
    /// `__gt__`, `__ge__`) in its namespace dictionary. Used by `@total_ordering` to determine
    /// if synthesis is valid.
    ///
    /// If the namespace is dynamic, returns `true` since we can't know if ordering methods exist.
    pub(crate) fn has_own_ordering_method(self, db: &'db dyn Db) -> bool {
        const ORDERING_METHODS: &[&str] = &["__lt__", "__le__", "__gt__", "__ge__"];
        ORDERING_METHODS
            .iter()
            .any(|name| !self.own_class_member(db, name).is_undefined())
    }

    /// Returns a new [`DynamicClassLiteral`] with the given dataclass params, preserving all other fields.
    pub(crate) fn with_dataclass_params(
        self,
        db: &'db dyn Db,
        dataclass_params: Option<DataclassParams<'db>>,
    ) -> Self {
        Self::new(
            db,
            self.name(db).clone(),
            self.anchor(db).clone(),
            self.members(db),
            self.has_dynamic_namespace(db),
            dataclass_params,
        )
    }
}

/// Error for metaclass conflicts in dynamic classes.
///
/// This mirrors `MetaclassErrorKind::Conflict` for regular classes.
#[derive(Debug, Clone)]
pub(crate) struct DynamicMetaclassConflict<'db> {
    /// The first conflicting metaclass and its originating base class.
    pub(crate) metaclass1: ClassType<'db>,
    pub(crate) base1: ClassBase<'db>,
    /// The second conflicting metaclass and its originating base class.
    pub(crate) metaclass2: ClassType<'db>,
    pub(crate) base2: ClassBase<'db>,
}
