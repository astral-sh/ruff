//! Admission for generated input conversion and native equality of source-query values.

use std::sync::Arc;

use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{
    NativeValueOperation, NativeValueQuote, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::Configuration;
use ty_python_core::definition::Definition;
use ty_python_core::expression::Expression;
use ty_python_core::predicate::CallableAndCallExpr;
use ty_python_core::scope::ScopeId;
use ty_python_core::{PlaceTable, Program, ProgramFile, UseDefMap};

use crate::FxIndexMap;
use crate::place::Place;
use crate::types::abstract_methods::AbstractMethod;
use crate::types::class::slots::InstanceLayout;
use crate::types::class::{
    ClassInstanceFlags, CodeGeneratorKind, KnownClassArgument, KnownClassLookupError,
    StaticClassLiteral,
};
use crate::types::constraints::OwnedConstraintSet;
use crate::types::enums::EnumMetadata;
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::infer::builder::source_definition::controlled::receiver_constraints::receiver_constraint_child_at;
use crate::types::local_transfer::local_with_fixed_transfers_at;
use crate::types::mro::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::narrow::{ExpressionNarrowingConstraints, admission};
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, GenericAlias, GenericContext, MemberLookupKey,
    MemberLookupResult, ResolvedMember, Truthiness, Type, TypePair,
};

use super::super::{
    DefinitionInference, ExpressionInference, FunctionDecoratorInference, InferExpression,
    InferScope, ScopeInference, native_values as inference,
};

// These implementations certify the generated FromIdWithDb path, not arbitrary Clone behavior.
// An interned handle reconstructs its identity; a supertype additionally selects a finite variant.
pub(super) trait DirectInput {
    const CONVERSION_WORK: usize;
}

macro_rules! direct_inputs {
    ($($input:ty => $work:expr),+ $(,)?) => {
        $(impl DirectInput for $input {
            const CONVERSION_WORK: usize = $work;
        })+
    };
}

direct_inputs! {
    Program<'_> => 1,
    ProgramFile<'_> => 1,
    ScopeId<'_> => 1,
    InferScope<'_> => 3,
    InferExpression<'_> => 3,
    Definition<'_> => 1,
    Expression<'_> => 1,
    FunctionType<'_> => 1,
    OverloadLiteral<'_> => 1,
    CallableAndCallExpr<'_> => 1,
    KnownClassArgument<'_> => 1,
    StaticClassLiteral<'_> => 1,
    GenericAlias<'_> => 1,
    ClassLiteral<'_> => 6,
    ClassType<'_> => 9,
    MemberLookupKey<'_> => 1,
    TypePair<'_> => 1,
    BoundTypeVarInstance<'_> => 1,
    TypeVarInstance<'_> => 1,
}

pub(super) trait OutputProfile<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run;
}

pub(super) async fn quote<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: Configuration,
    C::Input<'db>: DirectInput,
    C::Output<'db>: OutputProfile<'db>,
{
    let work = match operation {
        NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
            <C::Input<'db> as DirectInput>::CONVERSION_WORK
        }
        NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
            return Err(RunError::Contract(
                "source input profile requires generated handle conversion",
            ));
        }
        NativeValueOperation::Comparison { left, right } => {
            <C::Output<'db> as OutputProfile<'db>>::comparison_work(endpoint, left, right).await?
        }
    };
    Ok(NativeValueQuote {
        work,
        requested_bytes: 0,
        cleanup_work: 0,
    })
}

fn checked(work: Option<usize>) -> RunResult<usize> {
    work.ok_or(RunError::Contract("source native value quotation overflow"))
}

macro_rules! finite_outputs {
    ($($output:ty => $work:expr),+ $(,)?) => {
        $(impl<'db> OutputProfile<'db> for $output {
            async fn comparison_work<'run>(
                _endpoint: TaskEndpoint<'run, 'db>,
                _left: &Self,
                _right: &Self,
            ) -> RunResult<usize>
            where
                'db: 'run,
            {
                Ok($work)
            }
        })+
    };
}

// GenericContext equality compares its generated identity, without visiting its type variables.
finite_outputs! {
    bool => 1,
    ClassInstanceFlags => 1,
    Truthiness => 1,
    Option<GenericContext<'db>> => 2,
    Option<ScopeId<'db>> => 2,
    Option<CodeGeneratorKind<'db>> => 4,
}

async fn inline_comparison<'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    fields: usize,
    payload_bytes: impl FnOnce() -> Option<usize>,
) -> RunResult<usize> {
    Ok(endpoint
        .local_call(|| {
            endpoint.admit_work(fields)?;
            endpoint.check_completion()?;
            checked(fields.checked_add(checked(payload_bytes())?))
        })
        .await)
}

impl<'db> OutputProfile<'db> for Type<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        inline_comparison(endpoint, 2, || {
            left.inline_payload_bytes()
                .checked_add(right.inline_payload_bytes())
        })
        .await
    }
}

impl<'db> OutputProfile<'db> for Option<Type<'db>> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        inline_comparison(endpoint, 4, || {
            left.map_or(0, Type::inline_payload_bytes)
                .checked_add(right.map_or(0, Type::inline_payload_bytes))
        })
        .await
    }
}

