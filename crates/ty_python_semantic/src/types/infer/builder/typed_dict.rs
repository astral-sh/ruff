use ruff_python_ast::{self as ast, name::Name};

use super::TypeInferenceBuilder;
use crate::types::diagnostic::{
    report_cannot_pop_required_field_on_typed_dict, report_invalid_key_on_typed_dict,
};
use crate::types::{
    CallableType, Parameter, Parameters, Signature, Type, TypeContext, TypedDictType, UnionBuilder,
    UnionType, callable::CallableTypeKind, signatures::CallableSignature,
};

/// Result of attempting to specialize a `TypedDict` method call for a known literal key.
pub(super) enum TypedDictMethodSpecialization<'db> {
    /// A specialized callable that should replace the generic method type for normal call
    /// resolution.
    Callable(Type<'db>),

    /// A diagnostic was emitted (e.g. invalid key, pop on required field). The caller should
    /// return the contained type directly, bypassing normal call resolution.
    Diagnosed(Type<'db>),
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Return an empty function-like callable to model a `TypedDict` method that is statically
    /// unavailable for the current key shape.
    ///
    /// A callable with zero overloads will fail at every call site with a `call-non-callable`
    /// diagnostic, which is the desired behavior for e.g. `pop()` on a required field.
    fn non_callable_typed_dict_method(&self) -> Type<'db> {
        Type::Callable(CallableType::new(
            self.db(),
            CallableSignature::from_overloads([]),
            CallableTypeKind::FunctionLike,
        ))
    }

    /// Build a field-specific callable signature for a known-key `TypedDict` method call.
    ///
    /// Returns `None` if the method name or arity is unsupported, or if a required default type
    /// was not provided.
    fn specialize_typed_dict_known_key_method_for_field(
        &self,
        key: &str,
        method_name: &str,
        arguments_len: usize,
        field_ty: Type<'db>,
        field_is_required: bool,
        default_ty: Option<Type<'db>>,
    ) -> Option<Type<'db>> {
        let db = self.db();
        let key_parameter = Parameter::positional_only(Some(Name::new_static("key")))
            .with_annotated_type(Type::string_literal(db, key));

        Some(match (method_name, arguments_len) {
            ("get", 1) => Type::function_like_callable(
                db,
                Signature::new(
                    Parameters::new(db, [key_parameter]),
                    if field_is_required {
                        field_ty
                    } else {
                        UnionType::from_two_elements(db, field_ty, Type::none(db))
                    },
                ),
            ),
            ("get", 2) => {
                let default_ty = default_ty?;
                Type::function_like_callable(
                    db,
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                key_parameter,
                                Parameter::positional_only(Some(Name::new_static("default")))
                                    .with_annotated_type(default_ty),
                            ],
                        ),
                        if field_is_required || default_ty.is_assignable_to(db, field_ty) {
                            field_ty
                        } else {
                            UnionType::from_two_elements(db, field_ty, default_ty)
                        },
                    ),
                )
            }
            ("pop", 1) if !field_is_required => Type::function_like_callable(
                db,
                Signature::new(Parameters::new(db, [key_parameter]), field_ty),
            ),
            ("pop", 2) if !field_is_required => {
                let default_ty = default_ty?;
                Type::function_like_callable(
                    db,
                    Signature::new(
                        Parameters::new(
                            db,
                            [
                                key_parameter,
                                Parameter::positional_only(Some(Name::new_static("default")))
                                    .with_annotated_type(default_ty),
                            ],
                        ),
                        if default_ty.is_assignable_to(db, field_ty) {
                            field_ty
                        } else {
                            UnionType::from_two_elements(db, field_ty, default_ty)
                        },
                    ),
                )
            }
            ("setdefault", 2) => Type::function_like_callable(
                db,
                Signature::new(
                    Parameters::new(
                        db,
                        [
                            key_parameter,
                            Parameter::positional_only(Some(Name::new_static("default")))
                                .with_annotated_type(field_ty),
                        ],
                    ),
                    field_ty,
                ),
            ),
            ("pop", 1 | 2) => self.non_callable_typed_dict_method(),
            // Unsupported method or arity — fall back to generic overload resolution, which
            // will produce the appropriate arity/type errors.
            _ => return None,
        })
    }

    /// Resolve the field type for a literal key on a `TypedDict` or union of
    /// `TypedDict` instances.
    ///
    /// For unions, every arm must contain the key; the resulting field type is the union of the
    /// per-arm field types, and the field is treated as required only if it is required in every
    /// arm.
    fn known_typed_dict_field_for_key(
        &self,
        value_type: Type<'db>,
        key: &str,
    ) -> Option<(Type<'db>, bool)> {
        match value_type {
            Type::TypedDict(typed_dict_ty) => typed_dict_ty
                .items(self.db())
                .get(key)
                .map(|field| (field.declared_ty, field.is_required())),
            Type::Union(union) => {
                let mut field_types = UnionBuilder::new(self.db());
                let mut all_required = true;

                for element in union.elements(self.db()) {
                    let typed_dict_ty = element.as_typed_dict()?;
                    let field = typed_dict_ty.items(self.db()).get(key)?;
                    field_types.add_in_place(field.declared_ty);
                    all_required &= field.is_required();
                }

                Some((field_types.build(), all_required))
            }
            _ => None,
        }
    }

    /// Infer a known-key `get()` or `pop()` default, optionally trying the field type as
    /// bidirectional context and keeping it only if that inference remains assignable.
    fn infer_typed_dict_known_key_default(
        &mut self,
        default_arg: &ast::Expr,
        field_ty: Type<'db>,
        use_field_context: bool,
    ) -> Type<'db> {
        // We use speculative inference here because we only need the *type* of the default
        // argument to construct the specialized callable. The expression type will be properly
        // stored during normal argument inference (with the correct type context from the
        // specialized callable's parameter annotation). `speculate()` calls `context.defuse()`,
        // so no diagnostics leak from these trial inferences.
        let infer_speculatively =
            |builder: &mut Self, tcx| builder.speculate().infer_expression(default_arg, tcx);

        if use_field_context {
            let inferred_ty = infer_speculatively(self, TypeContext::new(Some(field_ty)));
            if inferred_ty.is_assignable_to(self.db(), field_ty) {
                return inferred_ty;
            }
        }

        infer_speculatively(self, TypeContext::default())
    }

    /// Specialize a `TypedDict` method call for a known literal key.
    ///
    /// For concrete `TypedDict`s, this validates key existence and required-field constraints
    /// (emitting diagnostics as needed) and builds a field-specific callable signature. For unions
    /// of `TypedDict`s, it builds per-arm specialized callables when all arms define the key.
    pub(super) fn specialize_typed_dict_known_key_method_call(
        &mut self,
        value_type: Type<'db>,
        method_name: &str,
        arguments: &ast::Arguments,
    ) -> Option<TypedDictMethodSpecialization<'db>> {
        if !arguments.keywords.is_empty() {
            return None;
        }

        let first_arg = arguments.args.first()?;
        let ast::Expr::StringLiteral(ast::ExprStringLiteral {
            value: key_literal, ..
        }) = first_arg
        else {
            return None;
        };

        let key = key_literal.to_str();

        // For concrete TypedDicts, validate key existence and required-field constraints for
        // `pop`/`setdefault` before attempting specialization. These produce definitive
        // diagnostics that should short-circuit normal call resolution.
        if let Type::TypedDict(typed_dict_ty) = value_type
            && matches!(method_name, "pop" | "setdefault")
        {
            if let Some(diagnosed) =
                self.check_typed_dict_key_constraints(typed_dict_ty, method_name, key, first_arg)
            {
                return Some(diagnosed);
            }
        }

        // Compute the default type for the default argument, if present. For `get()` and `pop()`
        // two-arg forms, we use the aggregate field type as bidirectional context so that literals
        // like `{}` can be inferred as the declared field type.
        let default_ty = match (method_name, &*arguments.args) {
            ("get", [_, default_arg]) => {
                let (field_ty, field_is_required) =
                    self.known_typed_dict_field_for_key(value_type, key)?;
                Some(self.infer_typed_dict_known_key_default(
                    default_arg,
                    field_ty,
                    !field_is_required,
                ))
            }
            ("pop", [_, default_arg]) => {
                let (field_ty, _) = self.known_typed_dict_field_for_key(value_type, key)?;
                Some(self.infer_typed_dict_known_key_default(default_arg, field_ty, true))
            }
            ("get" | "pop", [_]) | ("setdefault", [_, _]) => None,
            _ => return None,
        };

        let specialized = match value_type {
            Type::TypedDict(typed_dict_ty) => {
                let field = typed_dict_ty.items(self.db()).get(key)?;
                self.specialize_typed_dict_known_key_method_for_field(
                    key,
                    method_name,
                    arguments.args.len(),
                    field.declared_ty,
                    field.is_required(),
                    default_ty,
                )
            }
            Type::Union(union) => UnionType::try_from_elements(
                self.db(),
                union.elements(self.db()).iter().map(|element| {
                    let typed_dict_ty = element.as_typed_dict()?;
                    let field = typed_dict_ty.items(self.db()).get(key)?;
                    self.specialize_typed_dict_known_key_method_for_field(
                        key,
                        method_name,
                        arguments.args.len(),
                        field.declared_ty,
                        field.is_required(),
                        default_ty,
                    )
                }),
            ),
            _ => None,
        };

        specialized.map(TypedDictMethodSpecialization::Callable)
    }

    /// Validate literal-key constraints for `pop()` and `setdefault()` on a concrete `TypedDict`.
    ///
    /// Returns `Some(Diagnosed(Unknown))` when a definitive diagnostic is emitted (invalid key,
    /// or pop on a required field); otherwise returns `None` to continue with specialization.
    fn check_typed_dict_key_constraints(
        &mut self,
        typed_dict_ty: TypedDictType<'db>,
        method_name: &str,
        key: &str,
        key_expr: &ast::Expr,
    ) -> Option<TypedDictMethodSpecialization<'db>> {
        let items = typed_dict_ty.items(self.db());

        if let Some((_, field)) = items
            .iter()
            .find(|(field_name, _)| field_name.as_str() == key)
        {
            if method_name == "pop" && field.is_required() {
                report_cannot_pop_required_field_on_typed_dict(
                    &self.context,
                    key_expr.into(),
                    Type::TypedDict(typed_dict_ty),
                    key,
                );
                return Some(TypedDictMethodSpecialization::Diagnosed(Type::unknown()));
            }

            return None;
        }

        let key_ty = Type::string_literal(self.db(), key);
        report_invalid_key_on_typed_dict(
            &self.context,
            key_expr.into(),
            key_expr.into(),
            Type::TypedDict(typed_dict_ty),
            None,
            key_ty,
            items,
        );

        // Return `Unknown` to prevent the overload system from generating its own error.
        Some(TypedDictMethodSpecialization::Diagnosed(Type::unknown()))
    }
}
