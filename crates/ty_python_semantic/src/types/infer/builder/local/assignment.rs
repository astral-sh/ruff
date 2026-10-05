//! Assignment-call classification retains definition ownership through the local driver.

use std::convert::Infallible;

use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::scope::ScopeKind;

use super::super::typevar::legacy;
use super::super::*;
use super::call::{OrdinaryCallEffects, SynchronousCallEffects};

pub(super) enum Start<'db, 'expr> {
    Complete(Type<'db>),
    LegacyTypeVar(legacy::State<'db, 'expr>),
    Call,
}

pub(super) enum SpecialCall {
    NamedTuple(NamedTupleKind),
    TypedDict(TypingModule),
    NewClass,
    ParamSpec(KnownClass),
    TypeVarTuple(KnownClass),
    NewType,
    BuiltinType,
    TypeAliasType(TypingModule),
}

pub(super) enum OptionalSpecialCall {
    Enum(KnownClass),
    Sentinel,
}

pub(super) struct AssignmentFacts;
pub(super) struct OrdinaryAssignmentEffects;

shared_semantic_family! {
    #[synchronous(SynchronousAssignmentEffects)]
    pub(super) trait AssignmentEffects<'db, 'ast> {
        type Error;
        #[operation(child)]
        async fn named_tuple_kind(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<NamedTupleKind>, Self::Error>;
        #[operation(child)]
        async fn typed_dict_module(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<TypingModule>, Self::Error>;
        #[operation(child)]
        async fn is_new_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn enum_base(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(child)]
        async fn known_class(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(source)]
        async fn special_call(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, call: &ast::ExprCall, definition: Definition<'db>, kind: SpecialCall) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn optional_special_call(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, call: &ast::ExprCall, definition: Definition<'db>, kind: OptionalSpecialCall) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn is_class_scope(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn desugared_decorator(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, callable_type: Type<'db>, call: &ast::ExprCall, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[finite_capability]
    impl AssignmentFacts {
        fn legacy_typevar<'db, 'expr>(&self, target: &'expr ast::Expr, call: &'expr ast::ExprCall, definition: Definition<'db>, class: KnownClass) -> legacy::State<'db, 'expr> {
            legacy::new(target, call, definition, class)
        }
        fn type_alias_module(&self, class: KnownClass) -> Option<TypingModule> {
            TypingModule::from_type_alias_class(class)
        }
        fn is_name(&self, target: &ast::Expr) -> bool {
            target.as_name_expr().is_some()
        }
    }

    #[synchronous(start_sync)]
    #[capabilities(effects = AssignmentEffects, facts = AssignmentFacts)]
    #[passive_values(Start::Complete, Start::LegacyTypeVar, Start::Call, SpecialCall::NamedTuple, SpecialCall::TypedDict, SpecialCall::NewClass, SpecialCall::ParamSpec, SpecialCall::TypeVarTuple, SpecialCall::NewType, SpecialCall::BuiltinType, SpecialCall::TypeAliasType, OptionalSpecialCall::Enum, OptionalSpecialCall::Sentinel)]
    pub(super) async fn start_with<'db, 'ast, 'expr, E: AssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &'expr ast::Expr, call: &'expr ast::ExprCall, definition: Definition<'db>, callable_type: Type<'db>, facts: AssignmentFacts, effects: &E,
    ) -> Result<Start<'db, 'expr>, E::Error> {
        if let Some(kind) = effects.named_tuple_kind(builder, callable_type).await? {
            return Ok(Start::Complete(effects.special_call(builder, target, call, definition, SpecialCall::NamedTuple(kind)).await?));
        }
        if let Some(module) = effects.typed_dict_module(builder, callable_type).await? {
            return Ok(Start::Complete(effects.special_call(builder, target, call, definition, SpecialCall::TypedDict(module)).await?));
        }
        if effects.is_new_class(builder, callable_type).await? {
            return Ok(Start::Complete(effects.special_call(builder, target, call, definition, SpecialCall::NewClass).await?));
        }
        if let Some(base) = effects.enum_base(builder, callable_type).await? {
            if let Some(ty) = effects.optional_special_call(builder, target, call, definition, OptionalSpecialCall::Enum(base)).await? {
                return Ok(Start::Complete(ty));
            }
        }
        let kind = match effects.known_class(builder, callable_type).await? {
            Some(class @ (KnownClass::TypeVar | KnownClass::ExtensionsTypeVar)) => {
                return Ok(Start::LegacyTypeVar(facts.legacy_typevar(target, call, definition, class)));
            }
            Some(class @ (KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec)) => SpecialCall::ParamSpec(class),
            Some(class @ (KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple)) => SpecialCall::TypeVarTuple(class),
            Some(KnownClass::NewType) => SpecialCall::NewType,
            Some(KnownClass::Type) => SpecialCall::BuiltinType,
            Some(class) if let Some(module) = facts.type_alias_module(class) => SpecialCall::TypeAliasType(module),
            Some(KnownClass::Sentinel) => {
                return Ok(match effects.optional_special_call(builder, target, call, definition, OptionalSpecialCall::Sentinel).await? {
                    Some(ty) => Start::Complete(ty),
                    None => Start::Call,
                });
            }
            Some(_) | None => return Ok(Start::Call),
        };
        Ok(Start::Complete(effects.special_call(builder, target, call, definition, kind).await?))
    }

    #[synchronous(finish_sync)]
    #[capabilities(effects = AssignmentEffects, facts = AssignmentFacts)]
    #[passive_values()]
    pub(super) async fn finish_with<'db, 'ast, E: AssignmentEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, call: &ast::ExprCall, callable_type: Type<'db>, ty: Type<'db>, facts: AssignmentFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        if facts.is_name(target) && effects.is_class_scope(builder).await? {
            effects.desugared_decorator(builder, callable_type, call, ty).await
        } else {
            Ok(ty)
        }
    }
}

impl<'db, 'ast> SynchronousAssignmentEffects<'db, 'ast> for OrdinaryAssignmentEffects {
    type Error = Infallible;

    fn named_tuple_kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<NamedTupleKind>, Self::Error> {
        OrdinaryCallEffects.named_tuple_kind(builder, ty)
    }

    fn typed_dict_module(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<TypingModule>, Self::Error> {
        OrdinaryCallEffects.typed_dict_module(builder, ty)
    }

    fn is_new_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        OrdinaryCallEffects.function_is_known(builder, ty, KnownFunction::NewClass)
    }

    fn enum_base(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        OrdinaryCallEffects.enum_base(builder, ty)
    }

    fn known_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        ty: Type<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(ty
            .as_class_literal()
            .and_then(|class| class.known(builder.db())))
    }

    fn special_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        call: &ast::ExprCall,
        definition: Definition<'db>,
        kind: SpecialCall,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(match kind {
            SpecialCall::NamedTuple(kind) => {
                builder.infer_namedtuple_call_expression(call, Some(definition), kind)
            }
            SpecialCall::TypedDict(module) => {
                builder.infer_typeddict_call_expression(call, Some(definition), module)
            }
            SpecialCall::NewClass => builder.infer_new_class_call(call, Some(definition)),
            SpecialCall::ParamSpec(class) => {
                builder.infer_legacy_paramspec(target, call, definition, class)
            }
            SpecialCall::TypeVarTuple(class) => {
                builder.infer_legacy_typevartuple(target, call, definition, class)
            }
            SpecialCall::NewType => builder.infer_newtype_expression(target, call, definition),
            SpecialCall::BuiltinType => {
                // Try to extract the dynamic class with definition.
                // This returns `None` if it's not a three-arg call to `type()`,
                // signalling that we must fall back to normal call inference.
                builder.infer_builtins_type_call(call, Some(definition))
            }
            SpecialCall::TypeAliasType(module) => {
                builder.infer_typealiastype_call(target, call, definition, module)
            }
        })
    }

    fn optional_special_call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        call: &ast::ExprCall,
        definition: Definition<'db>,
        kind: OptionalSpecialCall,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match kind {
            OptionalSpecialCall::Enum(base) => {
                builder.infer_enum_call_expression(call, Some(definition), base)
            }
            OptionalSpecialCall::Sentinel => {
                builder.infer_sentinel_expression(target, call, definition)
            }
        })
    }

    fn is_class_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Self::Error> {
        Ok(builder
            .index
            .scope(builder.scope().file_scope_id(builder.db()))
            .kind()
            == ScopeKind::Class)
    }

    fn desugared_decorator(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        callable_type: Type<'db>,
        call: &ast::ExprCall,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.apply_desugared_decorator(callable_type, call, ty))
    }
}