impl<'db> OutputProfile<'db> for OwnedConstraintSet<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = local_with_fixed_transfers_at(&endpoint, 2, 0, || {
            DynamicComparisonQuote::new(&endpoint)
        })
        .await?;
        receiver_constraint_child_at(&endpoint, || quote.receiver_constraints(left)).await?;
        receiver_constraint_child_at(&endpoint, || quote.receiver_constraints(right)).await?;
        local_with_fixed_transfers_at(&endpoint, 2, 0, || quote.work).await
    }
}

impl<'db> OutputProfile<'db> for Box<[ClassLiteral<'db>]> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        // Each literal comparison selects a finite variant and compares an interned identity.
        super::local_with_fixed_transfers_at(&endpoint, 4, 0, || {
            checked(left.len().min(right.len()).checked_mul(8).and_then(|work| work.checked_add(2)))
        }).await?
    }
}

impl<'db> OutputProfile<'db> for Box<[Name]> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        for names in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            let mut entries = names.iter();
            while entries.len() != 0 {
                let chunk = entries.len().min(64);
                quote.scan(chunk).await?;
                for name in entries.by_ref().take(chunk) {
                    quote.add(checked(name.len().checked_add(1))?)?;
                }
            }
        }
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for InstanceLayout {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        quote.scan(2).await?;
        quote.add(2)?;
        for layout in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            let mut entries = layout.slot_names().iter();
            while entries.len() != 0 {
                let chunk = entries.len().min(64);
                quote.scan(chunk).await?;
                for name in entries.by_ref().take(chunk) {
                    quote.add(checked(name.len().checked_add(1))?)?;
                }
            }
        }
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for Option<FxHashSet<Name>> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        for value in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            if let Some(names) = value {
                quote.scan(name_table_slots(names.capacity())?).await?;
                let mut name_bytes = 0usize;
                let mut entries = names.iter();
                while entries.len() != 0 {
                    let chunk = entries.len().min(64);
                    quote.scan(chunk).await?;
                    for name in entries.by_ref().take(chunk) {
                        name_bytes = checked(name_bytes.checked_add(name.len()))?;
                    }
                }
                quote.add(name_table_work(names.len(), names.capacity(), name_bytes)?)?;
            }
        }
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for FxIndexMap<Name, AbstractMethod<'db>> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        for methods in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            let mut name_bytes = 0usize;
            let mut entries = methods.iter();
            while entries.len() != 0 {
                let chunk = entries.len().min(64);
                quote.scan(chunk).await?;
                for (name, _) in entries.by_ref().take(chunk) {
                    name_bytes = checked(name_bytes.checked_add(name.len()))?;
                }
            }
            quote.add(name_table_work(methods.len(), methods.capacity(), name_bytes)?)?;
            // AbstractMethod equality compares the ClassType discriminants and handle identity,
            // the Definition identity, and the AbstractMethodKind; it reads no class or definition
            // fields. Eight scalar steps per entry cover those comparisons and their control flow.
            quote.add(checked(methods.len().checked_mul(8))?)?;
        }
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for ExpressionNarrowingConstraints<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        admission::comparison_work(&endpoint, left, right).await
    }
}

impl<'db> OutputProfile<'db>
    for Result<Option<StaticClassLiteral<'db>>, KnownClassLookupError<'db>>
{
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let payload = |value: &Self| match value {
            Err(KnownClassLookupError::SymbolNotAClass { found_type, .. }) => {
                found_type.inline_payload_bytes()
            }
            _ => 0,
        };
        inline_comparison(endpoint, 8, || payload(left).checked_add(payload(right))).await
    }
}

impl<'db> OutputProfile<'db> for MemberLookupResult<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let payload = |value: &Self| match value {
            Ok(ResolvedMember::Plain(member)) => match member.place {
                Place::Defined(place) => place.ty.inline_payload_bytes(),
                Place::Undefined => 0,
            },
            Ok(ResolvedMember::WithMetadata(_)) | Err(_) => 0,
        };
        inline_comparison(endpoint, 16, || payload(left).checked_add(payload(right))).await
    }
}

impl<'db> OutputProfile<'db> for Box<[Type<'db>]> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut work = 2usize;
        for value in [left, right] {
            endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    endpoint.check_completion()
                })
                .await;
            for ty in value.iter() {
                work = endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        endpoint.check_completion()?;
                        checked(
                            work.checked_add(1)
                                .and_then(|work| work.checked_add(ty.inline_payload_bytes())),
                        )
                    })
                    .await;
            }
        }
        Ok(work)
    }
}

struct DynamicComparisonQuote<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
    work: usize,
}

