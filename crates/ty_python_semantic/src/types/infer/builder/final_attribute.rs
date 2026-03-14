use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use crate::{
    TypeQualifiers,
    types::{Type, diagnostic::INVALID_ASSIGNMENT, infer::TypeInferenceBuilder},
};

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn invalid_assignment_to_final_attribute(
        &self,
        diagnostic_range: TextRange,
        object_ty: Type<'db>,
        attribute: &str,
        qualifiers: TypeQualifiers,
        emit_diagnostics: bool,
    ) -> bool {
        if !qualifiers.contains(TypeQualifiers::FINAL) {
            return false;
        }

        let db = self.db();

        let emit_invalid_final = || {
            if emit_diagnostics
                && let Some(builder) = self
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, diagnostic_range)
            {
                builder.into_diagnostic(format_args!(
                    "Cannot assign to final attribute `{attribute}` on type `{}`",
                    object_ty.display(db)
                ));
            }
        };

        let is_in_init = self
            .current_function_definition()
            .is_some_and(|func| func.name.id == "__init__");

        if !is_in_init {
            emit_invalid_final();
            return true;
        }

        let Some(class_ty) = self.class_context_of_current_method() else {
            emit_invalid_final();
            return true;
        };

        let class_instance_ty = Type::instance(db, class_ty);
        let class_literal = class_ty.class_literal(db);
        // When `__init__` has a `self: Self` annotation, `object_ty` may be a `Self` typevar
        // (a class type) rather than an instance type. Bind the typevar, then try `to_instance`
        // to convert e.g. `type[C]` -> `C`; fall back to the bound type if it's already an
        // instance.
        let object_instance_ty = object_ty
            .bind_self_typevars(db, class_instance_ty)
            .to_instance(db)
            .unwrap_or_else(|| object_ty.bind_self_typevars(db, class_instance_ty));
        let is_current_class_instance =
            object_instance_ty
                .as_nominal_instance()
                .is_some_and(|instance| {
                    instance
                        .class(db)
                        .iter_mro(db)
                        .filter_map(crate::types::ClassBase::into_class)
                        .any(|mro_class| mro_class.class_literal(db) == class_literal)
                })
                || object_instance_ty.is_subtype_of(db, class_instance_ty);
        if !is_current_class_instance {
            emit_invalid_final();
            return true;
        }

        if let Some((class_literal, _)) = class_ty.static_class_literal(db) {
            let class_scope_id = class_literal.body_scope(db).file_scope_id(db);
            let place_table = self.index.place_table(class_scope_id);

            if let Some(symbol) = place_table.symbol_by_name(attribute)
                && symbol.is_bound()
            {
                if emit_diagnostics
                    && let Some(diag_builder) = self
                        .context
                        .report_lint(&INVALID_ASSIGNMENT, diagnostic_range)
                {
                    diag_builder.into_diagnostic(format_args!(
                        "Cannot assign to final attribute `{attribute}` in `__init__` \
                        because it already has a value at class level"
                    ));
                }

                return true;
            }
        }

        false
    }

    pub(super) fn validate_final_attribute_assignment(
        &mut self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
        emit_diagnostics: bool,
    ) {
        let db = self.db();

        match object_ty {
            Type::Union(union) => {
                for elem in union.elements(db) {
                    self.validate_final_attribute_assignment(
                        target,
                        *elem,
                        attribute,
                        emit_diagnostics,
                    );
                }
            }
            Type::Intersection(intersection) => {
                for elem in intersection.positive(db) {
                    self.validate_final_attribute_assignment(
                        target,
                        *elem,
                        attribute,
                        emit_diagnostics,
                    );
                }
            }
            Type::TypeAlias(alias) => self.validate_final_attribute_assignment(
                target,
                alias.value_type(db),
                attribute,
                emit_diagnostics,
            ),
            Type::NominalInstance(..)
            | Type::ProtocolInstance(_)
            | Type::LiteralValue(..)
            | Type::SpecialForm(..)
            | Type::KnownInstance(..)
            | Type::PropertyInstance(..)
            | Type::FunctionLiteral(..)
            | Type::Callable(..)
            | Type::BoundMethod(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::DataclassDecorator(_)
            | Type::DataclassTransformer(_)
            | Type::TypeVar(..)
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypedDict(_)
            | Type::NewTypeInstance(_)
            | Type::ClassLiteral(..)
            | Type::GenericAlias(..)
            | Type::SubclassOf(..) => {
                let Some((meta_attr, fallback_attr)) =
                    self.assignment_attribute_members(object_ty, attribute)
                else {
                    return;
                };

                if !self.invalid_assignment_to_final_attribute(
                    target.range(),
                    object_ty,
                    attribute,
                    meta_attr.qualifiers,
                    emit_diagnostics,
                ) && let Some(fallback_attr) = fallback_attr
                {
                    self.invalid_assignment_to_final_attribute(
                        target.range(),
                        object_ty,
                        attribute,
                        fallback_attr.qualifiers,
                        emit_diagnostics,
                    );
                }
            }
            Type::Dynamic(..) | Type::Never | Type::ModuleLiteral(_) | Type::BoundSuper(_) => {}
        }
    }
}
