//! Attribute reads retain member diagnostics even when local assignments determine the value.

use std::convert::Infallible;

use ruff_db::source::source_text;
use ruff_python_ast::helpers::is_dotted_name;
use ruff_python_ast::{self as ast, ExprContext};
use ruff_text_size::Ranged;
use ty_mapping_probe_macros::shared_semantic_family;
use ty_module_resolver::{ImportingFile, ModuleName, resolve_module};
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::FileScopeId;

use super::TypeInferenceBuilder;
use crate::place::{
    DefinedPlace, Definedness, LookupError, LookupResult, Place, PlaceAndQualifiers, TypeOrigin,
};
use crate::types::diagnostic::{
    INVALID_ATTRIBUTE_ACCESS, POSSIBLY_MISSING_SUBMODULE, UNRESOLVED_ATTRIBUTE,
    hint_if_stdlib_attribute_exists_on_other_versions, report_possibly_missing_attribute,
};
use crate::types::generics::bind_typevar;
use crate::types::subclass_of::SubclassOfInner;
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, KnownClass, KnownInstanceType, MemberLookupError, MemberLookupResult,
    PropertyDeprecations, ResolvedMember, SubclassOfType, Type, TypeAndQualifiers, TypeContext,
    TypeQualifiers,
};
use crate::{Db, DisplaySettings, FxIndexSet, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub enum AttributeOperation {
    ParamSpecBinding,
    PlaceExpression,
    MemberLookup,
    MemberLookupDiagnostic,
    Narrowing,
    GenericAccess,
    UndefinedDiagnostic,
    PossiblyUndefinedDiagnostic,
    PropertyDeprecation,
    Deletion,
}

pub(in crate::types::infer) type AttributeLoadResult<'db> =
    Result<TypeAndQualifiers<'db>, TypeAndQualifiers<'db>>;

pub(in crate::types::infer) struct AttributeFacts;
pub(super) struct OrdinaryAttributeEffects;

shared_semantic_family! {
    #[synchronous(SynchronousAttributeEffects)]
    pub(in crate::types::infer) trait AttributeEffects<'db, 'ast> {
        type Error;
        #[operation(source)]
        async fn receiver(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, standalone: bool) -> Result<Type<'db>, Self::Error>;
        #[operation(child)]
        async fn load(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute) -> Result<AttributeLoadResult<'db>, Self::Error>;
        #[operation(child)]
        async fn load_on_receiver(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>) -> Result<AttributeLoadResult<'db>, Self::Error>;
        #[operation(local)]
        async fn is_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, typevar: TypeVarInstance<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn bind_paramspec(&self, builder: &TypeInferenceBuilder<'db, 'ast>, typevar: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(local)]
        async fn empty_constraints(&self) -> Result<Vec<(FileScopeId, ConstraintKey)>, Self::Error>;
        #[operation(local)]
        async fn place_expression(&self, attribute: &ast::ExprAttribute) -> Result<Option<PlaceExpr>, Self::Error>;
        #[operation(source)]
        async fn assigned_place(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, place: PlaceExpr) -> Result<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>), Self::Error>;
        #[operation(child)]
        async fn member_lookup(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>) -> Result<MemberLookupResult<'db>, Self::Error>;
        #[operation(source)]
        async fn recover_member(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>, assigned: Option<Type<'db>>, error: MemberLookupError<'db>) -> Result<ResolvedMember<'db>, Self::Error>;
        #[operation(local)]
        async fn member_place(&self, builder: &TypeInferenceBuilder<'db, 'ast>, member: ResolvedMember<'db>) -> Result<PlaceAndQualifiers<'db>, Self::Error>;
        #[operation(child)]
        async fn narrow(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, ty: Type<'db>, constraints: &[(FileScopeId, ConstraintKey)]) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn validate_generic_access(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn has_generic_instance_attribute(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn report_generic_access(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn place_lookup(&self, builder: &TypeInferenceBuilder<'db, 'ast>, place: PlaceAndQualifiers<'db>) -> Result<LookupResult<'db>, Self::Error>;
        #[operation(source)]
        async fn recover_lookup(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>, error: LookupError<'db>) -> Result<TypeAndQualifiers<'db>, Self::Error>;
        #[operation(source)]
        async fn check_deprecated(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn deprecated_properties(&self, builder: &TypeInferenceBuilder<'db, 'ast>, member: ResolvedMember<'db>) -> Result<Option<PropertyDeprecations<'db>>, Self::Error>;
        #[operation(source)]
        async fn check_deprecated_property(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, properties: PropertyDeprecations<'db>, access: ExprContext) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn stored_receiver(&self, builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn validate_deletion(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl AttributeFacts {
        fn receiver<'expr>(&self, attribute: &'expr ast::ExprAttribute) -> &'expr ast::Expr { &attribute.value }
        fn context(&self, attribute: &ast::ExprAttribute) -> ExprContext { attribute.ctx }
        fn is_store(&self, attribute: &ast::ExprAttribute) -> bool { attribute.ctx.is_store() }
        fn never<'db>(&self) -> Type<'db> { Type::Never }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn value<'db>(&self, result: AttributeLoadResult<'db>) -> Type<'db> {
            match result { Ok(ty) | Err(ty) => ty.inner_type() }
        }
        fn assigned<'db>(&self, resolved: PlaceAndQualifiers<'db>) -> Option<TypeAndQualifiers<'db>> {
            match resolved.place {
                Place::Defined(place @ DefinedPlace { definedness: Definedness::AlwaysDefined, .. }) => Some(TypeAndQualifiers::new(place.ty, place.origin, resolved.qualifiers).with_provenance(place.provenance)),
                _ => None,
            }
        }
        fn inner<'db>(&self, ty: TypeAndQualifiers<'db>) -> Type<'db> { ty.inner_type() }
        fn assigned_inner<'db>(&self, ty: Option<TypeAndQualifiers<'db>>) -> Option<Type<'db>> { ty.map(|ty| ty.inner_type()) }
        fn with_type<'db>(&self, place: PlaceAndQualifiers<'db>, ty: Type<'db>) -> PlaceAndQualifiers<'db> { place.map_type(|_| ty) }
        fn access(&self, attribute: &ast::ExprAttribute) -> ExprContext { if attribute.ctx == ExprContext::Del { ExprContext::Del } else { ExprContext::Load } }
        fn preferred<'db>(&self, assigned: Option<TypeAndQualifiers<'db>>, resolved: TypeAndQualifiers<'db>) -> TypeAndQualifiers<'db> { assigned.unwrap_or(resolved) }
    }

    #[synchronous(infer_attribute_expression_sync)]
    #[capabilities(effects = AttributeEffects, facts = AttributeFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn infer_attribute_expression_with<'db, 'ast, E: AttributeEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, facts: AttributeFacts, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match facts.context(attribute) {
            ExprContext::Load => {
                let loaded = effects.load(builder, attribute).await?;
                Ok(facts.value(loaded))
            }
            ExprContext::Store => {
                effects.receiver(builder, facts.receiver(attribute), false).await?;
                Ok(facts.never())
            }
            ExprContext::Del => {
                let _ = effects.load(builder, attribute).await?;
                let receiver = effects.stored_receiver(builder, attribute).await?;
                effects.validate_deletion(builder, attribute, receiver).await?;
                Ok(facts.never())
            }
            ExprContext::Invalid => {
                effects.receiver(builder, facts.receiver(attribute), false).await?;
                Ok(facts.unknown())
            }
        }
    }

    #[synchronous(validate_generic_class_attribute_access_sync)]
    #[capabilities(effects = AttributeEffects)]
    #[passive_values()]
    pub(in crate::types::infer) async fn validate_generic_class_attribute_access_with<'db, 'ast, E: AttributeEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>, emit_diagnostics: bool, effects: &E,
    ) -> Result<bool, E::Error> {
        if !effects.has_generic_instance_attribute(builder, attribute, receiver).await? {
            return Ok(true);
        }
        if emit_diagnostics {
            effects.report_generic_access(builder, attribute).await?;
        }
        Ok(false)
    }

    #[synchronous(infer_attribute_load_sync)]
    #[capabilities(effects = AttributeEffects, facts = AttributeFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn infer_attribute_load_with<'db, 'ast, E: AttributeEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, facts: AttributeFacts, effects: &E,
    ) -> Result<AttributeLoadResult<'db>, E::Error> {
        let receiver = effects.receiver(builder, facts.receiver(attribute), true).await?;
        effects.load_on_receiver(builder, attribute, receiver).await
    }

    #[synchronous(infer_attribute_load_impl_sync)]
    #[capabilities(effects = AttributeEffects, facts = AttributeFacts)]
    #[passive_values(Type::TypeVar, Err)]
    pub(in crate::types::infer) async fn infer_attribute_load_impl_with<'db, 'ast, E: AttributeEffects<'db, 'ast>>(
        builder: &mut TypeInferenceBuilder<'db, 'ast>, attribute: &ast::ExprAttribute, receiver: Type<'db>, facts: AttributeFacts, effects: &E,
    ) -> Result<AttributeLoadResult<'db>, E::Error> {
        let empty_constraints = effects.empty_constraints().await?;
        let receiver = if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = receiver
            && effects.is_paramspec(builder, typevar).await?
            && let Some(bound) = effects.bind_paramspec(builder, typevar).await?
        {
            Type::TypeVar(bound)
        } else {
            receiver
        };

        let (assigned, constraint_keys) = if let Some(place) = effects.place_expression(attribute).await? {
            let (resolved, keys) = effects.assigned_place(builder, attribute, place).await?;
            (facts.assigned(resolved), keys)
        } else {
            (None, empty_constraints)
        };
        let member = match effects.member_lookup(builder, attribute, receiver).await? {
            Ok(member) => member,
            Err(error) => effects.recover_member(builder, attribute, receiver, facts.assigned_inner(assigned), error).await?,
        };
        let fallback = effects.member_place(builder, member).await?;
        let fallback = if let Place::Defined(place) = fallback.place {
            let narrowed = effects.narrow(builder, attribute, place.ty, &constraint_keys).await?;
            facts.with_type(fallback, narrowed)
        } else {
            fallback
        };

        // Augmented assignment loads its target, but its write validation reports invalid
        // generic access. Other reads validate after narrowing, before lookup recovery.
        if !facts.is_store(attribute) {
            effects.validate_generic_access(builder, attribute, receiver).await?;
        }
        let lookup = effects.place_lookup(builder, fallback).await?;
        let resolved = match lookup {
            Ok(ty) => ty,
            Err(error) => effects.recover_lookup(builder, attribute, receiver, error).await?,
        };
        effects.check_deprecated(builder, attribute, facts.inner(resolved)).await?;
        // Deletion does not invoke a getter. An augmented assignment still reads the property.
        if let Some(properties) = effects.deprecated_properties(builder, member).await? {
            effects.check_deprecated_property(builder, attribute, properties, facts.access(attribute)).await?;
        }
        let inferred = facts.preferred(assigned, resolved);
        // Assignment precedence affects the value, while the normal lookup retains its status.
        Ok(match lookup {
            Ok(_) => Ok(inferred),
            Err(_) => Err(inferred),
        })
    }
}

