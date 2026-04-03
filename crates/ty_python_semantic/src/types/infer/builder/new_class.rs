use super::{ArgumentsIter, DynamicClassKind, TypeInferenceBuilder};
use crate::semantic_index::definition::Definition;
use crate::types::call::CallArguments;
use crate::types::class::{
    ClassLiteral, DynamicClassAnchor, DynamicClassLiteral, DynamicMetaclassConflict,
};
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, NO_MATCHING_OVERLOAD, report_conflicting_metaclass_from_bases,
    report_instance_layout_conflict,
};
use crate::types::{KnownClass, SubclassOfType, Type, TypeContext, definition_expression_type};
use ruff_python_ast::{self as ast, HasNodeIndex, NodeIndex};

impl<'db> TypeInferenceBuilder<'db, '_> {
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
        let inferred_type = definition_expression_type(db, definition, call_expr);
        let Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class)) = inferred_type else {
            return;
        };

        // Find the bases argument: second positional, or `bases=` keyword.
        let bases_arg = call.arguments.args.get(1).or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|kw| kw.arg.as_deref() == Some("bases"))
                .map(|kw| &kw.value)
        });

        let Some(bases_arg) = bases_arg else {
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
        let bases_arg = args.get(1).or_else(|| {
            keywords
                .iter()
                .find(|kw| kw.arg.as_deref() == Some("bases"))
                .map(|kw| &kw.value)
        });

        self.validate_new_class_call_arguments(call_expr, name_node, bases_arg, definition);

        let name_type = name_node
            .map(|node| self.expression_type(node))
            .unwrap_or_else(Type::unknown);

        let name = if let Some(literal) = name_type.as_string_literal() {
            ast::name::Name::new(literal.value(db))
        } else {
            if let Some(name_node) = name_node
                && !name_type.is_assignable_to(db, KnownClass::Str.to_instance(db))
                && let Some(builder) = self.context.report_lint(&INVALID_ARGUMENT_TYPE, name_node)
            {
                let mut diagnostic = builder.into_diagnostic(
                    "Invalid argument to parameter 1 (`name`) of `types.new_class()`",
                );
                diagnostic.set_primary_message(format_args!(
                    "Expected `str`, found `{}`",
                    name_type.display(db)
                ));
            }
            ast::name::Name::new_static("<unknown>")
        };

        // For assigned `new_class()` calls, bases inference is deferred to handle forward
        // references and recursive references, matching the `type()` pattern. For dangling
        // calls, infer and extract bases eagerly (they'll be stored in the anchor).
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
            let call_node_index = call_expr.node_index().load();
            let scope_anchor = scope.node(db).node_index().unwrap_or(NodeIndex::from(0));
            let anchor_u32 = scope_anchor
                .as_u32()
                .expect("scope anchor should not be NodeIndex::NONE");
            let call_u32 = call_node_index
                .as_u32()
                .expect("call node should not be NodeIndex::NONE");

            // Use [Unknown] as fallback if bases extraction failed (e.g., not a tuple).
            let anchor_bases = explicit_bases
                .clone()
                .unwrap_or_else(|| Box::from([Type::unknown()]));

            DynamicClassAnchor::ScopeOffset {
                scope,
                offset: call_u32 - anchor_u32,
                explicit_bases: anchor_bases,
            }
        };

        // `new_class()` doesn't accept a namespace dict, so members are always empty.
        // If `exec_body` is provided (and is not `None`), it can populate the namespace
        // dynamically, so we mark it as dynamic. Without `exec_body`, no members can be added.
        let exec_body_arg = args.get(3).or_else(|| {
            keywords
                .iter()
                .find(|kw| kw.arg.as_deref() == Some("exec_body"))
                .map(|kw| &kw.value)
        });
        let has_exec_body = exec_body_arg.is_some_and(|arg| !arg.is_none_literal_expr());
        let members: Box<[(ast::name::Name, Type<'db>)]> = Box::new([]);
        let dynamic_class =
            DynamicClassLiteral::new(db, name.clone(), anchor, members, has_exec_body, None);

        // For dangling calls, validate bases eagerly. For assigned calls, validation is
        // deferred along with bases inference.
        if let Some(explicit_bases) = &explicit_bases
            && let Some(bases_arg) = bases_arg
        {
            let mut disjoint_bases = self.validate_dynamic_type_bases(
                bases_arg,
                explicit_bases,
                &name,
                DynamicClassKind::NewClass,
            );

            if super::report_dynamic_mro_errors(&self.context, dynamic_class, call_expr, bases_arg)
            {
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
                    base1.display(db),
                    metaclass2,
                    base2.display(db),
                );
            }
        }

        Type::ClassLiteral(ClassLiteral::Dynamic(dynamic_class))
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
        let callable_type = self.expression_type(call_expr.func.as_ref());
        let iterable_object = KnownClass::Iterable.to_specialized_instance(db, &[Type::object()]);

        let mut call_arguments = CallArguments::from_arguments(
            &call_expr.arguments,
            |arg_or_keyword, splatted_value| {
                let ty = self.infer_expression(splatted_value, TypeContext::default());
                if let ast::ArgOrKeyword::Arg(argument) = arg_or_keyword
                    && argument.is_starred_expr()
                {
                    self.store_expression_type(argument, ty);
                } else if let Some(ty) = self.try_narrow_dict_kwargs(ty, arg_or_keyword) {
                    return ty;
                }

                ty
            },
        );

        // Validate that starred arguments are iterable.
        for arg in &call_expr.arguments.args {
            if let ast::Expr::Starred(ast::ExprStarred { value, .. }) = arg {
                let iterable_type = self.expression_type(value);
                if let Err(err) = iterable_type.try_iterate(db) {
                    err.report_diagnostic(&self.context, iterable_type, value.as_ref().into());
                }
            }
        }

        // Validate that double-starred keyword arguments are mappings.
        for keyword in call_expr
            .arguments
            .keywords
            .iter()
            .filter(|kw| kw.arg.is_none())
        {
            let mapping_type = self.expression_type(&keyword.value);

            if mapping_type.as_paramspec_typevar(db).is_some()
                || mapping_type.unpack_keys_and_items(db).is_some()
            {
                continue;
            }

            let Some(builder) = self
                .context
                .report_lint(&INVALID_ARGUMENT_TYPE, &keyword.value)
            else {
                continue;
            };

            builder
                .into_diagnostic("Argument expression after ** must be a mapping type")
                .set_primary_message(format_args!("Found `{}`", mapping_type.display(db)));
        }

        let mut bindings = callable_type
            .bindings(db)
            .match_parameters(db, &call_arguments);
        let bindings_result = self.infer_and_check_argument_types(
            ArgumentsIter::from_ast(&call_expr.arguments),
            &mut call_arguments,
            &mut |builder, (_, expr, tcx)| {
                if name_node.is_some_and(|name| std::ptr::eq(expr, name)) {
                    let _ = builder.infer_expression(expr, tcx);
                    KnownClass::Str.to_instance(builder.db())
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
