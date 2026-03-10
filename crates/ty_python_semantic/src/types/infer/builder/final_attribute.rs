use ruff_db::diagnostic::{Annotation, Span};
use ruff_db::parsed::parsed_module;
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};

use crate::place::place_from_declarations;
use crate::semantic_index::definition::DefinitionKind;
use crate::semantic_index::place::{PlaceExpr, ScopedPlaceId};
use crate::semantic_index::{place_table, use_def_map};
use crate::types::class::ClassType;
use crate::{
    TypeQualifiers,
    types::{
        Type, TypeVarBoundOrConstraints, diagnostic::INVALID_ASSIGNMENT,
        infer::TypeInferenceBuilder,
    },
};

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Try to find the declaration of a `Final` attribute in a class body scope
    /// and annotate the diagnostic with a secondary annotation pointing to it.
    fn annotate_final_declaration(
        &self,
        diagnostic: &mut impl std::ops::DerefMut<Target = ruff_db::diagnostic::Diagnostic>,
        class_ty: ClassType<'db>,
        attribute: &str,
    ) {
        let db = self.db();
        let Some((class_literal, _)) = class_ty.static_class_literal(db) else {
            return;
        };

        let body_scope = class_literal.body_scope(db);
        let table = place_table(db, body_scope);

        let Some(symbol_id) = table.symbol_id(attribute) else {
            return;
        };

        let udm = use_def_map(db, body_scope);
        let declarations = udm.end_of_scope_symbol_declarations(symbol_id);
        let result = place_from_declarations(db, declarations);

        let Some(first_declaration) = result.first_declaration else {
            return;
        };

        let file = body_scope.file(db);
        let module = parsed_module(db, file).load(db);

        let range =
            if let DefinitionKind::AnnotatedAssignment(assignment) = first_declaration.kind(db) {
                assignment.annotation(&module).range()
            } else {
                first_declaration.kind(db).target_range(&module)
            };

        diagnostic.annotate(
            Annotation::secondary(Span::from(file).with_range(range))
                .message("Attribute declared as `Final` here"),
        );
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
        diagnostic_range: TextRange,
        object_ty: Type<'db>,
        target: Option<&ast::ExprAttribute>,
        attribute: &str,
        qualifiers: TypeQualifiers,
    ) -> bool {
        if !qualifiers.contains(TypeQualifiers::FINAL) {
            return false;
        }

        let db = self.db();

        let is_in_init = self
            .current_function_definition()
            .is_some_and(|func| func.name.id == "__init__");

        if !is_in_init {
            if let Some(builder) = self
                .context
                .report_lint(&INVALID_ASSIGNMENT, diagnostic_range)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot assign to final attribute `{attribute}` on type `{}`; \
                    `Final` attributes can only be assigned in the class body or `__init__`",
                    object_ty.display(db)
                ));
                if let Some(class_ty) = self.class_owning_final_attribute(object_ty, attribute) {
                    self.annotate_final_declaration(&mut diagnostic, class_ty, attribute);
                }
            }
            return true;
        }

        let Some(class_ty) = self.class_context_of_current_method() else {
            if let Some(builder) = self
                .context
                .report_lint(&INVALID_ASSIGNMENT, diagnostic_range)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot assign to final attribute `{attribute}` on type `{}`",
                    object_ty.display(db)
                ));
                if let Some(class_ty) = self.class_owning_final_attribute(object_ty, attribute) {
                    self.annotate_final_declaration(&mut diagnostic, class_ty, attribute);
                }
            }
            return true;
        };

        // Check that the target attribute expression is an instance attribute assignment
        // (i.e. the object is the implicit `self`/`cls` receiver), not just any parameter
        // that happens to have the right type.
        let is_self_parameter =
            target.is_none_or(|target| self.is_instance_attribute_assignment(target));

        let class_instance_ty = Type::instance(db, class_ty).top_materialization(db);
        let object_instance_ty = object_ty.bind_self_typevars(db, class_instance_ty);
        let is_current_class_instance =
            is_self_parameter && object_instance_ty.is_subtype_of(db, class_instance_ty);
        if !is_current_class_instance {
            if let Some(builder) = self
                .context
                .report_lint(&INVALID_ASSIGNMENT, diagnostic_range)
            {
                let mut diagnostic = builder.into_diagnostic(format_args!(
                    "Cannot assign to final attribute `{attribute}` on type `{}`",
                    object_ty.display(db)
                ));
                if let Some(class_ty) = self.class_owning_final_attribute(object_ty, attribute) {
                    self.annotate_final_declaration(&mut diagnostic, class_ty, attribute);
                }
            }
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
                    .report_lint(&INVALID_ASSIGNMENT, diagnostic_range)
                {
                    let mut diagnostic = diag_builder.into_diagnostic(format_args!(
                        "Cannot assign to final attribute `{attribute}` in `__init__` \
                        because it already has a value at class level"
                    ));
                    self.annotate_final_declaration(&mut diagnostic, class_ty, attribute);
                }

                return true;
            }
        }

        false
    }

    /// Find the class that owns the `Final` declaration for an attribute,
    /// given the object type being assigned to.
    fn class_owning_final_attribute(
        &self,
        object_ty: Type<'db>,
        attribute: &str,
    ) -> Option<ClassType<'db>> {
        let db = self.db();
        let class_ty = object_ty
            .nominal_class(db)
            .or_else(|| {
                let Type::TypeVar(typevar) = object_ty else {
                    return None;
                };

                let TypeVarBoundOrConstraints::UpperBound(bound) =
                    typevar.typevar(db).bound_or_constraints(db)?
                else {
                    return None;
                };

                bound.nominal_class(db)
            })
            .or_else(|| object_ty.to_class_type(db))?;

        // Verify the class body scope has this symbol declared.
        let (class_literal, _) = class_ty.static_class_literal(db)?;
        let body_scope = class_literal.body_scope(db);
        let table = place_table(db, body_scope);

        if table.symbol_id(attribute).is_some() {
            Some(class_ty)
        } else {
            None
        }
    }

    pub(super) fn validate_final_attribute_assignment(
        &mut self,
        target: &ast::ExprAttribute,
        diagnostic_range: TextRange,
        object_ty: Type<'db>,
        attribute: &str,
        emit_diagnostics: bool,
    ) {
        if !emit_diagnostics {
            return;
        }

        let db = self.db();

        match object_ty {
            Type::Union(union) => {
                for elem in union.elements(db) {
                    self.validate_final_attribute_assignment(
                        target,
                        diagnostic_range,
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
                        diagnostic_range,
                        *elem,
                        attribute,
                        emit_diagnostics,
                    );
                }
            }
            Type::TypeAlias(alias) => self.validate_final_attribute_assignment(
                target,
                diagnostic_range,
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
                    diagnostic_range,
                    object_ty,
                    Some(target),
                    attribute,
                    meta_attr.qualifiers,
                ) && let Some(fallback_attr) = fallback_attr
                {
                    self.invalid_assignment_to_final_attribute(
                        diagnostic_range,
                        object_ty,
                        Some(target),
                        attribute,
                        fallback_attr.qualifiers,
                    );
                }
            }
            Type::Dynamic(..) | Type::Never | Type::ModuleLiteral(_) | Type::BoundSuper(_) => {}
        }
    }
}