impl<'db, 'ast> SynchronousAttributeEffects<'db, 'ast> for OrdinaryAttributeEffects {
    type Error = Infallible;

    fn receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        standalone: bool,
    ) -> Result<Type<'db>, Infallible> {
        Ok(if standalone {
            builder.infer_maybe_standalone_expression(expression, TypeContext::default())
        } else {
            builder.infer_expression(expression, TypeContext::default())
        })
    }
    fn load(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
    ) -> Result<AttributeLoadResult<'db>, Infallible> {
        infer_attribute_load_sync(builder, attribute, AttributeFacts, self)
    }
    fn load_on_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> Result<AttributeLoadResult<'db>, Infallible> {
        infer_attribute_load_impl_sync(builder, attribute, receiver, AttributeFacts, self)
    }
    fn is_paramspec(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<bool, Infallible> {
        Ok(typevar.is_paramspec(builder.db()))
    }
    fn bind_paramspec(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let db = builder.db();
        Ok(bind_typevar(
            db,
            builder.index,
            builder.scope().file_scope_id(db),
            builder.typevar_binding_context,
            typevar,
        ))
    }
    fn empty_constraints(&self) -> Result<Vec<(FileScopeId, ConstraintKey)>, Infallible> {
        Ok(Vec::new())
    }
    fn place_expression(
        &self,
        attribute: &ast::ExprAttribute,
    ) -> Result<Option<PlaceExpr>, Infallible> {
        Ok(PlaceExpr::try_from_expr(attribute))
    }
    fn assigned_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        place: PlaceExpr,
    ) -> Result<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>), Infallible> {
        Ok(builder.infer_place_load(place, ast::ExprRef::Attribute(attribute)))
    }
    fn member_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> Result<MemberLookupResult<'db>, Infallible> {
        Ok(receiver.try_member_lookup(
            builder.db(),
            builder.program_environment(),
            &attribute.attr.id,
        ))
    }
    fn recover_member(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
        assigned: Option<Type<'db>>,
        error: MemberLookupError<'db>,
    ) -> Result<ResolvedMember<'db>, Infallible> {
        error.report_diagnostic(&builder.context, receiver, attribute, assigned);
        Ok(error.fallback_member(builder.db()))
    }
    fn member_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        member: ResolvedMember<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Infallible> {
        Ok(member.member(builder.db()))
    }
    fn narrow(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.narrow_expr_with_applicable_constraints(attribute, ty, constraints))
    }
    fn validate_generic_access(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.validate_generic_class_attribute_access(attribute, receiver, true);
        Ok(())
    }
    fn has_generic_instance_attribute(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(receiver.has_generic_instance_attribute(
            builder.db(),
            builder.program_environment(),
            &attribute.attr.id,
        ))
    }
    fn report_generic_access(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
    ) -> Result<(), Infallible> {
        if let Some(diagnostic) = builder
            .context
            .report_lint(&INVALID_ATTRIBUTE_ACCESS, attribute)
        {
            diagnostic.into_diagnostic(format_args!(
                "Cannot access generic instance attribute `{}` through a class",
                attribute.attr.id,
            ));
        }
        Ok(())
    }
    fn place_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        place: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Infallible> {
        Ok(place.into_lookup_result(builder.db(), builder.program_environment()))
    }
    fn recover_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
        error: LookupError<'db>,
    ) -> Result<TypeAndQualifiers<'db>, Infallible> {
        Ok(builder.recover_attribute_lookup(attribute, receiver, error))
    }
    fn check_deprecated(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        ty: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.check_deprecated(&attribute.attr, ty);
        Ok(())
    }
    fn deprecated_properties(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        member: ResolvedMember<'db>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Infallible> {
        Ok(member.deprecated_properties(builder.db()))
    }
    fn check_deprecated_property(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        properties: PropertyDeprecations<'db>,
        access: ExprContext,
    ) -> Result<(), Infallible> {
        builder.check_deprecated_property(attribute, properties, access);
        Ok(())
    }
    fn stored_receiver(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.expression_type(&attribute.value))
    }
    fn validate_deletion(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> Result<(), Infallible> {
        builder.validate_attribute_deletion(attribute, receiver, attribute.attr.as_str(), true);
        Ok(())
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    fn recover_attribute_lookup(
        &self,
        attribute: &ast::ExprAttribute,
        value_type: Type<'db>,
        lookup_err: LookupError<'db>,
    ) -> TypeAndQualifiers<'db> {
        fn union_elements_missing_attribute<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            attr_name: &str,
            missing_types: &mut FxIndexSet<Type<'db>>,
        ) {
            if let Some(union) = ty.as_union_like(db) {
                for element in union.elements(db) {
                    union_elements_missing_attribute(db, env, *element, attr_name, missing_types);
                }
            } else if ty.member(db, env, attr_name).place.is_undefined() {
                missing_types.insert(ty);
            }
        }

        let db = self.db();
        let env = self.program_environment();
        let ast::ExprAttribute { value, attr, .. } = attribute;
        let attr_name = &attr.id;
        match lookup_err {
            LookupError::Undefined(_) => {
                let fallback = || {
                    TypeAndQualifiers::new(
                        Type::unknown(),
                        TypeOrigin::Inferred,
                        TypeQualifiers::empty(),
                    )
                };

                let bound_on_instance = match value_type {
                    Type::ClassLiteral(class) => {
                        !class.instance_member(db, env, None, attr).is_undefined()
                    }
                    Type::SubclassOf(subclass_of @ SubclassOfType { .. }) => {
                        match subclass_of.subclass_of() {
                            SubclassOfInner::Class(class) => {
                                !class.instance_member(db, env, attr).is_undefined()
                            }
                            SubclassOfInner::Dynamic(_) => unreachable!(
                                "Attribute lookup on a dynamic `SubclassOf` type \
                                should always return a bound symbol"
                            ),
                            SubclassOfInner::Protocol(_) => false,
                            SubclassOfInner::TypeVar(_) => false,
                        }
                    }
                    _ => false,
                };

                if let Type::ModuleLiteral(module) = value_type {
                    let module = module.module(db);
                    let module_name = module.name(db);
                    if module.kind(db).is_package()
                        && let Some(relative_submodule) = ModuleName::new(attr_name)
                    {
                        let mut maybe_submodule_name = module_name.clone();
                        maybe_submodule_name.extend(&relative_submodule);
                        if resolve_module(
                            db,
                            ImportingFile::File(
                                self.file(),
                                self.program_environment().resolver_environment(db),
                            ),
                            &maybe_submodule_name,
                        )
                        .is_some()
                        {
                            if let Some(builder) = self
                                .context
                                .report_lint(&POSSIBLY_MISSING_SUBMODULE, attribute)
                            {
                                let mut diag = builder.into_diagnostic(format_args!(
                                    "Submodule `{attr_name}` might not have been imported"
                                ));
                                diag.help(format_args!(
                                    "Consider explicitly importing `{maybe_submodule_name}`"
                                ));
                            }
                            return fallback();
                        }
                    }
                }

                if let Type::SpecialForm(special_form) = value_type {
                    if let Some(builder) =
                        self.context.report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                    {
                        let mut diag = builder.into_diagnostic(format_args!(
                            "Special form `{special_form}` has no attribute `{attr_name}`",
                        ));
                        if let Ok(defined_type) = value_type.in_type_expression(
                            db,
                            self.scope(),
                            self.typevar_binding_context,
                            self.inference_flags(),
                        ) && !defined_type.member(db, env, attr_name).place.is_undefined()
                        {
                            diag.help(format_args!(
                                "Objects with type `{ty}` have a{maybe_n} `{attr_name}` \
                                attribute, but the symbol `{special_form}` \
                                does not itself inhabit the type `{ty}`",
                                maybe_n = if attr_name.starts_with(['a', 'e', 'i', 'o', 'u']) {
                                    "n"
                                } else {
                                    ""
                                },
                                ty = defined_type.display(db, env)
                            ));
                            if is_dotted_name(value) {
                                let source = &source_text(self.db(), self.file())[value.range()];
                                diag.help(format_args!(
                                    "This error may indicate that `{source}` was defined as \
                                    `{source} = {special_form}` when \
                                    `{source}: {special_form}` was intended"
                                ));
                            }
                        }
                    }
                    return fallback();
                }

                let Some(builder) = self.context.report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                else {
                    return fallback();
                };

                if bound_on_instance {
                    builder.into_diagnostic(format_args!(
                        "Attribute `{attr_name}` can only be accessed on instances, \
                        not on the class object `{}` itself.",
                        value_type.display(db, env)
                    ));
                    return fallback();
                }

                let mut diagnostic = match value_type {
                    Type::ModuleLiteral(module) => builder.into_diagnostic(format_args!(
                        "Module `{module_name}` has no member `{attr_name}`",
                        module_name = module.module(db).name(db),
                    )),
                    Type::ClassLiteral(class) => builder.into_diagnostic(format_args!(
                        "Class `{}` has no attribute `{attr_name}`",
                        class.name(db),
                    )),
                    Type::GenericAlias(alias) => builder.into_diagnostic(format_args!(
                        "Class `{}` has no attribute `{attr_name}`",
                        alias.display(db, env),
                    )),
                    Type::FunctionLiteral(function) => builder.into_diagnostic(format_args!(
                        "Function `{}` has no attribute `{attr_name}`",
                        function.name(db),
                    )),
                    _ => builder.into_diagnostic(format_args!(
                        "Object of type `{}` has no attribute `{attr_name}`",
                        value_type.display(db, env),
                    )),
                };

                if value_type.is_callable_type()
                    && KnownClass::FunctionType
                        .to_instance(db, env)
                        .member(db, env, attr_name)
                        .place
                        .is_definitely_bound()
                {
                    diagnostic.help(format_args!(
                        "Function objects have a{maybe_n} `{attr_name}` attribute, \
                        but not all callable objects are functions",
                        maybe_n = if attr_name
                            .trim_start_matches('_')
                            .starts_with(['a', 'e', 'i', 'o', 'u'])
                        {
                            "n"
                        } else {
                            ""
                        },
                    ));

                    // without the <> around the URL, if you double click on the URL in the terminal it tries to load
                    // https://docs.astral.sh/ty/reference/typing-faq/#why-does-ty-say-callable-has-no-attribute-__name
                    // (without the __ suffix at the end of the URL). That doesn't exist, so the page loaded in the
                    // browser opens at the top of the FAQs page instead of taking you directly to the relevant FAQ.
                    diagnostic.help(
                        "See this FAQ for more information: \
                        <https://docs.astral.sh/ty/reference/typing-faq/\
                        #why-does-ty-say-callable-has-no-attribute-__name__>",
                    );
                } else {
                    hint_if_stdlib_attribute_exists_on_other_versions(
                        db,
                        self.program_file(),
                        diagnostic,
                        value_type,
                        attr_name,
                        &format!("resolving the `{attr_name}` attribute"),
                    );
                }

                fallback()
            }
            LookupError::PossiblyUndefined(type_when_bound) => {
                // `PossiblyUndefined` is ambiguous here. It could be because an attribute is
                // conditionally defined, for example:
                // ```
                // class Foo:
                //     if flag:
                //         x = 42
                // ```
                // That is indeed a "possibly missing attribute", and it's a warning by default, because
                // there's a high false positive rate.
                //
                // On the other hand, we could be looking at a union where some elements have
                // the attribute but others definitely don't. That's a very different case, and
                // we want it to be an error. Use `as_union_like` here to handle type aliases
                // of unions and `NewType`s of float/complex in addition to explicit unions.
                //
                // Attribute lookup on a bounded type variable delegates to its upper bound, so
                // use that bound here too when determining whether the lookup was on a union.
                let union_like_type = if let Type::TypeVar(typevar) = value_type
                    && let Some(bound) = typevar.typevar(db).upper_bound(db, env)
                {
                    bound
                } else {
                    value_type
                };

                if let Some(union) = union_like_type.as_union_like(db) {
                    let mut elements_missing_the_attribute = FxIndexSet::default();
                    for element in union.elements(db) {
                        union_elements_missing_attribute(
                            db,
                            env,
                            *element,
                            attr_name,
                            &mut elements_missing_the_attribute,
                        );
                    }

                    if !elements_missing_the_attribute.is_empty() {
                        if let Some(builder) =
                            self.context.report_lint(&UNRESOLVED_ATTRIBUTE, attribute)
                        {
                            let types = std::iter::once(union_like_type)
                                .chain(elements_missing_the_attribute.iter().copied());
                            let settings =
                                DisplaySettings::from_possibly_ambiguous_types(db, env, types);
                            let missing_types = elements_missing_the_attribute
                                .iter()
                                .map(|ty| {
                                    format!("`{}`", ty.display_with(db, env, settings.clone()))
                                })
                                .collect::<Vec<_>>()
                                .join(", ");

                            builder.into_diagnostic(format_args!(
                                "Attribute `{attr_name}` is not defined on {} \
                                in union `{union_like_type}`",
                                missing_types,
                                union_like_type = union_like_type.display_with(db, env, settings),
                            ));
                        }
                        return type_when_bound;
                    }
                }

                report_possibly_missing_attribute(&self.context, attribute, &attr.id, value_type);

                type_when_bound
            }
        }
    }
}
