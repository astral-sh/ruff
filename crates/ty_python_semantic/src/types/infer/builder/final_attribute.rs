use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::semantic_index::place::{PlaceExpr, ScopedPlaceId};
use crate::{
    TypeQualifiers,
    types::{Type, diagnostic::INVALID_ASSIGNMENT, infer::TypeInferenceBuilder},
};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Check if the target attribute expression (e.g. `self.x`) is an instance attribute
    /// assignment, i.e. the object is the implicit `self`/`cls` receiver.
    ///
    /// This delegates to the `is_instance_attribute` flag computed during semantic indexing,
    /// which checks that the object expression refers to the first parameter of the
    /// enclosing method and has not been shadowed in intermediate scopes.
    fn is_instance_attribute_assignment(&self, target: &ast::ExprAttribute) -> bool {
        let Some(place_expr) = PlaceExpr::try_from_expr(target) else {
            return false;
        };
        let file_scope_id = self.scope().file_scope_id(self.db());
        let place_table = self.index.place_table(file_scope_id);
        let Some(ScopedPlaceId::Member(member_id)) = place_table.place_id(&place_expr) else {
            return false;
        };
        place_table.member(member_id).is_instance_attribute()
    }

    pub(super) fn invalid_assignment_to_final_attribute(
        &self,
        object_ty: Type<'db>,
        target: &ast::ExprAttribute,
        attribute: &str,
        qualifiers: TypeQualifiers,
    ) -> bool {
        if !qualifiers.contains(TypeQualifiers::FINAL) {
            return false;
        }

        let db = self.db();

        // TODO: Point to the `Final` declaration once we can reliably resolve the owning
        // declaration for this attribute, including inherited members and locally introduced
        // `Final` annotations on assignments.

        // TODO: Use the full assignment statement range for these diagnostics instead of
        // just the attribute target range.

        let is_in_init = self
            .current_function_definition()
            .is_some_and(|func| func.name.id == "__init__");

        let report_not_in_init = || {
            let Some(builder) = self
                .context
                .report_lint(&INVALID_ASSIGNMENT, target.range())
            else {
                return;
            };
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Cannot assign to final attribute `{attribute}` on type `{}`",
                object_ty.display(db)
            ));
            diagnostic.set_primary_message(
                "`Final` attributes can only be assigned in the class body or `__init__`",
            );
        };

        if !is_in_init {
            report_not_in_init();
            return true;
        }

        let Some(class_ty) = self.class_context_of_current_method() else {
            report_not_in_init();
            return true;
        };

        // Check that the target attribute expression is an instance attribute assignment
        // (i.e. the object is the implicit `self`/`cls` receiver), not just any parameter
        // that happens to have the right type.
        let is_self_parameter = self.is_instance_attribute_assignment(target);

        let class_instance_ty = Type::instance(db, class_ty).top_materialization(db);
        let object_instance_ty = object_ty.bind_self_typevars(db, class_instance_ty);
        let is_current_class_instance =
            is_self_parameter && object_instance_ty.is_subtype_of(db, class_instance_ty);
        if !is_current_class_instance {
            report_not_in_init();
            return true;
        }

        if let Some((class_literal, _)) = class_ty.static_class_literal(db) {
            let class_scope_id = class_literal.body_scope(db).file_scope_id(db);
            let pt = self.index.place_table(class_scope_id);

            if let Some(symbol) = pt.symbol_by_name(attribute)
                && symbol.is_bound()
            {
                if let Some(diag_builder) = self
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, target.range())
                {
                    let mut diagnostic =
                        diag_builder.into_diagnostic("Invalid assignment to final attribute");
                    diagnostic.set_primary_message(format_args!(
                        "`{attribute}` already has a value in the class body"
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
    ) {
        let db = self.db();

        match object_ty {
            Type::Union(union) => {
                for elem in union.elements(db) {
                    self.validate_final_attribute_assignment(target, *elem, attribute);
                }
            }
            Type::Intersection(intersection) => {
                for elem in intersection.positive(db) {
                    self.validate_final_attribute_assignment(target, *elem, attribute);
                }
            }
            Type::TypeAlias(..)
            | Type::NominalInstance(..)
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
                    object_ty,
                    target,
                    attribute,
                    meta_attr.qualifiers,
                ) && let Some(fallback_attr) = fallback_attr
                {
                    self.invalid_assignment_to_final_attribute(
                        object_ty,
                        target,
                        attribute,
                        fallback_attr.qualifiers,
                    );
                }
            }
            Type::Dynamic(..) | Type::Never | Type::ModuleLiteral(_) | Type::BoundSuper(_) => {}
        }
    }
}
