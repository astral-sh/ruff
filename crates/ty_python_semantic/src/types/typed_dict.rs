use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

use bitflags::bitflags;
use ruff_db::diagnostic::{Annotation, Diagnostic, Span, SubDiagnostic, SubDiagnosticSeverity};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::Arguments;
use ruff_python_ast::{self as ast, AnyNodeRef, StmtClassDef, name::Name};
use ruff_text_size::Ranged;

use super::class::{ClassType, CodeGeneratorKind, Field};
use super::context::InferContext;
use super::diagnostic::{
    self, INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, report_invalid_key_on_typed_dict,
    report_missing_typed_dict_key,
};
use super::{ApplyTypeMappingVisitor, Type, TypeMapping, visitor};
use crate::Db;
use crate::semantic_index::definition::Definition;
use crate::types::class::FieldKind;
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::generics::InferableTypeVars;
use crate::types::{
    HasRelationToVisitor, IsDisjointVisitor, IsEquivalentVisitor, NormalizedVisitor, TypeContext,
    TypeRelation,
};

use ordermap::OrderSet;

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
            let (class_literal, specialization) = class.class_literal(db);
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
            Self::Class(defining_class) => class_based_items(db, defining_class),
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

    // Subtyping between `TypedDict`s follows the algorithm described at:
    // https://typing.python.org/en/latest/spec/typeddict.html#subtyping-between-typeddict-types
    pub(super) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        target: TypedDictType<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // First do a quick nominal check that (if it succeeds) means that we can avoid
        // materializing the full `TypedDict` schema for either `self` or `target`.
        // This should be cheaper in many cases, and also helps us avoid some cycles.
        if let Some(defining_class) = self.defining_class()
            && let Some(target_defining_class) = target.defining_class()
            && defining_class.is_subclass_of(db, target_defining_class)
        {
            return ConstraintSet::from(true);
        }

        let self_items = self.items(db);
        let target_items = target.items(db);
        // Many rules violations short-circuit with "never", but asking whether one field is
        // [relation] to/of another can produce more complicated constraints, and we collect those.
        let mut constraints = ConstraintSet::from(true);
        for (target_item_name, target_item_field) in target_items {
            let field_constraints = if target_item_field.is_required() {
                // required target fields
                let Some(self_item_field) = self_items.get(target_item_name) else {
                    // Self is missing a required field.
                    return ConstraintSet::from(false);
                };
                if !self_item_field.is_required() {
                    // A required field is not required in self.
                    return ConstraintSet::from(false);
                }
                if target_item_field.is_read_only() {
                    // For `ReadOnly[]` fields in the target, the corresponding fields in
                    // self need to have the same assignability/subtyping/etc relation
                    // individually that we're looking for overall between the
                    // `TypedDict`s.
                    self_item_field.declared_ty.has_relation_to_impl(
                        db,
                        target_item_field.declared_ty,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    )
                } else {
                    if self_item_field.is_read_only() {
                        // A read-only field can't be assigned to a mutable target.
                        return ConstraintSet::from(false);
                    }
                    // For mutable fields in the target, the relation needs to apply both
                    // ways, or else mutating the target could violate the structural
                    // invariants of self. For fully-static types, this is "equivalence".
                    // For gradual types, it depends on the relation, but mutual
                    // assignability is "consistency".
                    self_item_field
                        .declared_ty
                        .has_relation_to_impl(
                            db,
                            target_item_field.declared_ty,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                        .and(db, || {
                            target_item_field.declared_ty.has_relation_to_impl(
                                db,
                                self_item_field.declared_ty,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        })
                }
            } else {
                // `NotRequired[]` target fields
                if target_item_field.is_read_only() {
                    // As above, for `NotRequired[]` + `ReadOnly[]` fields in the target. It's
                    // tempting to refactor things and unify some of these calls to
                    // `has_relation_to_impl`, but this branch will get more complicated when we
                    // add support for `closed` and `extra_items` (which is why the rules in the
                    // spec are structured like they are), and following the structure of the spec
                    // makes it easier to check the logic here.
                    if let Some(self_item_field) = self_items.get(target_item_name) {
                        self_item_field.declared_ty.has_relation_to_impl(
                            db,
                            target_item_field.declared_ty,
                            inferable,
                            relation,
                            relation_visitor,
                            disjointness_visitor,
                        )
                    } else {
                        // Self is missing this not-required, read-only item. However, since all
                        // `TypedDict`s by default are allowed to have "extra items" of any type
                        // (until we support `closed` and explicit `extra_items`), this key could
                        // actually turn out to have a value. To make sure this is type-safe, the
                        // not-required field in the target needs to be assignable from `object`.
                        // TODO: `closed` and `extra_items` support will go here.
                        Type::object().when_assignable_to(
                            db,
                            target_item_field.declared_ty,
                            inferable,
                        )
                    }
                } else {
                    // As above, for `NotRequired[]` mutable fields in the target. Again the logic
                    // is largely the same for now, but it will get more complicated with `closed`
                    // and `extra_items`.
                    if let Some(self_item_field) = self_items.get(target_item_name) {
                        if self_item_field.is_read_only() {
                            // A read-only field can't be assigned to a mutable target.
                            return ConstraintSet::from(false);
                        }
                        if self_item_field.is_required() {
                            // A required field can't be assigned to a not-required, mutable field
                            // in the target, because `del` is allowed on the target field.
                            return ConstraintSet::from(false);
                        }

                        // As above, for mutable fields in the target, the relation needs
                        // to apply both ways.
                        self_item_field
                            .declared_ty
                            .has_relation_to_impl(
                                db,
                                target_item_field.declared_ty,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                            .and(db, || {
                                target_item_field.declared_ty.has_relation_to_impl(
                                    db,
                                    self_item_field.declared_ty,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                    } else {
                        // Self is missing this not-required, mutable field. This isn't ok if self
                        // has read-only extra items, which all `TypedDict`s effectively do until
                        // we support `closed` and explicit `extra_items`. See "A subtle
                        // interaction between two structural assignability rules prevents
                        // unsoundness" in `typed_dict.md`.
                        // TODO: `closed` and `extra_items` support will go here.
                        ConstraintSet::from(false)
                    }
                }
            };
            constraints.intersect(db, field_constraints);
            if constraints.is_never_satisfied(db) {
                return constraints;
            }
        }
        constraints
    }

    pub fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self {
            TypedDictType::Class(defining_class) => Some(defining_class.definition(db)),
            TypedDictType::Synthesized(_) => None,
        }
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        match self {
            TypedDictType::Class(_) => {
                let synthesized = SynthesizedTypedDictType::new(db, self.items(db));
                TypedDictType::Synthesized(synthesized.normalized_impl(db, visitor))
            }
            TypedDictType::Synthesized(synthesized) => {
                TypedDictType::Synthesized(synthesized.normalized_impl(db, visitor))
            }
        }
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: TypedDictType<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // TODO: `closed` and `extra_items` support will go here. Until then we don't look at the
        // params at all, because `total` is already incorporated into `FieldKind`.

        // Since both sides' fields are pre-sorted into `BTreeMap`s, we can iterate over them in
        // sorted order instead of paying for a lookup for each field, as long as their lengths are
        // the same.
        if self.items(db).len() != other.items(db).len() {
            return ConstraintSet::from(false);
        }
        self.items(db).iter().zip(other.items(db)).when_all(
            db,
            |((name, field), (other_name, other_field))| {
                if name != other_name || field.flags != other_field.flags {
                    return ConstraintSet::from(false);
                }
                field.declared_ty.is_equivalent_to_impl(
                    db,
                    other_field.declared_ty,
                    inferable,
                    visitor,
                )
            },
        )
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

/// Validates assignment of a value to a specific key on a `TypedDict`.
///
/// Returns true if the assignment is valid, or false otherwise.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_typed_dict_key_assignment<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    full_object_ty: Option<Type<'db>>,
    key: &str,
    value_ty: Type<'db>,
    typed_dict_node: impl Into<AnyNodeRef<'ast>> + Copy,
    key_node: impl Into<AnyNodeRef<'ast>>,
    value_node: impl Into<AnyNodeRef<'ast>>,
    assignment_kind: TypedDictAssignmentKind,
    emit_diagnostic: bool,
) -> bool {
    let db = context.db();
    let items = typed_dict.items(db);

    // Check if key exists in `TypedDict`
    let Some((_, item)) = items.iter().find(|(name, _)| *name == key) else {
        if emit_diagnostic {
            report_invalid_key_on_typed_dict(
                context,
                typed_dict_node.into(),
                key_node.into(),
                Type::TypedDict(typed_dict),
                full_object_ty,
                Type::string_literal(db, key),
                items,
            );
        }

        return false;
    };

    let add_object_type_annotation =
        |diagnostic: &mut Diagnostic| {
            if let Some(full_object_ty) = full_object_ty {
                diagnostic.annotate(context.secondary(typed_dict_node.into()).message(
                    format_args!(
                        "TypedDict `{}` in {kind} type `{}`",
                        Type::TypedDict(typed_dict).display(db),
                        full_object_ty.display(db),
                        kind = if full_object_ty.is_union() {
                            "union"
                        } else {
                            "intersection"
                        },
                    ),
                ));
            } else {
                diagnostic.annotate(context.secondary(typed_dict_node.into()).message(
                    format_args!("TypedDict `{}`", Type::TypedDict(typed_dict).display(db)),
                ));
            }
        };

    let add_item_definition_subdiagnostic = |diagnostic: &mut Diagnostic, message| {
        if let Some(declaration) = item.first_declaration {
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
    };

    if assignment_kind.is_subscript() && item.is_read_only() {
        if emit_diagnostic
            && let Some(builder) =
                context.report_lint(assignment_kind.diagnostic_type(), key_node.into())
        {
            let typed_dict_ty = Type::TypedDict(typed_dict);
            let typed_dict_d = typed_dict_ty.display(db);

            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Cannot assign to key \"{key}\" on TypedDict `{typed_dict_d}`",
            ));

            diagnostic.set_primary_message(format_args!("key is marked read-only"));
            add_object_type_annotation(&mut diagnostic);
            add_item_definition_subdiagnostic(&mut diagnostic, "Read-only item declared here");
        }

        return false;
    }

    // Key exists, check if value type is assignable to declared type
    if value_ty.is_assignable_to(db, item.declared_ty) {
        return true;
    }

    let value_node = value_node.into();
    if diagnostic::is_invalid_typed_dict_literal(context.db(), item.declared_ty, value_node) {
        return false;
    }

    // Invalid assignment - emit diagnostic
    if emit_diagnostic
        && let Some(builder) = context.report_lint(assignment_kind.diagnostic_type(), value_node)
    {
        let typed_dict_ty = Type::TypedDict(typed_dict);
        let typed_dict_d = typed_dict_ty.display(db);
        let value_d = value_ty.display(db);
        let item_type_d = item.declared_ty.display(db);

        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Invalid {} to key \"{key}\" with declared type `{item_type_d}` on TypedDict `{typed_dict_d}`",
            assignment_kind.diagnostic_name(),
        ));

        diagnostic.set_primary_message(format_args!("value of type `{value_d}`"));

        diagnostic.annotate(
            context
                .secondary(key_node.into())
                .message(format_args!("key has declared type `{item_type_d}`")),
        );

        add_item_definition_subdiagnostic(&mut diagnostic, "Item declared here");
        add_object_type_annotation(&mut diagnostic);
    }

    false
}

/// Validates that all required keys are provided in a `TypedDict` construction.
///
/// Reports errors for any keys that are required but not provided.
///
/// Returns true if the assignment is valid, or false otherwise.
pub(super) fn validate_typed_dict_required_keys<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    provided_keys: &OrderSet<&str>,
    error_node: AnyNodeRef<'ast>,
) -> bool {
    let db = context.db();
    let items = typed_dict.items(db);

    let required_keys: OrderSet<&str> = items
        .iter()
        .filter_map(|(key_name, field)| field.is_required().then_some(key_name.as_str()))
        .collect();

    let missing_keys = required_keys.difference(provided_keys);

    let mut has_missing_key = false;
    for missing_key in missing_keys {
        has_missing_key = true;

        report_missing_typed_dict_key(
            context,
            error_node,
            Type::TypedDict(typed_dict),
            missing_key,
        );
    }

    !has_missing_key
}

