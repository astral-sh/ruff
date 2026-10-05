//! Exercises source relation conversion and finite metaclass inputs under the source runtime.
//! These controls inspect the controlled route and cleanup, which Python mdtests cannot observe.

use salsa::execution_probe::ExecutionWork;

use super::*;
use crate::types::ClassType;
use crate::types::relation::source::RelationSourceEffects;

/// Converts preconstructed metaclass input without claiming cold metaclass inference.
#[derive(Clone, Copy, Debug)]
struct FiniteMetaclassRequest<'db>(Type<'db>);

impl<'db> MemberOperation<'db> for FiniteMetaclassRequest<'db> {
    type Output = Type<'db>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        SourceEffects::new(access, program)
            .metaclass_instance_value(self.0)
            .await
    }
}

/// Dynamic metaclass instances retain their class-object constraint instead of becoming Any or Unknown.
/// Each case supplies finite provider input; it does not infer a metaclass from Python source.
#[test_case::test_case(Type::any(), SubclassOfType::subclass_of_any(); "any metaclass")]
#[test_case::test_case(Type::unknown(), SubclassOfType::subclass_of_unknown(); "unknown metaclass")]
#[test_case::test_case(SubclassOfType::subclass_of_unknown(), SubclassOfType::subclass_of_unknown(); "unknown subclass metaclass")]
fn dynamic_metaclass_instances_remain_class_objects(
    metaclass: Type<'static>,
    expected: Type<'static>,
) {
    let db = database(IMPLICIT_META);
    let prepared = prepare(&db);
    observations::reset(None);
    let result = capture(&db, || {
        controlled_member_operation(&prepared, FiniteMetaclassRequest(metaclass), &funded())
    })
    .unwrap();
    assert!(matches!(
        result.check_root_reads(),
        Err(salsa::prepared_source_probe::CaptureError::NoRootReads)
    ));
    assert_eq!(result.value, Ok(AnalysisOutcome::Complete(expected)));
    assert_cleanup();
}

/// Infers a real alias before invoking the source relation's class-metaclass-instance hook.
#[derive(Clone, Copy, Debug)]
struct AliasInstanceRequest<'db> {
    expression: Expression<'db>,
    key: ExpressionNodeKey,
}

impl<'db> MemberOperation<'db> for AliasInstanceRequest<'db> {
    type Output = (ClassType<'db>, Type<'db>);

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let inference = access
            .expression(self.expression, TypeContext::default())
            .await?;
        let endpoint = access.endpoint();
        let (class, env) = endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: size_of::<(ClassType<'db>, ProgramEnvironment<'db>)>(),
                })?;
                endpoint.check_completion()?;
                let Type::GenericAlias(alias) = inference.expression_type(self.key) else {
                    return Err(RunError::Contract(
                        "fixture expression is not a generic alias",
                    ));
                };
                Ok((
                    ClassType::Generic(alias),
                    ProgramEnvironment::from_program(program),
                ))
            })
            .await;
        let instance = RelationSourceEffects::class_metaclass_instance(
            &SourceEffects::new(access, program),
            &env,
            class,
        )
        .await?;
        Ok((class, instance))
    }
}

/// A cold alias reaches the relation conversion hook and preserves the ordinary metaclass instance.
/// Its source expression is uncached before the controlled run; ordinary conversion runs afterward.
#[test]
fn cold_alias_relation_conversion_matches_ordinary_instance() {
    let db = database(
        "from typing import Generic, TypeVar\nT = TypeVar(\"T\")\nclass Meta(type): pass\nclass Product(Generic[T], metaclass=Meta): pass\nleft = right = Product[bool]\n",
    );
    let prepared = prepare(&db);
    let key = expression_key(&prepared);
    let expression = prepared.semantic_index().expression(key);
    assert_eq!(
        FinalSourceMemo::certify(
            &db as &dyn Db,
            expression_inference_ingredient(&db),
            expression.as_id(),
        )
        .map(|_| ()),
        Err(FinalSourceError::MissingMemo),
    );
    observations::reset(None);
    let result = capture(&db, || {
        controlled_member_operation(
            &prepared,
            AliasInstanceRequest { expression, key },
            &funded(),
        )
    })
    .unwrap();
    result.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete((class, instance))) = result.value else {
        panic!("cold alias relation conversion: {:?}", result.value);
    };
    let env = ProgramEnvironment::from_file(prepared.program_file());
    assert_eq!(instance, class.metaclass_instance_type(&db, &env));
    assert_cleanup();
}
