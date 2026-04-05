use itertools::Itertools;
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::Ranged;

use crate::types::class::DynamicClassLiteral;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    CYCLIC_CLASS_DEFINITION, DUPLICATE_BASE, INCONSISTENT_MRO, INVALID_ARGUMENT_TYPE, INVALID_BASE,
    IncompatibleBases, SUBCLASS_OF_FINAL_CLASS, UNSUPPORTED_DYNAMIC_BASE,
};
use crate::types::enums::is_enum_class_by_inheritance;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::mro::DynamicMroErrorKind;
use crate::types::{ClassBase, KnownClass, Type, extract_fixed_length_iterable_element_types};

/// Whether a dynamic class-like value is being created via `type()`, `types.new_class()`,
/// or `make_dataclass()`.
///
/// This is used to adjust validation rules and diagnostic messages for dynamic class
/// creation. For example, `types.new_class()` properly handles metaclasses and
/// `__mro_entries__`, so enum-specific restrictions only apply to `type()` and
/// `make_dataclass()`, while `Generic` and `TypedDict` bases are rejected for all
/// three entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DynamicClassKind {
    TypeCall,
    NewClass,
    MakeDataclass,
}

impl DynamicClassKind {
    const fn function_name(self) -> &'static str {
        match self {
            Self::TypeCall => "type()",
            Self::NewClass => "types.new_class()",
            Self::MakeDataclass => "make_dataclass()",
        }
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn extract_dynamic_namespace_members(
        &self,
        namespace_arg: &ast::Expr,
        namespace_type: Type<'db>,
        none_is_empty_namespace: bool,
    ) -> (Box<[(Name, Type<'db>)]>, bool) {
        let db = self.db();

        if none_is_empty_namespace
            && (namespace_arg.is_none_literal_expr() || namespace_type.is_none(db))
        {
            return (Box::default(), false);
        }

        if let ast::Expr::Dict(dict) = namespace_arg {
            let all_keys_are_string_literals = dict.items.iter().all(|item| {
                item.key
                    .as_ref()
                    .is_some_and(|key| self.expression_type(key).is_string_literal())
            });
            let members = dict
                .items
                .iter()
                .filter_map(|item| {
                    let key_expr = item.key.as_ref()?;
                    let key_name = self.expression_type(key_expr).as_string_literal()?;
                    let key_name = Name::new(key_name.value(db));
                    let value_ty = self.expression_type(&item.value);
                    Some((key_name, value_ty))
                })
                .collect();
            (members, !all_keys_are_string_literals)
        } else if let Type::TypedDict(typed_dict) = namespace_type {
            let members = typed_dict
                .items(db)
                .iter()
                .map(|(name, field)| (name.clone(), field.declared_ty))
                .collect();
            (members, true)
        } else {
            (Box::default(), true)
        }
    }

    /// Extract base classes from the bases argument of a `type()` or `types.new_class()` call.
    ///
    /// Emits a diagnostic if `bases_type` is not a valid bases iterable for the given kind.
    ///
    /// Returns `None` if the bases cannot be extracted.
    pub(super) fn extract_explicit_bases(
        &mut self,
        bases_node: &ast::Expr,
        bases_type: Type<'db>,
        kind: DynamicClassKind,
    ) -> Option<Box<[Type<'db>]>> {
        let db = self.db();
        let fn_name = kind.function_name();
        let formal_parameter_type = match kind {
            DynamicClassKind::TypeCall | DynamicClassKind::MakeDataclass => {
                Type::homogeneous_tuple(db, Type::object())
            }
            DynamicClassKind::NewClass => {
                KnownClass::Iterable.to_specialized_instance(db, &[Type::object()])
            }
        };

        if !bases_type.is_assignable_to(db, formal_parameter_type)
            && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, bases_node)
        {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter 2 (`bases`) of `{fn_name}`"
            ));
            diagnostic.set_primary_message(format_args!(
                "Expected `{}`, found `{}`",
                formal_parameter_type.display(db),
                bases_type.display(db)
            ));
        }