impl<'a, 'run, 'db: 'run> DynamicComparisonQuote<'a, 'run, 'db> {
    fn new(endpoint: &'a TaskEndpoint<'run, 'db>) -> Self {
        Self { endpoint, work: 0 }
    }

    async fn scan(&self, work: usize) -> RunResult<()> {
        self.endpoint
            .local_call(|| self.endpoint.admit_work(work))
            .await;
        self.endpoint.checkpoint()?.await
    }

    fn add(&mut self, work: usize) -> RunResult<()> {
        self.work = checked(self.work.checked_add(work))?;
        Ok(())
    }

    fn ty(&mut self, ty: Type<'db>) -> RunResult<()> {
        self.add(checked(ty.inline_payload_bytes().checked_add(1))?)
    }

    async fn types(
        &mut self,
        mut types: impl ExactSizeIterator<Item = Type<'db>>,
    ) -> RunResult<()> {
        while types.len() != 0 {
            let chunk = types.len().min(64);
            self.scan(chunk).await?;
            for ty in types.by_ref().take(chunk) {
                self.ty(ty)?;
            }
        }
        Ok(())
    }

    async fn mro(&mut self, mro: &Mro<'db>) -> RunResult<()> {
        self.scan(1).await?;
        self.add(1)?;
        self.types(mro.iter().map(Type::from)).await
    }

    async fn mro_error(&mut self, error: &StaticMroError<'db>) -> RunResult<()> {
        self.scan(8).await?;
        self.add(8)?;
        match error.reason() {
            StaticMroErrorKind::InvalidBases(bases) => {
                self.add(bases.len())?;
                self.types(bases.iter().map(|(_, ty)| *ty)).await?;
            }
            StaticMroErrorKind::DuplicateBases(bases) => {
                for base in bases {
                    self.scan(4).await?;
                    self.add(checked(base.later_indices.len().checked_add(4))?)?;
                    self.ty(Type::from(base.duplicate_base))?;
                }
            }
            StaticMroErrorKind::UnresolvableMro { bases_list, .. } => {
                self.types(bases_list.iter().copied()).await?;
            }
            StaticMroErrorKind::Pep695ClassWithGenericInheritance
            | StaticMroErrorKind::InheritanceCycle => {}
        }
        self.mro(error.fallback_mro()).await
    }

    /// Quotes all retained constraint equality fields, including source-order-only constraints.
    async fn receiver_constraints(
        &mut self,
        constraints: &OwnedConstraintSet<'db>,
    ) -> RunResult<()> {
        let endpoint = self.endpoint;
        let mut pairs = local_with_fixed_transfers_at(endpoint, 18, 0, || {
            // This metadata accessor reads container lengths without traversing their entries.
            // The multiplier covers the scalar fields of constraints, decision nodes, supports
            // and source-order nodes; inline Type payloads are counted separately below.
            self.add(checked(
                constraints
                    .retirement_work()
                    .and_then(|work| work.checked_mul(8)),
            )?)?;
            Ok::<_, RunError>(constraints.native_comparison_type_pairs())
        })
        .await??;
        loop {
            let pair = local_with_fixed_transfers_at(endpoint, 4, 0, || pairs.next()).await?;
            let Some([left, right]) = pair else {
                break;
            };
            local_with_fixed_transfers_at(endpoint, 8, 0, || {
                self.ty(left)?;
                self.ty(right)
            })
            .await??;
            endpoint.checkpoint()?.await?;
        }
        local_with_fixed_transfers_at(endpoint, 2, 0, || ()).await
    }

    /// Quotes native equality of a retained signature without resolving its semantic handles.
    async fn signature(&mut self, signature: &Signature<'db>) -> RunResult<()> {
        self.scan(16).await?;
        self.add(16)?;
        self.ty(signature.return_ty)?;
        let mut parameters = signature.parameters().iter();
        while parameters.len() != 0 {
            let chunk = parameters.len().min(64);
            self.scan(checked(chunk.checked_mul(8))?).await?;
            for parameter in parameters.by_ref().take(chunk) {
                self.add(8)?;
                self.add(parameter.name().map(|name| name.len()).unwrap_or(0))?;
                self.ty(parameter.annotated_type())?;
                if let Some(default) = parameter.eager_default_type() {
                    self.ty(default)?;
                }
            }
        }
        if let Some(constraints) = signature.receiver_constraints() {
            let endpoint = self.endpoint;
            receiver_constraint_child_at(endpoint, || self.receiver_constraints(constraints))
                .await?;
        }
        Ok(())
    }

    async fn enum_metadata(&mut self, metadata: &EnumMetadata<'db>) -> RunResult<()> {
        self.scan(16).await?;
        self.add(16)?;
        if let Some(annotation) = metadata.value_annotation_type() {
            self.ty(annotation)?;
        }

        let mut member_names = 0usize;
        let mut members = metadata.members.iter();
        while members.len() != 0 {
            let chunk = members.len().min(64);
            self.scan(checked(chunk.checked_mul(2))?).await?;
            for (name, ty) in members.by_ref().take(chunk) {
                member_names = checked(member_names.checked_add(name.len()))?;
                self.ty(*ty)?;
            }
        }
        self.add(name_table_work(
            metadata.members.len(),
            metadata.members.capacity(),
            member_names,
        )?)?;

        let aliases = metadata.aliases();
        self.scan(name_table_slots(aliases.capacity())?).await?;
        let mut alias_names = 0usize;
        let mut entries = aliases.iter();
        while entries.len() != 0 {
            let chunk = entries.len().min(64);
            self.scan(checked(chunk.checked_mul(2))?).await?;
            for (name, target) in entries.by_ref().take(chunk) {
                alias_names = checked(
                    alias_names
                        .checked_add(name.len())
                        .and_then(|bytes| bytes.checked_add(target.len())),
                )?;
            }
        }
        self.add(name_table_work(
            aliases.len(),
            aliases.capacity(),
            alias_names,
        )?)?;

        self.scan(name_table_slots(metadata.auto_members.capacity())?)
            .await?;
        let mut auto_names = 0usize;
        let mut names = metadata.auto_members.iter();
        while names.len() != 0 {
            let chunk = names.len().min(64);
            self.scan(chunk).await?;
            for name in names.by_ref().take(chunk) {
                auto_names = checked(auto_names.checked_add(name.len()))?;
            }
        }
        self.add(name_table_work(
            metadata.auto_members.len(),
            metadata.auto_members.capacity(),
            auto_names,
        )?)
    }
}

