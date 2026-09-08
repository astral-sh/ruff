use crate::types::class::{
    ClassLiteral, DynamicClassAnchor, DynamicClassKind, DynamicClassLiteral,
    DynamicMetaclassConflict, dynamic_class_bases_argument,
};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, NO_MATCHING_OVERLOAD, report_conflicting_metaclass_from_bases,
    report_instance_layout_conflict,
};
use crate::types::infer::builder::{
    ArgumentsIter, TypeInferenceBuilder,
    dynamic_class::{report_dynamic_mro_errors, report_inconsistent_dynamic_generic_bases},
};
use crate::types::{KnownClass, SubclassOfType, Type, TypeContext, definition_expression_type};
use ruff_python_ast as ast;
use ty_python_core::definition::Definition;

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer a `types.new_class(name, bases, kwds, exec_body)` call.
    ///
    /// This method *does not* call `infer_expression` on the object being called;
    /// it is assumed that the type for this AST node has already been inferred before this method
    /// is called.
    pub(super) fn infer_new_class_call(
        &mut self,
        call_expr: &ast::ExprCall,
        definition: Option<Definition<'db>>,
    ) -> Type<'db> {
        let env = self.program_environment();
        let db = self.db();

        let ast::Arguments {
            args,
            keywords,
            range: _,
            node_index: _,
        } = &call_expr.arguments;

        // `new_class(name, bases=(), kwds=None, exec_body=None)`
        // We need at least the `name` argument.
        let no_positional_args = args.is_empty();
        if no_positional_args {
            // Check if `name` is provided as a keyword argument.
            let name_keyword = keywords.iter().find(|kw| kw.arg.as_deref() == Some("name"));

            if name_keyword.is_none() {
                // Infer all keyword values for side effects.
                for keyword in keywords {
                    self.infer_expression(&keyword.value, TypeContext::default());
                }
                if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                    builder.into_diagnostic("No overload of `types.new_class` matches arguments");
                }
                return SubclassOfType::subclass_of_unknown();
            }
        }

        // Find the arguments we treat specially while preserving normal call-binding diagnostics.
        let name_node = args.first().or_else(|| {
            keywords
                .iter()
                .find(|kw| kw.arg.as_deref() == Some("name"))
                .map(|kw| &kw.value)
        });
        let bases_arg = dynamic_class_bases_argument(&call_expr.arguments);

        self.validate_new_class_call_arguments(call_expr, name_node, bases_arg, definition);

        let name_type = name_node
            .map(|node| self.expression_type(node))
            .unwrap_or_else(Type::unknown);

        if name_type.as_string_literal().is_none() {
            if let Some(name_node) = name_node
                && !name_type.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_node)
            {
                let mut diagnostic = builder.into_diagnostic(
                    "Invalid argument to parameter 1 (`name`) of `types.new_class()`",
                );
                diagnostic.set_primary_annotation_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db, env)
                ));
            }
        }

        // Assigned calls defer base validation so forward and recursive references can use the
        // class binding. Dangling calls infer bases here only to validate them immediately; the
        // class shape query reconstructs bases from the source anchor when they are requested.
        let explicit_bases: Option<Box<[Type<'db>]>> = if definition.is_none() {
            if let Some(bases_arg) = bases_arg {
                let bases_type = self.expression_type(bases_arg);
                self.extract_explicit_bases(bases_arg, bases_type, DynamicClassKind::NewClass)
            } else {
                Some(Box::from([]))
            }
        } else {
            None
        };

        let scope = self.scope();

        // Create the anchor for identifying this dynamic class.
        let anchor = if let Some(def) = definition {
            // Register for deferred inference to infer bases and validate later.
            self.deferred.insert(def);
            DynamicClassAnchor::Definition(def)
        } else {
            DynamicClassAnchor::ScopeOffset {
                scope,
                offset: self.dynamic_class_scope_offset(call_expr),
            }
        };

        // TODO: Model `kwds`, especially `{"metaclass": Meta}`. `types.new_class()` uses the
        // third argument for explicit metaclass overrides, but we currently only account for
        // metaclass behavior that follows from the resolved bases.
        let dynamic_class = DynamicClassLiteral::new(db, anchor, DynamicClassKind::NewClass);

        // For dangling calls, validate bases eagerly. For assigned calls, validation is
        // deferred along with bases inference.
        if let Some(explicit_bases) = &explicit_bases
            && let Some(bases_arg) = bases_arg
        {
            let mut disjoint_bases = self.validate_dynamic_type_bases(
                bases_arg,
                explicit_bases,
                dynamic_class.name(db),
                DynamicClassKind::NewClass,
            );

            if report_dynamic_mro_errors(&self.context, dynamic_class, call_expr, bases_arg) {
                report_inconsistent_dynamic_generic_bases(&self.context, dynamic_class, bases_arg);

                // MRO succeeded, check for instance-layout-conflict.
                disjoint_bases.remove_redundant_entries(db);
                if disjoint_bases.len() > 1 {
                    report_instance_layout_conflict(
                        &self.context,
                        dynamic_class.header_range(db),
                        bases_arg.as_tuple_expr().map(|tuple| tuple.elts.as_slice()),
                        &disjoint_bases,
                    );
                }
            }

            // Check for metaclass conflicts.
            if let Err(DynamicMetaclassConflict {
                metaclass1,
                base1,
                metaclass2,
                base2,
            }) = dynamic_class.try_metaclass(db)
            {
                report_conflicting_metaclass_from_bases(
                    &self.context,
                    call_expr.into(),
                    dynamic_class.name(db),
                    metaclass1,
                    base1.display(db, env),
                    metaclass2,
                    base2.display(db, env),
                );
            }
        }

        Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class))
    }

    /// Deferred inference for assigned `types.new_class()` calls.
    ///
    /// Infers the bases argument that was skipped during initial inference to handle
    /// forward references and recursive definitions.
    pub(super) fn infer_new_class_deferred(
        &mut self,
        definition: Definition<'db>,
        call_expr: &ast::Expr,
    ) {
        let db = self.db();

        let ast::Expr::Call(call) = call_expr else {
            return;
        };

        // Get the already-inferred class type from the initial pass.
        let inferred_type = definition_expression_type(self.db(), definition, call_expr);
        let Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class)) = inferred_type else {
            return;
        };

        let Some(bases_arg) = dynamic_class_bases_argument(&call.arguments) else {
            return;
        };

        // Set the typevar binding context to allow legacy typevar binding in expressions
        // like `Generic[T]`. This matches the context used during initial inference.
        let previous_context = self.typevar_binding_context.replace(definition);

        // Infer the bases argument (this was skipped during initial inference).
        let bases_type = self.infer_expression(bases_arg, TypeContext::default());

        // Restore the previous context.
        self.typevar_binding_context = previous_context;

        // Extract and validate bases.
        let Some(bases) =
            self.extract_explicit_bases(bases_arg, bases_type, DynamicClassKind::NewClass)
        else {
            return;
        };

        // Validate individual bases for special types that aren't allowed in dynamic classes.
        let name = dynamic_class.name(db);
        self.validate_dynamic_type_bases(bases_arg, &bases, name, DynamicClassKind::NewClass);
    }

    /// Preserve normal call-binding diagnostics for `types.new_class()` while still allowing
    /// special inference of the name and bases arguments.
    fn validate_new_class_call_arguments(
        &mut self,
        call_expr: &ast::ExprCall,
        name_node: Option<&ast::Expr>,
        bases_arg: Option<&ast::Expr>,
        definition: Option<Definition<'db>>,
    ) {
        let db = self.db();
        let env = self.program_environment();
        let callable_type = self.expression_type(call_expr.func.as_ref());
        let iterable_object =
            KnownClass::Iterable.to_specialized_instance(db, env, &[Type::object()]);
        let mut call_arguments = self.prepare_call_arguments(&call_expr.arguments);

        let mut bindings =
            callable_type
                .bindings(db, env)
                .match_parameters(db, env, &call_arguments);
        let bindings_result = self.infer_and_check_argument_types(
            ArgumentsIter::from_ast(&call_expr.arguments),
            &mut call_arguments,
            &mut |builder, (_, expr, tcx)| {
                if name_node.is_some_and(|name| std::ptr::eq(expr, name)) {
                    let _ = builder.infer_expression(expr, tcx);
                    KnownClass::Str.to_instance(db, env)
                } else if bases_arg.is_some_and(|bases| std::ptr::eq(expr, bases)) {
                    if definition.is_none() {
                        let _ = builder.infer_expression(expr, tcx);
                    }
                    iterable_object
                } else {
                    builder.infer_expression(expr, tcx)
                }
            },
            &mut bindings,
            TypeContext::default(),
        );

        if bindings_result.is_err() {
            bindings.report_diagnostics(&self.context, call_expr.into());
        }
    }
}
