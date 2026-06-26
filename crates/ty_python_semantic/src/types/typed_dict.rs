use std::cmp::Ordering;
use std::collections::{BTreeMap, btree_map::Entry};
use std::ops::{Deref, DerefMut};

use bitflags::bitflags;
use ordermap::OrderSet;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::Arguments;
use ruff_python_ast::{self as ast, AnyNodeRef, StmtClassDef, name::Name};
use ruff_text_size::Ranged;

use super::class::{ClassLiteral, ClassType, CodeGeneratorKind, Field, KnownClass};
use super::context::InferContext;
use super::diagnostic::{
    self, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, INVALID_KEY, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, report_invalid_key_on_typed_dict, report_missing_typed_dict_key,
};
use super::infer::{TypeExpressionFlags, infer_deferred_types};
use super::{
    ApplyTypeMappingVisitor, ErrorContext, IntersectionType, Type, TypeMapping, TypeQualifiers,
    UnionBuilder, definition_expression_annotation, definition_expression_type, visitor,
};
use crate::Db;
use crate::types::TypeContext;
use crate::types::TypeDefinition;
use crate::types::class::FieldKind;
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::relation::{DisjointnessChecker, TypeRelation, TypeRelationChecker};
use ty_python_core::definition::Definition;

bitflags! {
    /// Used for `TypedDict` class parameters.
    /// Keeps track of the keyword arguments that were passed-in during class definition.
    /// (see https://typing.python.org/en/latest/spec/typeddict.html)
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TypedDictParams: u8 {
        /// Whether keys are required by default (`total=True`)
        const TOTAL = 1 << 0;
    }
}

impl get_size2::GetSize for TypedDictParams {}

impl Default for TypedDictParams {
    fn default() -> Self {
        Self::TOTAL
    }
}

/// The undeclared-item policy of a `TypedDict`.
///
/// An implicitly open `TypedDict` may contain hidden items, but those items are not directly
/// accessible through most operations. A `TypedDict` with explicit extra items exposes those items
/// with a known type.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub enum TypedDictOpenness<'db> {
    /// Undeclared items may exist at runtime, but are not directly accessible through most
    /// `TypedDict` operations.
    #[default]
    ImplicitlyOpen,
    /// Undeclared items cannot exist.
    Closed,
    /// Undeclared items are explicitly exposed with the given value type and mutability.
    Extra(TypedDictExtraItems<'db>),
}

impl<'db> TypedDictOpenness<'db> {
    /// Creates an explicit extra-items policy, canonicalizing `Never` to [`Self::Closed`].
    ///
    /// This makes the two equivalent declarations below share the same internal representation:
    ///
    /// ```python
    /// class ByExtraItems(TypedDict, extra_items=Never): ...
    /// class ByClosed(TypedDict, closed=True): ...
    /// ```
    pub(crate) fn extra(db: &'db dyn Db, declared_ty: Type<'db>, is_read_only: bool) -> Self {
        if declared_ty.resolve_type_alias(db).is_never() {
            Self::Closed
        } else {
            Self::Extra(TypedDictExtraItems {
                declared_ty,
                is_read_only,
            })
        }
    }

    /// Returns extra items only when they were explicitly declared.
    ///
    /// An implicitly open `TypedDict` returns `None` here because its hidden items are not directly
    /// accessible. Use [`Self::effective_extra_items`] for structural relations that must account
    /// for those hidden items.
    pub(crate) const fn explicit_extra_items(self) -> Option<TypedDictExtraItems<'db>> {
        match self {
            Self::Extra(extra_items) => Some(extra_items),
            Self::ImplicitlyOpen | Self::Closed => None,
        }
    }

    /// Returns the effective extra-items policy.
    ///
    /// An implicitly open `TypedDict` behaves like it has read-only extra items of type `object`
    /// for these purposes, while a closed `TypedDict` has no extra items.
    pub(crate) fn effective_extra_items(self) -> Option<TypedDictExtraItems<'db>> {
        match self {
            Self::ImplicitlyOpen => Some(TypedDictExtraItems {
                declared_ty: Type::object(),
                is_read_only: true,
            }),
            Self::Closed => None,
            Self::Extra(extra_items) => Some(extra_items),
        }
    }

    pub(crate) const fn is_implicitly_open(self) -> bool {
        matches!(self, Self::ImplicitlyOpen)
    }

    pub(crate) const fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }

    fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        match self {
            Self::ImplicitlyOpen | Self::Closed => self,
            Self::Extra(extra_items) => Self::extra(
                db,
                extra_items
                    .declared_ty
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
                extra_items.is_read_only,
            ),
        }
    }

    pub(crate) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        match self {
            Self::ImplicitlyOpen | Self::Closed => Some(self),
            Self::Extra(extra_items) => {
                let declared_ty = extra_items
                    .declared_ty
                    .recursive_type_normalized_impl(db, div, true);
                let declared_ty = if nested {
                    declared_ty?
                } else {
                    declared_ty.unwrap_or(div)
                };
                Some(Self::extra(db, declared_ty, extra_items.is_read_only))
            }
        }
    }
}

/// The value type and mutability of a `TypedDict`'s extra items.
///
/// This represents either an explicit `extra_items` declaration or the synthetic read-only
/// `object` policy returned for an implicitly open `TypedDict` by
/// [`TypedDictOpenness::effective_extra_items`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub struct TypedDictExtraItems<'db> {
    pub(crate) declared_ty: Type<'db>,
    is_read_only: bool,
}

impl TypedDictExtraItems<'_> {
    pub(crate) const fn is_read_only(self) -> bool {
        self.is_read_only
    }
}

pub(super) fn functional_typed_dict_field(
    declared_ty: Type<'_>,
    qualifiers: TypeQualifiers,
    total: bool,
) -> TypedDictField<'_> {
    let required = if qualifiers.contains(TypeQualifiers::REQUIRED) {
        true
    } else if qualifiers.contains(TypeQualifiers::NOT_REQUIRED) {
        false
    } else {
        total
    };

    TypedDictFieldBuilder::new(declared_ty)
        .required(required)
        .read_only(qualifiers.contains(TypeQualifiers::READ_ONLY))
        .build()
}

