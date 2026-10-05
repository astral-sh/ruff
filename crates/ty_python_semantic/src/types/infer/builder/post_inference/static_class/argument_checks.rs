//! Shared validation of TypedDict headers and inherited `__init_subclass__` arguments.

use std::convert::Infallible;

use ruff_python_ast::{self as ast, PythonVersion};

use crate::types::call::Argument;
use crate::types::class::CodeGeneratorKind;
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    INVALID_ARGUMENT_TYPE, INVALID_TYPED_DICT_HEADER, UNKNOWN_ARGUMENT,
    report_subclass_of_class_with_non_callable_init_subclass,
};
use crate::types::{CallArguments, MemberLookupPolicy, StaticClassLiteral, Type, TypingModule};

pub(super) struct OrdinaryClassArgumentCheckEffects<'a, 'db, 'ast, F> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) file_expression_type: &'a F,
}

pub(in crate::types::infer::builder) struct ClassArgumentCheckFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousClassArgumentCheckEffects)]
    pub(in crate::types::infer::builder) trait ClassArgumentCheckEffects<'db> {
        type Error;

        #[operation(source)]
        async fn in_stub(&self) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn typed_dict_module(&self, class: StaticClassLiteral<'db>) -> Result<Option<TypingModule>, Self::Error>;
        #[operation(source)]
        async fn python_version(&self) -> Result<PythonVersion, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_keyword<'node>(&self, arguments: &'node ast::Arguments, cursor: &mut usize) -> Result<Option<&'node ast::Keyword>, Self::Error>;
        #[operation(child)]
        async fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn report_pep_728_unavailable(&self, keyword: &ast::Keyword, argument_name: &str) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_invalid_boolean(&self, keyword: &ast::Keyword, argument_name: &str, passed_type: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_custom_metaclass(&self, keyword: &ast::Keyword) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_unknown_keyword(&self, keyword: &ast::Keyword, argument_name: &str) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_keyword_variadic(&self, keyword: &ast::Keyword) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn new_call_arguments<'node>(&self, arguments: &'node ast::Arguments) -> Result<CallArguments<'node, 'db>, Self::Error>;
        #[operation(local)]
        async fn append_argument<'node>(&self, arguments: &mut CallArguments<'node, 'db>, argument: Argument<'node>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn check_inherited_init_subclass(&self, class: StaticClassLiteral<'db>, class_node: &ast::StmtClassDef, call_arguments: CallArguments<'_, 'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl ClassArgumentCheckFacts {
        fn arguments<'node>(&self, class_node: &'node ast::StmtClassDef) -> Option<&'node ast::Arguments> {
            class_node.arguments.as_deref()
        }

        fn keyword_name<'node>(&self, keyword: &'node ast::Keyword) -> Option<&'node str> {
            keyword.arg.as_deref()
        }

        fn is_boolean_literal(&self, expression: &ast::Expr) -> bool {
            expression.is_boolean_literal_expr()
        }

        fn supports_pep_728(&self, version: PythonVersion) -> bool {
            version >= PythonVersion::PY315
        }
    }

    #[synchronous(check_arguments_sync)]
    #[capabilities(effects = ClassArgumentCheckEffects, facts = ClassArgumentCheckFacts)]
    #[passive_values(Argument::Keyword, Argument::Keywords)]
    pub(in crate::types::infer::builder) async fn check_arguments_with<'db, E: ClassArgumentCheckEffects<'db>>(
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        facts: ClassArgumentCheckFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        let Some(arguments) = facts.arguments(class_node) else {
            return Ok(());
        };

        if matches!(class_kind, Some(CodeGeneratorKind::TypedDict)) {
            let supports_pep_728 = effects.in_stub().await?
                || matches!(effects.typed_dict_module(class).await?, Some(TypingModule::TypingExtensions))
                || facts.supports_pep_728(effects.python_version().await?);

            let mut cursor = 0;
            #[cursor_loop]
            while let Some(keyword) = effects.next_keyword(arguments, &mut cursor).await? {
                if !supports_pep_728
                    && let Some(argument_name @ ("closed" | "extra_items")) = facts.keyword_name(keyword)
                {
                    effects.report_pep_728_unavailable(keyword, argument_name).await?;
                }

                match facts.keyword_name(keyword) {
                    Some(argument_name @ ("total" | "closed")) => {
                        let passed_type = effects.expression_type(&keyword.value).await?;
                        if !facts.is_boolean_literal(&keyword.value) {
                            effects.report_invalid_boolean(keyword, argument_name, passed_type).await?;
                        }
                    }
                    Some("extra_items") => {
                        // TODO: validate that passed arguments here are annotation expressions
                    }
                    Some("metaclass") => {
                        effects.report_custom_metaclass(keyword).await?;
                    }
                    Some(other) => {
                        effects.report_unknown_keyword(keyword, other).await?;
                    }
                    None => {
                        effects.report_keyword_variadic(keyword).await?;
                    }
                }
            }
        } else {
            let mut call_arguments = effects.new_call_arguments(arguments).await?;
            let mut cursor = 0;
            #[cursor_loop]
            while let Some(keyword) = effects.next_keyword(arguments, &mut cursor).await? {
                let argument = match facts.keyword_name(keyword) {
                    // Python consumes the metaclass argument before calling `__init_subclass__`.
                    Some("metaclass") => continue,
                    Some(name) => Argument::Keyword(name),
                    None => Argument::Keywords,
                };
                let ty = effects.expression_type(&keyword.value).await?;
                effects.append_argument(&mut call_arguments, argument, ty).await?;
            }

            effects.check_inherited_init_subclass(class, class_node, call_arguments).await?;
        }
        Ok(())
    }
}

