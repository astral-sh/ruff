//! Source dependencies for the shared nominal equality and comparison-method classifiers.

use crate::types::SubclassOfInner;
use crate::types::infer::builder::source_definition::controlled::SourceOperation;
use crate::types::subclass_of::{
    SubclassConstructionEffects, SubclassConstructionFacts, subclass_from_with,
};

use crate::types::ClassBase;
use crate::types::class::StaticClassLiteral;
use crate::types::generics::Specialization;
use crate::types::instance::tuple_spec::{
    TupleSpecEffects, TupleSpecFacts, nominal_tuple_spec_with, version_info_spec_with,
};
use crate::types::instance::{NominalClassEffects, NominalInstanceClass};
use crate::types::mro::MroIterator;
use crate::types::tuple::TupleType;
use ruff_python_ast::PythonVersion;
use salsa::execution_probe::BorrowOrCopy;
use std::borrow::Cow;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::super::class_selection::FixedFieldCopy;
use super::{EqualitySourceEffects, SourceAccess, SourceEffects, equality_type_key_work};
use crate::ProgramEnvironment;
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::class::KnownClassInstanceEffects;
use crate::types::class::static_literal::static_finality_with;
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::enums::EnumMetadata;
use crate::types::equality::nominal_source::{
    NominalTuplePairs, next_builtin_semantics, tuple_pairs,
};
use crate::types::equality::source::{self, EqualityFacts, EqualityOperation};
use crate::types::equality::{
    ComparisonBranch, ComparisonEvaluator, ComparisonOperator, ComparisonResult,
    ComparisonSoundnessPolicy, KnownComparisonSemantics,
};
use crate::types::function::{FunctionLiteral, FunctionType};
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::local_transfer::generated_field_quote;
use crate::types::member_lookup::general::{
    GeneralMemberBranch, GeneralMemberEffects, GeneralMemberFacts, GeneralMemberName,
    GeneralMemberOperation, GeneralMemberPredicate, member_lookup_entry_with,
};
use crate::types::promotion::classification::{SingletonFacts, classify_singleton_with};
use crate::types::relation::source::{disjointness_condition, equivalence_condition};
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::{CallableType, MemberEntryEffects, MemberLookupKey, MemberLookupResult};
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, InternedType, KnownBoundMethodType, KnownClass,
    MemberLookupPolicy, ModuleLiteralType, NominalInstanceType, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Selects a class literal without reading a generic alias's specialization.
    pub(in crate::types::infer::builder::source_definition::controlled) async fn equality_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<ClassLiteral<'db>> {
        let class = self.local_with_fixed_transfers(2, 0, || class).await?;
        match class {
            ClassType::NonGeneric(literal) => Ok(literal),
            ClassType::Generic(alias) => {
                let quote = generated_field_quote(
                    |alias: GenericAlias<'db>, context| alias.field_requests(context),
                    |alias: GenericAlias<'db>, context| alias.field_requests(context).origin(),
                );
                let endpoint = self.access.endpoint();
                let read = self
                    .boxed_future_with_fixed_transfers(quote, || {
                        let request = alias
                            .field_requests(endpoint.field_request_context())
                            .origin();
                        endpoint.read_field(request, &FixedFieldCopy)
                    })
                    .await?;
                let origin = read.await;
                self.local_with_fixed_transfers(2, 0, || ClassLiteral::Static(origin))
                    .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> EqualitySourceEffects<'_, '_, 'run, 'db, A> {
    /// Compares stored types after admitting any debug TODO message traversal.
    pub(super) async fn types_equal(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        let work = self
            .source
            .local_with_fixed_transfers(
                32,
                16 * size_of::<usize>() + 16 * size_of::<Option<usize>>(),
                || {
                    let left = equality_type_key_work(left)?;
                    let right = equality_type_key_work(right)?;
                    Some(left.max(right))
                },
            )
            .await?
            .ok_or(RunError::Contract("type equality quotation overflow"))?;
        self.source
            .local_with_fixed_transfers(work, 2 * size_of::<Type<'db>>(), || left == right)
            .await
    }

    /// Compares complete member results, retaining qualifiers and all place metadata.
    pub(super) async fn members_equal(
        &self,
        left: PlaceAndQualifiers<'db>,
        right: PlaceAndQualifiers<'db>,
    ) -> RunResult<bool> {
        let work = self
            .source
            .local_with_fixed_transfers(
                40,
                20 * size_of::<usize>() + 20 * size_of::<Option<usize>>(),
                || {
                    let left = match left.place {
                        Place::Defined(place) => equality_type_key_work(place.ty)?,
                        Place::Undefined => 0,
                    };
                    let right = match right.place {
                        Place::Defined(place) => equality_type_key_work(place.ty)?,
                        Place::Undefined => 0,
                    };
                    left.max(right).checked_add(10)
                },
            )
            .await?
            .ok_or(RunError::Contract("member equality quotation overflow"))?;
        self.source
            .local_with_fixed_transfers(work, 2 * size_of::<PlaceAndQualifiers<'db>>(), || {
                left == right
            })
            .await
    }

    /// Funds the fixed decisions and copied carriers for one shared equality-body invocation.
    ///
    /// Each finite-alternative, structural, nominal-comparison, known-semantics,
    /// instance-semantics, member-implementation, identity-semantics or singleton body
    /// calls this once on entry; nested bodies each fund their own invocation.
    pub(super) async fn nominal_checkpoint(&self) -> RunResult<()> {
        // structural_other_with has the largest arm inventory: 5 + 2 + 7 + 2 arms
        // across its four matches, including the nested TypedDict semantics match.
        // The per-invocation bound adds twenty-four tag/guard decisions, sixteen fixed
        // carrier initializations and eight return/scrutinee steps to those sixteen probes.
        // Cursor steps and semantic children have their own admissions.
        let bytes = size_of::<Type<'db>>() * 8
            + size_of::<PlaceAndQualifiers<'db>>() * 4
            + size_of::<ClassType<'db>>() * 4
            + size_of::<ComparisonOperator>() * 4
            + size_of::<ComparisonResult<'db>>() * 4;
        self.source
            .local_with_fixed_transfers(64, bytes, || ())
            .await
    }

    /// Classifies the inherited comparison method with the caller's soundness policy.
    pub(super) async fn known_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
        policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        self.source
            .type_parameter_future(|| {
                source::known_semantics_with(env, ty, operator, policy, EqualityFacts, self)
            })
            .await?
            .await
    }

    /// Classifies an instance through the shared ordered builtin-method lookups.
    pub(super) async fn instance_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        self.source
            .type_parameter_future(|| {
                source::instance_semantics_with(env, ty, operator, EqualityFacts, self)
            })
            .await?
            .await
    }

    /// Reports unsupported enum, intersection, and fallback comparison classification.
    pub(super) async fn known_semantics_specialized(
        &self,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
        _operator: ComparisonOperator,
        _policy: ComparisonSoundnessPolicy,
    ) -> RunResult<Option<KnownComparisonSemantics>> {
        self.unavailable(EqualityOperation::ComparisonSemantics)
            .await
    }

    /// Checks class finality after selecting only the nominal class and its origin.
    pub(super) async fn nominal_is_final(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        let class = self
            .source
            .type_parameter_future(|| self.source.equality_nominal_class(env, instance))
            .await?
            .await?;
        self.source
            .type_parameter_future(|| self.source.equality_class_finality(class))
            .await?
            .await
    }

    /// Compares the stored nominal known-class tag with the requested builtin.
    pub(super) async fn nominal_has_known_class(
        &self,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> RunResult<bool> {
        let known = self
            .source
            .type_parameter_future(|| {
                nominal_known_class_with(instance, NominalClassFacts, self.source)
            })
            .await?
            .await?;
        self.source
            .local_with_fixed_transfers(2, 0, || known == Some(class))
            .await
    }

    /// Checks nominal-class availability using the caller's environment.
    pub(super) async fn nominal_class_available(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let class = match ty {
            Type::NominalInstance(instance) => Some(
                self.source
                    .type_parameter_future(|| self.source.equality_nominal_class(env, instance))
                    .await?
                    .await?,
            ),
            _ => {
                self.source
                    .type_parameter_future(|| {
                        NominalSelectionEffects::nominal_class(self.source, ty)
                    })
                    .await?
                    .await?
            }
        };
        self.source
            .local_with_fixed_transfers(2, 0, || class.is_some())
            .await
    }

    /// Obtains the class object used for comparison-method lookup.
    pub(super) async fn equality_meta_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let ty = self.source.local_with_fixed_transfers(2, 0, || ty).await?;
        match ty {
            Type::NominalInstance(instance) => {
                self.source
                    .type_parameter_future(|| self.source.equality_nominal_meta_type(env, instance))
                    .await?
                    .await
            }
            _ => {
                self.source
                    .type_parameter_future(|| MemberEntryEffects::meta_type(self.source, ty))
                    .await?
                    .await
            }
        }
    }

    /// Looks up a named comparison member and returns its place and qualifiers.
    /// Applies ordinary member-entry rules and the no-object-fallback comparison policy.
    pub(super) async fn equality_dunder(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        name: &'static str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let effects = self
            .source
            .local_with_fixed_transfers(3, 0, || EqualityMemberEntryEffects {
                source: self.source,
                env,
            })
            .await?;
        let result = self
            .source
            .type_parameter_future(|| {
                member_lookup_entry_with(
                    ty,
                    GeneralMemberName::Text(name),
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                    None,
                    GeneralMemberFacts,
                    &effects,
                )
            })
            .await?
            .await?;
        let parts = self
            .source
            .type_parameter_future(|| self.source.member_lookup_parts(result))
            .await?
            .await?;
        self.source
            .local_with_fixed_transfers(2, 0, || parts.member)
            .await
    }

    /// Resolves a builtin class literal in the caller's program.
    pub(super) async fn equality_known_class(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| self.source.environment_program(env))
            .await?
            .await?;
        self.source
            .type_parameter_future(|| KnownClassInstanceEffects::class_literal(self.source, class))
            .await?
            .await
    }

    /// Compares qualifiers and function identities through the shared member algorithm.
    pub(super) async fn same_member_implementation(
        &self,
        left: PlaceAndQualifiers<'db>,
        right: PlaceAndQualifiers<'db>,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| {
                source::same_member_implementation_with(left, right, EqualityFacts, self)
            })
            .await?
            .await
    }

    /// Reads the function literal that identifies a looked-up implementation.
    pub(super) async fn function_identity(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<FunctionLiteral<'db>> {
        let quote = generated_field_quote(
            |function: FunctionType<'db>, context| function.field_requests(context),
            |function: FunctionType<'db>, context| function.field_requests(context).literal(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = function
                    .field_requests(endpoint.field_request_context())
                    .literal();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        Ok(read.await)
    }

    /// Advances the shared int, str, bytes, tuple, dict implementation sequence.
    pub(super) async fn next_builtin_semantics(
        &self,
        index: &mut usize,
    ) -> RunResult<Option<(KnownClass, KnownComparisonSemantics)>> {
        self.source
            .local_with_fixed_transfers(8, 0, || next_builtin_semantics(index))
            .await
    }

    /// Checks whether the type is a singleton whose comparison uses object identity.
    pub(super) async fn identity_semantics(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| {
                source::identity_semantics_with(env, ty, operator, EqualityFacts, self)
            })
            .await?
            .await
    }

    /// Obtains the instances of a class's lookup metaclass.
    pub(super) async fn metaclass_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| {
                self.source
                    .class_metaclass_instance_value(env, ClassType::NonGeneric(class))
            })
            .await?
            .await
    }

    /// Returns whether the type is known to have a single inhabitant.
    /// Uses the complete shared singleton dispatch and preserves unsupported children.
    pub(super) async fn singleton_type(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| source::singleton_type_with(env, ty, EqualityFacts, self))
            .await?
            .await
    }

    /// Returns whether the nominal instance type is known to have a single inhabitant.
    /// Uses the existing nominal singleton classifier, including its enum dependency.
    pub(super) async fn singleton_nominal(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| {
                classify_singleton_with(instance, SingletonFacts, self.source)
            })
            .await?
            .await
    }

    /// Reports unsupported recursive, constrained, and other semantic singleton children.
    pub(super) async fn singleton_specialized(
        &self,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(EqualityOperation::Singleton).await
    }

    /// Reports unsupported complement, intersection, and NewType finite expansion.
    pub(super) async fn finite_specialized(
        &self,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
        _operator: ComparisonOperator,
    ) -> RunResult<Option<Vec<Type<'db>>>> {
        self.unavailable(EqualityOperation::FiniteAlternatives)
            .await
    }

    /// Allocates the ordinary true-then-false finite alternatives with prepaid disposal.
    pub(super) async fn boolean_alternatives(&self) -> RunResult<Vec<Type<'db>>> {
        // Two Copy elements, one allocation, two writes and the retained buffer's disposal.
        self.source
            .local_with_fixed_transfers(9, 2 * size_of::<Type<'db>>(), || {
                vec![Type::bool_literal(true), Type::bool_literal(false)]
            })
            .await
    }

    /// Checks canonical enum identity, returning absence for non-enums and refusing member expansion for recognized enums.
    pub(super) async fn enum_alternatives(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Vec<Type<'db>>>> {
        let class = self
            .source
            .type_parameter_future(|| self.source.equality_nominal_class(env, instance))
            .await?
            .await?;
        let class = self
            .source
            .type_parameter_future(|| self.source.equality_class_literal(class))
            .await?
            .await?;
        let enum_class = self
            .source
            .type_parameter_future(|| self.source.enum_class_literal_source(class))
            .await?
            .await?;
        match enum_class {
            None => self.source.local_with_fixed_transfers(2, 0, || None).await,
            Some(_) => {
                self.unavailable(EqualityOperation::FiniteAlternatives)
                    .await
            }
        }
    }

    /// Reports structural descendants that are not connected to source equality.
    pub(super) async fn structural_specialized(
        &self,
        _evaluator: &mut ComparisonEvaluator<'db>,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
        _branch: ComparisonBranch,
        _operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(EqualityOperation::StructuralComparison)
            .await
    }

    /// Reports the unsupported intersection proof needed to exclude a string literal.
    pub(super) async fn excluded_string_literal(
        &self,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(EqualityOperation::StructuralComparison)
            .await
    }

    /// Compares the underlying imported module identities in left-to-right read order.
    pub(super) async fn same_module(&self, left: Type<'db>, right: Type<'db>) -> RunResult<bool> {
        let pair = self
            .source
            .local_with_fixed_transfers(3, 0, || match (left, right) {
                (Type::ModuleLiteral(left), Type::ModuleLiteral(right)) => Some((left, right)),
                _ => None,
            })
            .await?;
        let Some((left, right)) = pair else {
            return Err(RunError::Contract(
                "module equality requires module literals",
            ));
        };
        let left = self
            .source
            .type_parameter_future(|| self.module_identity(left))
            .await?
            .await?;
        let right = self
            .source
            .type_parameter_future(|| self.module_identity(right))
            .await?
            .await?;
        self.source
            .local_with_fixed_transfers(2, 0, || left == right)
            .await
    }

    /// Reads the imported module identity, without consulting the importing file.
    async fn module_identity(
        &self,
        module: ModuleLiteralType<'db>,
    ) -> RunResult<ty_module_resolver::Module<'db>> {
        let quote = generated_field_quote(
            |module: ModuleLiteralType<'db>, context| module.field_requests(context),
            |module: ModuleLiteralType<'db>, context| module.field_requests(context).module(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = module
                    .field_requests(endpoint.field_request_context())
                    .module();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        Ok(read.await)
    }

    /// Proves identity for matching FunctionTypeDunderGet or DunderCall wrappers of a FunctionLiteral.
    ///
    /// Reads the left wrapped type before comparing the wrappers' interned identities.
    /// A false result means this proof does not apply; it does not prove inequality.
    pub(super) async fn bound_method_identity(
        &self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        let pair = self
            .source
            .local_with_fixed_transfers(4, 0, || match (left, right) {
                (
                    Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(left)),
                    Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(right)),
                )
                | (
                    Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(left)),
                    Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(right)),
                ) => Some((left, right)),
                _ => None,
            })
            .await?;
        let Some((left, right)) = pair else {
            return Ok(false);
        };
        let quote = generated_field_quote(
            |ty: InternedType<'db>, context| ty.field_requests(context),
            |ty: InternedType<'db>, context| ty.field_requests(context).inner(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = left
                    .field_requests(endpoint.field_request_context())
                    .inner();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let inner = read.await;
        self.source
            .local_with_fixed_transfers(3, 0, || inner.is_function_literal() && left == right)
            .await
    }

    /// Uses the existing controlled equivalence relation with its retained inference owners.
    pub(super) async fn equality_equivalent(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| {
                equivalence_condition(self.source.db(), env, left, right, self.source)
            })
            .await?
            .await
    }

    /// Uses ordinary disjointness criteria through the existing controlled relation.
    pub(super) async fn equality_disjoint(
        &self,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| {
                disjointness_condition(self.source.db(), env, left, right, self.source)
            })
            .await?
            .await
    }

    /// Reports the unsupported ancestry walk for differing comparison implementations.
    pub(super) async fn different_semantics(
        &self,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
        _operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.unavailable(EqualityOperation::ComparisonSemantics)
            .await
    }

    /// Runs the shared nominal comparison while retaining the existing evaluator.
    pub(super) async fn nominal_comparison(
        &self,
        evaluator: &mut ComparisonEvaluator<'db>,
        left: NominalInstanceType<'db>,
        right: NominalInstanceType<'db>,
        operator: ComparisonOperator,
    ) -> RunResult<ComparisonResult<'db>> {
        self.source
            .type_parameter_future(|| {
                source::nominal_comparison_with(
                    evaluator,
                    left,
                    right,
                    operator,
                    EqualityFacts,
                    self,
                )
            })
            .await?
            .await
    }

    /// Reads tuple storage for the original nominal instance and environment.
    pub(super) async fn equality_tuple_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        self.source
            .type_parameter_future(|| self.source.equality_nominal_tuple_spec(env, instance))
            .await?
            .await
    }

    /// Borrows fixed tuple elements in their ordinary left-to-right pairing order.
    pub(super) async fn equality_tuple_pairs<'tuple>(
        &self,
        left: &'tuple FixedLengthTuple<Type<'db>>,
        right: &'tuple FixedLengthTuple<Type<'db>>,
    ) -> RunResult<NominalTuplePairs<'tuple, 'db>> {
        self.source
            .local_with_fixed_transfers(8, 0, || tuple_pairs(left, right))
            .await
    }

    /// Advances the retained tuple cursor and copies one pair after admission.
    pub(super) async fn next_equality_tuple_pair(
        &self,
        pairs: &mut NominalTuplePairs<'_, 'db>,
    ) -> RunResult<Option<(Type<'db>, Type<'db>)>> {
        self.source
            .local_with_fixed_transfers(6, size_of::<Type<'db>>() * 2, || pairs.next())
            .await
    }
}