pub(super) fn validate_typed_dict_constructor<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    arguments: &'ast Arguments,
    error_node: AnyNodeRef<'ast>,
    expression_type_fn: impl Fn(&ast::Expr) -> Type<'db>,
) {
    let has_positional_dict = arguments.args.len() == 1 && arguments.args[0].is_dict_expr();

    let provided_keys = if has_positional_dict {
        validate_from_dict_literal(
            context,
            typed_dict,
            arguments,
            error_node,
            &expression_type_fn,
        )
    } else {
        validate_from_keywords(
            context,
            typed_dict,
            arguments,
            error_node,
            &expression_type_fn,
        )
    };

    validate_typed_dict_required_keys(context, typed_dict, &provided_keys, error_node);
}

/// Validates a `TypedDict` constructor call with a single positional dictionary argument
/// e.g. `Person({"name": "Alice", "age": 30})`
fn validate_from_dict_literal<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    arguments: &'ast Arguments,
    error_node: AnyNodeRef<'ast>,
    expression_type_fn: &impl Fn(&ast::Expr) -> Type<'db>,
) -> OrderSet<&'ast str> {
    let mut provided_keys = OrderSet::new();

    if let ast::Expr::Dict(dict_expr) = &arguments.args[0] {
        // Validate dict entries
        for dict_item in &dict_expr.items {
            if let Some(ref key_expr) = dict_item.key
                && let ast::Expr::StringLiteral(ast::ExprStringLiteral {
                    value: key_value, ..
                }) = key_expr
            {
                let key_str = key_value.to_str();
                provided_keys.insert(key_str);

                // Get the already-inferred argument type
                let value_type = expression_type_fn(&dict_item.value);
                validate_typed_dict_key_assignment(
                    context,
                    typed_dict,
                    None,
                    key_str,
                    value_type,
                    error_node,
                    key_expr,
                    &dict_item.value,
                    TypedDictAssignmentKind::Constructor,
                    true,
                );
            }
        }
    }

    provided_keys
}