/// Type that represents the set of all inhabitants (`dict` instances) that conform to
/// a given `TypedDict` schema.
#[derive(Debug, Copy, Clone, PartialEq, Eq, salsa::Update, Hash, get_size2::GetSize)]
pub enum TypedDictType<'db> {
    /// A reference to the class (inheriting from `typing.TypedDict`) that specifies the
    /// schema of this `TypedDict`.
    Class(ClassType<'db>),
    /// A `TypedDict` that doesn't correspond to a class definition, either because it's been
    /// `normalized`, or because it's been synthesized to represent constraints.
    Synthesized(SynthesizedTypedDictType<'db>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub enum SynthesizedTypedDictKind {
    Schema,
    Patch,
}

impl<'db> TypedDictType<'db> {
    pub(crate) fn new(defining_class: ClassType<'db>) -> Self {
        Self::Class(defining_class)
    }

    pub(crate) fn defining_class(self) -> Option<ClassType<'db>> {
        match self {
            Self::Class(defining_class) => Some(defining_class),
            Self::Synthesized(_) => None,
        }
    }

    /// Returns whether this `TypedDict` is implicitly open, closed, or has explicit extra items.
    ///
    /// A class-based `TypedDict` inherits the first explicit policy from its bases unless it
    /// declares its own `closed` or `extra_items` argument.
    pub(crate) fn openness(self, db: &'db dyn Db) -> TypedDictOpenness<'db> {
        #[salsa::tracked(
            cycle_initial=|_, _, _| TypedDictOpenness::ImplicitlyOpen,
            heap_size=ruff_memory_usage::heap_size
        )]
        fn class_based_openness<'db>(
            db: &'db dyn Db,
            class: ClassType<'db>,
        ) -> TypedDictOpenness<'db> {
            let (class_literal, specialization) = class.class_literal_and_specialization(db);
            let static_class = match class_literal {
                ClassLiteral::Static(static_class) => static_class,
                ClassLiteral::DynamicTypedDict(dynamic) => return dynamic.openness(db),
                ClassLiteral::Dynamic(_)
                | ClassLiteral::DynamicNamedTuple(_)
                | ClassLiteral::DynamicEnum(_) => {
                    // A `TypedDictType::Class` is only constructed from classes known to be TypedDicts.
                    unreachable!("non-TypedDict dynamic class wrapped in `TypedDictType`")
                }
            };

            let module = parsed_module(db, static_class.file(db)).load(db);
            let class_definition = static_class.definition(db);
            let class_stmt = class_definition
                .kind(db)
                .as_class()
                .expect("StaticClassLiteral definition should be a class")
                .node(&module);

            if let Some(arguments) = &class_stmt.arguments {
                if let Some(extra_items) = arguments.find_keyword("extra_items") {
                    let annotation =
                        definition_expression_annotation(db, class_definition, &extra_items.value)
                            .map_type(|ty| ty.apply_optional_specialization(db, specialization));
                    return TypedDictOpenness::extra(
                        db,
                        annotation.inner_type(),
                        annotation.qualifiers().contains(TypeQualifiers::READ_ONLY),
                    );
                }

                if let Some(closed) = arguments.find_keyword("closed") {
                    let closed_ty = definition_expression_type(db, class_definition, &closed.value);
                    return if closed_ty.bool(db).is_always_true() {
                        TypedDictOpenness::Closed
                    } else {
                        TypedDictOpenness::ImplicitlyOpen
                    };
                }
            }

            for base in static_class.explicit_bases(db) {
                let base = base.apply_optional_specialization(db, specialization);
                let base_class = match base {
                    Type::ClassLiteral(base) => ClassType::NonGeneric(base),
                    Type::GenericAlias(base) => ClassType::Generic(base),
                    _ => continue,
                };

                if base_class.class_literal(db).is_typed_dict(db) {
                    let openness = TypedDictType::new(base_class).openness(db);
                    if !openness.is_implicitly_open() {
                        return openness;
                    }
                }
            }

            TypedDictOpenness::ImplicitlyOpen
        }

        match self {
            Self::Class(defining_class) => class_based_openness(db, defining_class),
            Self::Synthesized(synthesized) => synthesized.openness(db),
        }
    }

    /// Returns extra items only when they were explicitly declared.
    pub(crate) fn explicit_extra_items(self, db: &'db dyn Db) -> Option<TypedDictExtraItems<'db>> {
        self.openness(db).explicit_extra_items()
    }

    /// Returns a type that contains every value that may be stored in this `TypedDict`.
    ///
    /// An implicitly open `TypedDict` immediately returns `object` because hidden items may have
    /// any value type. This also avoids unnecessarily materializing its declared items.
    pub(crate) fn value_type(self, db: &'db dyn Db) -> Type<'db> {
        let openness = self.openness(db);
        if openness.is_implicitly_open() {
            return Type::object();
        }

        let mut builder = UnionBuilder::new(db);
        for field in self.items(db).values() {
            builder = builder.add(field.declared_ty);
        }
        if let Some(extra_items) = openness.explicit_extra_items() {
            builder = builder.add(extra_items.declared_ty);
        }
        builder.build()
    }

    /// Returns the type of keys that may be present in this `TypedDict`.
    ///
    /// A closed `TypedDict` has a finite set of literal keys. Open and extra-items `TypedDict`s may
    /// contain arbitrary string keys.
    pub(crate) fn key_type(self, db: &'db dyn Db) -> Type<'db> {
        if !self.openness(db).is_closed() {
            return KnownClass::Str.to_instance(db);
        }

        self.items(db)
            .iter()
            .filter(|(_, field)| field.may_be_present(db))
            .fold(UnionBuilder::new(db), |builder, (name, _)| {
                builder.add(Type::string_literal(db, name))
            })
            .build()
    }

    /// Returns the field exposed by a literal key.
    ///
    /// Undeclared keys synthesize a field only for explicit extra items. Hidden items on an
    /// implicitly open `TypedDict` are intentionally not directly accessible.
    pub(crate) fn item(self, db: &'db dyn Db, key: &str) -> Option<TypedDictField<'db>> {
        self.items(db).get(key).cloned().or_else(|| {
            let extra_items = self.explicit_extra_items(db)?;
            Some(
                TypedDictFieldBuilder::new(extra_items.declared_ty)
                    .read_only(extra_items.is_read_only())
                    .build(),
            )
        })
    }

    /// Returns the type that a value must be assignable to when initializing an arbitrary string
    /// key.
    ///
    /// The runtime key may name either an extra item or any declared item, so the result is the
    /// intersection of all possible destination item types. Returns `None` unless extra items are
    /// explicit.
    pub(crate) fn arbitrary_key_initialization_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        self.arbitrary_key_initialization_type_excluding(db, &OrderSet::new())
    }

    /// Returns the arbitrary-key initialization type after excluding keys that are known to be
    /// shadowed.
    ///
    /// This is used while validating merged dictionary literals, where later entries determine the
    /// final value for a known key.
    fn arbitrary_key_initialization_type_excluding(
        self,
        db: &'db dyn Db,
        excluded_keys: &OrderSet<Name>,
    ) -> Option<Type<'db>> {
        let extra_items = self.explicit_extra_items(db)?;

        Some(IntersectionType::from_elements(
            db,
            std::iter::once(extra_items.declared_ty).chain(
                self.items(db)
                    .iter()
                    .filter(|(name, _)| !excluded_keys.contains(*name))
                    .map(|(_, field)| field.declared_ty),
            ),
        ))
    }

    /// Returns the type that a value must be assignable to when mutating an arbitrary string key.
    ///
    /// A mutation may target any declared or extra item, so no such mutation is allowed if any
    /// possible destination is read-only.
    pub(crate) fn arbitrary_key_mutation_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        if self
            .explicit_extra_items(db)
            .is_some_and(TypedDictExtraItems::is_read_only)
            || self.items(db).values().any(TypedDictField::is_read_only)
        {
            return None;
        }

        self.arbitrary_key_initialization_type(db)
    }

    /// Returns whether operations that delete an arbitrary key are safe.
    ///
    /// Operations such as `clear()` and `popitem()` require a closed `TypedDict` or mutable explicit
    /// extra items, and cannot be exposed when any declared item is required or read-only.
    pub(crate) fn supports_arbitrary_key_deletion(self, db: &'db dyn Db) -> bool {
        let openness_supports_deletion = match self.openness(db) {
            TypedDictOpenness::ImplicitlyOpen => false,
            TypedDictOpenness::Closed => true,
            TypedDictOpenness::Extra(extra_items) => !extra_items.is_read_only(),
        };

        openness_supports_deletion
            && self
                .items(db)
                .values()
                .all(|field| !field.is_required() && !field.is_read_only())
    }

    /// Returns the value type if this `TypedDict` is a subtype of `dict[str, VT]`.
    ///
    /// This requires mutable explicit extra items and optional, mutable declared items whose value
    /// types are equivalent to the extra-items type.
    pub(crate) fn dict_value_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let extra_items = self.explicit_extra_items(db)?;
        if extra_items.is_read_only()
            || self.items(db).values().any(|field| {
                field.is_required()
                    || field.is_read_only()
                    || !field
                        .declared_ty
                        .is_equivalent_to(db, extra_items.declared_ty)
            })
        {
            return None;
        }
        Some(extra_items.declared_ty)
    }

    /// Returns the value type if this `TypedDict` is assignable to `dict[str, VT]`.
    ///
    /// This uses mutual assignability rather than equivalence so gradual value types can satisfy
    /// the mutable `dict` contract.
    pub(crate) fn assignable_dict_value_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let extra_items = self.explicit_extra_items(db)?;
        if extra_items.is_read_only()
            || self.items(db).values().any(|field| {
                field.is_required()
                    || field.is_read_only()
                    || !field
                        .declared_ty
                        .is_assignable_to(db, extra_items.declared_ty)
                    || !extra_items
                        .declared_ty
                        .is_assignable_to(db, field.declared_ty)
            })
        {
            return None;
        }
        Some(extra_items.declared_ty)
    }

    pub(crate) fn items(self, db: &'db dyn Db) -> &'db TypedDictSchema<'db> {
        // Field annotations can recursively inspect this schema while the class fields are still
        // being collected, e.g. through `typing.Self` in a `TypedDict` field.
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _| TypedDictSchema::default(),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn class_based_items<'db>(db: &'db dyn Db, class: ClassType<'db>) -> TypedDictSchema<'db> {
            let Some((class_literal, specialization)) = class.static_class_literal(db) else {
                return TypedDictSchema::default();
            };
            class_literal
                .fields(db, specialization, CodeGeneratorKind::TypedDict)
                .into_iter()
                .map(|(name, field)| {
                    let field = match field {
                        Field {
                            first_declaration,
                            declared_ty,
                            kind:
                                FieldKind::TypedDict {
                                    is_required,
                                    is_read_only,
                                },
                        } => TypedDictFieldBuilder::new(*declared_ty)
                            .required(*is_required)
                            .read_only(*is_read_only)
                            .first_declaration(*first_declaration)
                            .build(),
                        _ => unreachable!("TypedDict field expected"),
                    };
                    (name.clone(), field)
                })
                .collect()
        }

        match self {
            Self::Class(defining_class) => {
                // Check if this is a dynamic TypedDict
                if let ClassLiteral::DynamicTypedDict(class) = defining_class.class_literal(db) {
                    return class.items(db);
                }
                class_based_items(db, defining_class)
            }
            Self::Synthesized(synthesized) => synthesized.items(db),
        }
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        // TODO: Materialization of gradual TypedDicts needs more logic
        match self {
            Self::Class(defining_class) => {
                Self::Class(defining_class.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
            Self::Synthesized(synthesized) => Self::Synthesized(
                synthesized.apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            ),
        }
    }

    pub(crate) fn from_schema_items(db: &'db dyn Db, items: TypedDictSchema<'db>) -> Self {
        Self::from_schema_items_with_openness(db, items, TypedDictOpenness::ImplicitlyOpen)
    }

    /// Creates a synthesized schema while preserving its undeclared-item policy.
    pub(crate) fn from_schema_items_with_openness(
        db: &'db dyn Db,
        items: TypedDictSchema<'db>,
        openness: TypedDictOpenness<'db>,
    ) -> Self {
        Self::Synthesized(SynthesizedTypedDictType::schema(db, items, openness))
    }

    fn from_patch_items_with_openness(
        db: &'db dyn Db,
        items: TypedDictSchema<'db>,
        openness: TypedDictOpenness<'db>,
    ) -> Self {
        Self::Synthesized(SynthesizedTypedDictType::patch(db, items, openness))
    }

    /// Returns a partial version of this `TypedDict` where all fields are optional. This is used
    /// to model non-mutating PEP 584 merge operands, accepting dictionary literals that supply any
    /// subset of known keys, and also accepting other `TypedDict`s as long as any overlapping keys
    /// are compatible.
    pub(crate) fn to_partial(self, db: &'db dyn Db) -> Self {
        let items: TypedDictSchema<'db> = self
            .items(db)
            .iter()
            .map(|(name, field)| (name.clone(), field.clone().with_required(false)))
            .collect();

        Self::from_patch_items_with_openness(db, items, self.openness(db))
    }

    /// Returns a patch version of this `TypedDict` for in-place mutations such as `update()` and
    /// `__ior__` (`|=`).
    ///
    /// All fields become optional, and read-only fields become bottom-typed. This preserves the
    /// PEP 705 rule that these operations must reject any source that can write a read-only key,
    /// while still accepting `NotRequired[Never]` placeholders for keys that cannot be present.
    pub(crate) fn to_update_patch(self, db: &'db dyn Db) -> Self {
        let items: TypedDictSchema<'db> = self
            .items(db)
            .iter()
            .map(|(name, field)| {
                let mut field = field.clone().with_required(false);
                if field.is_read_only() {
                    field.declared_ty = Type::Never;
                }
                (name.clone(), field)
            })
            .collect();

        let openness = match self.openness(db) {
            TypedDictOpenness::ImplicitlyOpen => TypedDictOpenness::ImplicitlyOpen,
            TypedDictOpenness::Extra(extra_items) if !extra_items.is_read_only() => {
                TypedDictOpenness::Extra(extra_items)
            }
            TypedDictOpenness::Closed | TypedDictOpenness::Extra(_) => TypedDictOpenness::Closed,
        };

        Self::from_patch_items_with_openness(db, items, openness)
    }

    pub fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self {
            TypedDictType::Class(defining_class) => defining_class.definition(db),
            TypedDictType::Synthesized(_) => None,
        }
    }

    pub fn type_definition(self, db: &'db dyn Db) -> Option<TypeDefinition<'db>> {
        match self {
            TypedDictType::Class(defining_class) => defining_class.type_definition(db),
            TypedDictType::Synthesized(_) => None,
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    // Subtyping between `TypedDict`s follows the algorithm described at:
    // https://typing.python.org/en/latest/spec/typeddict.html#subtyping-between-typeddict-types
    pub(super) fn check_typeddict_pair(
        &self,
        db: &'db dyn Db,
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if let TypedDictType::Synthesized(synthesized_target) = target
            && synthesized_target.is_patch(db)
        {
            let source_items = source.items(db);
            let target_items = synthesized_target.items(db);
            let target_openness = synthesized_target.openness(db);
            let mut result = self.always();

            for (source_item_name, source_item_field) in source_items {
                let target_ty = if let Some(target_item_field) = target_items.get(source_item_name)
                {
                    target_item_field.declared_ty
                } else {
                    match target_openness {
                        TypedDictOpenness::ImplicitlyOpen => continue,
                        TypedDictOpenness::Closed => return self.never(),
                        TypedDictOpenness::Extra(extra_items) => extra_items.declared_ty,
                    }
                };

                result.intersect(
                    db,
                    self.constraints,
                    self.check_type_pair(db, source_item_field.declared_ty, target_ty),
                );

                if result.is_never_satisfied(db) {
                    return result;
                }
            }

            let source_extra_items = if target_openness.is_implicitly_open() {
                source.explicit_extra_items(db)
            } else {
                source.openness(db).effective_extra_items()
            };
            if let Some(source_extra_items) = source_extra_items {
                for (target_item_name, target_item_field) in target_items {
                    if source_items.contains_key(target_item_name) {
                        continue;
                    }
                    result.intersect(
                        db,
                        self.constraints,
                        self.check_type_pair(
                            db,
                            source_extra_items.declared_ty,
                            target_item_field.declared_ty,
                        ),
                    );
                    if result.is_never_satisfied(db) {
                        return result;
                    }
                }

                match target_openness {
                    TypedDictOpenness::ImplicitlyOpen => {}
                    TypedDictOpenness::Closed => return self.never(),
                    TypedDictOpenness::Extra(target_extra_items) => {
                        result.intersect(
                            db,
                            self.constraints,
                            self.check_type_pair(
                                db,
                                source_extra_items.declared_ty,
                                target_extra_items.declared_ty,
                            ),
                        );
                    }
                }
            }

            return result;
        }

        // First do a quick nominal check that (if it succeeds) means that we can avoid
        // materializing the full `TypedDict` schema for either `source` or `target`.
        // This should be cheaper in many cases, and also helps us avoid some cycles.
        if let Some(defining_class) = source.defining_class()
            && let Some(target_defining_class) = target.defining_class()
            && defining_class.is_subclass_of(db, target_defining_class)
        {
            return self.always();
        }

        let source_items = source.items(db);
        let target_items = target.items(db);
        let source_openness = source.openness(db);
        let target_openness = target.openness(db);
        // Many rules violations short-circuit with "never", but asking whether one field is
        // [relation] to/of another can produce more complicated constraints, and we collect those.
        let mut result = self.always();
        for (target_item_name, target_item_field) in target_items {
            let field_constraints = if target_item_field.is_required() {
                // required target fields
                let Some(source_item_field) = source_items.get(target_item_name) else {
                    // Self is missing a required field.
                    self.provide_context(|| ErrorContext::TypedDictFieldMissing {
                        field_name: target_item_name.clone(),
                        source,
                    });
                    return self.never();
                };
                if !source_item_field.is_required() {
                    // A required field is not required in self.
                    self.provide_context(|| ErrorContext::TypedDictFieldNotRequiredInSource {
                        field_name: target_item_name.clone(),
                        source,
                        target,
                    });
                    return self.never();
                }
                if target_item_field.is_read_only() {
                    // For `ReadOnly[]` fields in the target, the corresponding fields in
                    // self need to have the same assignability/subtyping/etc relation
                    // individually that we're looking for overall between the
                    // `TypedDict`s.
                    self.check_type_pair(
                        db,
                        source_item_field.declared_ty,
                        target_item_field.declared_ty,
                    )
                } else {
                    if source_item_field.is_read_only() {
                        // A read-only field can't be assigned to a mutable target.
                        self.provide_context(|| ErrorContext::TypedDictFieldReadOnlyInSource {
                            field_name: target_item_name.clone(),
                            source,
                            target,
                        });
                        return self.never();
                    }
                    // For mutable fields in the target, the relation needs to apply both
                    // ways, or else mutating the target could violate the structural
                    // invariants of self. For fully-static types, this is "equivalence".
                    // For gradual types, it depends on the relation, but mutual
                    // assignability is "consistency".
                    self.check_type_pair(
                        db,
                        source_item_field.declared_ty,
                        target_item_field.declared_ty,
                    )
                    .and(db, self.constraints, || {
                        self.check_type_pair(
                            db,
                            target_item_field.declared_ty,
                            source_item_field.declared_ty,
                        )
                    })
                }
            } else {
                // `NotRequired[]` target fields
                if target_item_field.is_read_only() {
                    // A missing read-only field is checked against the source's effective extra
                    // items. Missing mutable fields below require explicit mutable extra items and
                    // a relation in both directions.
                    if let Some(source_item_field) = source_items.get(target_item_name) {
                        self.check_type_pair(
                            db,
                            source_item_field.declared_ty,
                            target_item_field.declared_ty,
                        )
                    } else {
                        match source_openness.effective_extra_items() {
                            // A closed source cannot contain this key, so the check succeeds.
                            None => self.always(),
                            Some(source_extra_items) => self.check_type_pair(
                                db,
                                source_extra_items.declared_ty,
                                target_item_field.declared_ty,
                            ),
                        }
                    }
                } else {
                    if let Some(source_item_field) = source_items.get(target_item_name) {
                        if source_item_field.is_read_only() {
                            // A read-only field can't be assigned to a mutable target.
                            self.provide_context(|| ErrorContext::TypedDictFieldReadOnlyInSource {
                                field_name: target_item_name.clone(),
                                source,
                                target,
                            });
                            return self.never();
                        }
                        if source_item_field.is_required() {
                            // A required field can't be assigned to a not-required, mutable field
                            // in the target, because `del` is allowed on the target field.
                            self.provide_context(|| {
                                ErrorContext::TypedDictFieldNotRequiredAndMutableInTarget {
                                    field_name: target_item_name.clone(),
                                    source,
                                    target,
                                }
                            });
                            return self.never();
                        }

                        // As above, for mutable fields in the target, the relation needs
                        // to apply both ways.
                        self.check_type_pair(
                            db,
                            source_item_field.declared_ty,
                            target_item_field.declared_ty,
                        )
                        .and(db, self.constraints, || {
                            self.check_type_pair(
                                db,
                                target_item_field.declared_ty,
                                source_item_field.declared_ty,
                            )
                        })
                    } else {
                        let Some(source_extra_items) = source_openness.explicit_extra_items()
                        else {
                            return self.never();
                        };
                        if source_extra_items.is_read_only() {
                            return self.never();
                        }
                        self.check_type_pair(
                            db,
                            source_extra_items.declared_ty,
                            target_item_field.declared_ty,
                        )
                        .and(db, self.constraints, || {
                            self.check_type_pair(
                                db,
                                target_item_field.declared_ty,
                                source_extra_items.declared_ty,
                            )
                        })
                    }
                }
            };
            result.intersect(db, self.constraints, field_constraints);
            if result.is_never_satisfied(db) {
                if let Some(source_item_field) = source_items.get(target_item_name) {
                    self.provide_context(|| ErrorContext::TypedDictFieldIncompatible {
                        field_name: target_item_name.clone(),
                        source,
                        target,
                        source_field: source_item_field.declared_ty,
                        target_field: target_item_field.declared_ty,
                    });
                }
                return result;
            }
        }

        match target_openness {
            TypedDictOpenness::Closed => {
                if !source_openness.is_closed()
                    || source_items
                        .keys()
                        .any(|source_item_name| !target_items.contains_key(source_item_name))
                {
                    return self.never();
                }
            }
            TypedDictOpenness::Extra(target_extra_items) if !target_extra_items.is_read_only() => {
                let Some(source_extra_items) = source_openness.explicit_extra_items() else {
                    return self.never();
                };
                if source_extra_items.is_read_only() {
                    return self.never();
                }
                result.intersect(
                    db,
                    self.constraints,
                    self.check_type_pair(
                        db,
                        source_extra_items.declared_ty,
                        target_extra_items.declared_ty,
                    )
                    .and(db, self.constraints, || {
                        self.check_type_pair(
                            db,
                            target_extra_items.declared_ty,
                            source_extra_items.declared_ty,
                        )
                    }),
                );
                for (source_item_name, source_item_field) in source_items {
                    if !target_items.contains_key(source_item_name) {
                        if source_item_field.is_required() || source_item_field.is_read_only() {
                            return self.never();
                        }
                        result.intersect(
                            db,
                            self.constraints,
                            self.check_type_pair(
                                db,
                                source_item_field.declared_ty,
                                target_extra_items.declared_ty,
                            )
                            .and(db, self.constraints, || {
                                self.check_type_pair(
                                    db,
                                    target_extra_items.declared_ty,
                                    source_item_field.declared_ty,
                                )
                            }),
                        );
                    }
                }
            }
            TypedDictOpenness::ImplicitlyOpen | TypedDictOpenness::Extra(_) => {
                // An open target shares the read-only extra-items path, using `object` as its
                // effective extra-items type.
                let target_extra_items = target_openness.effective_extra_items().expect(
                    "open or read-only extra-items TypedDict should have effective extra items",
                );
                if let Some(source_extra_items) = source_openness.effective_extra_items() {
                    result.intersect(
                        db,
                        self.constraints,
                        self.check_type_pair(
                            db,
                            source_extra_items.declared_ty,
                            target_extra_items.declared_ty,
                        ),
                    );
                }
                for (source_item_name, source_item_field) in source_items {
                    if !target_items.contains_key(source_item_name) {
                        result.intersect(
                            db,
                            self.constraints,
                            self.check_type_pair(
                                db,
                                source_item_field.declared_ty,
                                target_extra_items.declared_ty,
                            ),
                        );
                    }
                }
            }
        }

        result
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    /// Two `TypedDict`s `A` and `B` are disjoint if it's impossible to come up with a third
    /// `TypedDict` `C` that's fully-static and assignable to both of them.
    ///
    /// `TypedDict` assignability is determined field-by-field, so we determine disjointness
    /// similarly. Fields that are present in both `A` and `B` can conflict directly. A required or
    /// mutable optional field that's only in one side can also conflict with the other side's
    /// openness: a closed `TypedDict` rejects it, and explicit extra items constrain whether it can
    /// be present.
    ///
    /// There are three properties of each field to consider: the declared type, whether it's
    /// mutable ("mut" vs "imm" below), and whether it's required ("req" vs "opt" below). Here's a
    /// table summary of the restrictions on the declared type of a source field (for us that means
    /// in `C`, which we want to be assignable to both `A` and `B`) given a destination field (for
    /// us that means in either `A` or `B`). For completeness we'll also include the possibility
    /// that the source field is missing entirely, though we'll soon see that we can ignore that
    /// case. This table is essentially what [`TypeRelationChecker::check_typeddict_pair`] implements
    /// above. Here "equivalent" means the source and destination types must be equivalent/compatible,
    /// "assignable" means the source must be assignable to the destination, and "-" means the
    /// assignment is never allowed:
    ///
    /// | dest ↓ source →  | mut + req  | mut + opt  | imm + req  | imm + opt  |   \[missing]  |
    /// |------------------|------------|------------|------------|------------|---------------|
    /// |    mut + req     | equivalent |     -      |     -      |     -      |       -       |
    /// |    mut + opt     |     -      | equivalent |     -      |     -      |       -       |
    /// |    imm + req     | assignable |     -      | assignable |     -      |       -       |
    /// |    imm + opt     | assignable | assignable | assignable | assignable | \[dest is obj]|
    ///
    /// We can cut that table down substantially by noticing two things:
    ///
    /// - We don't need to consider the cases where the source field (in `C`) is `ReadOnly`/"imm",
    ///   because the mutable version of the same field is always "strictly more assignable". In
    ///   other words, nothing in the `TypedDict` assignability rules ever requires a source field
    ///   to be immutable.
    /// - We don't need to consider the special case where the source field is missing, because
    ///   that's only allowed when the destination is `ReadOnly[NotRequired[object]]`, which is
    ///   compatible with *any* choice of source field.
    ///
    /// The cases we actually need to reason about are this smaller table:
    ///
    /// | dest ↓ source →  | mut + req  | mut + opt  |
    /// |------------------|------------|------------|
    /// |    mut + req     | equivalent |     -      |
    /// |    mut + opt     |     -      | equivalent |
    /// |    imm + req     | assignable |     -      |
    /// |    imm + opt     | assignable | assignable |
    ///
    /// So, given a field name that's in both `A` and `B`, here are the conditions where it's
    /// *impossible* to choose a source field for `C` that's compatible with both destinations,
    /// which tells us that `A` and `B` are disjoint:
    ///
    /// 1. If one side is "mut+opt" (which forces the field in `C` to be "opt") and the other side
    ///    is "req" (which forces the field in `C` to be "req").
    /// 2. If both sides are mutable, and their types are not equivalent/compatible. (Because the
    ///    type in `C` must be compatible with both of them.)
    /// 3. If one sides is mutable, and its type is not assignable to the immutable side's type.
    ///    (Because the type in `C` must be compatible with the mutable side.)
    /// 4. If both sides are immutable, and their types are disjoint. (Because the type in `C` must
    ///    be assignable to both.)
    ///
    pub(super) fn check_typeddict_pair(
        &self,
        db: &'db dyn Db,
        left: TypedDictType<'db>,
        right: TypedDictType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let left_items = left.items(db);
        let right_items = right.items(db);
        let fields_in_common = btreemap_values_with_same_key(left_items, right_items);
        let common_fields_disjoint =
            fields_in_common.when_any(db, self.constraints, |(left_field, right_field)| {
                // Condition 1 above.
                if left_field.is_required() || right_field.is_required() {
                    if (!left_field.is_required() && !left_field.is_read_only())
                        || (!right_field.is_required() && !right_field.is_read_only())
                    {
                        // One side demands a `Required` source field, while the other side demands a
                        // `NotRequired` one. They must be disjoint.
                        return self.always();
                    }
                }
                if !left_field.is_read_only() && !right_field.is_read_only() {
                    // Condition 2 above. This field is mutable on both sides, so the so the types must
                    // be compatible, i.e. mutually assignable.
                    let relation_checker = self.as_relation_checker(TypeRelation::Assignability);
                    relation_checker
                        .check_type_pair(db, left_field.declared_ty, right_field.declared_ty)
                        .and(db, self.constraints, || {
                            relation_checker.check_type_pair(
                                db,
                                right_field.declared_ty,
                                left_field.declared_ty,
                            )
                        })
                        .negate(db, self.constraints)
                } else if !left_field.is_read_only() {
                    // Half of condition 3 above.
                    self.as_relation_checker(TypeRelation::Assignability)
                        .check_type_pair(db, left_field.declared_ty, right_field.declared_ty)
                        .negate(db, self.constraints)
                } else if !right_field.is_read_only() {
                    // The other half of condition 3 above.
                    self.as_relation_checker(TypeRelation::Assignability)
                        .check_type_pair(db, right_field.declared_ty, left_field.declared_ty)
                        .negate(db, self.constraints)
                } else {
                    // Condition 4 above.
                    self.check_type_pair(db, left_field.declared_ty, right_field.declared_ty)
                }
            });

        let required_fields_disjoint = common_fields_disjoint.or(db, self.constraints, || {
            left_items
                .iter()
                .filter(|(name, field)| field.is_required() && !right_items.contains_key(*name))
                .map(|(_, field)| (field, right.openness(db)))
                .chain(
                    right_items
                        .iter()
                        .filter(|(name, field)| {
                            field.is_required() && !left_items.contains_key(*name)
                        })
                        .map(|(_, field)| (field, left.openness(db))),
                )
                .when_any(db, self.constraints, |(required_field, other_openness)| {
                    let check_read_only_extra_items = |extra_items_ty| {
                        if required_field.is_read_only() {
                            self.check_type_pair(db, required_field.declared_ty, extra_items_ty)
                        } else {
                            self.as_relation_checker(TypeRelation::Assignability)
                                .check_type_pair(db, required_field.declared_ty, extra_items_ty)
                                .negate(db, self.constraints)
                        }
                    };

                    match other_openness {
                        TypedDictOpenness::Closed => self.always(),
                        TypedDictOpenness::Extra(extra_items) if !extra_items.is_read_only() => {
                            self.always()
                        }
                        TypedDictOpenness::ImplicitlyOpen => {
                            check_read_only_extra_items(Type::object())
                        }
                        TypedDictOpenness::Extra(extra_items) => {
                            check_read_only_extra_items(extra_items.declared_ty)
                        }
                    }
                })
        });

        let unshared_fields_disjoint = required_fields_disjoint.or(db, self.constraints, || {
            left_items
                .iter()
                .map(|(name, field)| (name, field, right_items, right.openness(db)))
                .chain(
                    right_items
                        .iter()
                        .map(|(name, field)| (name, field, left_items, left.openness(db))),
                )
                .filter_map(|(name, field, other_items, other_openness)| {
                    if field.is_required() || other_items.contains_key(name) {
                        return None;
                    }
                    match other_openness {
                        TypedDictOpenness::Closed if !field.is_read_only() => Some((field, None)),
                        TypedDictOpenness::Extra(extra_items)
                            if !field.is_read_only() || !extra_items.is_read_only() =>
                        {
                            Some((field, Some(extra_items)))
                        }
                        TypedDictOpenness::ImplicitlyOpen
                        | TypedDictOpenness::Closed
                        | TypedDictOpenness::Extra(_) => None,
                    }
                })
                .when_any(db, self.constraints, |(field, extra_items)| {
                    let Some(extra_items) = extra_items else {
                        return self.always();
                    };
                    let relation_checker = self.as_relation_checker(TypeRelation::Assignability);
                    if field.is_read_only() {
                        relation_checker
                            .check_type_pair(db, extra_items.declared_ty, field.declared_ty)
                            .negate(db, self.constraints)
                    } else if extra_items.is_read_only() {
                        relation_checker
                            .check_type_pair(db, field.declared_ty, extra_items.declared_ty)
                            .negate(db, self.constraints)
                    } else {
                        relation_checker
                            .check_type_pair(db, field.declared_ty, extra_items.declared_ty)
                            .and(db, self.constraints, || {
                                relation_checker.check_type_pair(
                                    db,
                                    extra_items.declared_ty,
                                    field.declared_ty,
                                )
                            })
                            .negate(db, self.constraints)
                    }
                })
        });

        unshared_fields_disjoint.or(db, self.constraints, || {
            let left_openness = left.openness(db);
            let right_openness = right.openness(db);
            let relation_checker = self.as_relation_checker(TypeRelation::Assignability);

            match (left_openness, right_openness) {
                (TypedDictOpenness::Closed, TypedDictOpenness::Extra(extra_items))
                | (TypedDictOpenness::Extra(extra_items), TypedDictOpenness::Closed)
                    if !extra_items.is_read_only() =>
                {
                    self.always()
                }
                (TypedDictOpenness::Extra(left_extra), TypedDictOpenness::Extra(right_extra))
                    if !left_extra.is_read_only() && !right_extra.is_read_only() =>
                {
                    relation_checker
                        .check_type_pair(db, left_extra.declared_ty, right_extra.declared_ty)
                        .and(db, self.constraints, || {
                            relation_checker.check_type_pair(
                                db,
                                right_extra.declared_ty,
                                left_extra.declared_ty,
                            )
                        })
                        .negate(db, self.constraints)
                }
                (TypedDictOpenness::Extra(mutable_extra), other)
                | (other, TypedDictOpenness::Extra(mutable_extra))
                    if !mutable_extra.is_read_only() =>
                {
                    other.effective_extra_items().map_or_else(
                        || self.always(),
                        |other_extra| {
                            relation_checker
                                .check_type_pair(
                                    db,
                                    mutable_extra.declared_ty,
                                    other_extra.declared_ty,
                                )
                                .negate(db, self.constraints)
                        },
                    )
                }
                _ => self.never(),
            }
        })
    }
}

pub(crate) fn walk_typed_dict_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    visitor: &V,
) {
    match typed_dict {
        TypedDictType::Class(defining_class) => {
            visitor.visit_type(db, defining_class.into());
        }
        TypedDictType::Synthesized(synthesized) => {
            for field in synthesized.items(db).values() {
                visitor.visit_type(db, field.declared_ty);
            }
            if let Some(extra_items) = synthesized.openness(db).explicit_extra_items() {
                visitor.visit_type(db, extra_items.declared_ty);
            }
        }
    }
}

#[salsa::tracked(
    returns(ref),
    cycle_initial = |_, _, _|TypedDictSchema::default(),
    heap_size = ruff_memory_usage::heap_size
)]
pub(super) fn deferred_functional_typed_dict_schema<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypedDictSchema<'db> {
    let module = parsed_module(db, definition.file(db)).load(db);
    let node = definition
        .kind(db)
        .value(&module)
        .expect("Expected `TypedDict` definition to be an assignment")
        .as_call_expr()
        .expect("Expected `TypedDict` definition r.h.s. to be a call expression");

    let deferred_inference = infer_deferred_types(db, definition);

    let total = node.arguments.find_keyword("total").is_none_or(|total_kw| {
        let total_ty = definition_expression_type(db, definition, &total_kw.value);
        !total_ty.bool(db).is_always_false()
    });

    let mut schema = TypedDictSchema::default();

    if let Some(fields_arg) = node.arguments.args.get(1) {
        let ast::Expr::Dict(dict_expr) = fields_arg else {
            return schema;
        };

        for item in &dict_expr.items {
            let Some(key) = &item.key else {
                return TypedDictSchema::default();
            };

            let key_ty = definition_expression_type(db, definition, key);
            let Some(key_lit) = key_ty.as_string_literal() else {
                return TypedDictSchema::default();
            };

            let field_ty = deferred_inference.expression_type(&item.value);
            let qualifiers = deferred_inference.qualifiers(&item.value);

            schema.insert(
                Name::new(key_lit.value(db)),
                functional_typed_dict_field(field_ty, qualifiers, total),
            );
        }
    }

    schema
}

