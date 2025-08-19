use bitflags::bitflags;
use ruff_python_ast::{self as ast, AnyNodeRef, StmtClassDef, name::Name};

use super::class::{ClassType, CodeGeneratorKind, Field};
use super::context::InferContext;
use super::diagnostic::{
    INVALID_ARGUMENT_TYPE, INVALID_ASSIGNMENT, report_invalid_key_on_typed_dict,
    report_missing_typed_dict_required_field,
};
use super::{ApplyTypeMappingVisitor, Type, TypeMapping, visitor};
use crate::{Db, FxOrderMap};

use ordermap::OrderSet;

bitflags! {
    /// Used for `TypedDict` class parameters.
    /// Keeps track of the arguments that were passed in class definition.
    /// (see https://typing.python.org/en/latest/spec/typeddict.html)
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TypedDictParams: u8 {
        /// Whether fields are required by default (`total=True`)
        const TOTAL = 1 << 0;
        // https://peps.python.org/pep-0728/
        // const EXTRA_ITEMS = 1 << 1;
        // const CLOSED = 1 << 2;
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
pub struct TypedDictType<'db> {
    /// A reference to the class (inheriting from `typing.TypedDict`) that specifies the
    /// schema of this `TypedDict`.
    defining_class: ClassType<'db>,
}

impl<'db> TypedDictType<'db> {
    pub(crate) fn new(defining_class: ClassType<'db>) -> Self {
        Self { defining_class }
    }

    pub(crate) fn defining_class(self) -> ClassType<'db> {
        self.defining_class
    }

    pub(crate) fn items(self, db: &'db dyn Db) -> FxOrderMap<Name, Field<'db>> {
        let (class_literal, specialization) = self.defining_class.class_literal(db);
        class_literal.fields(db, specialization, CodeGeneratorKind::TypedDict)
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            defining_class: self
                .defining_class
                .apply_type_mapping_impl(db, type_mapping, visitor),
        }
    }
}

pub(crate) fn walk_typed_dict_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typed_dict: TypedDictType<'db>,
    visitor: &V,
) {
    visitor.visit_type(db, typed_dict.defining_class.into());
}

pub(super) fn compute_typed_dict_params_from_class_def(
    class_stmt: &StmtClassDef,
) -> TypedDictParams {
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
    /// For constructor arguments like `Dict(key=value)`
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
}

/// Validates assignment of a value to a specific key on a `TypedDict`.
/// Returns true if the assignment is valid, false otherwise.
#[allow(clippy::too_many_arguments)]
pub(super) fn validate_typed_dict_key_assignment<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    key: &str,
    value_ty: Type<'db>,
    typed_dict_node: impl Into<AnyNodeRef<'ast>>,
    key_node: impl Into<AnyNodeRef<'ast>>,
    value_node: impl Into<AnyNodeRef<'ast>>,
    assignment_kind: TypedDictAssignmentKind,
) -> bool {
    let db = context.db();
    let items = typed_dict.items(db);

    // Check if key exists in `TypedDict`
    let Some((_, item)) = items.iter().find(|(name, _)| *name == key) else {
        report_invalid_key_on_typed_dict(
            context,
            typed_dict_node.into(),
            key_node.into(),
            Type::TypedDict(typed_dict),
            Type::string_literal(db, key),
            &items,
        );
        return false;
    };

    // Key exists, check if value type is assignable to declared type
    if value_ty.is_assignable_to(db, item.declared_ty) {
        return true;
    }

    // Invalid assignment - emit diagnostic
    if let Some(builder) = context.report_lint(assignment_kind.diagnostic_type(), value_node.into())
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
                .secondary(typed_dict_node.into())
                .message(format_args!("TypedDict `{typed_dict_d}`")),
        );

        diagnostic.annotate(
            context
                .secondary(key_node.into())
                .message(format_args!("key has declared type `{item_type_d}`")),
        );
    }

    false
}

/// Validates that all required fields are provided in a `TypedDict` construction.
/// Reports missing required field errors for any fields that are required but not provided.
pub(super) fn validate_typed_dict_required_fields<'db, 'ast>(
    context: &InferContext<'db, 'ast>,
    typed_dict: TypedDictType<'db>,
    provided_fields: &OrderSet<&str>,
    error_node: AnyNodeRef<'ast>,
) {
    let db = context.db();
    let items = typed_dict.items(db);

    let required_fields: OrderSet<&str> = items
        .iter()
        .filter_map(|(field_name, field)| field.is_required().then_some(field_name.as_str()))
        .collect();

    for missing_field in required_fields.difference(provided_fields) {
        report_missing_typed_dict_required_field(
            context,
            error_node,
            Type::TypedDict(typed_dict),
            missing_field,
        );
    }
}
