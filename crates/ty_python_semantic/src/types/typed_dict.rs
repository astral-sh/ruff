use std::cmp::Ordering;
use std::collections::{BTreeMap, btree_map::Entry};
use std::ops::{Deref, DerefMut};

use bitflags::bitflags;
use ordermap::OrderSet;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_python_ast::Arguments;
use ruff_python_ast::{self as ast, AnyNodeRef, StmtClassDef, name::Name};
use ruff_text_size::Ranged;

use super::class::{ClassLiteral, ClassType, CodeGeneratorKind, Field};
use super::context::InferContext;
use super::diagnostic::{
    self, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, PARAMETER_ALREADY_ASSIGNED,
    TOO_MANY_POSITIONAL_ARGUMENTS, report_invalid_key_on_typed_dict, report_missing_typed_dict_key,
};
use super::infer::infer_deferred_types;
use super::{
    ApplyTypeMappingVisitor, IntersectionType, SpecialFormType, Type, TypeMapping, TypeQualifiers,
    UnionBuilder, definition_expression_type, visitor,
};
use crate::types::TypeContext;
use crate::types::TypeDefinition;
use crate::types::class::FieldKind;
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::relation::{DisjointnessChecker, TypeRelation, TypeRelationChecker};
use crate::{Db, HasType, SemanticModel};
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

    pub(crate) fn items(self, db: &'db dyn Db) -> &'db TypedDictSchema<'db> {
        #[salsa::tracked(returns(ref), heap_size=ruff_memory_usage::heap_size)]
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
        Self::Synthesized(SynthesizedTypedDictType::schema(db, items))
    }

    fn from_patch_items(db: &'db dyn Db, items: TypedDictSchema<'db>) -> Self {
        Self::Synthesized(SynthesizedTypedDictType::patch(db, items))
    }

    /// Returns a partial version of this `TypedDict` where all fields are optional. This is used
    /// to model PEP 584 update operands, accepting dictionary literals that update any subset of
    /// known keys, and also accepting other `TypedDict`s as long as any overlapping keys are
    /// compatible.
    pub(crate) fn to_partial(self, db: &'db dyn Db) -> Self {
        let items: TypedDictSchema<'db> = self
            .items(db)
            .iter()
            .map(|(name, field)| (name.clone(), field.clone().with_required(false)))
            .collect();

        Self::from_patch_items(db, items)
    }

    /// Returns a patch version of this `TypedDict` for `TypedDict.update()`.
    ///
    /// All fields become optional, and read-only fields become bottom-typed. This preserves the
    /// PEP 705 rule that `update()` must reject any source that can write a read-only key, while
    /// still accepting `NotRequired[Never]` placeholders for keys that cannot be present.
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

        Self::from_patch_items(db, items)
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
            let mut result = self.always();

            for (source_item_name, source_item_field) in source_items {
                let Some(target_item_field) = target_items.get(source_item_name) else {
                    continue;
                };

                result.intersect(
                    db,
                    self.constraints,
                    self.check_type_pair(
                        db,
                        source_item_field.declared_ty,
                        target_item_field.declared_ty,
                    ),
                );

                if result.is_never_satisfied(db) {
                    return result;
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
        // Many rules violations short-circuit with "never", but asking whether one field is
        // [relation] to/of another can produce more complicated constraints, and we collect those.
        let mut result = self.always();
        for (target_item_name, target_item_field) in target_items {
            let field_constraints = if target_item_field.is_required() {
                // required target fields
                let Some(source_item_field) = source_items.get(target_item_name) else {
                    // Self is missing a required field.
                    return self.never();
                };
                if !source_item_field.is_required() {
                    // A required field is not required in self.
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
                    // As above, for `NotRequired[]` + `ReadOnly[]` fields in the target. It's
                    // tempting to refactor things and unify some of these calls to
                    // `check_typeddict_pair`, but this branch will get more complicated when we
                    // add support for `closed` and `extra_items` (which is why the rules in the
                    // spec are structured like they are), and following the structure of the spec
                    // makes it easier to check the logic here.
                    if let Some(source_item_field) = source_items.get(target_item_name) {
                        self.check_type_pair(
                            db,
                            source_item_field.declared_ty,
                            target_item_field.declared_ty,
                        )
                    } else {
                        // `source` is missing this not-required, read-only item. However, since all
                        // `TypedDict`s by default are allowed to have "extra items" of any type
                        // (until we support `closed` and explicit `extra_items`), this key could
                        // actually turn out to have a value. To make sure this is type-safe, the
                        // not-required field in the target needs to be assignable from `object`.
                        // TODO: `closed` and `extra_items` support will go here.
                        Type::object().when_assignable_to(
                            db,
                            target_item_field.declared_ty,
                            self.constraints,
                            self.inferable,
                        )
                    }
                } else {
                    // As above, for `NotRequired[]` mutable fields in the target. Again the logic
                    // is largely the same for now, but it will get more complicated with `closed`
                    // and `extra_items`.
                    if let Some(source_item_field) = source_items.get(target_item_name) {
                        if source_item_field.is_read_only() {
                            // A read-only field can't be assigned to a mutable target.
                            return self.never();
                        }
                        if source_item_field.is_required() {
                            // A required field can't be assigned to a not-required, mutable field
                            // in the target, because `del` is allowed on the target field.
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
                        // `source` is missing this not-required, mutable field. This isn't OK if
                        // `source has read-only extra items, which all `TypedDict`s effectively
                        // do until we support `closed` and explicit `extra_items`. See "A subtle
                        // interaction between two structural assignability rules prevents
                        // unsoundness" in `typed_dict.md`.
                        //
                        // TODO: `closed` and `extra_items` support will go here.
                        self.never()
                    }
                }
            };
            result.intersect(db, self.constraints, field_constraints);
            if result.is_never_satisfied(db) {
                return result;
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
    /// similarly. For any field that's only in `A`, it's always possible for our hypothetical `C`
    /// to copy/paste that field without losing assignability to `B` (and vice versa), so we only
    /// need to consider fields that are present in both `A` and `B`.
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
    /// TODO: Adding support for `closed` and `extra_items` will complicate this.
    pub(super) fn check_typeddict_pair(
        &self,
        db: &'db dyn Db,
        left: TypedDictType<'db>,
        right: TypedDictType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let fields_in_common = btreemap_values_with_same_key(left.items(db), right.items(db));
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

        // Check if key exists in `TypedDict`
        let Some((_, item)) = items.iter().find(|(name, _)| *name == self.key) else {
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
                    item,
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
                item,
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
                        },
                    )
                })
                .collect();
            Some(keys)
        }
        Type::Intersection(intersection) => {
            // Collect key maps from all TypedDicts in the intersection
            let all_key_maps: Vec<_> = intersection
                .positive(db)
                .iter()
                .filter_map(|element| {
                    extract_unpacked_typed_dict_keys_from_value_type(db, *element)
                })
                .collect();

            if all_key_maps.is_empty() {
                return None;
            }

            // Union all keys from all TypedDicts, intersecting value types for shared keys.
            let mut result: BTreeMap<Name, UnpackedTypedDictKey<'db>> = BTreeMap::new();

            for key_map in all_key_maps {
                for (key, unpacked_key) in key_map {
                    result
                        .entry(key)
                        .and_modify(|existing| {
                            existing.value_ty = IntersectionType::from_two_elements(
                                db,
                                existing.value_ty,
                                unpacked_key.value_ty,
                            );
                            if unpacked_key.is_required {
                                existing.is_required = true;
                            }
                        })
                        .or_insert(unpacked_key);
                }
            }

            Some(result)
        }
        Type::Union(union) => {
            let key_maps: Vec<_> = union
                .elements(db)
                .iter()
                .map(|element| extract_unpacked_typed_dict_keys_from_value_type(db, *element))
                .collect::<Option<_>>()?;

            let all_keys: OrderSet<Name> = key_maps
                .iter()
                .flat_map(|key_map| key_map.keys().cloned())
                .collect();
            let mut result = BTreeMap::new();

            for key in all_keys {
                let mut value_ty = UnionBuilder::new(db);
                let mut is_required = true;
                let mut saw_key = false;

                for key_map in &key_maps {
                    if let Some(unpacked_key) = key_map.get(&key) {
                        saw_key = true;
                        value_ty = value_ty.add(unpacked_key.value_ty);
                        is_required &= unpacked_key.is_required;
                    } else {
                        is_required = false;
                    }
                }

                if saw_key {
                    result.insert(
                        key,
                        UnpackedTypedDictKey {
                            value_ty: value_ty.build(),
                            is_required,
                        },
                    );
                }
            }

            Some(result)
        }
        Type::TypeAlias(alias) => {
            extract_unpacked_typed_dict_keys_from_value_type(db, alias.value_type(db))
        }
        // All other types cannot contain a TypedDict
        Type::Dynamic(_)
        | Type::Divergent(_)
        | Type::Never
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
        | Type::NewTypeInstance(_) => None,
    }
}