/// Resolves the undeclared-item policy of a functional `TypedDict` definition.
///
/// The `extra_items` expression is inferred as an annotation so qualifiers such as `ReadOnly` are
/// preserved. `extra_items=Never` is canonicalized to the same closed policy as `closed=True`.
///
/// ```python
/// Movie = TypedDict("Movie", {"name": str}, extra_items=ReadOnly[int])
/// ```
#[salsa::tracked(
    cycle_initial = |_, _, _| TypedDictOpenness::ImplicitlyOpen,
    heap_size = ruff_memory_usage::heap_size
)]
pub(super) fn deferred_functional_typed_dict_openness<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
) -> TypedDictOpenness<'db> {
    let module = parsed_module(db, definition.file(db)).load(db);
    let node = definition
        .kind(db)
        .value(&module)
        .expect("Expected `TypedDict` definition to be an assignment")
        .as_call_expr()
        .expect("Expected `TypedDict` definition r.h.s. to be a call expression");

    if let Some(extra_items) = node.arguments.find_keyword("extra_items") {
        let deferred_inference = infer_deferred_types(db, definition);
        return TypedDictOpenness::extra(
            db,
            deferred_inference.expression_type(&extra_items.value),
            deferred_inference
                .qualifiers(&extra_items.value)
                .contains(TypeQualifiers::READ_ONLY),
        );
    }

    if let Some(closed) = node.arguments.find_keyword("closed") {
        let closed_ty = definition_expression_type(db, definition, &closed.value);
        if closed_ty.bool(db).is_always_true() {
            return TypedDictOpenness::Closed;
        }
    }

    TypedDictOpenness::ImplicitlyOpen
}

