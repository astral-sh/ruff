//! Tuple specification reads retain canonical strings, unions, and known-class instances.

use std::borrow::Cow;

use ruff_python_ast::PythonVersion;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class::{ClassType, GenericAlias, KnownClass};
use crate::types::instance::tuple_spec::{
    TupleSpecEffects, TupleSpecFacts, TupleSpecOperation, nominal_tuple_spec_with,
    tuple_instance_spec_with, version_info_spec_with,
};
use crate::types::instance::{NominalClassFacts, NominalInstanceClass, NominalInstanceType};
use crate::types::mro::MroIterator;
use crate::types::set_theoretic::RecursivelyDefined;
use crate::types::tuple::{TupleSpec, TupleType, VariableLengthTuple, VariableSegment};
use crate::types::{ClassBase, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn tuple_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        let env = ProgramEnvironment::from_program(self.environment_program(env).await?);
        tuple_instance_spec_with(ty, &env, TupleSpecFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleSpecEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn nominal_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        nominal_tuple_spec_with(instance, env, TupleSpecFacts, self).await
    }

    async fn exact_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        self.field(
            tuple
                .field_requests(self.access.endpoint().field_request_context())
                .tuple(),
        )
        .await
    }

    async fn version_info(&self, env: &ProgramEnvironment<'db>) -> RunResult<TupleSpec<'db>> {
        version_info_spec_with(env, TupleSpecFacts, self).await
    }

    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        self.local(2, 0, || {
            NominalClassFacts.class(salsa::FieldReads::new(self.db()), class)
        })
        .await
    }

    async fn class_known(&self, class: ClassType<'db>) -> RunResult<Option<KnownClass>> {
        self.local(3, 0, || class.known(self.db())).await
    }

    async fn mro(&self, _class: ClassType<'db>) -> RunResult<MroIterator<'db>> {
        self.unavailable(SourceOperation::TupleSpec(TupleSpecOperation::Mro))
            .await
    }

    async fn next_mro(&self, _mro: &mut MroIterator<'db>) -> RunResult<Option<ClassBase<'db>>> {
        self.unavailable(SourceOperation::TupleSpec(TupleSpecOperation::Mro))
            .await
    }

    async fn retire_mro(&self, _mro: MroIterator<'db>) -> RunResult<()> {
        self.unavailable(SourceOperation::TupleSpec(TupleSpecOperation::Mro))
            .await
    }

    async fn specialization_tuple(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        self.local(3, 0, || alias.specialization(self.db()).tuple(self.db()))
            .await
    }

    /// Constructs a homogeneous Unknown specification with its empty-owner retirement prepaid.
    async fn unknown_tuple(&self) -> RunResult<TupleSpec<'db>> {
        // A homogeneous specification contains no fixed-element allocation.
        // Bound twelve constructor steps, eight fixed transfers and four retirement steps.
        // The nested carriers below precede the final result covered by the transfer helper.
        let bytes = const {
            4 * size_of::<Type<'db>>()
                + 2 * size_of::<VariableSegment<'db>>()
                + 4 * size_of::<smallvec::SmallVec<[Type<'db>; 0]>>()
                + 2 * size_of::<VariableLengthTuple<Type<'db>, VariableSegment<'db>>>()
                + 2 * size_of::<usize>()
        };
        self.local_with_fixed_transfers(24, bytes, || TupleSpec::homogeneous(Type::unknown()))
            .await
    }

    async fn python_version(&self, env: &ProgramEnvironment<'db>) -> RunResult<PythonVersion> {
        let program = self.environment_program(env).await?;
        let fields = self.access.endpoint().field_request_context();
        let environment = self
            .field(program.field_requests(fields).resolver_environment())
            .await?;
        self.field(environment.read_fields(fields).python_version())
            .await
    }

    async fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        let program = self.environment_program(env).await?;
        self.access.known_class_instance(program, class).await
    }

    async fn string_literal(&self, value: &str) -> RunResult<Type<'db>> {
        self.access.string_literal(value).await
    }

    async fn release_elements(&self) -> RunResult<Vec<Type<'db>>> {
        let bytes = Self::checked(4usize.checked_mul(size_of::<Type<'db>>()))?;
        // Reserve all four slots and pay for retiring any initialized prefix after refusal.
        let work = Self::checked(bytes.checked_mul(2).and_then(|n| n.checked_add(6)))?;
        self.local(work, bytes, || Vec::with_capacity(4)).await
    }

    async fn append_release_element(
        &self,
        elements: &mut Vec<Type<'db>>,
        element: Type<'db>,
    ) -> RunResult<()> {
        self.local(3, 0, || elements.push(element)).await
    }

    async fn release_union(&self, elements: Vec<Type<'db>>) -> RunResult<Type<'db>> {
        let count = elements.len();
        let bytes = Self::checked(count.checked_mul(size_of::<Type<'db>>()))?;
        let work = Self::checked(
            bytes
                .checked_mul(2)
                .and_then(|n| n.checked_add(count))
                .and_then(|n| n.checked_add(3)),
        )?;
        let elements = self
            .local(work, bytes, || elements.into_boxed_slice())
            .await?;
        Ok(Type::Union(
            self.access
                .intern_union(elements, RecursivelyDefined::No)
                .await?,
        ))
    }

    async fn fixed_tuple(&self, elements: [Type<'db>; 5]) -> RunResult<TupleSpec<'db>> {
        let bytes = Self::checked(TupleSpec::fixed_clone_requested_bytes(elements.len()))?;
        let retirement = Self::checked(TupleSpec::fixed_retirement_work(elements.len()))?;
        let work = Self::checked(
            bytes
                .checked_mul(2)
                .and_then(|n| n.checked_add(retirement))
                .and_then(|n| n.checked_add(4)),
        )?;
        self.local(work, bytes, || TupleSpec::heterogeneous(elements))
            .await
    }
}