        extract_fixed_length_iterable_element_types(db, bases_node, |expr| {
            self.expression_type(expr)
        })
    }

    /// Validate base classes from the second argument of a `type()` or `types.new_class()` call.
    ///
    /// This validates bases that are valid `ClassBase` variants but aren't allowed
    /// for dynamic classes. Invalid bases that can't be converted to `ClassBase` at all
    /// are handled by `DynamicMroErrorKind::InvalidBases`.
    ///
    /// Returns disjoint bases found (for instance-layout-conflict checking).
    pub(super) fn validate_dynamic_type_bases(
        &mut self,
        bases_node: &ast::Expr,
        bases: &[Type<'db>],
        name: &Name,
        kind: DynamicClassKind,
    ) -> IncompatibleBases<'db> {
        let db = self.db();

        let bases_tuple_elts = bases_node
            .as_tuple_expr()
            .map(|tuple| tuple.elts.as_slice());
        let mut disjoint_bases = IncompatibleBases::default();
        let fn_name = kind.function_name();

        for (idx, base) in bases.iter().enumerate() {
            let diagnostic_node = bases_tuple_elts
                .and_then(|elts| elts.get(idx))
                .unwrap_or(bases_node);

            let Some(class_base) = ClassBase::try_from_type(db, *base, None) else {
                continue;
            };

            match class_base {
                ClassBase::Generic | ClassBase::TypedDict => {
                    if let Some(builder) = self.context.report_lint(&INVALID_BASE, diagnostic_node)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Invalid base for class created via `{fn_name}`"
                        ));
                        diagnostic
                            .set_primary_message(format_args!("Has type `{}`", base.display(db)));
                        match class_base {
                            ClassBase::Generic => {
                                diagnostic.info(format_args!(
                                    "Classes created via `{fn_name}` cannot be generic"
                                ));
                                diagnostic.info(format_args!(
                                    "Consider using `class {name}(Generic[...]): ...` instead"
                                ));
                            }
                            ClassBase::TypedDict => {
                                diagnostic.info(format_args!(
                                    "Classes created via `{fn_name}` cannot be TypedDicts"
                                ));
                                diagnostic.info(format_args!(
                                    "Consider using `TypedDict(\"{name}\", {{}})` instead"
                                ));
                            }
                            _ => unreachable!(),
                        }
                    }
                }
                ClassBase::Protocol => {
                    if let Some(builder) = self
                        .context
                        .report_lint(&UNSUPPORTED_DYNAMIC_BASE, diagnostic_node)
                    {
                        let mut diagnostic = builder.into_diagnostic(format_args!(
                            "Unsupported base for class created via `{fn_name}`"
                        ));
                        diagnostic
                            .set_primary_message(format_args!("Has type `{}`", base.display(db)));
                        diagnostic.info(format_args!(
                            "Classes created via `{fn_name}` cannot be protocols",
                        ));
                        diagnostic.info(format_args!(
                            "Consider using `class {name}(Protocol): ...` instead"
                        ));
                    }
                }
                ClassBase::Class(class_type) => {
                    if class_type.is_final(db) {
                        if let Some(builder) = self
                            .context
                            .report_lint(&SUBCLASS_OF_FINAL_CLASS, diagnostic_node)
                        {
                            builder.into_diagnostic(format_args!(
                                "Class `{name}` cannot inherit from final class `{}`",
                                class_type.name(db)
                            ));
                        }
                        if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                            disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                        }
                        continue;
                    }

                    if matches!(
                        kind,
                        DynamicClassKind::TypeCall | DynamicClassKind::MakeDataclass
                    ) && let Some((static_class, _)) = class_type.static_class_literal(db)
                        && is_enum_class_by_inheritance(db, static_class)
                    {
                        if let Some(builder) =
                            self.context.report_lint(&INVALID_BASE, diagnostic_node)
                        {
                            let mut diagnostic = builder.into_diagnostic(format_args!(
                                "Invalid base for class created via `{fn_name}`"
                            ));
                            diagnostic.set_primary_message(format_args!(
                                "Has type `{}`",
                                base.display(db)
                            ));
                            diagnostic.info(format_args!(
                                "Creating an enum class via `{fn_name}` is not supported"
                            ));
                            diagnostic.info(format_args!(
                                "Consider using `Enum(\"{name}\", [])` instead"
                            ));
                        }
                        if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                            disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                        }
                        continue;
                    }

                    if let Some(disjoint_base) = class_type.nearest_disjoint_base(db) {
                        disjoint_bases.insert(disjoint_base, idx, class_type.class_literal(db));
                    }
                }
                ClassBase::Dynamic(_) | ClassBase::Divergent(_) => {}
            }
        }

        disjoint_bases
    }
}