/// Reads equality operands' tuple specifications using the caller's environment.
struct EqualityTupleSpecEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Returns the nominal tuple specification using the caller's environment.
    pub(in crate::types::infer::builder::source_definition::controlled) async fn equality_nominal_tuple_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        let effects = self
            .local_with_fixed_transfers(3, 0, || EqualityTupleSpecEffects { source: self })
            .await?;
        self.type_parameter_future(|| {
            nominal_tuple_spec_with(instance, env, TupleSpecFacts, &effects)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleSpecEffects<'db>
    for EqualityTupleSpecEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // The nominal dispatch inspects one instance tag and a class tag, tests optional
        // results, and constructs its borrowed or owned result. MRO progress is a child.
        let bytes = size_of::<NominalInstanceType<'db>>() * 2
            + size_of::<ClassType<'db>>() * 2
            + size_of::<Option<KnownClass>>() * 2
            + size_of::<Option<Cow<'db, TupleSpec<'db>>>>() * 2;
        self.source
            .local_with_fixed_transfers(16, bytes, || ())
            .await
    }

    async fn nominal_spec(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Option<Cow<'db, TupleSpec<'db>>>> {
        self.source
            .type_parameter_future(|| nominal_tuple_spec_with(instance, env, TupleSpecFacts, self))
            .await?
            .await
    }

    async fn exact_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        // The tuple field returns a borrowed specification; reading it does not copy the
        // specification or its element storage.
        let quote = generated_field_quote(
            |tuple: TupleType<'db>, context| tuple.field_requests(context),
            |tuple: TupleType<'db>, context| tuple.field_requests(context).tuple(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = tuple
                    .field_requests(endpoint.field_request_context())
                    .tuple();
                endpoint.read_field(request, &BorrowOrCopy)
            })
            .await?;
        Ok(read.await)
    }

    async fn version_info(&self, env: &ProgramEnvironment<'db>) -> RunResult<TupleSpec<'db>> {
        // The shared version-info body constructs two integer literals and a five-element
        // array. Its string, union and tuple allocations remain separately admitted children.
        let bytes =
            size_of::<[Type<'db>; 5]>() * 2 + size_of::<PythonVersion>() * 2 + size_of::<i64>() * 2;
        self.source
            .local_with_fixed_transfers(16, bytes, || ())
            .await?;
        self.source
            .type_parameter_future(|| version_info_spec_with(env, TupleSpecFacts, self))
            .await?
            .await
    }

    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        self.source
            .type_parameter_future(|| NominalClassEffects::non_tuple_class(self.source, class))
            .await?
            .await
    }

    async fn class_known(&self, class: ClassType<'db>) -> RunResult<Option<KnownClass>> {
        let class = self
            .source
            .type_parameter_future(|| self.source.equality_class_literal(class))
            .await?
            .await?;
        match class {
            ClassLiteral::Static(class) => {
                let quote = generated_field_quote(
                    |class: StaticClassLiteral<'db>, context| class.field_requests(context),
                    |class: StaticClassLiteral<'db>, context| class.field_requests(context).known(),
                );
                let endpoint = self.source.access.endpoint();
                let read = self
                    .source
                    .boxed_future_with_fixed_transfers(quote, || {
                        let request = class
                            .field_requests(endpoint.field_request_context())
                            .known();
                        endpoint.read_field(request, &FixedFieldCopy)
                    })
                    .await?;
                Ok(read.await)
            }
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => {
                self.source.local_with_fixed_transfers(2, 0, || None).await
            }
        }
    }

    async fn mro(&self, class: ClassType<'db>) -> RunResult<MroIterator<'db>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::mro(self.source, class))
            .await?
            .await
    }

    async fn next_mro(&self, mro: &mut MroIterator<'db>) -> RunResult<Option<ClassBase<'db>>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::next_mro(self.source, mro))
            .await?
            .await
    }

    async fn retire_mro(&self, mro: MroIterator<'db>) -> RunResult<()> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::retire_mro(self.source, mro))
            .await?
            .await
    }

    async fn specialization_tuple(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Option<&'db TupleSpec<'db>>> {
        let quote = generated_field_quote(
            |alias: GenericAlias<'db>, context| alias.field_requests(context),
            |alias: GenericAlias<'db>, context| alias.field_requests(context).specialization(),
        );
        let endpoint = self.source.access.endpoint();
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = alias
                    .field_requests(endpoint.field_request_context())
                    .specialization();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let specialization = read.await;
        let quote = generated_field_quote(
            |specialization: Specialization<'db>, context| specialization.field_requests(context),
            |specialization: Specialization<'db>, context| specialization.tuple_request(context),
        );
        let read = self
            .source
            .boxed_future_with_fixed_transfers(quote, || {
                let request = specialization.tuple_request(endpoint.field_request_context());
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        match read.await {
            Some(tuple) => {
                let tuple = self
                    .source
                    .type_parameter_future(|| self.exact_spec(tuple))
                    .await?
                    .await?;
                self.source
                    .local_with_fixed_transfers(2, 0, || Some(tuple))
                    .await
            }
            None => self.source.local_with_fixed_transfers(2, 0, || None).await,
        }
    }

    async fn unknown_tuple(&self) -> RunResult<TupleSpec<'db>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::unknown_tuple(self.source))
            .await?
            .await
    }

    async fn python_version(&self, env: &ProgramEnvironment<'db>) -> RunResult<PythonVersion> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::python_version(self.source, env))
            .await?
            .await
    }

    async fn known_instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: KnownClass,
    ) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::known_instance(self.source, env, class))
            .await?
            .await
    }

    async fn string_literal(&self, value: &str) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::string_literal(self.source, value))
            .await?
            .await
    }

    async fn release_elements(&self) -> RunResult<Vec<Type<'db>>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::release_elements(self.source))
            .await?
            .await
    }

    async fn append_release_element(
        &self,
        elements: &mut Vec<Type<'db>>,
        element: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .type_parameter_future(|| {
                TupleSpecEffects::append_release_element(self.source, elements, element)
            })
            .await?
            .await
    }

    async fn release_union(&self, elements: Vec<Type<'db>>) -> RunResult<Type<'db>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::release_union(self.source, elements))
            .await?
            .await
    }

    async fn fixed_tuple(&self, elements: [Type<'db>; 5]) -> RunResult<TupleSpec<'db>> {
        self.source
            .type_parameter_future(|| TupleSpecEffects::fixed_tuple(self.source, elements))
            .await?
            .await
    }
}

/// Normalizes a nominal comparison operand's class using its original environment.
struct EqualitySubclassConstructionEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    env: &'effects ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Checks class finality using only the literal selected by the ordinary class operation.
    pub(in crate::types::infer::builder::source_definition::controlled) async fn equality_class_finality(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<bool> {
        let literal = self
            .type_parameter_future(|| self.equality_class_literal(class))
            .await?
            .await?;
        match literal {
            ClassLiteral::Static(class) => {
                self.type_parameter_future(|| static_finality_with(class, self))
                    .await?
                    .await
            }
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_) => {
                self.local_with_fixed_transfers(2, 0, || false).await
            }
            ClassLiteral::DynamicEnum(_) => {
                self.unavailable(SourceOperation::Equality(
                    EqualityOperation::ComparisonSemantics,
                ))
                .await
            }
        }
    }

    /// Constructs the meta-type of a nominal instance with ordinary finality and object normalization.
    pub(in crate::types::infer::builder::source_definition::controlled) async fn equality_nominal_meta_type(
        &self,
        env: &ProgramEnvironment<'db>,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<Type<'db>> {
        let class = self
            .type_parameter_future(|| self.equality_nominal_class(env, instance))
            .await?
            .await?;
        let effects = self
            .local_with_fixed_transfers(4, 0, || EqualitySubclassConstructionEffects {
                source: self,
                env,
            })
            .await?;
        let class = self
            .local_with_fixed_transfers(2, 0, || SubclassOfInner::Class(class))
            .await?;
        self.type_parameter_future(|| {
            subclass_from_with(class, SubclassConstructionFacts, &effects)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassConstructionEffects<'db>
    for EqualitySubclassConstructionEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // The shared constructor checks the class branch, finality and object identity,
        // then creates one inline Type result. Its semantic children fund their own reads.
        let bytes = size_of::<SubclassOfInner<'db>>() * 2
            + size_of::<ClassType<'db>>() * 2
            + size_of::<Type<'db>>() * 2;
        self.source
            .local_with_fixed_transfers(10, bytes, || ())
            .await
    }

    async fn is_final(&self, class: ClassType<'db>) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| self.source.equality_class_finality(class))
            .await?
            .await
    }

    async fn is_object(&self, class: ClassType<'db>) -> RunResult<bool> {
        let literal = self
            .source
            .type_parameter_future(|| self.source.equality_class_literal(class))
            .await?
            .await?;
        match literal {
            ClassLiteral::Static(class) => {
                let quote = generated_field_quote(
                    |class: StaticClassLiteral<'db>, context| class.field_requests(context),
                    |class: StaticClassLiteral<'db>, context| class.field_requests(context).known(),
                );
                let endpoint = self.source.access.endpoint();
                let read = self
                    .source
                    .boxed_future_with_fixed_transfers(quote, || {
                        let request = class
                            .field_requests(endpoint.field_request_context())
                            .known();
                        endpoint.read_field(request, &FixedFieldCopy)
                    })
                    .await?;
                let known = read.await;
                self.source
                    .local_with_fixed_transfers(2, 0, || known == Some(KnownClass::Object))
                    .await
            }
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => {
                self.source.local_with_fixed_transfers(2, 0, || false).await
            }
        }
    }

    async fn subclass_of_object(&self) -> RunResult<Type<'db>> {
        let program = self
            .source
            .type_parameter_future(|| self.source.environment_program(self.env))
            .await?
            .await?;
        self.source
            .type_parameter_future(|| {
                self.source
                    .access
                    .known_class_instance(program, KnownClass::Type)
            })
            .await?
            .await
    }
}

/// Applies the shared member-entry rules before a comparison-method query is needed.
struct EqualityMemberEntryEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    env: &'effects ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GeneralMemberEffects<'db>
    for EqualityMemberEntryEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        let work = self
            .source
            .local_with_fixed_transfers(
                8,
                2 * size_of::<usize>() + 2 * size_of::<Option<usize>>(),
                || name.len().checked_add(16),
            )
            .await?
            .ok_or(RunError::Contract("member entry quotation overflow"))?;
        // The entry checks a materialized fallback, the __class__ spelling and finite
        // type tags. It does not execute the general member-dispatch match.
        let bytes = 4 * size_of::<Type<'db>>()
            + 2 * size_of::<GeneralMemberName<'_>>()
            + 2 * size_of::<MemberLookupPolicy>();
        self.source
            .local_with_fixed_transfers(work, bytes, || ())
            .await
    }

    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        self.source
            .type_parameter_future(|| GeneralMemberEffects::key_parts(self.source, key))
            .await?
            .await
    }

    async fn predicate(
        &self,
        predicate: GeneralMemberPredicate<'db>,
        name: &str,
    ) -> RunResult<bool> {
        self.source
            .type_parameter_future(|| GeneralMemberEffects::predicate(self.source, predicate, name))
            .await?
            .await
    }

    async fn wrapper_descriptor(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<Type<'db>>> {
        self.source
            .type_parameter_future(|| {
                GeneralMemberEffects::wrapper_descriptor(self.source, ty, name, policy)
            })
            .await?
            .await
    }

    async fn callable_runtime_class(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.source
            .type_parameter_future(|| {
                GeneralMemberEffects::callable_runtime_class(self.source, callable)
            })
            .await?
            .await
    }

    async fn nominal_enum_member(
        &self,
        instance: NominalInstanceType<'db>,
        name: &str,
    ) -> RunResult<Option<(ClassLiteral<'db>, &'db EnumMetadata<'db>)>> {
        self.source
            .type_parameter_future(|| {
                GeneralMemberEffects::nominal_enum_member(self.source, instance, name)
            })
            .await?
            .await
    }

    async fn execute(
        &self,
        branch: GeneralMemberBranch<'db>,
        key: MemberLookupKey<'db>,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| {
                GeneralMemberEffects::execute(self.source, branch, key, receiver)
            })
            .await?
            .await
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        if receiver.is_some() {
            return self
                .source
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::ExplicitReceiver,
                ))
                .await;
        }
        let quote = self
            .source
            .local_with_fixed_transfers(
                20,
                8 * size_of::<usize>() + 8 * size_of::<Option<usize>>(),
                || match name {
                    GeneralMemberName::Text(text) => {
                        let work = text.len().checked_mul(2)?.checked_add(16)?;
                        let bytes = text
                            .len()
                            .checked_add(2 * size_of::<usize>())?
                            .checked_add(2 * size_of::<Name>())?;
                        Some((work, bytes))
                    }
                    GeneralMemberName::Shared(_) => Some((8, 2 * size_of::<Name>())),
                },
            )
            .await?
            .ok_or(RunError::Contract(
                "equality member name quotation overflow",
            ));
        // Text owns at most its exact CharStr allocation plus two metadata words. A
        // shared name clone retains that allocation; either owner survives query drainage.
        let name = self
            .source
            .local_quoted_with_fixed_transfers(quote, || match name {
                GeneralMemberName::Text(text) => Name::new(text),
                GeneralMemberName::Shared(name) => name.clone(),
            })
            .await?;
        self.source
            .type_parameter_future(|| self.source.environment_program(self.env))
            .await?
            .await?;
        self.source
            .type_parameter_future(|| self.source.access.member_lookup(ty, &name, policy))
            .await?
            .await
    }

    async fn fallback(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| {
                member_lookup_entry_with(ty, name, policy, receiver, GeneralMemberFacts, self)
            })
            .await?
            .await
    }

    async fn dunder_class(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .type_parameter_future(|| GeneralMemberEffects::dunder_class(self.source, ty))
            .await?
            .await
    }

    async fn bound(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .local_with_fixed_transfers(
                12,
                2 * size_of::<Place<'db>>() + 2 * size_of::<MemberLookupResult<'db>>(),
                || MemberLookupResult::from(Place::bound(ty)),
            )
            .await
    }
}