/// Extracts unpacked `TypedDict` keys for a `**kwargs` annotation only when the annotation
/// explicitly uses `Unpack[...]`.
///
/// Per [PEP 692](https://peps.python.org/pep-0692/#typeddict-unions), this accepts only a concrete
/// `TypedDict` target, or a type alias resolving to one.
pub(crate) fn extract_unpacked_typed_dict_keys_from_kwargs_annotation<'db>(
    db: &'db dyn Db,
    file: File,
    annotation: &ast::Expr,
    annotated_type: Type<'db>,
    expression_type: impl FnOnce(&ast::Expr) -> Type<'db>,
) -> Option<BTreeMap<Name, UnpackedTypedDictKey<'db>>> {
    Some(
        resolve_unpacked_typed_dict_kwargs_annotation(
            db,
            file,
            annotation,
            annotated_type,
            expression_type,
        )?
        .items(db)
        .iter()
        .map(|(name, field)| {
            (
                name.clone(),
                UnpackedTypedDictKey {
                    value_ty: field.declared_ty,
                    is_required: field.is_required(),
                },
            )
        })
        .collect(),
    )
}

/// Resolve the concrete `TypedDict` target of a `**kwargs` annotation that explicitly uses
/// `Unpack[...]`.
///
/// This helper accepts both ordinary annotations like `Unpack[TD]` and stringized annotations
/// that parse to the same form. It returns `None` unless the annotation syntax itself explicitly
/// names `Unpack[...]`; a bare `TypedDict` annotation on `**kwargs` is therefore rejected here.
///
/// Once the annotation has been confirmed to use `Unpack[...]`, the resolved annotation type is
/// validated via [`resolve_unpacked_typed_dict_kwargs_annotation_target`], which accepts only a
/// concrete `TypedDict` target or a type alias resolving to one.
fn resolve_unpacked_typed_dict_kwargs_annotation<'db>(
    db: &'db dyn Db,
    file: File,
    annotation: &ast::Expr,
    annotated_type: Type<'db>,
    expression_type: impl FnOnce(&ast::Expr) -> Type<'db>,
) -> Option<TypedDictType<'db>> {
    let explicitly_uses_unpack = match annotation {
        ast::Expr::Subscript(ast::ExprSubscript { value, .. }) => {
            expression_type(value) == Type::SpecialForm(SpecialFormType::Unpack)
        }
        ast::Expr::StringLiteral(string) => {
            let model = SemanticModel::new(db, file);
            let (parsed, string_model) = model.enter_string_annotation(string)?;
            let ast::Expr::Subscript(ast::ExprSubscript { value, .. }) = parsed.expr() else {
                return None;
            };

            value.inferred_type(&string_model) == Some(Type::SpecialForm(SpecialFormType::Unpack))
        }
        _ => false,
    };

    explicitly_uses_unpack
        .then(|| resolve_unpacked_typed_dict_kwargs_annotation_target(db, annotated_type))
        .flatten()
}