/// Advances through keywords borrowed from the original class arguments.
/// Controlled callers admit the cursor step before invoking this helper.
pub(in crate::types::infer::builder) fn next_class_keyword<'node>(
    arguments: &'node ast::Arguments,
    cursor: &mut usize,
) -> Option<&'node ast::Keyword> {
    let keyword = arguments.keywords.get(*cursor)?;
    *cursor += 1;
    Some(keyword)
}

impl<'db, F: Fn(&ast::Expr) -> Type<'db>> SynchronousClassArgumentCheckEffects<'db>
    for OrdinaryClassArgumentCheckEffects<'_, 'db, '_, F>
{
    type Error = Infallible;

    fn in_stub(&self) -> Result<bool, Self::Error> {
        Ok(self.context.in_stub())
    }

    fn typed_dict_module(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<TypingModule>, Self::Error> {
        Ok(class.typed_dict_module(self.context.db()))
    }

    fn python_version(&self) -> Result<PythonVersion, Self::Error> {
        Ok(self
            .context
            .program_environment()
            .python_version(self.context.db()))
    }

    fn next_keyword<'node>(
        &self,
        arguments: &'node ast::Arguments,
        cursor: &mut usize,
    ) -> Result<Option<&'node ast::Keyword>, Self::Error> {
        Ok(next_class_keyword(arguments, cursor))
    }

    fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error> {
        Ok((self.file_expression_type)(expression))
    }

    fn report_pep_728_unavailable(
        &self,
        keyword: &ast::Keyword,
        argument_name: &str,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, keyword) {
            builder.into_diagnostic(format_args!(
                "The `{argument_name}` parameter of `typing.TypedDict` was added in Python 3.15"
            ));
        }
        Ok(())
    }

    fn report_invalid_boolean(
        &self,
        keyword: &ast::Keyword,
        argument_name: &str,
        passed_type: Type<'db>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        if let Some(builder) = context.report_lint(&INVALID_ARGUMENT_TYPE, keyword) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Invalid argument to parameter `{argument_name}` \
                    in `TypedDict` definition",
            ));
            diagnostic.set_primary_annotation_message(format_args!(
                "Expected either `True` or `False`, got object of type `{}`",
                passed_type.display(context.db(), context.program_environment())
            ));
        }
        Ok(())
    }

    fn report_custom_metaclass(&self, keyword: &ast::Keyword) -> Result<(), Self::Error> {
        if let Some(builder) = self
            .context
            .report_lint(&INVALID_TYPED_DICT_HEADER, keyword)
        {
            builder.into_diagnostic(format_args!(
                "Custom metaclasses are not supported in `TypedDict` definitions",
            ));
        }
        Ok(())
    }

    fn report_unknown_keyword(
        &self,
        keyword: &ast::Keyword,
        argument_name: &str,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&UNKNOWN_ARGUMENT, keyword) {
            builder.into_diagnostic(format_args!(
                "Unknown keyword argument `{argument_name}` \
                    in `TypedDict` definition",
            ));
        }
        Ok(())
    }

    fn report_keyword_variadic(&self, keyword: &ast::Keyword) -> Result<(), Self::Error> {
        if let Some(builder) = self
            .context
            .report_lint(&INVALID_TYPED_DICT_HEADER, keyword)
        {
            builder.into_diagnostic(format_args!(
                "Keyword-variadic arguments are not supported \
                in `TypedDict` definitions",
            ));
        }
        Ok(())
    }

    fn new_call_arguments<'node>(
        &self,
        arguments: &'node ast::Arguments,
    ) -> Result<CallArguments<'node, 'db>, Self::Error> {
        Ok(CallArguments::with_capacity(arguments.keywords.len()))
    }

    fn append_argument<'node>(
        &self,
        arguments: &mut CallArguments<'node, 'db>,
        argument: Argument<'node>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        arguments.push_argument(argument, Some(ty));
        Ok(())
    }

    fn check_inherited_init_subclass(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        call_arguments: CallArguments<'_, 'db>,
    ) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        let env = context.program_environment();
        let init_subclass_type = class
            .class_member_from_mro(
                db,
                env,
                "__init_subclass__",
                MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                // Skip the current class and only consider base classes.
                class.iter_mro(db, None).skip(1),
            )
            .ignore_possibly_undefined();

        if let Some(init_subclass) = init_subclass_type {
            let call_arguments =
                call_arguments.with_self(Some(Type::from(class.identity_specialization(db))));
            if let Err(call_error) = init_subclass.try_call(db, env, &call_arguments) {
                report_subclass_of_class_with_non_callable_init_subclass(
                    context, call_error, class, class_node,
                );
            }
        }
        Ok(())
    }
}