pub(super) fn typed_dict_params_from_class_def(class_stmt: &StmtClassDef) -> TypedDictParams {
    let mut typed_dict_params = TypedDictParams::default();

    // Check for `total` keyword argument in the class definition
    // Note that it is fine to only check for Boolean literals here
    // (https://typing.python.org/en/latest/spec/typeddict.html#totality)
    if let Some(arguments) = &class_stmt.arguments {
        for keyword in &arguments.keywords {
            if keyword.arg.as_deref() == Some("total")
                && matches!(
                    &keyword.value,
                    ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: false, .. })
                )
            {
                typed_dict_params.remove(TypedDictParams::TOTAL);
            }
        }
    }

    typed_dict_params
}

#[derive(Debug, Clone, Copy)]
pub(super) enum TypedDictAssignmentKind {
    /// For subscript assignments like `d["key"] = value`
    Subscript,
    /// For constructor arguments like `MyTypedDict(key=value)`
    Constructor,
}

impl TypedDictAssignmentKind {
    fn diagnostic_name(self) -> &'static str {
        match self {
            Self::Subscript => "assignment",
            Self::Constructor => "argument",
        }
    }

    fn diagnostic_type(self) -> &'static crate::lint::LintMetadata {
        match self {
            Self::Subscript => &INVALID_ASSIGNMENT,
            Self::Constructor => &INVALID_ARGUMENT_TYPE,
        }
    }

    const fn is_subscript(self) -> bool {
        matches!(self, Self::Subscript)
    }
}

/// A helper that validates assignments of a value to a specific key on a `TypedDict`.
pub(super) struct TypedDictKeyAssignment<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) typed_dict: TypedDictType<'db>,
    pub(super) full_object_ty: Option<Type<'db>>,
    pub(super) key: &'a str,
    pub(super) value_ty: Type<'db>,
    pub(super) typed_dict_node: AnyNodeRef<'ast>,
    pub(super) key_node: AnyNodeRef<'ast>,
    pub(super) value_node: AnyNodeRef<'ast>,
    pub(super) assignment_kind: TypedDictAssignmentKind,
    pub(super) emit_diagnostic: bool,
}

impl<'db> TypedDictKeyAssignment<'_, 'db, '_> {
    pub(super) fn validate(&self) -> bool {
        let db = self.context.db();
        let items = self.typed_dict.items(db);

        // Check if key exists in `TypedDict` or is accepted by explicit extra items.
        let Some(item) = self.typed_dict.item(db, self.key) else {
            if self.emit_diagnostic {
                report_invalid_key_on_typed_dict(
                    self.context,
                    self.typed_dict_node,
                    self.key_node,
                    Type::TypedDict(self.typed_dict),
                    self.full_object_ty,
                    Type::string_literal(db, self.key),
                    items,
                );
            }

            return false;
        };

        if self.assignment_kind.is_subscript() && item.is_read_only() {
            if self.emit_diagnostic
                && let Some(builder) = self
                    .context
                    .report_lint(self.assignment_kind.diagnostic_type(), self.key_node)
            {
                let typed_dict_ty = Type::TypedDict(self.typed_dict);
                let typed_dict_d = typed_dict_ty.display(db);

                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot assign to key \"{}\" on TypedDict `{typed_dict_d}`",
                    self.key,
                ));

                diagnostic.set_primary_message(format_args!("key is marked read-only"));
                self.add_object_type_annotation(db, &mut diagnostic);
                Self::add_item_definition_subdiagnostic(
                    db,
                    &item,
                    &mut diagnostic,
                    "Read-only item declared here",
                );
            }

            return false;
        }

        // Key exists, check if value type is assignable to declared type
        if self.value_ty.is_assignable_to(db, item.declared_ty) {
            return true;
        }

        if diagnostic::is_invalid_typed_dict_literal(db, item.declared_ty, self.value_node) {
            return false;
        }

        // Invalid assignment - emit diagnostic
        if self.emit_diagnostic
            && let Some(builder) = self
                .context
                .report_lint(self.assignment_kind.diagnostic_type(), self.value_node)
        {
            let typed_dict_ty = Type::TypedDict(self.typed_dict);
            let typed_dict_d = typed_dict_ty.display(db);
            let value_d = self.value_ty.display(db);
            let item_type_d = item.declared_ty.display(db);

            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid {} to key \"{}\" with declared type `{item_type_d}` \
                on TypedDict `{typed_dict_d}`",
                self.assignment_kind.diagnostic_name(),
                self.key,
            ));

            diagnostic.set_primary_message(format_args!("value of type `{value_d}`"));

            diagnostic.annotate(
                self.context
                    .secondary(self.key_node)
                    .message(format_args!("key has declared type `{item_type_d}`")),
            );

            Self::add_item_definition_subdiagnostic(
                db,
                &item,
                &mut diagnostic,
                "Item declared here",
            );
            self.add_object_type_annotation(db, &mut diagnostic);
        }

        false
    }

    fn add_object_type_annotation(&self, db: &'db dyn Db, diagnostic: &mut Diagnostic) {
        if let Some(full_object_ty) = self.full_object_ty {
            diagnostic.annotate(self.context.secondary(self.typed_dict_node).message(
                format_args!(
                    "TypedDict `{}` in {kind} type `{}`",
                    Type::TypedDict(self.typed_dict).display(db),
                    full_object_ty.display(db),
                    kind = if full_object_ty.is_union() {
                        "union"
                    } else {
                        "intersection"
                    },
                ),
            ));
        } else {
            diagnostic.annotate(self.context.secondary(self.typed_dict_node).message(
                format_args!(
                    "TypedDict `{}`",
                    Type::TypedDict(self.typed_dict).display(db)
                ),
            ));
        }
    }

    fn add_item_definition_subdiagnostic(
        db: &'db dyn Db,
        item: &TypedDictField<'db>,
        diagnostic: &mut Diagnostic,
        message: &str,
    ) {
        if let Some(declaration) = item.first_declaration() {
            let file = declaration.file(db);
            let module = parsed_module(db, file).load(db);

            let mut sub = SubDiagnostic::new(SubDiagnosticSeverity::Info, "Item declaration");
            sub.annotate(
                Annotation::secondary(
                    Span::from(file).with_range(declaration.full_range(db, &module).range()),
                )
                .message(message),
            );
            diagnostic.sub(sub);
        }
    }
}

