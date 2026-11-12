use ruff_db::diagnostic::{Annotation, Diagnostic, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_python_stdlib::identifiers::is_mangled_private;
use ruff_text_size::Ranged;

use crate::place::{Place, place_from_declarations};
use crate::types::annotation_target::{
    is_final_attribute_initializer, is_instance_attribute_assignment,
};
use crate::{
    TypeQualifiers,
    types::{
        StaticClassLiteral, Type, attribute_write::assignment_attribute_members,
        diagnostic::INVALID_ASSIGNMENT, infer::TypeInferenceBuilder,
    },
};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::{place_table, use_def_map};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Point to the `Final` declaration when its location is known.
    fn annotate_final_declaration(
        &self,
        diagnostic: &mut Diagnostic,
        declaration: Option<Definition<'db>>,
    ) {
        let Some(declaration) = declaration else {
            return;
        };
        let db = self.db();
        let file = declaration.file(db);
        let module = parsed_module(db, declaration.python_file(db)).load(db);
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

    /// Find the class that declares a final attribute, and its declaration site when available.
    fn final_attribute_declaration(
        &self,
        object_ty: Type<'db>,
        attribute: &str,
    ) -> Option<(StaticClassLiteral<'db>, Option<Definition<'db>>)> {
        let db = self.db();
        let env = self.program_environment();
        let class_ty = object_ty.nominal_class(db, env)?;

        for base in class_ty.iter_mro(db) {
            let Some(class) = base.into_class() else {
                continue;
            };
            let Some((class_literal, _)) = class.static_class_literal(db) else {
                continue;
            };

            let class_body_scope = class_literal.body_scope(db);
            if let Some(symbol_id) = place_table(db, class_body_scope).symbol_id(attribute) {
                let use_def = use_def_map(db, class_body_scope);
                let declarations = place_from_declarations(
                    db,
                    env,
                    use_def.end_of_scope_symbol_declarations(symbol_id),
                );
                if declarations.qualifiers().contains(TypeQualifiers::FINAL) {
                    return Some((class_literal, declarations.first_declaration));
                }
            }

            let member = class.own_instance_member(db, env, attribute);
            if member.qualifiers().contains(TypeQualifiers::FINAL)
                && let Place::Defined(place) = member.inner.place
            {
                let declaration = place
                    .provenance
                    .definition()
                    .filter(|def| matches!(def.kind(db), DefinitionKind::AnnotatedAssignment(_)));
                return Some((class_literal, declaration));
            }
        }

        None
    }

    pub(super) fn invalid_assignment_to_final_attribute(
        &self,
        object_ty: Type<'db>,
        target: &ast::ExprAttribute,
        attribute: &str,
        qualifiers: TypeQualifiers,
    ) -> bool {
        let env = self.program_environment();
        let db = self.db();
        if !qualifiers.contains(TypeQualifiers::FINAL) {
            return false;
        }
        let owner_and_declaration = self.final_attribute_declaration(object_ty, attribute);
        let final_declaration = owner_and_declaration.and_then(|(_, definition)| definition);

        // TODO: Use the full assignment statement range for these diagnostics instead of
        // just the attribute target range.

        let is_in_allowed_initializer = self
            .current_function_definition()
            .is_some_and(|method| is_final_attribute_initializer(db, self.index, method));

        let report_not_in_init = || {
            let is_dataclass_like = object_ty
                .nominal_class(db, env)
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
                object_ty.display(db, env)
            ));
            diagnostic.set_primary_annotation_message(if is_dataclass_like {
                "`Final` attributes can only be assigned in the class body, `__init__`, or `__post_init__` on dataclass-like classes"
            } else {
                "`Final` attributes can only be assigned in the class body or `__init__`"
            });
            self.annotate_final_declaration(&mut diagnostic, final_declaration);
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
        let is_self_parameter =
            is_instance_attribute_assignment(db, self.index, self.scope(), target);

        // Final ownership is nominal: checking structural protocol requirements can
        // incorrectly reject the declaring class's own receiver.
        let is_current_class_instance = is_self_parameter
            && object_ty
                .nominal_class(db, env)
                .is_some_and(|object_class| {
                    object_class.is_subtype_of_class_literal(db, class_ty.class_literal(db))
                });
        if !is_current_class_instance {
            report_not_in_init();
            return true;
        }

        if let Some((class_literal, _)) = class_ty.static_class_literal(db) {
            if owner_and_declaration.is_some_and(|(owner, _)| owner != class_literal) {
                // Name mangling gives each class its own attribute for this spelling.
                if is_mangled_private(attribute) {
                    return false;
                }
                if let Some(builder) = self
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, target.range())
                {
                    let mut diagnostic = builder.into_diagnostic(format_args!(
                        "Cannot assign to inherited final attribute `{attribute}`"
                    ));
                    diagnostic.set_primary_annotation_message(
                        "Only the class that declares a final attribute can initialize it",
                    );
                    self.annotate_final_declaration(&mut diagnostic, final_declaration);
                }
                return true;
            }
            let table = place_table(db, class_literal.body_scope(db));
            if let Some(symbol) = table.symbol_by_name(attribute)
                && symbol.is_bound()
            {
                if let Some(diag_builder) = self
                    .context
                    .report_lint(&INVALID_ASSIGNMENT, target.range())
                {
                    let mut diagnostic =
                        diag_builder.into_diagnostic("Invalid assignment to final attribute");
                    diagnostic.set_primary_annotation_message(format_args!(
                        "`{attribute}` already has a value in the class body"
                    ));
                    self.annotate_final_declaration(&mut diagnostic, final_declaration);
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
        let db = self.db();
        if !qualifiers.contains(TypeQualifiers::FINAL) {
            return false;
        }

        if emit_diagnostics
            && let Some(builder) = self
                .context
                .report_lint(&INVALID_ASSIGNMENT, target.range())
        {
            let final_declaration = self
                .final_attribute_declaration(object_ty, attribute)
                .and_then(|(_, definition)| definition);
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Cannot delete final attribute `{attribute}` on type `{}`",
                object_ty.display(db, self.program_environment())
            ));
            diagnostic.set_primary_annotation_message("`Final` attributes cannot be deleted");
            self.annotate_final_declaration(&mut diagnostic, final_declaration);
        }

        true
    }

    pub(super) fn validate_final_attribute_assignment(
        &mut self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
    ) -> bool {
        let db = self.db();
        // Check each possible receiver type separately. If one of them makes the
        // attribute `Final`, another type must not make it writable.
        match object_ty {
            Type::TypeAlias(alias) => {
                return self.validate_final_attribute_assignment(
                    target,
                    alias.value_type(db),
                    attribute,
                );
            }
            Type::Union(union) => {
                return union.elements(db).iter().any(|element| {
                    self.validate_final_attribute_assignment(target, *element, attribute)
                });
            }
            Type::Intersection(intersection) => {
                return intersection.positive(db).iter().any(|element| {
                    self.validate_final_attribute_assignment(target, *element, attribute)
                });
            }
            _ => {}
        }
        let Some(members) =
            assignment_attribute_members(db, self.program_environment(), object_ty, attribute)
        else {
            return false;
        };

        members.effective_members().any(|member| {
            self.invalid_assignment_to_final_attribute(
                object_ty,
                target,
                attribute,
                member.qualifiers,
            )
        })
    }

    pub(super) fn validate_final_attribute_deletion(
        &self,
        target: &ast::ExprAttribute,
        object_ty: Type<'db>,
        attribute: &str,
        emit_diagnostics: bool,
    ) -> bool {
        let db = self.db();
        match object_ty {
            Type::Union(union) => {
                return union.elements(db).iter().any(|element| {
                    self.validate_final_attribute_deletion(
                        target,
                        *element,
                        attribute,
                        emit_diagnostics,
                    )
                });
            }
            Type::Intersection(intersection) => {
                return intersection.positive(db).iter().any(|element| {
                    self.validate_final_attribute_deletion(
                        target,
                        *element,
                        attribute,
                        emit_diagnostics,
                    )
                });
            }
            _ => {}
        }
        let Some(members) =
            assignment_attribute_members(db, self.program_environment(), object_ty, attribute)
        else {
            return false;
        };

        members.effective_members().any(|member| {
            self.invalid_deletion_of_final_attribute(
                object_ty,
                target,
                attribute,
                member.qualifiers,
                emit_diagnostics,
            )
        })
    }
}
