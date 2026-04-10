use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::place::place_from_declarations;
use crate::semantic_index::definition::{Definition, DefinitionKind};
use crate::semantic_index::place::{PlaceExpr, ScopedPlaceId};
use crate::semantic_index::semantic_index;
use crate::{
    TypeQualifiers,
    types::{Type, diagnostic::INVALID_ASSIGNMENT, infer::TypeInferenceBuilder},
};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Add a secondary annotation to a diagnostic pointing to the `Final` declaration site.
    fn annotate_final_declaration(
        &self,
        diagnostic: &mut Diagnostic,
        declaration: Definition<'db>,
    ) {
        let db = self.db();
        let file = declaration.file(db);
        let module = parsed_module(db, file).load(db);
        let range = match declaration.kind(db) {
            DefinitionKind::AnnotatedAssignment(assignment) => {
                assignment.annotation(&module).range()
            }
            kind => kind.target_range(&module),
        };

        diagnostic.annotate(
            Annotation::secondary(Span::from(file).with_range(range))
                .message("Attribute declared as `Final` here"),
        );
    }

    /// Try to find the unique `Final` declaration for `attribute` on `object_ty`.
    ///
    /// Returns `None` if the attribute is not `Final`, if there are multiple `Final`
    /// declarations, or if the owning class cannot be determined.
    fn precise_final_attribute_declaration(
        &self,
        object_ty: Type<'db>,
        attribute: &str,
    ) -> Option<Definition<'db>> {
        let db = self.db();
        let class_ty = object_ty.nominal_class(db)?;

        for base in class_ty.iter_mro(db) {
            let Some(class) = base.into_class() else {
                continue;
            };
            let Some((class_literal, _)) = class.static_class_literal(db) else {
                continue;
            };

            let class_body_scope = class_literal.body_scope(db);
            let class_scope_id = class_body_scope.file_scope_id(db);
            let class_index = semantic_index(db, class_body_scope.file(db));
            let place_table = class_index.place_table(class_scope_id);
            let Some(symbol_id) = place_table.symbol_id(attribute) else {
                continue;
            };

            let use_def = class_index.use_def_map(class_scope_id);

            let place_and_quals_result =
                place_from_declarations(db, use_def.end_of_scope_symbol_declarations(symbol_id));

            let Some(declaration) = place_and_quals_result.first_declaration else {
                continue;
            };

            if !place_and_quals_result
                .ignore_conflicting_declarations()
                .qualifiers
                .contains(TypeQualifiers::FINAL)
            {
                continue;
            }

            return Some(declaration);
        }

        None
    }

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
        let final_declaration = self.precise_final_attribute_declaration(object_ty, attribute);

        // TODO: Use the full assignment statement range for these diagnostics instead of
        // just the attribute target range.

        let is_in_allowed_initializer = self
            .current_function_definition()
            .is_some_and(|func| func.name.id == "__init__" || func.name.id == "__post_init__");

        let report_not_in_init = || {
            let is_dataclass_like = object_ty
                .nominal_class(db)
                .or_else(|| object_ty.to_class_type(db))
                .and_then(|cls| cls.static_class_literal(db))
                .is_some_and(|(class_literal, _)| class_literal.is_dataclass_like(db));
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
            diagnostic.set_primary_message(if is_dataclass_like {
                "`Final` attributes can only be assigned in the class body, `__init__`, or `__post_init__` on dataclass-like classes"
            } else {
                "`Final` attributes can only be assigned in the class body or `__init__`"
            });
            if let Some(final_declaration) = final_declaration {
                self.annotate_final_declaration(&mut diagnostic, final_declaration);
            }
        };

        if !is_in_allowed_initializer {
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
            let class_body_scope = class_literal.body_scope(db);
            let class_scope_id = class_body_scope.file_scope_id(db);
            let class_index = semantic_index(db, class_body_scope.file(db));
            let pt = class_index.place_table(class_scope_id);

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
                    if let Some(final_declaration) = final_declaration {
                        self.annotate_final_declaration(&mut diagnostic, final_declaration);
                    }
                }

                return true;
            }
        }

        false
    }

    fn invalid_deletion_of_final_attribute(
        &self,
        object_ty: Type<'db>,
        target: &ast::ExprAttribute,
        attribute: &str,
        qualifiers: TypeQualifiers,
        emit_diagnostics: bool,
    ) -> bool {
        if !qualifiers.contains(TypeQualifiers::FINAL) {
            return false;
        }

        if emit_diagnostics {
            let db = self.db();
            let final_declaration = self.precise_final_attribute_declaration(object_ty, attribute);

            if let Some(builder) = self
                .context
                .report_lint(&INVALID_ASSIGNMENT, target.range())
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot delete final attribute `{attribute}` on type `{}`",
                    object_ty.display(db)
                ));
                diagnostic.set_primary_message("`Final` attributes cannot be deleted");
                if let Some(final_declaration) = final_declaration {
                    self.annotate_final_declaration(&mut diagnostic, final_declaration);
                }
            }
        }

        true
    }

    pub(super) fn validate_final_attribute_assignment(
        &mut self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
    ) {
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

    pub(super) fn validate_final_attribute_deletion(
        &self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
        emit_diagnostics: bool,
    ) -> bool {
        let Some((meta_attr, fallback_attr)) =
            self.assignment_attribute_members(object_ty, attribute)
        else {
            return false;
        };

        self.invalid_deletion_of_final_attribute(
            object_ty,
            target,
            attribute,
            meta_attr.qualifiers,
            emit_diagnostics,
        ) || fallback_attr.is_some_and(|fallback_attr| {
            self.invalid_deletion_of_final_attribute(
                object_ty,
                target,
                attribute,
                fallback_attr.qualifiers,
                emit_diagnostics,
            )
        })
    }
}