/// Validates that all required keys are provided in a `TypedDict` construction.
///
/// Reports errors for any keys that are required but not provided.
///
/// Returns true if the assignment is valid, or false otherwise.
pub(super) fn validate_typed_dict_required_keys<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    provided_keys: &OrderSet<Name>,
    error_node: AnyNodeRef<'ast>,
) -> bool {
    let db = context.db();
    let items = typed_dict.items(db);

    let required_keys: OrderSet<Name> = items
        .iter()
        .filter_map(|(key_name, field)| field.is_required().then_some(key_name.clone()))
        .collect();

    let missing_keys = required_keys.difference(provided_keys);

    let mut has_missing_key = false;
    for missing_key in missing_keys {
        has_missing_key = true;

        report_missing_typed_dict_key(
            context,
            error_node,
            Type::TypedDict(typed_dict),
            missing_key.as_str(),
        );
    }

    !has_missing_key
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UnpackedTypedDictKey<'db> {
    pub(crate) value_ty: Type<'db>,
    pub(crate) is_required: bool,
    pub(crate) definition: Option<Definition<'db>>,
}

/// A normalized view of a `TypedDict`-shaped value used when unpacking it.
///
/// Union and intersection inputs are combined into one set of possible keys and one openness
/// policy describing arbitrary undeclared keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnpackedTypedDict<'db> {
    /// Declared keys that may be present after unpacking.
    pub(crate) keys: BTreeMap<Name, UnpackedTypedDictKey<'db>>,
    pub(crate) openness: TypedDictOpenness<'db>,
}

/// Combines the openness policies of intersected `TypedDict`-shaped values.
///
/// An intersection must satisfy every constituent, so `Closed` dominates, `ImplicitlyOpen` adds no
/// constraint, and explicit extra-item value types are intersected. Conceptually,
/// `ImplicitlyOpen & Extra[int]` becomes a read-only `Extra[int]`, while
/// `Closed & Extra[int]` is `Closed`.
///
/// The combined explicit policy can be read-only because unpacking observes extra items but never
/// writes through the synthesized policy.
fn intersect_unpacked_typed_dict_openness<'db>(
    db: &'db dyn Db,
    openness: impl IntoIterator<Item = TypedDictOpenness<'db>>,
) -> TypedDictOpenness<'db> {
    let mut explicit_value_types = Vec::new();

    for openness in openness {
        match openness {
            TypedDictOpenness::Closed => return TypedDictOpenness::Closed,
            TypedDictOpenness::ImplicitlyOpen => {}
            TypedDictOpenness::Extra(extra_items) => {
                explicit_value_types.push(extra_items.declared_ty);
            }
        }
    }

    if explicit_value_types.is_empty() {
        TypedDictOpenness::ImplicitlyOpen
    } else {
        TypedDictOpenness::extra(
            db,
            IntersectionType::from_elements(db, explicit_value_types),
            true,
        )
    }
}

/// Combines the openness policies of unioned `TypedDict`-shaped values.
///
/// A union may contain extra items from any constituent, so `Closed` contributes no values and
/// explicit extra-item value types are unioned. An `ImplicitlyOpen` constituent widens explicit
/// extra items to `object` while preserving explicit-extra-item behavior: conceptually,
/// `ImplicitlyOpen | Extra[int]` becomes a read-only `Extra[object]`. With no explicit extra items,
/// any implicitly open constituent makes the result `ImplicitlyOpen`; otherwise the result is
/// `Closed`.
///
/// As with intersections, a synthesized explicit policy is read-only because unpacking only
/// observes its values.
fn union_unpacked_typed_dict_openness<'db>(
    db: &'db dyn Db,
    openness: impl IntoIterator<Item = TypedDictOpenness<'db>>,
) -> TypedDictOpenness<'db> {
    let mut value_types = UnionBuilder::new(db);
    let mut has_implicitly_open = false;
    let mut has_explicit_extra_items = false;

    for openness in openness {
        match openness {
            TypedDictOpenness::Closed => {}
            TypedDictOpenness::ImplicitlyOpen => has_implicitly_open = true,
            TypedDictOpenness::Extra(extra_items) => {
                value_types = value_types.add(extra_items.declared_ty);
                has_explicit_extra_items = true;
            }
        }
    }

    if has_implicitly_open && has_explicit_extra_items {
        TypedDictOpenness::extra(db, Type::object(), true)
    } else if has_implicitly_open {
        TypedDictOpenness::ImplicitlyOpen
    } else if has_explicit_extra_items {
        TypedDictOpenness::extra(db, value_types.build(), true)
    } else {
        TypedDictOpenness::Closed
    }
}

/// Extracts `TypedDict` keys, their value types, and whether they are required when an unpacked
/// `**kwargs` value has this type, resolving type aliases and handling intersections and unions.
///
/// For intersections, returns ALL declared keys from ALL `TypedDict` types (union of keys),
/// because unpacking a value of an intersection type may expose any key declared by any
/// constituent `TypedDict`. For keys that appear in multiple `TypedDict`s, the value types are
/// intersected, and the key is considered required if any constituent `TypedDict` requires it.
/// For unions, returns all keys that may appear in any arm, unioning value types for shared keys,
/// and a key is only considered required if every arm requires it.
pub(crate) fn extract_unpacked_typed_dict_keys_from_value_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<BTreeMap<Name, UnpackedTypedDictKey<'db>>> {
    extract_unpacked_typed_dict_from_value_type(db, ty).map(|unpacked| unpacked.keys)
}

/// Extracts the declared keys and openness from a `TypedDict`-shaped value.
pub(crate) fn extract_unpacked_typed_dict_from_value_type<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<UnpackedTypedDict<'db>> {
    match ty {
        Type::TypedDict(td) => {
            let keys = td
                .items(db)
                .iter()
                .map(|(name, field)| {
                    (
                        name.clone(),
                        UnpackedTypedDictKey {
                            value_ty: field.declared_ty,
                            is_required: field.is_required(),
                            definition: field.first_declaration(),
                        },
                    )
                })
                .collect();
            Some(UnpackedTypedDict {
                keys,
                openness: td.openness(db),
            })
        }
        Type::Intersection(intersection) => {
            // Collect TypedDict shapes from all TypedDicts in the intersection.
            let unpacked_elements: Vec<_> = intersection
                .positive(db)
                .iter()
                .filter_map(|element| extract_unpacked_typed_dict_from_value_type(db, *element))
                .collect();

            if unpacked_elements.is_empty() {
                return None;
            }

            // Union all keys from all TypedDicts, intersecting value types for shared keys.
            let mut result: BTreeMap<Name, UnpackedTypedDictKey<'db>> = BTreeMap::new();

            for unpacked in &unpacked_elements {
                for (key, unpacked_key) in &unpacked.keys {
                    result
                        .entry(key.clone())
                        .and_modify(|existing| {
                            existing.value_ty = IntersectionType::from_two_elements(
                                db,
                                existing.value_ty,
                                unpacked_key.value_ty,
                            );
                            existing.is_required |= unpacked_key.is_required;
                            existing.definition = merge_unpacked_key_definitions(
                                existing.definition,
                                unpacked_key.definition,
                            );
                        })
                        .or_insert(*unpacked_key);
                }
            }

            for (key, unpacked_key) in &mut result {
                for unpacked in &unpacked_elements {
                    if unpacked.keys.contains_key(key) {
                        continue;
                    }
                    if let Some(extra_items) = unpacked.openness.effective_extra_items() {
                        unpacked_key.value_ty = IntersectionType::from_two_elements(
                            db,
                            unpacked_key.value_ty,
                            extra_items.declared_ty,
                        );
                        unpacked_key.definition = None;
                    }
                }
            }

            let openness = intersect_unpacked_typed_dict_openness(
                db,
                unpacked_elements.iter().map(|unpacked| unpacked.openness),
            );

            Some(UnpackedTypedDict {
                keys: result,
                openness,
            })
        }
        Type::Union(union) => {
            let unpacked_elements: Vec<_> = union
                .elements(db)
                .iter()
                .map(|element| extract_unpacked_typed_dict_from_value_type(db, *element))
                .collect::<Option<_>>()?;

            let all_keys: OrderSet<Name> = unpacked_elements
                .iter()
                .flat_map(|unpacked| unpacked.keys.keys().cloned())
                .collect();
            let mut result = BTreeMap::new();

            for key in all_keys {
                let mut value_ty = UnionBuilder::new(db);
                let mut is_required = true;
                let mut definition = None;
                let mut saw_key = false;

                for unpacked in &unpacked_elements {
                    if let Some(unpacked_key) = unpacked.keys.get(&key) {
                        saw_key = true;
                        value_ty = value_ty.add(unpacked_key.value_ty);
                        is_required &= unpacked_key.is_required;
                        definition = Some(if let Some(definition) = definition {
                            merge_unpacked_key_definitions(definition, unpacked_key.definition)
                        } else {
                            unpacked_key.definition
                        });
                    } else if let Some(extra_items) = unpacked.openness.effective_extra_items() {
                        saw_key = true;
                        value_ty = value_ty.add(extra_items.declared_ty);
                        is_required = false;
                        definition = Some(None);
                    } else {
                        is_required = false;
                        definition = Some(None);
                    }
                }

                if saw_key {
                    result.insert(
                        key,
                        UnpackedTypedDictKey {
                            value_ty: value_ty.build(),
                            is_required,
                            definition: definition.flatten(),
                        },
                    );
                }
            }

            let openness = union_unpacked_typed_dict_openness(
                db,
                unpacked_elements.iter().map(|unpacked| unpacked.openness),
            );

            Some(UnpackedTypedDict {
                keys: result,
                openness,
            })
        }
        Type::TypeAlias(alias) => {
            extract_unpacked_typed_dict_from_value_type(db, alias.value_type(db))
        }
        // All other types cannot contain a TypedDict
        Type::Dynamic(_)
        | Type::Divergent(_)
        | Type::Never
        | Type::EnumComplement(_)
        | Type::FunctionLiteral(_)
        | Type::BoundMethod(_)
        | Type::KnownBoundMethod(_)
        | Type::WrapperDescriptor(_)
        | Type::DataclassDecorator(_)
        | Type::DataclassTransformer(_)
        | Type::Callable(_)
        | Type::ModuleLiteral(_)
        | Type::ClassLiteral(_)
        | Type::GenericAlias(_)
        | Type::SubclassOf(_)
        | Type::NominalInstance(_)
        | Type::ProtocolInstance(_)
        | Type::SpecialForm(_)
        | Type::KnownInstance(_)
        | Type::PropertyInstance(_)
        | Type::AlwaysTruthy
        | Type::AlwaysFalsy
        | Type::LiteralValue(_)
        | Type::TypeVar(_)
        | Type::BoundSuper(_)
        | Type::TypeIs(_)
        | Type::TypeGuard(_)
        | Type::TypeForm(_)
        | Type::NewTypeInstance(_) => None,
    }
}

fn merge_unpacked_key_definitions<'db>(
    existing: Option<Definition<'db>>,
    new: Option<Definition<'db>>,
) -> Option<Definition<'db>> {
    if existing == new { existing } else { None }
}

/// Extracts unpacked `TypedDict` keys for a `**kwargs` annotation only when the annotation
/// explicitly uses `Unpack[...]`.
///
/// Per [PEP 692](https://peps.python.org/pep-0692/#typeddict-unions), this accepts only a concrete
/// `TypedDict` target, or a type alias resolving to one.
pub(crate) fn extract_unpacked_typed_dict_keys_from_kwargs_annotation<'db>(
    db: &'db dyn Db,
    annotated_type: Type<'db>,
    annotation_flags: TypeExpressionFlags,
) -> Option<BTreeMap<Name, UnpackedTypedDictKey<'db>>> {
    let typed_dict = annotation_flags
        .contains(TypeExpressionFlags::UNPACK)
        .then(|| annotated_type.resolve_type_alias(db).as_typed_dict())??;

    Some(
        typed_dict
            .items(db)
            .iter()
            .map(|(name, field)| {
                (
                    name.clone(),
                    UnpackedTypedDictKey {
                        value_ty: field.declared_ty,
                        is_required: field.is_required(),
                        definition: field.first_declaration(),
                    },
                )
            })
            .collect(),
    )
}