/// Report MRO errors for a dynamic class.
///
/// Returns `true` if the MRO is valid, `false` if there were errors.
pub(super) fn report_dynamic_mro_errors<'db>(
    context: &InferContext<'db, '_>,
    dynamic_class: DynamicClassLiteral<'db>,
    call_expr: &ast::ExprCall,
    bases: &ast::Expr,
) -> bool {
    let db = context.db();
    let Err(error) = dynamic_class.try_mro(db) else {
        return true;
    };

    let bases_tuple_elts = bases.as_tuple_expr().map(|tuple| tuple.elts.as_slice());

    match error.reason() {
        DynamicMroErrorKind::InvalidBases(invalid_bases) => {
            for (idx, base_type) in invalid_bases {
                let instance_of_type = KnownClass::Type.to_instance(db);
                let specific_base = bases_tuple_elts.and_then(|elts| elts.get(*idx));
                let diagnostic_range = specific_base
                    .map(ast::Expr::range)
                    .unwrap_or_else(|| bases.range());

                if base_type.is_assignable_to(db, instance_of_type) {
                    if let Some(builder) =
                        context.report_lint(&UNSUPPORTED_DYNAMIC_BASE, diagnostic_range)
                    {
                        let mut diagnostic = builder.into_diagnostic("Unsupported class base");
                        diagnostic.set_primary_message(format_args!(
                            "Has type `{}`",
                            base_type.display(db)
                        ));
                        diagnostic.info(format_args!(
                            "ty cannot determine a MRO for class `{}` due to this base",
                            dynamic_class.name(db)
                        ));
                        diagnostic.info("Only class objects or `Any` are supported as class bases");
                    }
                } else if let Some(builder) = context.report_lint(&INVALID_BASE, diagnostic_range) {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Invalid class base with type `{}`",
                        base_type.display(db)
                    ));
                    if specific_base.is_none() {
                        diagnostic
                            .info(format_args!("Element {} of the tuple is invalid", idx + 1));
                    }
                }
            }
        }
        DynamicMroErrorKind::InheritanceCycle => {
            if let Some(builder) = context.report_lint(&CYCLIC_CLASS_DEFINITION, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Cyclic definition of `{}`",
                    dynamic_class.name(db)
                ));
            }
        }
        DynamicMroErrorKind::DuplicateBases(duplicates) => {
            if let Some(builder) = context.report_lint(&DUPLICATE_BASE, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Duplicate base class{maybe_s} {dupes} in class `{class}`",
                    maybe_s = if duplicates.len() == 1 { "" } else { "es" },
                    dupes = duplicates
                        .iter()
                        .map(|base: &ClassBase<'_>| base.display(db))
                        .join(", "),
                    class = dynamic_class.name(db),
                ));
            }
        }
        DynamicMroErrorKind::UnresolvableMro => {
            if let Some(builder) = context.report_lint(&INCONSISTENT_MRO, call_expr) {
                builder.into_diagnostic(format_args!(
                    "Cannot create a consistent method resolution order (MRO) \
                        for class `{}` with bases `[{}]`",
                    dynamic_class.name(db),
                    dynamic_class
                        .explicit_bases(db)
                        .iter()
                        .map(|base| base.display(db))
                        .join(", ")
                ));
            }
        }
    }

    false
}
