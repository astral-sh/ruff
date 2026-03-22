use ruff_python_ast::{self as ast, name::Name};

use super::TypeInferenceBuilder;
use crate::types::diagnostic::{
    report_cannot_pop_required_field_on_typed_dict, report_invalid_key_on_typed_dict,
};
use crate::types::{
    CallableType, Parameter, Parameters, Signature, Type, TypeContext, TypedDictType, UnionBuilder,
    UnionType, callable::CallableTypeKind, signatures::CallableSignature,
};

impl<'db> TypeInferenceBuilder<'db, '_> {
    fn non_callable_typed_dict_method(&self) -> Type<'db> {
        Type::Callable(CallableType::new(
            self.db(),
            CallableSignature::from_overloads([]),
            CallableTypeKind::FunctionLike,
        ))
    }

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
            _ => return None,
        })
    }

    pub(super) fn known_typed_dict_field_for_key(
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

    pub(super) fn infer_typed_dict_known_key_default(
        &mut self,
        default_arg: &ast::Expr,
        field_ty: Type<'db>,
        use_field_context: bool,
    ) -> Type<'db> {
        // We use speculative inference here because we only need the *type* of the default
        // argument to construct the specialized callable. The expression type will be properly
        // stored during normal argument inference (with the correct type context from the
        // specialized callable's parameter annotation).
        if !use_field_context {
            return self.speculate().infer_expression(default_arg, TypeContext::default());
        }

        let inferred_ty = self
            .speculate()
            .infer_expression(default_arg, TypeContext::new(Some(field_ty)));

        if inferred_ty.is_assignable_to(self.db(), field_ty) {
            inferred_ty
        } else {
            self.speculate()
                .infer_expression(default_arg, TypeContext::default())
        }
    }

    pub(super) fn specialize_typed_dict_known_key_method_call(
        &mut self,
        value_type: Type<'db>,
        method_name: &str,
        arguments: &ast::Arguments,
    ) -> Option<Type<'db>> {
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

        match value_type {
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
        }
    }

    pub(super) fn check_typed_dict_pop_or_setdefault_call(
        &mut self,
        typed_dict_ty: TypedDictType<'db>,
        method_name: &str,
        arguments: &ast::Arguments,
    ) -> Option<Type<'db>> {
        let first_arg = arguments.args.first()?;
        let ast::Expr::StringLiteral(ast::ExprStringLiteral {
            value: key_literal, ..
        }) = first_arg
        else {
            return None;
        };

        let key = key_literal.to_str();
        let items = typed_dict_ty.items(self.db());

        if let Some((_, field)) = items
            .iter()
            .find(|(field_name, _)| field_name.as_str() == key)
        {
            if method_name == "pop" && field.is_required() {
                report_cannot_pop_required_field_on_typed_dict(
                    &self.context,
                    first_arg.into(),
                    Type::TypedDict(typed_dict_ty),
                    key,
                );
                return Some(Type::unknown());
            }

            return None;
        }

        let key_ty = Type::string_literal(self.db(), key);
        report_invalid_key_on_typed_dict(
            &self.context,
            first_arg.into(),
            first_arg.into(),
            Type::TypedDict(typed_dict_ty),
            None,
            key_ty,
            items,
        );

        // Return `Unknown` to prevent the overload system from generating its own error.
        Some(Type::unknown())
    }
}