fn name_table_slots(capacity: usize) -> RunResult<usize> {
    // Enum metadata's aliases and auto-member sets are insert-only. Its member IndexMap is
    // shrunk after construction, so its index table and ordered entries are bounded by the
    // retained capacity. These invariants also hold for ordinary-produced enum metadata.
    // Completed `__all__` sets are also shrunk before publication.
    // Completed abstract-method maps are also shrunk before publication.
    if capacity == 0 {
        return Ok(0);
    }
    checked(
        capacity
            .checked_add(1)
            .and_then(|capacity| capacity.checked_mul(4))
            .and_then(|slots| slots.checked_add(32)),
    )
}

fn name_table_work(len: usize, capacity: usize, name_bytes: usize) -> RunResult<usize> {
    let slots = name_table_slots(capacity)?;
    // Native map/set equality checks lengths before probing. Summing this bound for both
    // operands covers sparse traversal, every possible collision probe, and the bytes of
    // names hashed or compared by those probes. Map values are counted separately.
    checked(
        len.checked_add(1)
            .and_then(|count| count.checked_mul(slots.checked_add(name_bytes)?.checked_add(1)?)),
    )
}

impl<'db> OutputProfile<'db> for CallableSignature<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        for value in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            for signature in value {
                quote.signature(signature).await?;
            }
        }
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for Signature<'db> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        quote.signature(left).await?;
        quote.signature(right).await?;
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>) {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        // Slice lengths bound the comparisons of interned identities in source order. The fixed
        // cost includes lengths and the optional implementation; no semantic fields are read.
        inline_comparison(endpoint, 8, || left.0.len().checked_add(right.0.len())).await
    }
}

impl<'db> OutputProfile<'db> for Result<Mro<'db>, Box<StaticMroError<'db>>> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        for value in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            match value {
                Ok(mro) => quote.mro(mro).await?,
                Err(error) => quote.mro_error(error).await?,
            }
        }
        Ok(quote.work)
    }
}

impl<'db> OutputProfile<'db> for Option<EnumMetadata<'db>> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        let mut quote = DynamicComparisonQuote::new(&endpoint);
        for value in [left, right] {
            quote.scan(1).await?;
            quote.add(1)?;
            if let Some(metadata) = value {
                quote.enum_metadata(metadata).await?;
            }
        }
        Ok(quote.work)
    }
}

macro_rules! inference_outputs {
    ($($output:ty => $quote:path),+ $(,)?) => {
        $(impl<'db> OutputProfile<'db> for $output {
            async fn comparison_work<'run>(
                endpoint: TaskEndpoint<'run, 'db>,
                left: &Self,
                right: &Self,
            ) -> RunResult<usize>
            where
                'db: 'run,
            {
                $quote(endpoint, left, right).await
            }
        })+
    };
}

inference_outputs! {
    ScopeInference<'db> => inference::quote_scope_comparison,
    DefinitionInference<'db> => inference::quote_definition_comparison,
    ExpressionInference<'db> => inference::quote_expression_comparison,
    FunctionDecoratorInference<'db> => inference::quote_function_decorator_comparison,
}

impl<'db> OutputProfile<'db> for Arc<PlaceTable> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        PlaceTable::quote_comparison(endpoint, left, right).await
    }
}

