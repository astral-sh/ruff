use crate::types::class::{
    ClassLiteral, DynamicClassAnchor, DynamicClassKind, DynamicClassLiteral,
    DynamicMetaclassConflict,
};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, NO_MATCHING_OVERLOAD, report_conflicting_metaclass_from_bases,
    report_instance_layout_conflict,
};
use crate::types::infer::builder::{
    TypeInferenceBuilder,
    dynamic_class::{report_dynamic_mro_errors, report_inconsistent_dynamic_generic_bases},
};
use crate::types::{KnownClass, SubclassOfType, Type, TypeContext, definition_expression_type};
use ruff_python_ast as ast;
use ty_python_core::definition::Definition;

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Infer a call to `builtins.type()`.
    ///
    /// `builtins.type` has two overloads: a single-argument overload (e.g. `type("foo")`,
    /// and a 3-argument `type(name, bases, dict)` overload. Both are handled here.
    /// The `definition` parameter should be `Some()` if this call to `builtins.type()`
    /// occurs on the right-hand side of an assignment statement that has a [`Definition`]
    /// associated with it in the semantic index.
    ///
    /// If it's unclear which overload we should pick, we return `type[Unknown]`,
    /// to avoid cascading errors later on.
    pub(super) fn infer_builtins_type_call(
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

        for keyword in keywords {
            self.infer_expression(&keyword.value, TypeContext::default());
        }

        let [name_arg, bases_arg, namespace_arg] = match &**args {
            [single] => {
                let arg_type = self.infer_expression(single, TypeContext::default());

                return if keywords.is_empty() {
                    arg_type.dunder_class(db, env)
                } else {
                    if keywords.iter().any(|keyword| keyword.arg.is_some())
                        && let Some(builder) =
                            self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr)
                    {
                        let mut diagnostic = builder
                            .into_diagnostic("No overload of class `type` matches arguments");
                        diagnostic.help(format_args!(
                            "`builtins.type()` expects no keyword arguments",
                        ));
                    }
                    SubclassOfType::subclass_of_unknown()
                };
            }

            [first, second] if second.is_starred_expr() => {
                self.infer_expression(first, TypeContext::default());
                self.infer_expression(second, TypeContext::default());

                match &**keywords {
                    [single] if single.arg.is_none() => {
                        return SubclassOfType::subclass_of_unknown();
                    }
                    _ => {
                        if let Some(builder) =
                            self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr)
                        {
                            let mut diagnostic = builder
                                .into_diagnostic("No overload of class `type` matches arguments");
                            diagnostic.help(format_args!(
                                "`builtins.type()` expects no keyword arguments",
                            ));
                        }

                        return SubclassOfType::subclass_of_unknown();
                    }
                }
            }

            [name, bases, namespace] => [name, bases, namespace],

            _ => {
                for arg in args {
                    self.infer_expression(arg, TypeContext::default());
                }

                if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                    let mut diagnostic =
                        builder.into_diagnostic("No overload of class `type` matches arguments");
                    diagnostic.help(format_args!(
                        "`builtins.type()` can either be called with one or three \
                        positional arguments (got {})",
                        args.len()
                    ));
                }

                return SubclassOfType::subclass_of_unknown();
            }
        };

        let name_type = self.infer_expression(name_arg, TypeContext::default());

        let namespace_type = self.infer_expression(namespace_arg, TypeContext::default());

        // TODO: validate other keywords against `__init_subclass__` methods of superclasses
        if keywords
            .iter()
            .any(|keyword| keyword.arg.as_deref() == Some("metaclass"))
        {
            if let Some(builder) = self.context.report_lint(&NO_MATCHING_OVERLOAD, call_expr) {
                let mut diagnostic =
                    builder.into_diagnostic("No overload of class `type` matches arguments");
                diagnostic
                    .help("The `metaclass` keyword argument is not supported in `type()` calls");
            }
        }

        // If any argument is a starred expression, we can't know how many positional arguments
        // we're receiving, so fall back to `type[Unknown]` to avoid false-positive errors.
        if args.iter().any(ast::Expr::is_starred_expr) {
            return SubclassOfType::subclass_of_unknown();
        }

        if !matches!(namespace_type, Type::TypedDict(_))
            && {
                !namespace_type.is_assignable_to(
                    db,
                    env,
                    KnownClass::Dict.to_specialized_instance(
                        db,
                        env,
                        &[KnownClass::Str.to_instance(db, env), Type::any()],
                    ),
                )
            }
            && let Some(builder) = self
                .context
                .report_lint(&INVALID_ARGUMENT_TYPE, namespace_arg)
        {
            let mut diagnostic = builder
                .into_diagnostic("Invalid argument to parameter 3 (`namespace`) of `type()`");
            diagnostic.set_primary_annotation_message(format_args!(
                "Expected `dict[str, Any]`, found `{}`",
                namespace_type.display(db, env)
            ));
        }

        // Extract name and base classes.
        if name_type.as_string_literal().is_none() {
            if !name_type.is_assignable_to(db, env, KnownClass::Str.to_instance(db, env))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_arg)
            {
                let mut diagnostic =
                    builder.into_diagnostic("Invalid argument to parameter 1 (`name`) of `type()`");
                diagnostic.set_primary_annotation_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db, env)
                ));
            }
        }

        let scope = self.scope();

        // Assigned calls defer base validation so forward and recursive references can use the
        // class binding. Dangling calls infer bases here only to validate them immediately; the
        // class shape query reconstructs bases from the source anchor when they are requested.
        let explicit_bases = if definition.is_none() {
            let bases_type = self.infer_expression(bases_arg, TypeContext::default());
            self.extract_explicit_bases(bases_arg, bases_type, DynamicClassKind::TypeCall)
        } else {
            None
        };

        // Create the source anchor that identifies this dynamic class.
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

        let dynamic_class = DynamicClassLiteral::new(db, anchor, DynamicClassKind::TypeCall);

        // For dangling calls, validate bases eagerly. For assigned calls, validation is
        // deferred along with bases inference.
        if let Some(explicit_bases) = &explicit_bases {
            // Validate bases and collect disjoint bases for diagnostics.
            let mut disjoint_bases = self.validate_dynamic_type_bases(
                bases_arg,
                explicit_bases,
                dynamic_class.name(db),
                DynamicClassKind::TypeCall,
            );

            // Check for MRO errors.
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

    /// Deferred inference for assigned `type()` calls.
    ///
    /// Infers the bases argument that was skipped during initial inference to handle
    /// forward references and recursive definitions.
    pub(super) fn infer_builtins_type_deferred(
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

        let [_name_arg, bases_arg, _namespace_arg] = &*call.arguments.args else {
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
            self.extract_explicit_bases(bases_arg, bases_type, DynamicClassKind::TypeCall)
        else {
            return;
        };

        // Validate individual bases for special types that aren't allowed in dynamic classes.
        let name = dynamic_class.name(db);
        self.validate_dynamic_type_bases(bases_arg, &bases, name, DynamicClassKind::TypeCall);
    }
}