/// Infers each unpacked `**kwargs` constructor argument exactly once.
///
/// Mixed positional-and-keyword `TypedDict` construction needs to inspect unpacked keyword types
/// in multiple validation passes. Precomputing them avoids re-inference in speculative builders.
pub(super) fn infer_unpacked_keyword_types<'db>(
    arguments: &Arguments,
    mut expression_type_fn: impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) -> Vec<Option<Type<'db>>> {
    arguments
        .keywords
        .iter()
        .map(|keyword| {
            keyword
                .arg
                .is_none()
                .then(|| expression_type_fn(&keyword.value, TypeContext::default()))
        })
        .collect()
}

pub(super) fn unpacked_keyword_is_gradual<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    match ty.resolve_type_alias(db) {
        ty if ty.is_never() || ty.is_dynamic() => true,
        Type::Union(union) => union
            .elements(db)
            .iter()
            .any(|element| element.resolve_type_alias(db).is_dynamic()),
        _ => false,
    }
}

/// Collects constructor keys that are guaranteed to be provided by keyword arguments.
///
/// Explicit keyword arguments always provide their key. For `**kwargs`, only required keys are
/// guaranteed to be present; optional keys may be omitted at runtime and cannot suppress missing
/// key diagnostics for the positional mapping.
pub(super) fn collect_guaranteed_keyword_keys<'db>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    arguments: &Arguments,
    unpacked_keyword_types: &[Option<Type<'db>>],
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) -> OrderSet<Name> {
    debug_assert_eq!(arguments.keywords.len(), unpacked_keyword_types.len());

    let mut provided_keys: OrderSet<Name> = arguments
        .keywords
        .iter()
        .filter_map(|keyword| keyword.arg.as_ref().map(|arg| arg.id.clone()))
        .collect();

    for (keyword, unpacked_type) in arguments
        .keywords
        .iter()
        .zip(unpacked_keyword_types.iter().copied())
    {
        if keyword.arg.is_some() {
            continue;
        }

        let unpacked_type = if keyword.value.is_dict_expr() {
            Type::unknown()
        } else if let Some(unpacked_type) = unpacked_type {
            unpacked_type
        } else {
            continue;
        };

        collect_guaranteed_keys_from_merged_unpacked_keyword(
            db,
            typed_dict,
            &keyword.value,
            unpacked_type,
            &mut provided_keys,
            expression_type_fn,
        );
    }

    provided_keys
}

/// Collects keys guaranteed by one unpacked constructor argument.
fn collect_guaranteed_keys_from_merged_unpacked_keyword<'db>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    expr: &ast::Expr,
    unpacked_type: Type<'db>,
    provided_keys: &mut OrderSet<Name>,
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) {
    if let ast::Expr::Dict(dict_expr) = expr {
        for item in dict_expr.items.iter().rev() {
            if let Some(key_expr) = &item.key {
                let key_ty = expression_type_fn(key_expr, TypeContext::default());
                if let Some(key_literal) = key_ty.as_string_literal() {
                    provided_keys.insert(Name::new(key_literal.value(db)));
                }
            } else {
                let nested_ty = expression_type_fn(&item.value, TypeContext::default());
                collect_guaranteed_keys_from_merged_unpacked_keyword(
                    db,
                    typed_dict,
                    &item.value,
                    nested_ty,
                    provided_keys,
                    expression_type_fn,
                );
            }
        }
        return;
    }

    if unpacked_keyword_is_gradual(db, unpacked_type) {
        provided_keys.extend(typed_dict.items(db).keys().cloned());
    } else if let Some(unpacked_keys) =
        extract_unpacked_typed_dict_keys_from_value_type(db, unpacked_type)
    {
        for (key, unpacked_key) in unpacked_keys {
            if unpacked_key.is_required {
                provided_keys.insert(key);
            }
        }
    }
}

/// Returns a `TypedDict` schema with `excluded_keys` removed.
///
/// This is used for mixed positional-and-keyword constructor calls, where guaranteed keyword
/// arguments override any same-named keys from the positional mapping.
pub(super) fn typed_dict_without_keys<'db>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    excluded_keys: &OrderSet<Name>,
) -> TypedDictType<'db> {
    if excluded_keys.is_empty() {
        return typed_dict;
    }

    let filtered_items = typed_dict
        .items(db)
        .iter()
        .filter(|(name, _)| !excluded_keys.contains(*name))
        .map(|(name, field)| (name.clone(), field.clone()))
        .collect();

    TypedDictType::from_schema_items_with_openness(db, filtered_items, typed_dict.openness(db))
}

/// Returns a `TypedDict` schema for mixed positional-constructor inference.
///
/// Keys that are guaranteed to be overridden by later keyword arguments stay in the schema as
/// optional `object` fields. This preserves missing-key context for the remaining fields while
/// avoiding premature validation of shadowed keys inside nested dict-literal branches.
pub(super) fn typed_dict_with_relaxed_keys<'db>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    relaxed_keys: &OrderSet<Name>,
) -> TypedDictType<'db> {
    if relaxed_keys.is_empty() {
        return typed_dict;
    }

    let relaxed_items = typed_dict
        .items(db)
        .iter()
        .map(|(name, field)| {
            let mut field = field.clone();
            if relaxed_keys.contains(name) {
                field = field.with_required(false);
                field.declared_ty = Type::object();
            }
            (name.clone(), field)
        })
        .collect();

    TypedDictType::from_schema_items_with_openness(db, relaxed_items, typed_dict.openness(db))
}

fn full_object_ty_annotation(ty: Type<'_>) -> Option<Type<'_>> {
    (ty.is_union() || ty.is_intersection()).then_some(ty)
}

/// AST nodes attached to a `TypedDict` key assignment diagnostic.
///
/// Example: for `Target(source, b=2)`, this bundles the full constructor call together with the
/// expression nodes that should be highlighted for the key and value being validated.
#[derive(Clone, Copy)]
struct TypedDictAssignmentNodes<'ast> {
    /// The outer `TypedDict` constructor or unpacking site.
    ///
    /// Example: this is the `Target(source, b=2)` call when validating a mixed constructor.
    typed_dict: AnyNodeRef<'ast>,
    /// The syntax node used to label the key location in diagnostics.
    ///
    /// Example: this is the `b=2` keyword for an explicit key, or the `source` expression when a
    /// positional `TypedDict` supplies the key.
    key: AnyNodeRef<'ast>,
    /// The syntax node used to label the value location in diagnostics.
    ///
    /// Example: this is the `2` in `Target(source, b=2)`, or the `source` expression when the
    /// positional argument provides both the key and value type information.
    value: AnyNodeRef<'ast>,
}

/// Validates a set of extracted `TypedDict`-like keys against a constructor target.
///
/// This is shared by `**kwargs` validation and mixed constructor calls where the first positional
/// argument is itself `TypedDict`-shaped. It reports per-key diagnostics using the supplied
/// nodes and returns the subset of keys that are guaranteed to be present.
fn validate_extracted_typed_dict_keys<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    unpacked_keys: &BTreeMap<Name, UnpackedTypedDictKey<'db>>,
    nodes: TypedDictAssignmentNodes<'ast>,
    full_object_ty: Option<Type<'db>>,
    ignored_keys: &OrderSet<Name>,
) -> (OrderSet<Name>, bool) {
    let mut provided_keys = OrderSet::new();
    let mut valid = true;

    for (key_name, unpacked_key) in unpacked_keys {
        if ignored_keys.contains(key_name) {
            continue;
        }
        if unpacked_key.is_required {
            provided_keys.insert(key_name.clone());
        }
        valid &= TypedDictKeyAssignment {
            context,
            typed_dict,
            full_object_ty,
            key: key_name.as_str(),
            value_ty: unpacked_key.value_ty,
            typed_dict_node: nodes.typed_dict,
            key_node: nodes.key,
            value_node: nodes.value,
            assignment_kind: TypedDictAssignmentKind::Constructor,
            emit_diagnostic: true,
        }
        .validate();
    }

    (provided_keys, valid)
}

/// Validates the arbitrary keys of a `TypedDict`-shaped constructor argument.
///
/// The source's effective extra items must be valid for every target field they could provide and
/// for the target's explicit extra-items type. Implicitly open sources retain ty's existing
/// leniency when constructing another implicitly open `TypedDict`.
fn validate_extracted_typed_dict_openness<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    source_keys: &BTreeMap<Name, UnpackedTypedDictKey<'db>>,
    source_openness: TypedDictOpenness<'db>,
    nodes: TypedDictAssignmentNodes<'ast>,
    ignored_keys: &OrderSet<Name>,
) -> bool {
    let db = context.db();
    let Some(extra_items) = source_openness.effective_extra_items() else {
        return true;
    };
    let extra_items_ty = extra_items.declared_ty;
    let target_openness = typed_dict.openness(db);

    if target_openness.is_implicitly_open() && source_openness.is_implicitly_open() {
        return true;
    }

    let typed_dict_ty = Type::TypedDict(typed_dict);

    if let Some(target_extra_items) = target_openness.explicit_extra_items() {
        if let Some((target_name, target_field)) =
            typed_dict.items(db).iter().find(|(name, field)| {
                !source_keys.contains_key(*name)
                    && !ignored_keys.contains(*name)
                    && !extra_items_ty.is_assignable_to(db, field.declared_ty)
            })
        {
            if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, nodes.value) {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Unpacked argument has extra items of type `{}` that are not assignable to item `{target_name}` with type `{}` on TypedDict `{}`",
                    extra_items_ty.display(db),
                    target_field.declared_ty.display(db),
                    typed_dict_ty.display(db),
                ));
                diagnostic.annotate(
                    context
                        .secondary(nodes.typed_dict)
                        .message(format_args!("TypedDict `{}`", typed_dict_ty.display(db))),
                );
            }
            return false;
        }

        if extra_items_ty.is_assignable_to(db, target_extra_items.declared_ty) {
            return true;
        }

        if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, nodes.value) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Unpacked argument has extra items of type `{}` that are not assignable to extra items type `{}` on TypedDict `{}`",
                extra_items_ty.display(db),
                target_extra_items.declared_ty.display(db),
                typed_dict_ty.display(db),
            ));
            diagnostic.annotate(
                context
                    .secondary(nodes.typed_dict)
                    .message(format_args!("TypedDict `{}`", typed_dict_ty.display(db))),
            );
        }
        return false;
    }

    if let Some(builder) = context.report_lint(&INVALID_KEY, nodes.key) {
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Unpacked argument may contain unknown keys for TypedDict `{}`",
            typed_dict_ty.display(db),
        ));
        diagnostic.annotate(
            context
                .secondary(nodes.typed_dict)
                .message(format_args!("TypedDict `{}`", typed_dict_ty.display(db))),
        );
    }
    false
}

/// Validates a mixed-constructor positional argument when its type can be viewed as a `TypedDict`.
///
/// If `arg_ty` exposes concrete `TypedDict` keys, only keys that overlap the constructor target
/// are validated directly. This preserves the structural leniency of positional `TypedDict`
/// arguments while still checking declared keys precisely in mixed calls. Returns `None` when the
/// argument is not `TypedDict`-shaped and the caller should fall back to ordinary assignability
/// checks.
fn validate_from_typed_dict_argument<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    arg: &'ast ast::Expr,
    arg_ty: Type<'db>,
    typed_dict_node: AnyNodeRef<'ast>,
    ignored_keys: &OrderSet<Name>,
) -> Option<OrderSet<Name>> {
    let db = context.db();
    let typed_dict_items = typed_dict.items(db);
    let unpacked = extract_unpacked_typed_dict_from_value_type(db, arg_ty)?;
    let source_openness = unpacked.openness;
    let validate_extra_keys = !typed_dict.openness(db).is_implicitly_open();
    let unpacked_keys = unpacked
        .keys
        .into_iter()
        .filter(|(key_name, _)| validate_extra_keys || typed_dict_items.contains_key(key_name))
        .collect();

    let nodes = TypedDictAssignmentNodes {
        typed_dict: typed_dict_node,
        key: arg.into(),
        value: arg.into(),
    };
    let provided_keys = validate_extracted_typed_dict_keys(
        context,
        typed_dict,
        &unpacked_keys,
        nodes,
        full_object_ty_annotation(arg_ty),
        ignored_keys,
    )
    .0;
    validate_extracted_typed_dict_openness(
        context,
        typed_dict,
        &unpacked_keys,
        source_openness,
        nodes,
        ignored_keys,
    );

    Some(provided_keys)
}

fn report_duplicate_typed_dict_constructor_key<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    key: &str,
    duplicate_node: AnyNodeRef<'ast>,
    original_node: AnyNodeRef<'ast>,
) {
    let Some(builder) = context.report_lint(&PARAMETER_ALREADY_ASSIGNED, duplicate_node) else {
        return;
    };

    let typed_dict_display = Type::TypedDict(typed_dict).display(context.db());
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Multiple values provided for key \"{key}\" in TypedDict `{typed_dict_display}` constructor",
    ));
    diagnostic.annotate(
        context
            .secondary(original_node)
            .message(format_args!("first value provided here")),
    );
}