impl<'db> OutputProfile<'db> for Arc<UseDefMap<'db>> {
    async fn comparison_work<'run>(
        endpoint: TaskEndpoint<'run, 'db>,
        left: &Self,
        right: &Self,
    ) -> RunResult<usize>
    where
        'db: 'run,
    {
        UseDefMap::quote_comparison(endpoint, left, right).await
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::Stmt;
    use ruff_python_ast::name::Name;
    use rustc_hash::FxHashSet;
    use salsa::execution_probe::RegistryBuilder;
    use salsa::prepared_source_probe::assert_no_active_attempt;

    use super::{FunctionDecoratorInference, OutputProfile, name_table_slots, name_table_work};
    use crate::analysis::{
        AnalysisFailure, AnalysisIncomplete, AnalysisOutcome, AnalysisPolicy, PreparedAnalysisFile,
        prepare_file, with_analysis_session,
    };
    use crate::db::tests::setup_db;
    use crate::types::class::member_source::InlineMemberSourceEffects;
    use crate::types::class::slots::{InstanceLayout, SynchronousSlotSelectorEffects};
    use crate::types::function::{FunctionDecorators, OverloadLiteral};
    use crate::types::infer::native_values::observations;
    use crate::types::infer::{function_known_decorators, infer_definition_types};
    use crate::types::{ClassLiteral, TypeCheckDiagnostics, todo_type};

    // The recursive alias is retained by `cast` inference; the call and assignment expression
    // also leave state that function-definition inference consumes after decorator inference.
    const SOURCE: &str = "\
from typing import cast
Alias = list[\"Alias\"]
def identity(value):
    return value
@staticmethod
@identity((binding := 1))
@cast(Alias, None)
@missing_with_a_long_diagnostic_message
@suppressed_missing  # ty: ignore[unresolved-reference]
def decorated():
    pass
def undecorated():
    pass
";

    fn funded() -> AnalysisPolicy {
        AnalysisPolicy {
            semantic_work_limit: 1_000_000,
            requested_bytes_limit: 16 * 1024 * 1024,
        }
    }

    fn ordinary<'db>(
        db: &'db dyn crate::Db,
        prepared: &PreparedAnalysisFile<'db>,
        name: &str,
    ) -> &'db FunctionDecoratorInference<'db> {
        let function = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function) if function.name.as_str() == name => Some(function),
                _ => None,
            })
            .unwrap();
        function_known_decorators(
            db,
            prepared.semantic_index().expect_single_definition(function),
        )
    }

    fn copy_inference<'db>(
        inference: &FunctionDecoratorInference<'db>,
    ) -> FunctionDecoratorInference<'db> {
        let mut diagnostics = TypeCheckDiagnostics::default();
        diagnostics.extend(&inference.diagnostics);
        FunctionDecoratorInference {
            expression_types: inference.expression_types.iter().copied().collect(),
            bindings: inference.bindings.clone(),
            called_functions: inference.called_functions.clone(),
            implicit_aliases: inference.implicit_aliases.clone(),
            known_decorators: inference.known_decorators,
            has_unknown_decorators: inference.has_unknown_decorators,
            diagnostics,
        }
    }

    fn compare<'db>(
        prepared: &PreparedAnalysisFile<'db>,
        left: &FunctionDecoratorInference<'db>,
        right: &FunctionDecoratorInference<'db>,
        compared: &Cell<bool>,
        policy: &AnalysisPolicy,
    ) -> Result<AnalysisOutcome<(usize, bool)>, AnalysisFailure> {
        with_analysis_session(prepared, policy, |session| {
            let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
            run.run(|endpoint| async move {
                let work =
                    FunctionDecoratorInference::comparison_work(endpoint.clone(), left, right)
                        .await?;
                let equal = endpoint
                    .local_call(|| {
                        endpoint.admit_work(work)?;
                        endpoint.check_completion()?;
                        compared.set(true);
                        Ok(left == right)
                    })
                    .await;
                Ok((work, equal))
            })
        })
    }

    fn compare_overloads<'db>(
        prepared: &PreparedAnalysisFile<'db>,
        left: &(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>),
        right: &(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>),
        quoted: &Cell<Option<(usize, usize)>>,
        compared: &Cell<bool>,
        policy: &AnalysisPolicy,
    ) -> Result<AnalysisOutcome<(usize, bool)>, AnalysisFailure> {
        with_analysis_session(prepared, policy, |session| {
            let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
            run.run(|endpoint| async move {
                let work =
                    <(Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>)>::comparison_work(
                        endpoint.clone(),
                        left,
                        right,
                    )
                    .await?;
                quoted.set(Some((
                    work,
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db())
                        .unwrap(),
                )));
                let equal = endpoint
                    .local_call(|| {
                        endpoint.admit_work(work)?;
                        endpoint.check_completion()?;
                        compared.set(true);
                        Ok(left == right)
                    })
                    .await;
                Ok((work, equal))
            })
        })
    }

    fn compare_dunder_all(
        prepared: &PreparedAnalysisFile<'_>,
        left: &Option<FxHashSet<Name>>,
        right: &Option<FxHashSet<Name>>,
        quoted: &Cell<Option<(usize, usize)>>,
        compared: &Cell<bool>,
        policy: &AnalysisPolicy,
    ) -> Result<AnalysisOutcome<(usize, bool)>, AnalysisFailure> {
        with_analysis_session(prepared, policy, |session| {
            let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
            run.run(|endpoint| async move {
                let work =
                    Option::<FxHashSet<Name>>::comparison_work(endpoint.clone(), left, right)
                        .await?;
                quoted.set(Some((
                    work,
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db())
                        .unwrap(),
                )));
                let equal = endpoint
                    .local_call(|| {
                        endpoint.admit_work(work)?;
                        endpoint.check_completion()?;
                        compared.set(true);
                        Ok(left == right)
                    })
                    .await;
                Ok((work, equal))
            })
        })
    }

    #[test]
    fn dunder_all_native_comparison_preserves_optional_sets_and_admits_long_names() {
        let mut db = setup_db();
        db.write_file("src/main.py", "__all__ = []\n").unwrap();
        let file = system_path_to_file(&db, "src/main.py").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let revision = salsa::plumbing::current_revision(&db);
        let names = |items: &[&str]| {
            let mut names: FxHashSet<Name> = items.iter().map(Name::new).collect();
            names.shrink_to_fit();
            Some(names)
        };
        let prefix = "x".repeat(2048);
        let long_a = format!("{prefix}a");
        let long_b = format!("{prefix}b");
        for (left, right, expected) in [
            (None, None, true),
            (None, names(&[]), false),
            (names(&[]), None, false),
            (names(&[]), names(&[]), true),
            (names(&[]), names(&["value"]), false),
            (names(&["a", "b"]), names(&["b", "a"]), true),
            (names(&["a", "b"]), names(&["a", "c"]), false),
            (names(&[&long_a]), names(&[&long_a]), true),
            (names(&[&long_a]), names(&[&long_b]), false),
        ] {
            let quoted = Cell::new(None);
            let compared = Cell::new(false);
            let result =
                compare_dunder_all(&prepared, &left, &right, &quoted, &compared, &funded());
            let Ok(AnalysisOutcome::Complete((work, equal))) = result else {
                panic!("{result:?}");
            };
            assert_eq!(equal, expected);
            assert!(compared.get());
            let (quoted_work, remaining) = quoted.get().unwrap();
            assert_eq!(work, quoted_work);
            if left.as_ref().is_some_and(|names| names.contains(&*long_a)) {
                assert!(work >= long_a.len());
            }
            let before_equality = funded().semantic_work_limit - remaining;
            quoted.set(None);
            compared.set(false);
            assert_eq!(
                compare_dunder_all(
                    &prepared,
                    &left,
                    &right,
                    &quoted,
                    &compared,
                    &AnalysisPolicy {
                        semantic_work_limit: before_equality + work - 1,
                        ..funded()
                    }
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
            assert!(quoted.get().is_some());
            assert!(!compared.get());
            assert_no_active_attempt();
            assert_eq!(
                compare_dunder_all(&prepared, &left, &right, &quoted, &compared, &funded()),
                result
            );
            assert!(compared.get());
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }

    #[test]
    fn dunder_all_native_comparison_rejects_overflowing_bounds() {
        assert!(name_table_slots(usize::MAX).is_err());
        assert!(name_table_work(usize::MAX, 0, 0).is_err());
        assert!(name_table_work(1, 1, usize::MAX).is_err());
    }

    #[test]
    fn overload_collection_native_comparison_preserves_order_and_implementation_and_can_refuse() {
        let mut db = setup_db();
        db.write_file("src/main.py", "def first(): ...\ndef second(): ...\n")
            .unwrap();
        let file = system_path_to_file(&db, "src/main.py").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let literals = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .map(|statement| {
                let function = statement.as_function_def_stmt().unwrap();
                let definition = prepared.semantic_index().expect_single_definition(function);
                infer_definition_types(&db, definition)
                    .function_type(definition)
                    .unwrap()
                    .literal(&db)
                    .last_definition
            })
            .collect::<Vec<_>>();
        let first = literals[0];
        let second = literals[1];
        let left = (vec![first, second].into_boxed_slice(), Some(first));
        let revision = salsa::plumbing::current_revision(&db);
        for (overloads, implementation, expected) in [
            (vec![first, second], Some(first), true),
            (vec![second, first], Some(first), false),
            (vec![first], Some(first), false),
            (vec![first, second], None, false),
            (vec![first, second], Some(second), false),
            (vec![], None, false),
        ] {
            let right = (overloads.into_boxed_slice(), implementation);
            let quoted = Cell::new(None);
            let compared = Cell::new(false);
            let result = compare_overloads(&prepared, &left, &right, &quoted, &compared, &funded());
            let Ok(AnalysisOutcome::Complete((work, equal))) = result else {
                panic!("{result:?}");
            };
            assert_eq!(equal, expected);
            assert!(compared.get());
            let (quoted_work, remaining) = quoted.get().unwrap();
            assert_eq!(work, quoted_work);
            // Reach the equality admission, then leave it one unit short. Quotation completes,
            // but native equality must not run until the funded same-revision retry below.
            let before_equality = funded().semantic_work_limit - remaining;
            quoted.set(None);
            compared.set(false);
            assert_eq!(
                compare_overloads(
                    &prepared,
                    &left,
                    &right,
                    &quoted,
                    &compared,
                    &AnalysisPolicy {
                        semantic_work_limit: before_equality + work - 1,
                        ..funded()
                    }
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
            assert!(quoted.get().is_some());
            assert!(!compared.get());
            assert_no_active_attempt();
            assert_eq!(
                compare_overloads(&prepared, &left, &right, &quoted, &compared, &funded()),
                result
            );
            assert!(compared.get());
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }

    fn ordinary_instance_layout<'db>(
        db: &'db dyn crate::Db,
        prepared: &PreparedAnalysisFile<'db>,
        name: &str,
    ) -> &'db InstanceLayout {
        let class = prepared
            .parsed_module()
            .syntax()
            .body
            .iter()
            .filter_map(Stmt::as_class_def_stmt)
            .find(|class| class.name.as_str() == name)
            .unwrap();
        let definition = prepared.semantic_index().expect_single_definition(class);
        let Some(ClassLiteral::Static(class)) =
            infer_definition_types(db, definition).original_class_type(definition)
        else {
            panic!("fixture definition is not a static class");
        };
        match SynchronousSlotSelectorEffects::instance_layout(
            &InlineMemberSourceEffects::new(db),
            class,
        ) {
            Ok(layout) => layout,
            Err(never) => match never {},
        }
    }

    fn compare_instance_layouts(
        prepared: &PreparedAnalysisFile<'_>,
        left: &InstanceLayout,
        right: &InstanceLayout,
        quoted: &Cell<Option<(usize, usize)>>,
        compared: &Cell<bool>,
        policy: &AnalysisPolicy,
    ) -> Result<AnalysisOutcome<(usize, bool)>, AnalysisFailure> {
        with_analysis_session(prepared, policy, |session| {
            let run = RegistryBuilder::with_budget(session.db(), session.budget())?.seal()?;
            run.run(|endpoint| async move {
                let work = InstanceLayout::comparison_work(endpoint.clone(), left, right).await?;
                quoted.set(Some((
                    work,
                    salsa::attempt_probe::remaining_allowance_for_diagnostics(session.db())
                        .unwrap(),
                )));
                let equal = endpoint
                    .local_call(|| {
                        endpoint.admit_work(work)?;
                        endpoint.check_completion()?;
                        compared.set(true);
                        Ok(left == right)
                    })
                    .await;
                Ok((work, equal))
            })
        })
    }

    #[test]
    fn instance_layout_native_comparison_preserves_slots_and_dictionary_and_can_refuse() {
        let prefix = "x".repeat(2048);
        let long_a = format!("{prefix}a");
        let long_b = format!("{prefix}b");
        let many_slots = (0..129)
            .map(|index| format!("'slot_{index}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "
    class Empty:
        __slots__ = ()
    class Dictionary:
        pass
    class Slots:
        __slots__ = ('first', 'second')
    class EqualSlots:
        __slots__ = ('first', 'second')
    class ReversedSlots:
        __slots__ = ('second', 'first')
    class FewerSlots:
        __slots__ = ('first',)
    class ChangedSlots:
        __slots__ = ('first', 'third')
    class SlotsWithDictionary(Dictionary):
        __slots__ = ('first', 'second')
    class LongA:
        __slots__ = ('{long_a}',)
    class EqualLongA:
        __slots__ = ('{long_a}',)
    class LongB:
        __slots__ = ('{long_b}',)
    class ManySlots:
        __slots__ = [{many_slots}, 'last']
    class EqualManySlots:
        __slots__ = [{many_slots}, 'last']
    class ChangedManySlots:
        __slots__ = [{many_slots}, 'different']
    "
        );
        let mut ordinary_db = setup_db();
        ordinary_db.write_dedented("src/main.py", &source).unwrap();
        let ordinary_file = system_path_to_file(&ordinary_db, "src/main.py").unwrap();
        let ordinary_prepared = prepare_file(&ordinary_db, ordinary_file).unwrap();
        let layout = |name| ordinary_instance_layout(&ordinary_db, &ordinary_prepared, name);
        let empty = layout("Empty");
        let dictionary = layout("Dictionary");
        let slots = layout("Slots");
        let slots_with_dictionary = layout("SlotsWithDictionary");
        let long = layout("LongA");
        let many = layout("ManySlots");
        // Both pairs have identical slot names. Their unequal layouts therefore exercise the
        // instance dictionary inherited from `Dictionary`, or provided by `Dictionary` itself.
        assert_eq!(empty.slot_names(), dictionary.slot_names());
        assert_eq!(slots.slot_names(), slots_with_dictionary.slot_names());
        assert_eq!(
            slots.slot_names(),
            [Name::new("first"), Name::new("second")]
        );
        assert_eq!(long.slot_names(), [Name::new(&long_a)]);
        assert_eq!(many.slot_names().len(), 130);

        let mut db = setup_db();
        db.write_file("src/main.py", "value = 1\n").unwrap();
        let file = system_path_to_file(&db, "src/main.py").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let revision = salsa::plumbing::current_revision(&db);
        let policy = funded();
        for (left, right, expected) in [
            (empty, empty, true),
            (empty, dictionary, false),
            (dictionary, empty, false),
            (empty, slots, false),
            (slots, layout("EqualSlots"), true),
            (slots, layout("ReversedSlots"), false),
            (slots, layout("FewerSlots"), false),
            (slots, layout("ChangedSlots"), false),
            (slots, slots_with_dictionary, false),
            (slots_with_dictionary, slots, false),
            (long, layout("EqualLongA"), true),
            (long, layout("LongB"), false),
            (many, layout("EqualManySlots"), true),
            (many, layout("ChangedManySlots"), false),
        ] {
            let quoted = Cell::new(None);
            let compared = Cell::new(false);
            let result =
                compare_instance_layouts(&prepared, left, right, &quoted, &compared, &policy);
            let Ok(AnalysisOutcome::Complete((work, equal))) = result else {
                panic!("{result:?}");
            };
            assert_eq!(equal, expected);
            assert!(compared.get());
            let (quoted_work, remaining) = quoted.get().unwrap();
            assert_eq!(work, quoted_work);
            let name_bytes = left
                .slot_names()
                .iter()
                .chain(right.slot_names())
                .map(|name| name.len())
                .sum::<usize>();
            assert!(work >= name_bytes);

            // Complete quotation, then leave the equality admission one unit short. The same
            // funded policy must still complete a retry after the refused attempt is cleaned up.
            let before_equality = policy.semantic_work_limit - remaining;
            quoted.set(None);
            compared.set(false);
            assert_eq!(
                compare_instance_layouts(
                    &prepared,
                    left,
                    right,
                    &quoted,
                    &compared,
                    &AnalysisPolicy {
                        semantic_work_limit: before_equality + work - 1,
                        ..policy
                    }
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: ()
                })
            );
            assert!(quoted.get().is_some());
            assert!(!compared.get());
            assert_no_active_attempt();
            assert_eq!(
                compare_instance_layouts(&prepared, left, right, &quoted, &compared, &policy),
                result
            );
            assert!(compared.get());
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }

    #[derive(Clone, Copy, Debug)]
    enum Payload {
        Expressions,
        Bindings,
        Calls,
        Aliases,
        Diagnostics,
        Suppressions,
        Flags,
        Unknown,
    }

    #[test]
    fn function_decorator_native_comparison_covers_complete_ordinary_payload() {
        let mut db = setup_db();
        db.write_file("src/main.py", SOURCE).unwrap();
        let file = system_path_to_file(&db, "src/main.py").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let full = ordinary(&db, &prepared, "decorated");
        let empty = ordinary(&db, &prepared, "undecorated");
        assert!(full.expression_types.iter().len() > 0);
        assert!(!full.bindings.is_empty());
        assert!(!full.called_functions.is_empty());
        assert!(!full.implicit_aliases.is_empty());
        assert!((&full.diagnostics).into_iter().len() > 0);
        assert!(full.diagnostics.used_len() > 0);
        assert!(
            full.known_decorators
                .contains(FunctionDecorators::STATICMETHOD)
        );
        assert!(full.has_unknown_decorators);

        let compared = Cell::new(false);
        let result = compare(&prepared, full, empty, &compared, &funded());
        let Ok(AnalysisOutcome::Complete((work, false))) = result else {
            panic!("{result:?}");
        };
        assert!(compared.get());
        assert_eq!(
            compare(&prepared, empty, full, &compared, &funded()),
            result
        );
        assert!(matches!(
            compare(&prepared, full, full, &compared, &funded()),
            Ok(AnalysisOutcome::Complete((_, true)))
        ));

        for payload in [
            Payload::Expressions,
            Payload::Bindings,
            Payload::Calls,
            Payload::Aliases,
            Payload::Diagnostics,
            Payload::Suppressions,
            Payload::Flags,
            Payload::Unknown,
        ] {
            let mut reduced = copy_inference(full);
            match payload {
                Payload::Expressions => reduced.expression_types = Default::default(),
                Payload::Bindings => reduced.bindings = Box::default(),
                Payload::Calls => reduced.called_functions = Box::default(),
                Payload::Aliases => reduced.implicit_aliases = Box::default(),
                Payload::Diagnostics => {
                    reduced.diagnostics = TypeCheckDiagnostics::default();
                    reduced
                        .diagnostics
                        .extend_filtered(&full.diagnostics, |_| false);
                }
                Payload::Suppressions => {
                    reduced.diagnostics = TypeCheckDiagnostics::default();
                    reduced
                        .diagnostics
                        .extend_diagnostics((&full.diagnostics).into_iter().cloned());
                }
                Payload::Flags => reduced.known_decorators = FunctionDecorators::empty(),
                Payload::Unknown => reduced.has_unknown_decorators = false,
            }
            assert_ne!(&reduced, full, "{payload:?}");
            let result = compare(&prepared, &reduced, empty, &compared, &funded());
            let Ok(AnalysisOutcome::Complete((reduced_work, _))) = result else {
                panic!("{payload:?}: {result:?}");
            };
            if matches!(payload, Payload::Flags | Payload::Unknown) {
                assert_eq!(reduced_work, work, "{payload:?}");
            } else {
                assert!(reduced_work < work, "{payload:?}");
            }
        }

        let mut larger_expression_types = copy_inference(full);
        for (_, ty) in larger_expression_types.expression_types.iter_mut() {
            *ty = todo_type!("a decorator expression with retained inline type bytes");
        }
        let mut larger_binding_types = copy_inference(full);
        for (_, ty) in larger_binding_types.bindings.iter_mut() {
            *ty = todo_type!("a decorator binding with retained inline type bytes");
        }
        for larger_types in [larger_expression_types, larger_binding_types] {
            let result = compare(&prepared, &larger_types, empty, &compared, &funded());
            let Ok(AnalysisOutcome::Complete((larger_work, false))) = result else {
                panic!("{result:?}");
            };
            assert!(larger_work > work);
        }
        assert_no_active_attempt();
    }

    #[test]
    fn function_decorator_native_comparison_interrupts_scans_and_equality() {
        let mut db = setup_db();
        db.write_file("src/main.py", SOURCE).unwrap();
        let file = system_path_to_file(&db, "src/main.py").unwrap();
        let prepared = prepare_file(&db, file).unwrap();
        let full = ordinary(&db, &prepared, "decorated");
        let empty = ordinary(&db, &prepared, "undecorated");
        let revision = salsa::plumbing::current_revision(&db);
        let address = std::ptr::from_ref(full).addr();
        let compared = Cell::new(false);
        observations::reset(address);
        let result = compare(&prepared, full, empty, &compared, &funded());
        let Ok(AnalysisOutcome::Complete((work, false))) = result else {
            panic!("{result:?}");
        };
        let observation = observations::snapshot().unwrap();
        let before_scan = funded().semantic_work_limit - observation.started_remaining.unwrap();
        let after_scan = funded().semantic_work_limit - observation.finished_remaining.unwrap();
        assert!(after_scan > before_scan + 1);

        for limit in [before_scan, after_scan - 1, after_scan + work - 1] {
            observations::reset(address);
            compared.set(false);
            assert_eq!(
                compare(
                    &prepared,
                    full,
                    empty,
                    &compared,
                    &AnalysisPolicy {
                        semantic_work_limit: limit,
                        ..funded()
                    },
                ),
                Ok(AnalysisOutcome::Incomplete {
                    reason: AnalysisIncomplete::WorkLimit,
                    completed: (),
                }),
            );
            assert!(!compared.get());
            assert_eq!(
                observations::snapshot().unwrap().work.is_some(),
                limit >= after_scan
            );
            assert_no_active_attempt();
            assert_eq!(
                compare(&prepared, full, empty, &compared, &funded()),
                result
            );
            assert!(compared.get());
            assert!(std::ptr::eq(full, ordinary(&db, &prepared, "decorated")));
            assert_eq!(salsa::plumbing::current_revision(&db), revision);
            assert_no_active_attempt();
        }
    }
}
