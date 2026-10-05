mod typing_self;

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;
use ty_python_core::scope::{FileScopeId, ScopeId};
use ty_python_core::{ProgramFile, SemanticIndex};

use super::storage::slots;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::class::class_default_specialization_with;
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::context::InferContext;
use crate::types::diagnostic::MissingTypeArgumentEffects;
use crate::types::generics::binding::{TypeVarBindingEffects, bind_typevar_with};
use crate::types::generics::context_construction::{ContextVariables, context_from_typevars_with};
use crate::types::generics::defaults::default_specialization_with_effects;
use crate::types::infer::InferenceFlags;
use crate::types::instance::{
    NominalClassFacts, nominal_class_with, nominal_is_definition_generic_with,
    nominal_known_class_with,
};
use crate::types::known_instance::{InternedType, UnionTypeInstance};
use crate::types::legacy_typevars::find_legacy_typevars_with_effects;
use crate::types::mapping::effects::MappingWork;
use crate::types::mapping::specialization_start::SpecializationStartEffects;
use crate::types::subclass_of::{
    SubclassConstructionFacts, SubclassInstanceEffects, SubclassInstanceFacts, subclass_from_with,
    subclass_instance_inner_with,
};
use crate::types::tuple::TupleSpec;
use crate::types::tuple::construction::tuple_type;
use crate::types::type_expression_conversion::known_instance::{
    KnownInstanceConversionEffects, KnownInstanceConversionFacts,
    in_type_expression_known_instance_with,
};
use crate::types::type_expression_conversion::special_form::typing_self::{SelfAnnotationFacts, self_annotation_with};
use crate::types::type_expression_conversion::special_form::{
    SpecialFormConversionEffects, SpecialFormConversionFacts, in_type_expression_special_form_with,
};
use crate::types::type_expression_conversion::{
    DefaultTypeSpecializationEffects, SubclassArgumentEffects, TypeConversionOperation,
    TypeExpressionConversionEffects,
};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, GenericContext, IntersectionType,
    InvalidTypeExpression, InvalidTypeExpressionError, KnownClass, KnownInstanceType, KnownUnion,
    MaterializationKind, NominalInstanceType, ProtocolInstanceType, SpecialFormType,
    Specialization, SubclassOfInner, Type, TypeAliasType, TypeVarKind, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DefaultTypeSpecializationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn new_variables(&self) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        self.new_legacy_variables().await
    }
    async fn collect(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<()> {
        find_legacy_typevars_with_effects(self.db(), env, ty, None, variables, self).await
    }
    async fn context(
        &self,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        let work = Self::checked(
            slots(variables.capacity())
                .and_then(|slots| slots.checked_add(variables.len()))
                .and_then(|work| work.checked_add(4)),
        )?;
        let variables = self.local(work, 0, || variables.into_iter()).await?;
        context_from_typevars_with(self.db(), env, variables, self).await
    }
    async fn defaults(&self, context: GenericContext<'db>) -> RunResult<Specialization<'db>> {
        default_specialization_with_effects(self.db(), context, None, self).await
    }
    async fn apply(
        &self,
        ty: Type<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<Type<'db>> {
        match ty
            .specialization_start_with(self.db(), specialization, false, self)
            .await?
        {
            Some(ty) => Ok(ty),
            None => self.access.apply_specialization(ty, specialization, false).await,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SpecializationStartEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn checkpoint(&self, _work: MappingWork) -> RunResult<()> {
        self.work(1).await
    }
    async fn nominal_is_definition_generic(
        &self,
        _db: &'db dyn Db,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        nominal_is_definition_generic_with(instance, NominalClassFacts, self).await
    }
    async fn typevar_is_paramspec(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        let identity = self.field(variable.identity_request(fields)).await?;
        let kind = self
            .field(identity.identity.field_requests(fields).kind())
            .await?;
        self.local(1, 0, || kind.is_paramspec()).await
    }
    async fn lookup_typevar(
        &self,
        _db: &'db dyn Db,
        _specialization: Specialization<'db>,
        _variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::SpecializationTypeVarLookup,
        ))
        .await
    }
    async fn materialization_kind(
        &self,
        _db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.field(
            specialization
                .field_requests(self.access.endpoint().field_request_context())
                .materialization_kind(),
        )
        .await
    }
    async fn typevar_is_self(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        let typevar = self
            .field(variable.field_requests(fields).typevar())
            .await?;
        let identity = self
            .field(typevar.field_requests(fields).identity())
            .await?;
        let kind = self.field(identity.field_requests(fields).kind()).await?;
        self.local(1, 0, || matches!(kind, TypeVarKind::TypingSelf))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeExpressionConversionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn class_known(&self, class: ClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        let class = self.local(1, 0, || class.as_static()).await?;
        match class {
            Some(class) => {
                self.field(
                    class
                        .field_requests(self.access.endpoint().field_request_context())
                        .known(),
                )
                .await
            }
            None => Ok(None),
        }
    }
    async fn numeric_union(
        &self,
        _env: &ProgramEnvironment<'db>,
        _union: KnownUnion,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::NumericUnion,
        ))
        .await
    }
    async fn class_default(&self, class: ClassLiteral<'db>) -> RunResult<ClassType<'db>> {
        crate::types::type_expression_conversion::conversion_class_default_with(
            class,
            crate::types::type_expression_conversion::ConversionFacts,
            self,
        )
        .await
    }
    async fn static_class_default(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        class_default_specialization_with(class, self).await
    }
    async fn instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        Type::instance_with(self.db(), env, self, class).await
    }
    async fn nominal_known(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        nominal_known_class_with(instance, NominalClassFacts, self).await
    }
    async fn none(&self, env: &ProgramEnvironment<'db>) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.access
            .known_class_instance(self.program, KnownClass::NoneType)
            .await
    }
    async fn recursive(
        &self,
        _recursive: crate::types::RecursiveType<'db>,
        _scope: ScopeId<'db>,
        _binding: Option<Definition<'db>>,
        _flags: InferenceFlags,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::Recursive,
        ))
        .await
    }
    async fn unbound_recursive(
        &self,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::UnboundRecursiveVariable,
        ))
        .await
    }
    async fn known_instance(
        &self,
        known: KnownInstanceType<'db>,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        in_type_expression_known_instance_with(
            known,
            scope,
            binding,
            flags,
            KnownInstanceConversionFacts,
            self,
        )
        .await
    }
    async fn special_form(
        &self,
        special_form: SpecialFormType,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        in_type_expression_special_form_with(
            special_form,
            scope,
            binding,
            flags,
            SpecialFormConversionFacts,
            self,
        )
        .await
    }
    async fn union(
        &self,
        _union: UnionType<'db>,
        _scope: ScopeId<'db>,
        _binding: Option<Definition<'db>>,
        _flags: InferenceFlags,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::Union,
        ))
        .await
    }
    async fn alias(
        &self,
        _alias: TypeAliasType<'db>,
        _scope: ScopeId<'db>,
        _binding: Option<Definition<'db>>,
        _flags: InferenceFlags,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::TypeAlias,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SpecialFormConversionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn dispatch(&self) -> RunResult<()> {
        // Reserve the finite dispatch, flag predicates, and inline error wrapping.
        self.work(16).await
    }

    async fn known_class_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        self.access.known_class_instance(program, class).await
    }

    async fn homogeneous_tuple(
        &self,
        env: &ProgramEnvironment<'db>,
        element: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let spec = self.local(4, 0, || TupleSpec::homogeneous(element)).await?;
        Ok(Type::tuple(tuple_type(self.db(), env, &spec, self).await?))
    }

    async fn type_form(&self, argument: Type<'db>) -> RunResult<Type<'db>> {
        self.access.intern_typeform(argument).await
    }

    async fn intersection(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.access
            .intersection_from_two_elements(left, right)
            .await
    }

    async fn unknown_callable(&self) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::SpecialFormCallable,
        ))
        .await
    }

    async fn typing_self(
        &self,
        scope: ScopeId<'db>,
        binding: Option<Definition<'db>>,
        flags: InferenceFlags,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpression<'db>>> {
        self.type_parameter_future(|| {
            self_annotation_with(scope, binding, flags, SelfAnnotationFacts, self)
        }).await?.await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> KnownInstanceConversionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn dispatch(&self) -> RunResult<()> {
        // Reserve the finite dispatch, kind predicates, and inline result construction.
        self.work(16).await
    }

    async fn kind(&self, variable: TypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        TypeVarBindingEffects::kind(self, variable).await
    }

    async fn scope_program_file(&self, scope: ScopeId<'db>) -> RunResult<ProgramFile<'db>> {
        self.scope_file(scope).await
    }

    async fn semantic_index(&self, file: ProgramFile<'db>) -> RunResult<&'db SemanticIndex<'db>> {
        self.access.semantic_index(file).await
    }

    async fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> RunResult<FileScopeId> {
        self.field(
            scope
                .read_fields(self.access.endpoint().field_request_context())
                .file_scope_id(),
        )
        .await
    }

    async fn bind_typevar(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        binding: Option<Definition<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        bind_typevar_with(self.db(), index, scope, binding, variable, self).await
    }

    async fn interned_inner(&self, inner: InternedType<'db>) -> RunResult<Type<'db>> {
        self.field(
            inner
                .field_requests(self.access.endpoint().field_request_context())
                .inner(),
        )
        .await
    }

    async fn union_result(
        &self,
        _union: UnionTypeInstance<'db>,
    ) -> RunResult<Result<Type<'db>, InvalidTypeExpressionError<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::KnownInstanceUnionResult,
        ))
        .await
    }

    async fn to_meta_type(
        &self,
        _ty: Type<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::KnownInstanceMetaType,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassArgumentEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }
    async fn union_has_aliases(&self, union: UnionType<'db>) -> RunResult<bool> {
        let elements = self.union_elements_source(union).await?;
        let mut elements = elements.iter();
        while let Some(is_alias) = self
            .local(2, 0, || {
                elements.next().map(|element| element.is_alias_like())
            })
            .await?
        {
            if is_alias {
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn expand_union_aliases(
        &self,
        _env: &ProgramEnvironment<'db>,
        _union: UnionType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::UnionAliasExpansion,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassInstanceEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn nominal_class(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        self.environment_program(env).await?;
        nominal_class_with(instance, NominalClassFacts, self).await
    }
    async fn negative_empty(&self, intersection: IntersectionType<'db>) -> RunResult<bool> {
        let negative = self
            .field(
                intersection
                    .field_requests(self.access.endpoint().field_request_context())
                    .negative(),
            )
            .await?;
        self.local(1, 0, || negative.is_empty()).await
    }
    async fn union_conversion(
        &self,
        _env: &ProgramEnvironment<'db>,
        _union: UnionType<'db>,
    ) -> RunResult<Result<Type<'db>, Type<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::SubclassUnion,
        ))
        .await
    }
    async fn intersection_conversion(
        &self,
        _env: &ProgramEnvironment<'db>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<Result<Type<'db>, Type<'db>>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::SubclassIntersection,
        ))
        .await
    }
    async fn protocol_meta(
        &self,
        _env: &ProgramEnvironment<'db>,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::SubclassProtocol,
        ))
        .await
    }
    async fn inner(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<SubclassOfInner<'db>>> {
        subclass_instance_inner_with(env, ty, SubclassInstanceFacts, self).await
    }
    async fn subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        subclass_from_with(inner, SubclassConstructionFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MissingTypeArgumentEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    async fn generic_context(
        &self,
        _context: &InferContext<'db, '_>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let class = self.local(1, 0, || class.as_static()).await?;
        match class {
            Some(class) => self.access.class_generic_context(class).await,
            None => Ok(None),
        }
    }
    async fn variables(
        &self,
        _context: &InferContext<'db, '_>,
        generic: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        self.field(generic.variables_request(self.access.endpoint().field_request_context()))
            .await
    }
    async fn next_variable(
        &self,
        variables: &'db ContextVariables<'db>,
        index: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.local(2, 0, || {
            let variable = GenericContext::variable_at_in(variables, *index);
            if variable.is_some() {
                *index += 1;
            }
            variable
        })
        .await
    }
    async fn default_type(
        &self,
        _context: &InferContext<'db, '_>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.access.bound_typevar_default(variable).await
    }
    async fn report_class(
        &self,
        _context: &InferContext<'db, '_>,
        _class: ClassLiteral<'db>,
        _annotation: &ast::Expr,
        _required_count: usize,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::MissingArgumentDiagnostic,
        ))
        .await
    }
    async fn report_callable(
        &self,
        _context: &InferContext<'db, '_>,
        _annotation: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::TypeConversion(
            TypeConversionOperation::MissingArgumentDiagnostic,
        ))
        .await
    }
}
