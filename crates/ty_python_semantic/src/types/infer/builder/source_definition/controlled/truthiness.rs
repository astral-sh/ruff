//! Truthiness follows the ordinary decisions and admits each stored read separately.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::Truthiness;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::bool::source::{self, BoolEffects, BoolFacts, BoolResult, TruthinessOperation};
use crate::types::call::Bindings;
use crate::types::enums::EnumComplementType;
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::known_instance::InternedConstraintSet;
use crate::types::literal::{BytesLiteralType, EnumLiteralType, StringLiteralType};
use crate::types::newtype::NewType;
use crate::types::typevar::{BoundTypeVarInstance, TypeVarConstraints};
use crate::types::{
    CallDunderError, CallableType, ClassLiteral, ClassType, IntersectionType, KnownClass,
    NominalInstanceType, PropertyInstanceClass, PropertyInstanceType, RecursiveType,
    SubclassOfInner, Type, TypeAliasType, TypeVarBoundOrConstraints, TypedDictType, UnionType,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn try_type_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<BoolResult<'db>> {
        self.environment_program(env).await?;
        source::try_bool_with(
            ty,
            false,
            BoolFacts,
            &TruthinessSourceEffects { source: self, env },
        )
        .await
    }

    pub(in crate::types::infer::builder) async fn type_truthiness(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Truthiness> {
        self.environment_program(env).await?;
        let result = source::try_bool_with(
            ty,
            true,
            BoolFacts,
            &TruthinessSourceEffects { source: self, env },
        )
        .await?;
        self.local(1, 0, || {
            result.unwrap_or_else(|error| error.fallback_truthiness())
        })
        .await
    }
}