fn record_guaranteed_typed_dict_constructor_key<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    guaranteed_keys: &mut BTreeMap<Name, Option<AnyNodeRef<'ast>>>,
    key: Name,
    duplicate_node: AnyNodeRef<'ast>,
) {
    match guaranteed_keys.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(Some(duplicate_node));
        }
        Entry::Occupied(mut entry) => match *entry.get() {
            Some(original_node) => {
                report_duplicate_typed_dict_constructor_key(
                    context,
                    typed_dict,
                    entry.key().as_str(),
                    duplicate_node,
                    original_node,
                );
            }
            None => {
                entry.insert(Some(duplicate_node));
            }
        },
    }
}

/// Validates a `TypedDict` constructor call.
///
/// This handles keyword-only construction, a single positional mapping argument, and mixed
/// positional-and-keyword calls. Dictionary literals are validated entry-by-entry so we can report
/// extra keys and per-field type mismatches precisely; non-literal positional arguments fall back
/// to assignability against the target `TypedDict`.
pub(super) fn validate_typed_dict_constructor<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    arguments: &'ast Arguments,
    error_node: AnyNodeRef<'ast>,
    mut expression_type_fn: impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) {
    let db = context.db();
    let typed_dict_ty = Type::TypedDict(typed_dict);

    if arguments.args.len() > 1 {
        if let Some(builder) =
            context.report_lint(&TOO_MANY_POSITIONAL_ARGUMENTS, &arguments.args[1])
        {
            builder.into_diagnostic(format_args!(
                "Too many positional arguments to TypedDict `{}` constructor: expected 1, got {}",
                typed_dict_ty.display(db),
                arguments.args.len(),
            ));
        }
        // TODO: Consider validating the first positional argument too, without producing
        // duplicate TypedDict diagnostics for invalid multi-positional calls.
        return;
    }

    // Check for a single positional argument, and whether it's a dict literal.
    let has_single_positional_arg = arguments.args.len() == 1;
    let positional_dict_literal = arguments.args.first().and_then(ast::Expr::as_dict_expr);

    let unpacked_keyword_types = infer_unpacked_keyword_types(arguments, &mut expression_type_fn);

    if has_single_positional_arg && !arguments.keywords.is_empty() {
        // Mixed positional-and-keyword construction: guaranteed keyword-provided keys override the
        // positional mapping, so validate the positional argument against the remaining schema.
        let keyword_keys = validate_from_keywords(
            context,
            typed_dict,
            arguments,
            error_node,
            &unpacked_keyword_types,
            &mut expression_type_fn,
        );
        let mut provided_keys = if let Some(dict_expr) = positional_dict_literal {
            validate_from_dict_literal(
                context,
                typed_dict,
                dict_expr,
                error_node,
                &mut expression_type_fn,
                &keyword_keys,
            )
        } else {
            let arg = &arguments.args[0];
            let positional_inference_target =
                typed_dict_with_relaxed_keys(db, typed_dict, &keyword_keys);
            let positional_target = typed_dict_without_keys(db, typed_dict, &keyword_keys);
            let positional_target_is_unconstrained = positional_target.items(db).is_empty()
                && positional_target.openness(db).is_implicitly_open();
            let positional_target_ty = Type::TypedDict(positional_target);
            let positional_inference_target_ty = Type::TypedDict(positional_inference_target);
            let arg_ty =
                expression_type_fn(arg, TypeContext::new(Some(positional_inference_target_ty)));

            if let Some(provided_keys) = validate_from_typed_dict_argument(
                context,
                typed_dict,
                arg,
                arg_ty,
                error_node,
                &keyword_keys,
            ) {
                provided_keys
            } else {
                if !positional_target_is_unconstrained
                    && !arg_ty.is_assignable_to(db, positional_target_ty)
                {
                    if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, arg) {
                        builder.into_diagnostic(format_args!(
                            "Argument of type `{}` is not assignable to `{}`",
                            arg_ty.display(db),
                            positional_target_ty.display(db),
                        ));
                    }
                }

                positional_target
                    .items(db)
                    .iter()
                    .filter_map(|(key_name, field)| field.is_required().then_some(key_name.clone()))
                    .collect()
            }
        };

        provided_keys.extend(keyword_keys);
        validate_typed_dict_required_keys(context, typed_dict, &provided_keys, error_node);
    } else if let Some(dict_expr) = positional_dict_literal {
        // Single positional dict literal: validate keys and value types directly from the literal,
        // which also allows us to report extra keys that aren't in the `TypedDict` schema.
        let provided_keys = validate_from_dict_literal(
            context,
            typed_dict,
            dict_expr,
            error_node,
            &mut expression_type_fn,
            &OrderSet::new(),
        );
        validate_typed_dict_required_keys(context, typed_dict, &provided_keys, error_node);
    } else if has_single_positional_arg {
        // Single positional argument: check if assignable to the target TypedDict.
        // This handles TypedDict, intersections, unions, and type aliases correctly.
        // Assignability already checks for required keys and type compatibility,
        // so we don't need separate validation.
        let arg = &arguments.args[0];
        let arg_ty = expression_type_fn(arg, TypeContext::new(Some(typed_dict_ty)));

        if !arg_ty.is_assignable_to(db, typed_dict_ty) {
            if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, arg) {
                builder.into_diagnostic(format_args!(
                    "Argument of type `{}` is not assignable to `{}`",
                    arg_ty.display(db),
                    typed_dict_ty.display(db),
                ));
            }
        }
    } else {
        // Keyword-only construction: validate each keyword argument, then check for missing
        // required keys.
        let provided_keys = validate_from_keywords(
            context,
            typed_dict,
            arguments,
            error_node,
            &unpacked_keyword_types,
            &mut expression_type_fn,
        );
        validate_typed_dict_required_keys(context, typed_dict, &provided_keys, error_node);
    }
}

/// Validates a `TypedDict` constructor call with a single positional dictionary argument
/// e.g. `Person({"name": "Alice", "age": 30})`
fn validate_from_dict_literal<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    dict_expr: &'ast ast::ExprDict,
    typed_dict_node: AnyNodeRef<'ast>,
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
    ignored_keys: &OrderSet<Name>,
) -> OrderSet<Name> {
    let dict_node: AnyNodeRef<'ast> = dict_expr.into();
    let mut provided_keys = BTreeMap::new();
    let mut shadowed_keys = ignored_keys.clone();

    validate_merged_dict_literal(
        context,
        typed_dict,
        dict_expr,
        TypedDictAssignmentNodes {
            typed_dict: typed_dict_node,
            key: dict_node,
            value: dict_node,
        },
        &mut provided_keys,
        &mut shadowed_keys,
        expression_type_fn,
    );

    provided_keys.into_keys().collect()
}

/// Validates a `TypedDict` constructor call with keywords
/// e.g. `Person(name="Alice", age=30)` or `Person(**other_typed_dict)`
fn validate_from_keywords<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    arguments: &'ast Arguments,
    typed_dict_node: AnyNodeRef<'ast>,
    unpacked_keyword_types: &[Option<Type<'db>>],
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) -> OrderSet<Name> {
    let db = context.db();
    debug_assert_eq!(arguments.keywords.len(), unpacked_keyword_types.len());

    let mut guaranteed_keys = BTreeMap::new();

    // Validate that each key is assigned a type that is compatible with the key's value type
    for (keyword, unpacked_type) in arguments
        .keywords
        .iter()
        .zip(unpacked_keyword_types.iter().copied())
    {
        let keyword_node: AnyNodeRef<'ast> = keyword.into();

        if let Some(arg_name) = &keyword.arg {
            // Explicit keyword argument: e.g., `name="Alice"`
            record_guaranteed_typed_dict_constructor_key(
                context,
                typed_dict,
                &mut guaranteed_keys,
                arg_name.id.clone(),
                keyword_node,
            );

            let value_tcx = typed_dict
                .item(db, arg_name.id.as_str())
                .map(|field| TypeContext::new(Some(field.declared_ty)))
                .unwrap_or_default();
            let value_ty = expression_type_fn(&keyword.value, value_tcx);
            TypedDictKeyAssignment {
                context,
                typed_dict,
                full_object_ty: None,
                key: arg_name.as_str(),
                value_ty,
                typed_dict_node,
                key_node: keyword_node,
                value_node: (&keyword.value).into(),
                assignment_kind: TypedDictAssignmentKind::Constructor,
                emit_diagnostic: true,
            }
            .validate();
        } else {
            // Keyword unpacking: e.g., `**other_typed_dict`
            // Unlike positional TypedDict arguments, unpacking passes all keys as explicit
            // keyword arguments, so extra keys should be flagged as errors (consistent with
            // explicitly providing those keys).
            let Some(unpacked_type) = unpacked_type else {
                continue;
            };
            // Keep one unpack local while applying merged-dict overwrite semantics, then compare
            // the resulting keys against neighboring constructor keywords.
            let mut unpacked_guaranteed_keys = BTreeMap::new();
            let mut shadowed_keys = OrderSet::new();
            validate_merged_unpacked_keyword_argument(
                context,
                typed_dict,
                &keyword.value,
                unpacked_type,
                TypedDictAssignmentNodes {
                    typed_dict: typed_dict_node,
                    key: keyword_node,
                    value: (&keyword.value).into(),
                },
                &mut unpacked_guaranteed_keys,
                &mut shadowed_keys,
                expression_type_fn,
            );

            for (key_name, key_node) in unpacked_guaranteed_keys {
                if let Some(key_node) = key_node {
                    record_guaranteed_typed_dict_constructor_key(
                        context,
                        typed_dict,
                        &mut guaranteed_keys,
                        key_name,
                        key_node,
                    );
                } else {
                    guaranteed_keys.entry(key_name).or_insert(None);
                }
            }
        }
    }

    guaranteed_keys.into_keys().collect()
}

fn validate_merged_dict_literal<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    dict_expr: &'ast ast::ExprDict,
    nodes: TypedDictAssignmentNodes<'ast>,
    guaranteed_keys: &mut BTreeMap<Name, Option<AnyNodeRef<'ast>>>,
    shadowed_keys: &mut OrderSet<Name>,
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) -> bool {
    let db = context.db();
    let mut valid = true;

    for item in dict_expr.items.iter().rev() {
        if let Some(key_expr) = &item.key {
            let key_ty = expression_type_fn(key_expr, TypeContext::default());
            let Some(key_literal) = key_ty.as_string_literal() else {
                if key_ty.is_assignable_to(db, KnownClass::Str.to_instance(db)) {
                    if let Some(expected_ty) =
                        typed_dict.arbitrary_key_initialization_type_excluding(db, shadowed_keys)
                    {
                        let value_ty =
                            expression_type_fn(&item.value, TypeContext::new(Some(expected_ty)));
                        if !value_ty.is_assignable_to(db, expected_ty) {
                            valid = false;
                            if let Some(builder) =
                                context.report_lint(&INVALID_ARGUMENT_TYPE, &item.value)
                            {
                                builder.into_diagnostic(format_args!(
                                    "Value of type `{}` is not assignable to arbitrary key value type `{}` on TypedDict `{}`",
                                    value_ty.display(db),
                                    expected_ty.display(db),
                                    Type::TypedDict(typed_dict).display(db),
                                ));
                            }
                        }
                    } else if typed_dict.openness(db).is_closed() {
                        valid = false;
                        if let Some(builder) = context.report_lint(&INVALID_KEY, key_expr) {
                            builder.into_diagnostic(format_args!(
                                "Non-literal string key may be unknown for TypedDict `{}`",
                                Type::TypedDict(typed_dict).display(db),
                            ));
                        }
                    }
                } else {
                    valid = false;
                    if let Some(builder) = context.report_lint(&INVALID_KEY, key_expr) {
                        builder.into_diagnostic(format_args!(
                            "TypedDict `{}` requires string keys, got key of type `{}`",
                            Type::TypedDict(typed_dict).display(db),
                            key_ty.display(db),
                        ));
                    }
                }
                continue;
            };

            let key = Name::new(key_literal.value(db));
            let is_shadowed = shadowed_keys.contains(&key);

            if !is_shadowed {
                let value_tcx = typed_dict
                    .item(db, key.as_str())
                    .map(|field| TypeContext::new(Some(field.declared_ty)))
                    .unwrap_or_default();
                let value_ty = expression_type_fn(&item.value, value_tcx);
                valid &= TypedDictKeyAssignment {
                    context,
                    typed_dict,
                    full_object_ty: None,
                    key: key.as_str(),
                    value_ty,
                    typed_dict_node: nodes.typed_dict,
                    key_node: key_expr.into(),
                    value_node: (&item.value).into(),
                    assignment_kind: TypedDictAssignmentKind::Constructor,
                    emit_diagnostic: true,
                }
                .validate();
                guaranteed_keys
                    .entry(key.clone())
                    .or_insert(Some(key_expr.into()));
            }
            shadowed_keys.insert(key);
        } else {
            let nested_ty = expression_type_fn(&item.value, TypeContext::default());
            valid &= validate_merged_unpacked_keyword_argument(
                context,
                typed_dict,
                &item.value,
                nested_ty,
                TypedDictAssignmentNodes {
                    typed_dict: nodes.typed_dict,
                    key: (&item.value).into(),
                    value: (&item.value).into(),
                },
                guaranteed_keys,
                shadowed_keys,
                expression_type_fn,
            );
        }
    }

    valid
}