/// Resolve the `TypedDictType` target from a given `Unpack[...]` annotation.
///
/// Per [PEP 692](https://peps.python.org/pep-0692/#typeddict-unions), unions (for example) are not
/// allowed in such annotations.
pub(crate) fn resolve_unpacked_typed_dict_kwargs_annotation_target<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<TypedDictType<'db>> {
    match ty {
        Type::TypedDict(typed_dict) => Some(typed_dict),
        Type::TypeAlias(alias) => {
            resolve_unpacked_typed_dict_kwargs_annotation_target(db, alias.value_type(db))
        }
        Type::Dynamic(_)
        | Type::Divergent(_)
        | Type::Never
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
        | Type::NewTypeInstance(_)
        | Type::Union(_)
        | Type::Intersection(_) => None,
    }
}

/// Infers each unpacked `**kwargs` constructor argument exactly once.
///
/// Mixed positional-and-keyword `TypedDict` construction needs to inspect unpacked keyword types
/// in multiple validation passes. Precomputing them avoids re-inference in speculative builders.
pub(super) fn infer_unpacked_keyword_types<'db>(
    arguments: &Arguments,
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
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
) -> OrderSet<Name> {
    debug_assert_eq!(arguments.keywords.len(), unpacked_keyword_types.len());

    let mut provided_keys: OrderSet<Name> = arguments
        .keywords
        .iter()
        .filter_map(|keyword| keyword.arg.as_ref().map(|arg| arg.id.clone()))
        .collect();

    for unpacked_type in unpacked_keyword_types.iter().copied().flatten() {
        if unpacked_type.is_never() || unpacked_type.is_dynamic() {
            provided_keys.extend(
                typed_dict.items(db).iter().filter_map(|(key_name, field)| {
                    field.is_required().then_some(key_name.clone())
                }),
            );
        // TODO: also extract guaranteed keys from unpacked dict literals like `**{"a": 1}`.
        // Today we only suppress positional-key diagnostics for explicit keywords and unpacked
        // TypedDicts, which makes those literal-unpack cases inconsistent with equivalent calls.
        } else if let Some(unpacked_keys) =
            extract_unpacked_typed_dict_keys_from_value_type(db, unpacked_type)
        {
            provided_keys.extend(
                unpacked_keys
                    .into_iter()
                    .filter_map(|(key, unpacked_key)| unpacked_key.is_required.then_some(key)),
            );
        }
    }

    provided_keys
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

    TypedDictType::from_schema_items(db, filtered_items)
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

    TypedDictType::from_schema_items(db, relaxed_items)
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
) -> OrderSet<Name> {
    let mut provided_keys = OrderSet::new();

    for (key_name, unpacked_key) in unpacked_keys {
        if ignored_keys.contains(key_name) {
            continue;
        }
        if unpacked_key.is_required {
            provided_keys.insert(key_name.clone());
        }
        TypedDictKeyAssignment {
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

    provided_keys
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
    let unpacked_keys = extract_unpacked_typed_dict_keys_from_value_type(db, arg_ty)?
        .into_iter()
        .filter(|(key_name, _)| typed_dict_items.contains_key(key_name))
        .collect();

    Some(validate_extracted_typed_dict_keys(
        context,
        typed_dict,
        &unpacked_keys,
        TypedDictAssignmentNodes {
            typed_dict: typed_dict_node,
            key: arg.into(),
            value: arg.into(),
        },
        full_object_ty_annotation(arg_ty),
        ignored_keys,
    ))
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
    let has_positional_dict_literal = has_single_positional_arg && arguments.args[0].is_dict_expr();

    let unpacked_keyword_types = infer_unpacked_keyword_types(arguments, &mut expression_type_fn);

    if has_single_positional_arg && !arguments.keywords.is_empty() {
        // Mixed positional-and-keyword construction: guaranteed keyword-provided keys override the
        // positional mapping, so validate the positional argument against the remaining schema.
        let keyword_keys =
            collect_guaranteed_keyword_keys(db, typed_dict, arguments, &unpacked_keyword_types);
        let mut provided_keys = if has_positional_dict_literal {
            validate_from_dict_literal(
                context,
                typed_dict,
                arguments,
                error_node,
                &mut expression_type_fn,
                &keyword_keys,
            )
        } else {
            let arg = &arguments.args[0];
            let positional_inference_target =
                typed_dict_with_relaxed_keys(db, typed_dict, &keyword_keys);
            let positional_target = typed_dict_without_keys(db, typed_dict, &keyword_keys);
            let positional_target_is_empty = positional_target.items(db).is_empty();
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
                if !positional_target_is_empty && !arg_ty.is_assignable_to(db, positional_target_ty)
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

        provided_keys.extend(validate_from_keywords(
            context,
            typed_dict,
            arguments,
            error_node,
            &unpacked_keyword_types,
            &mut expression_type_fn,
        ));
        validate_typed_dict_required_keys(context, typed_dict, &provided_keys, error_node);
    } else if has_positional_dict_literal {
        // Single positional dict literal: validate keys and value types directly from the literal,
        // which also allows us to report extra keys that aren't in the `TypedDict` schema.
        let provided_keys = validate_from_dict_literal(
            context,
            typed_dict,
            arguments,
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
    arguments: &'ast Arguments,
    typed_dict_node: AnyNodeRef<'ast>,
    expression_type_fn: &mut impl FnMut(&ast::Expr, TypeContext<'db>) -> Type<'db>,
    ignored_keys: &OrderSet<Name>,
) -> OrderSet<Name> {
    let mut provided_keys = OrderSet::new();
    let items = typed_dict.items(context.db());

    if let ast::Expr::Dict(dict_expr) = &arguments.args[0] {
        // Validate dict entries
        for dict_item in &dict_expr.items {
            if let Some(ref key_expr) = dict_item.key
                && let Some(key_value) =
                    expression_type_fn(key_expr, TypeContext::default()).as_string_literal()
            {
                let key = key_value.value(context.db());
                if ignored_keys.contains(key) {
                    continue;
                }
                provided_keys.insert(Name::new(key));

                let value_tcx = items
                    .get(key)
                    .map(|field| TypeContext::new(Some(field.declared_ty)))
                    .unwrap_or_default();
                let value_ty = expression_type_fn(&dict_item.value, value_tcx);
                TypedDictKeyAssignment {
                    context,
                    typed_dict,
                    full_object_ty: None,
                    key,
                    value_ty,
                    typed_dict_node,
                    key_node: key_expr.into(),
                    value_node: (&dict_item.value).into(),
                    assignment_kind: TypedDictAssignmentKind::Constructor,
                    emit_diagnostic: true,
                }
                .validate();
            }
        }
    }

    provided_keys
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
    let items = typed_dict.items(db);
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

            let value_tcx = items
                .get(arg_name.id.as_str())
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
                key_node: keyword.into(),
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

            // Never and Dynamic types are special: they can have any keys, so we skip
            // validation and mark all required keys as provided.
            if unpacked_type.is_never() || unpacked_type.is_dynamic() {
                for (key_name, field) in typed_dict.items(db) {
                    if field.is_required() {
                        guaranteed_keys.entry(key_name.clone()).or_insert(None);
                    }
                }
            } else if let Some(unpacked_keys) =
                extract_unpacked_typed_dict_keys_from_value_type(db, unpacked_type)
            {
                for key_name in validate_extracted_typed_dict_keys(
                    context,
                    typed_dict,
                    &unpacked_keys,
                    TypedDictAssignmentNodes {
                        typed_dict: typed_dict_node,
                        key: keyword.into(),
                        value: (&keyword.value).into(),
                    },
                    full_object_ty_annotation(unpacked_type),
                    &OrderSet::new(),
                ) {
                    record_guaranteed_typed_dict_constructor_key(
                        context,
                        typed_dict,
                        &mut guaranteed_keys,
                        key_name,
                        keyword_node,
                    );
                }
            }
        }
    }

    guaranteed_keys.into_keys().collect()
}

/// Validates a `TypedDict` dictionary literal assignment,
/// e.g. `person: Person = {"name": "Alice", "age": 30}`
pub(super) fn validate_typed_dict_dict_literal<'db>(
    context: &InferContext<'db, '_>,
    typed_dict: TypedDictType<'db>,
    dict_expr: &ast::ExprDict,
    typed_dict_node: AnyNodeRef,
    expression_type_fn: impl Fn(&ast::Expr) -> Type<'db>,
) -> Result<OrderSet<Name>, OrderSet<Name>> {
    let mut valid = true;
    let mut provided_keys = OrderSet::new();

    // Validate each key-value pair in the dictionary literal
    for item in &dict_expr.items {
        if let Some(key_expr) = &item.key
            && let Some(key_str) = expression_type_fn(key_expr).as_string_literal()
        {
            let key = key_str.value(context.db());
            provided_keys.insert(Name::new(key));

            let value_ty = expression_type_fn(&item.value);

            valid &= TypedDictKeyAssignment {
                context,
                typed_dict,
                full_object_ty: None,
                key,
                value_ty,
                typed_dict_node,
                key_node: key_expr.into(),
                value_node: (&item.value).into(),
                assignment_kind: TypedDictAssignmentKind::Constructor,
                emit_diagnostic: true,
            }
            .validate();
        }
    }

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
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for SynthesizedTypedDictType<'_> {}

impl<'db> SynthesizedTypedDictType<'db> {
    fn schema(db: &'db dyn Db, items: TypedDictSchema<'db>) -> Self {
        Self::new(db, items, SynthesizedTypedDictKind::Schema)
    }

    fn patch(db: &'db dyn Db, items: TypedDictSchema<'db>) -> Self {
        Self::new(db, items, SynthesizedTypedDictKind::Patch)
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

        match self.kind(db) {
            SynthesizedTypedDictKind::Schema => Self::schema(db, items),
            SynthesizedTypedDictKind::Patch => Self::patch(db, items),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, get_size2::GetSize, salsa::Update)]
pub struct TypedDictSchema<'db>(BTreeMap<Name, TypedDictField<'db>>);

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