struct TruthinessSourceEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    env: &'effects ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TruthinessSourceEffects<'_, '_, 'run, 'db, A> {
    async fn unavailable<T>(&self, operation: TruthinessOperation) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::Truthiness(operation))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BoolEffects<'db>
    for TruthinessSourceEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn recurse(
        &self,
        _ty: Type<'db>,
        _allow_short_circuit: bool,
    ) -> RunResult<BoolResult<'db>> {
        self.unavailable(TruthinessOperation::RecursiveTruthiness)
            .await
    }

    async fn unbound_recursive(&self) -> RunResult<BoolResult<'db>> {
        self.unavailable(TruthinessOperation::UnboundRecursiveVariable)
            .await
    }

    async fn callable_has_runtime_class(&self, callable: CallableType<'db>) -> RunResult<bool> {
        self.source
            .local(1, 0, || callable.runtime_class(self.source.db()).is_some())
            .await
    }

    async fn unfold(&self, _recursive: RecursiveType<'db>) -> RunResult<Option<Type<'db>>> {
        self.unavailable(TruthinessOperation::RecursiveUnfold).await
    }

    async fn typed_dict_has_required_fields(&self, _td: TypedDictType<'db>) -> RunResult<bool> {
        self.unavailable(TruthinessOperation::TypedDictRequiredFields)
            .await
    }

    async fn typed_dict_is_closed(&self, _td: TypedDictType<'db>) -> RunResult<bool> {
        self.unavailable(TruthinessOperation::TypedDictOpenness)
            .await
    }

    async fn typed_dict_has_present_fields(&self, _td: TypedDictType<'db>) -> RunResult<bool> {
        self.unavailable(TruthinessOperation::TypedDictPresentFields)
            .await
    }

    async fn constraints_satisfied(
        &self,
        _constraints: InternedConstraintSet<'db>,
    ) -> RunResult<bool> {
        self.unavailable(TruthinessOperation::ConstraintSet).await
    }

    async fn property_class(
        &self,
        property: PropertyInstanceType<'db>,
    ) -> RunResult<PropertyInstanceClass<'db>> {
        self.source
            .local(1, 0, || property.instance_class(self.source.db()))
            .await
    }

    async fn instance(&self, _class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::Instance).await
    }

    async fn metaclass_instance(&self, _class: ClassType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::MetaclassInstance)
            .await
    }

    async fn class_metaclass_instance(&self, _class: ClassLiteral<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::MetaclassInstance)
            .await
    }

    async fn transpose_typevar(
        &self,
        _inner: SubclassOfInner<'db>,
    ) -> RunResult<SubclassOfInner<'db>> {
        self.unavailable(TruthinessOperation::TypeVarTranspose)
            .await
    }

    async fn typevar_bounds(
        &self,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        self.unavailable(TruthinessOperation::TypeVarBounds).await
    }

    async fn constraint_types(
        &self,
        _constraints: TypeVarConstraints<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::ConstraintTypes).await
    }

    async fn known_class_truthiness(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Truthiness>> {
        let known = self
            .source
            .allocate_future(|| nominal_known_class_with(instance, NominalClassFacts, self.source))
            .await?
            .await?;
        self.source
            .local_with_fixed_transfers(2, 0, || known.and_then(KnownClass::bool))
            .await
    }

    async fn dunders(&self, ty: Type<'db>) -> RunResult<BoolResult<'db>> {
        self.source
            .allocate_future(|| source::try_dunders_with(ty, BoolFacts, self))
            .await?
            .await
    }

    async fn union(
        &self,
        union: UnionType<'db>,
        allow_short_circuit: bool,
    ) -> RunResult<BoolResult<'db>> {
        source::try_union_with(union, allow_short_circuit, BoolFacts, self).await
    }

    async fn union_elements(&self, union: UnionType<'db>) -> RunResult<&'db [Type<'db>]> {
        self.source
            .local(1, 0, || union.elements(self.source.db()))
            .await
    }

    async fn next_element(
        &self,
        elements: &'db [Type<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .local(2, 0, || {
                let next = elements.get(*cursor).copied();
                *cursor += usize::from(next.is_some());
                next
            })
            .await
    }

    async fn intersection_alternatives(
        &self,
        intersection: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .intersection_alternatives(self.env, intersection)
            .await
    }

    async fn enum_literals(&self, _complement: EnumComplementType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::EnumLiterals).await
    }

    async fn enum_instance(&self, _literal: EnumLiteralType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::EnumInstance).await
    }

    async fn string_nonempty(&self, literal: StringLiteralType<'db>) -> RunResult<bool> {
        self.source
            .local(1, 0, || !literal.value(self.source.db()).is_empty())
            .await
    }

    async fn bytes_nonempty(&self, literal: BytesLiteralType<'db>) -> RunResult<bool> {
        self.source
            .local(1, 0, || !literal.value(self.source.db()).is_empty())
            .await
    }

    async fn alias(
        &self,
        _alias: TypeAliasType<'db>,
        _allow_short_circuit: bool,
    ) -> RunResult<BoolResult<'db>> {
        self.unavailable(TruthinessOperation::TypeAlias).await
    }

    async fn newtype_base(&self, _newtype: NewType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::NewType).await
    }

    async fn call_dunder(
        &self,
        _ty: Type<'db>,
        _name: &'static str,
    ) -> RunResult<Result<Bindings<'db>, CallDunderError<'db>>> {
        self.unavailable(TruthinessOperation::DunderCall).await
    }

    async fn return_type(&self, _bindings: &Bindings<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::DunderReturn).await
    }

    async fn known_instance(&self, _class: KnownClass) -> RunResult<Type<'db>> {
        self.unavailable(TruthinessOperation::KnownInstance).await
    }

    async fn assignable(&self, _source: Type<'db>, _target: Type<'db>) -> RunResult<bool> {
        self.unavailable(TruthinessOperation::ReturnAssignability)
            .await
    }

    async fn tuple_truthiness(
        &self,
        _instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Truthiness>> {
        self.unavailable(TruthinessOperation::TupleSpec).await
    }

    async fn instance_is_final(&self, _instance: NominalInstanceType<'db>) -> RunResult<bool> {
        self.unavailable(TruthinessOperation::ClassFinality).await
    }

    async fn len_truthiness(&self, ty: Type<'db>) -> RunResult<BoolResult<'db>> {
        source::try_len_with(ty, BoolFacts, self).await
    }
}