/// Validates one unpacked constructor argument while preserving merged `**{...}` overwrite
/// semantics.
#[expect(clippy::too_many_arguments)]
fn validate_merged_unpacked_keyword_argument<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    expr: &'ast ast::Expr,
    unpacked_type: Type<'db>,
    nodes: TypedDictAssignmentNodes<'ast>,
    guaranteed_keys: &mut BTreeMap<Name, Option<AnyNodeRef<'ast>>>,
    shadowed_keys: &mut OrderSet<Name>,
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) -> bool {
    let db = context.db();
    let items = typed_dict.items(db);

    if let ast::Expr::Dict(dict_expr) = expr {
        return validate_merged_dict_literal(
            context,
            typed_dict,
            dict_expr,
            nodes,
            guaranteed_keys,
            shadowed_keys,
            expression_type_fn,
        );
    }

    // Never and Dynamic types are special: they can have any keys, so we skip validation and mark
    // all target keys as provided.
    if unpacked_keyword_is_gradual(db, unpacked_type) {
        shadowed_keys.extend(items.keys().cloned());
        for key_name in items.keys() {
            guaranteed_keys.entry(key_name.clone()).or_insert(None);
        }
        return true;
    } else if let Some(unpacked) = extract_unpacked_typed_dict_from_value_type(db, unpacked_type) {
        let ignored_keys = shadowed_keys.clone();
        let (_, mut unpacked_valid) = validate_extracted_typed_dict_keys(
            context,
            typed_dict,
            &unpacked.keys,
            nodes,
            full_object_ty_annotation(unpacked_type),
            &ignored_keys,
        );
        unpacked_valid &= validate_extracted_typed_dict_openness(
            context,
            typed_dict,
            &unpacked.keys,
            unpacked.openness,
            nodes,
            &ignored_keys,
        );

        for (key_name, unpacked_key) in unpacked.keys {
            if unpacked_key.is_required && !ignored_keys.contains(&key_name) {
                guaranteed_keys
                    .entry(key_name.clone())
                    .and_modify(|node| {
                        if node.is_none() {
                            *node = Some(nodes.key);
                        }
                    })
                    .or_insert(Some(nodes.key));
                shadowed_keys.insert(key_name);
            }
        }

        return unpacked_valid;
    } else if let Some((key_ty, value_ty)) = unpacked_type.unpack_keys_and_items(db) {
        if !key_ty.is_assignable_to(db, KnownClass::Str.to_instance(db)) {
            if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, nodes.value) {
                builder.into_diagnostic(format_args!(
                    "Unpacked argument has key type `{}` that is not assignable to `str`",
                    key_ty.display(db),
                ));
            }
            return false;
        }

        if !typed_dict.openness(db).is_implicitly_open() {
            return validate_extracted_typed_dict_openness(
                context,
                typed_dict,
                &BTreeMap::new(),
                TypedDictOpenness::extra(db, value_ty, true),
                nodes,
                shadowed_keys,
            );
        }
    }

    true
}

/// Validates a `TypedDict` dictionary literal assignment,
/// e.g. `person: Person = {"name": "Alice", "age": 30}`
pub(super) fn validate_typed_dict_dict_literal<'db>(
    context: &InferContext<'db, '_>,
    typed_dict: TypedDictType<'db>,
    dict_expr: &ast::ExprDict,
    typed_dict_node: AnyNodeRef,
    mut expression_type_fn: impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
) -> Result<OrderSet<Name>, OrderSet<Name>> {
    let mut provided_keys = BTreeMap::new();
    let mut shadowed_keys = OrderSet::new();

    let mut valid = validate_merged_dict_literal(
        context,
        typed_dict,
        dict_expr,
        TypedDictAssignmentNodes {
            typed_dict: typed_dict_node,
            key: typed_dict_node,
            value: typed_dict_node,
        },
        &mut provided_keys,
        &mut shadowed_keys,
        &mut expression_type_fn,
    );

    let provided_keys: OrderSet<Name> = provided_keys.into_keys().collect();

    valid &=
        validate_typed_dict_required_keys(context, typed_dict, &provided_keys, typed_dict_node);

    if valid {
        Ok(provided_keys)
    } else {
        Err(provided_keys)
    }
}

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct SynthesizedTypedDictType<'db> {
    #[returns(ref)]
    pub(crate) items: TypedDictSchema<'db>,
    pub(crate) kind: SynthesizedTypedDictKind,
    /// Whether keys absent from `items` are hidden, forbidden, or explicitly typed.
    pub(crate) openness: TypedDictOpenness<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for SynthesizedTypedDictType<'_> {}

impl<'db> SynthesizedTypedDictType<'db> {
    fn schema(
        db: &'db dyn Db,
        items: TypedDictSchema<'db>,
        openness: TypedDictOpenness<'db>,
    ) -> Self {
        Self::new(db, items, SynthesizedTypedDictKind::Schema, openness)
    }

    fn patch(
        db: &'db dyn Db,
        items: TypedDictSchema<'db>,
        openness: TypedDictOpenness<'db>,
    ) -> Self {
        Self::new(db, items, SynthesizedTypedDictKind::Patch, openness)
    }

    fn is_patch(self, db: &'db dyn Db) -> bool {
        self.kind(db) == SynthesizedTypedDictKind::Patch
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let items = self
            .items(db)
            .iter()
            .map(|(name, field)| {
                let field = field
                    .clone()
                    .apply_type_mapping_impl(db, type_mapping, tcx, visitor);

                (name.clone(), field)
            })
            .collect::<TypedDictSchema<'db>>();

        let openness = self
            .openness(db)
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);

        match self.kind(db) {
            SynthesizedTypedDictKind::Schema => Self::schema(db, items, openness),
            SynthesizedTypedDictKind::Patch => Self::patch(db, items, openness),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, get_size2::GetSize, salsa::Update)]
pub struct TypedDictSchema<'db>(BTreeMap<Name, TypedDictField<'db>>);

impl<'db> TypedDictSchema<'db> {
    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        self.iter()
            .map(|(name, field)| {
                let declared_ty = field
                    .declared_ty
                    .recursive_type_normalized_impl(db, div, true);
                let declared_ty = if nested {
                    declared_ty?
                } else {
                    declared_ty.unwrap_or(div)
                };
                let mut field = field.clone();
                field.declared_ty = declared_ty;
                Some((name.clone(), field))
            })
            .collect()
    }
}

impl<'db> Deref for TypedDictSchema<'db> {
    type Target = BTreeMap<Name, TypedDictField<'db>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TypedDictSchema<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> IntoIterator for &'a TypedDictSchema<'_> {
    type Item = (&'a Name, &'a TypedDictField<'a>);
    type IntoIter = std::collections::btree_map::Iter<'a, Name, TypedDictField<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'db> FromIterator<(Name, TypedDictField<'db>)> for TypedDictSchema<'db> {
    fn from_iter<T: IntoIterator<Item = (Name, TypedDictField<'db>)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, get_size2::GetSize, salsa::Update)]
pub struct TypedDictField<'db> {
    pub(super) declared_ty: Type<'db>,
    flags: TypedDictFieldFlags,
    first_declaration: Option<Definition<'db>>,
}

impl<'db> TypedDictField<'db> {
    pub(crate) const fn is_required(&self) -> bool {
        self.flags.contains(TypedDictFieldFlags::REQUIRED)
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        self.flags.contains(TypedDictFieldFlags::READ_ONLY)
    }

    /// Returns `false` for optional fields whose declared type is uninhabited.
    pub(crate) fn may_be_present(&self, db: &'db dyn Db) -> bool {
        self.is_required() || !self.declared_ty.resolve_type_alias(db).is_never()
    }

    pub(crate) const fn first_declaration(&self) -> Option<Definition<'db>> {
        self.first_declaration
    }

    /// Create a `TypedDictField` from a [`Field`] with `FieldKind::TypedDict`.
    pub(crate) fn from_field(field: &super::class::Field<'db>) -> Self {
        TypedDictFieldBuilder::new(field.declared_ty)
            .required(field.is_required())
            .read_only(field.is_read_only())
            .first_declaration(field.first_declaration)
            .build()
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            declared_ty: self
                .declared_ty
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            flags: self.flags,
            first_declaration: self.first_declaration,
        }
    }

    fn with_required(mut self, yes: bool) -> Self {
        self.flags.set(TypedDictFieldFlags::REQUIRED, yes);
        self
    }
}

pub(super) struct TypedDictFieldBuilder<'db> {
    declared_ty: Type<'db>,
    flags: TypedDictFieldFlags,
    first_declaration: Option<Definition<'db>>,
}

impl<'db> TypedDictFieldBuilder<'db> {
    pub(crate) fn new(declared_ty: Type<'db>) -> Self {
        Self {
            declared_ty,
            flags: TypedDictFieldFlags::empty(),
            first_declaration: None,
        }
    }

    pub(crate) fn required(mut self, yes: bool) -> Self {
        self.flags.set(TypedDictFieldFlags::REQUIRED, yes);
        self
    }

    pub(crate) fn read_only(mut self, yes: bool) -> Self {
        self.flags.set(TypedDictFieldFlags::READ_ONLY, yes);
        self
    }

    pub(crate) fn first_declaration(mut self, definition: Option<Definition<'db>>) -> Self {
        self.first_declaration = definition;
        self
    }

    pub(crate) fn build(self) -> TypedDictField<'db> {
        TypedDictField {
            declared_ty: self.declared_ty,
            flags: self.flags,
            first_declaration: self.first_declaration,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
    struct TypedDictFieldFlags: u8 {
        const REQUIRED = 1 << 0;
        const READ_ONLY = 1 << 1;
    }
}

impl get_size2::GetSize for TypedDictFieldFlags {}

/// Yield all the key/val pairs where the same key is present in both `BTreeMap`s. Take advantage
/// of the fact that keys are sorted to walk through each map once without doing any lookups. It
/// would be nice if `BTreeMap` had something like `BTreeSet::intersection` that did this for us,
/// but as far as I know we have to do it ourselves. Life is hard.
fn btreemap_values_with_same_key<'a, K, V1, V2>(
    left: &'a BTreeMap<K, V1>,
    right: &'a BTreeMap<K, V2>,
) -> impl Iterator<Item = (&'a V1, &'a V2)>
where
    K: Ord,
{
    let mut left_items = left.iter().peekable();
    let mut right_items = right.iter().peekable();
    std::iter::from_fn(move || {
        while let (Some((left_key, left_val)), Some((right_key, right_val))) =
            (left_items.peek().copied(), right_items.peek().copied())
        {
            match left_key.cmp(right_key) {
                Ordering::Equal => {
                    // Matching keys. Yield this pair of values and advance both iterators.
                    left_items.next();
                    right_items.next();
                    return Some((left_val, right_val));
                }
                Ordering::Less => {
                    // `left_items` is behind `right_items` in key order. Advance `left_items`.
                    left_items.next();
                }
                Ordering::Greater => {
                    // The opposite.
                    right_items.next();
                }
            }
        }
        // We've exhausted one or both of the maps, so there can be no more matching keys.
        None
    })
}

#[test]
fn test_btreemap_overlapping_items() {
    // A case with partial overlap and gaps.
    let left = BTreeMap::from_iter([("a", 1), ("b", 2), ("c", 3), ("d", 4), ("e", 5)]);
    let right = BTreeMap::from_iter([("b", 2.0), ("d", 4.0), ("f", 6.0)]);
    assert_eq!(
        btreemap_values_with_same_key(&left, &right).collect::<Vec<_>>(),
        vec![(&2, &2.0), (&4, &4.0)],
    );
    assert_eq!(
        btreemap_values_with_same_key(&right, &left).collect::<Vec<_>>(),
        vec![(&2.0, &2), (&4.0, &4)],
    );

    // A case where one side is empty.
    let left = BTreeMap::<i32, i32>::new();
    let right = BTreeMap::<i32, i32>::from_iter([(1, 1), (2, 2)]);
    assert!(
        btreemap_values_with_same_key(&left, &right)
            .next()
            .is_none()
    );
    assert!(
        btreemap_values_with_same_key(&right, &left)
            .next()
            .is_none()
    );
}