/// Validates a `TypedDict` constructor call with keywords
/// e.g. `Person(name="Alice", age=30)`
fn validate_from_keywords<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    arguments: &'ast Arguments,
    error_node: AnyNodeRef<'ast>,
    expression_type_fn: &impl Fn(&ast::Expr) -> Type<'db>,
) -> OrderSet<&'ast str> {
    let provided_keys: OrderSet<&str> = arguments
        .keywords
        .iter()
        .filter_map(|kw| kw.arg.as_ref().map(|arg| arg.id.as_str()))
        .collect();

    // Validate that each key is assigned a type that is compatible with the keys's value type
    for keyword in &arguments.keywords {
        if let Some(arg_name) = &keyword.arg {
            // Get the already-inferred argument type
            let arg_type = expression_type_fn(&keyword.value);
            validate_typed_dict_key_assignment(
                context,
                typed_dict,
                None,
                arg_name.as_str(),
                arg_type,
                error_node,
                keyword,
                &keyword.value,
                TypedDictAssignmentKind::Constructor,
                true,
            );
        }
    }

    provided_keys
}

/// Validates a `TypedDict` dictionary literal assignment,
/// e.g. `person: Person = {"name": "Alice", "age": 30}`
pub(super) fn validate_typed_dict_dict_literal<'db>(
    context: &InferContext<'db, '_>,
    typed_dict: TypedDictType<'db>,
    dict_expr: &ast::ExprDict,
    error_node: AnyNodeRef,
    expression_type_fn: impl Fn(&ast::Expr) -> Type<'db>,
) -> Result<OrderSet<&'db str>, OrderSet<&'db str>> {
    let mut valid = true;
    let mut provided_keys = OrderSet::new();

    // Validate each key-value pair in the dictionary literal
    for item in &dict_expr.items {
        if let Some(key_expr) = &item.key
            && let Type::StringLiteral(key_str) = expression_type_fn(key_expr)
        {
            let key_str = key_str.value(context.db());
            provided_keys.insert(key_str);

            let value_type = expression_type_fn(&item.value);

            valid &= validate_typed_dict_key_assignment(
                context,
                typed_dict,
                None,
                key_str,
                value_type,
                error_node,
                key_expr,
                &item.value,
                TypedDictAssignmentKind::Constructor,
                true,
            );
        }
    }

    valid &= validate_typed_dict_required_keys(context, typed_dict, &provided_keys, error_node);

    if valid {
        Ok(provided_keys)
    } else {
        Err(provided_keys)
    }
}

#[salsa::interned(debug)]
pub struct SynthesizedTypedDictType<'db> {
    #[returns(ref)]
    pub(crate) items: TypedDictSchema<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for SynthesizedTypedDictType<'_> {}

impl<'db> SynthesizedTypedDictType<'db> {
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

        SynthesizedTypedDictType::new(db, items)
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let items = self
            .items(db)
            .iter()
            .map(|(name, field)| {
                let field = field.clone().normalized_impl(db, visitor);
                (name.clone(), field)
            })
            .collect::<TypedDictSchema<'db>>();
        Self::new(db, items)
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

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        Self {
            declared_ty: self.declared_ty.normalized_impl(db, visitor),
            flags: self.flags,
            // A normalized typed-dict field does not hold onto the original declaration,
            // since a normalized typed-dict is an abstract type where equality does not depend
            // on the source-code definition.
            first_declaration: None,
        }
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
