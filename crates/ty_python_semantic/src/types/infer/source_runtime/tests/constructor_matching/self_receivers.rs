//! Self receiver construction preserves exact lexical bindings and typed annotation errors.

mod installation;
mod signature_context;
pub(in crate::types::infer) mod specialization;
mod identity;
mod unused_self;
mod return_nominal;

use ty_python_core::node_key::NodeKey;
use ty_python_core::scope::{NodeWithScopeKey, ScopeId};

use super::*;
use crate::types::generics::typing_self;
use crate::types::infer::InferenceFlags;
use crate::types::type_expression_conversion::InlineConversion;
use crate::types::type_expression_conversion::special_form::{
    SpecialFormConversionFacts, special_form_type_expression_sync, special_form_type_expression_with,
};
use crate::types::{InvalidTypeExpression, SpecialFormType, TypeVarKind};

const SIMPLE: &str = "class Product[T]:\n    def target[U](self): ...\n";
const NESTED: &str = "class Outer:\n    def method(self):\n        class Product[T]:\n            def target[U](self): ...\n";

/// Stores only declaration keys and scopes from the prepared structural index.
#[derive(Clone, Copy, Debug)]
struct Seed<'db> {
    class: Definition<'db>,
    method: Definition<'db>,
    class_scope: ScopeId<'db>,
    method_scope: ScopeId<'db>,
    module_scope: ScopeId<'db>,
}

/// Selects the Product declaration and named method without a semantic request.
fn seed<'db>(prepared: &PreparedAnalysisFile<'db>, method_name: &str) -> Seed<'db> {
    let index = prepared.semantic_index();
    let mut pending = vec![prepared.parsed_module().syntax().body.as_slice()];
    while let Some(body) = pending.pop() {
        for statement in body {
            match statement {
                Stmt::ClassDef(class) if class.name.as_str() == "Product" => {
                    let method = class
                        .body
                        .iter()
                        .filter_map(Stmt::as_function_def_stmt)
                        .find(|method| method.name.as_str() == method_name)
                        .expect("fixture method");
                    return Seed {
                        class: index.expect_single_definition(class),
                        method: index.expect_single_definition(method),
                        class_scope: index.scope_id(
                            index.node_scope_by_key(NodeWithScopeKey::Class(NodeKey::from_node(
                                class,
                            ))),
                        ),
                        method_scope: index.scope_id(index.node_scope_by_key(
                            NodeWithScopeKey::Function(NodeKey::from_node(method)),
                        )),
                        module_scope: index.scope_id(FileScopeId::global()),
                    };
                }
                Stmt::ClassDef(class) => pending.push(&class.body),
                Stmt::FunctionDef(function) => pending.push(&function.body),
                _ => {}
            }
        }
    }
    panic!("fixture Product declaration");
}

/// Selects general lexical binding or the method API's supplied-body validation.
#[derive(Clone, Copy, Debug)]
enum ProducerMode {
    Function,
    ClassBody,
    ExplicitClass,
    ValidatedMethod,
    WrongBody,
}

/// Retains the canonical class and result from the controlled Self producer.
#[derive(Debug)]
struct Produced<'db> {
    class: ClassLiteral<'db>,
    variable: Option<BoundTypeVarInstance<'db>>,
}

#[derive(Clone, Copy, Debug)]
struct Producer<'db> {
    seed: Seed<'db>,
    mode: ProducerMode,
}

impl<'db> MemberOperation<'db> for Producer<'db> {
    type Output = Produced<'db>;

    /// Infers the containing class cold and invokes the selected production Self entry point.
    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        let inference = access.definition(self.seed.class).await?;
        let class = effects
            .local_with_fixed_transfers(3, 0, || {
                inference
                    .original_class_type(self.seed.class)
                    .ok_or(RunError::Contract("Self control requires a class"))
            })
            .await??;
        let variable = match self.mode {
            ProducerMode::Function => {
                effects
                    .typing_self_source(self.seed.class_scope, Some(self.seed.method), class)
                    .await?
            }
            ProducerMode::ClassBody => {
                effects
                    .typing_self_source(self.seed.class_scope, None, class)
                    .await?
            }
            ProducerMode::ExplicitClass => {
                effects
                    .typing_self_source(self.seed.class_scope, Some(self.seed.class), class)
                    .await?
            }
            ProducerMode::ValidatedMethod => {
                effects
                    .typing_self_for_method(self.seed.method_scope, self.seed.method, class)
                    .await?
            }
            ProducerMode::WrongBody => {
                effects
                    .typing_self_for_method(self.seed.class_scope, self.seed.method, class)
                    .await?
            }
        };
        effects
            .local_with_fixed_transfers(3, 0, || Produced { class, variable })
            .await
    }
}

/// Cold Self construction selects the exact method or class binding and matches the ordinary producer.
/// The nested fixture distinguishes the inner method from the surrounding Outer.method binding.
#[test_case::test_case(SIMPLE, ProducerMode::Function; "explicit function binding")]
#[test_case::test_case(NESTED, ProducerMode::Function; "nested class function binding")]
#[test_case::test_case(SIMPLE, ProducerMode::ClassBody; "class body without explicit binding")]
#[test_case::test_case(SIMPLE, ProducerMode::ExplicitClass; "explicit class binding")]
#[test_case::test_case(SIMPLE, ProducerMode::ValidatedMethod; "validated method body")]
fn cold_producer_preserves_scope_and_identity(source: &str, mode: ProducerMode) {
    let db = database(source);
    let prepared = prepare(&db);
    let seed = seed(&prepared, "target");
    let revision = salsa::plumbing::current_revision(&db);
    let captured = capture(&db, || {
        controlled_member_operation(&prepared, Producer { seed, mode }, &funded())
    })
    .unwrap();
    captured.check_root_reads().unwrap();
    let Ok(AnalysisOutcome::Complete(result)) = captured.value else {
        panic!("cold Self producer: {:?}", captured.value);
    };
    let variable = result.variable.expect("class or method Self binding");
    let (scope, binding, expected) = match mode {
        ProducerMode::Function => (seed.class_scope, Some(seed.method), seed.method),
        ProducerMode::ClassBody => (seed.class_scope, None, seed.class),
        ProducerMode::ExplicitClass => (seed.class_scope, Some(seed.class), seed.class),
        ProducerMode::ValidatedMethod => (seed.method_scope, Some(seed.method), seed.method),
        ProducerMode::WrongBody => panic!("positive producer case"),
    };
    assert_eq!(variable.binding_context(&db).definition(), Some(expected));
    assert_eq!(variable.typevar(&db).kind(&db), TypeVarKind::TypingSelf);
    assert_eq!(variable.typevar(&db).definition(&db), None);
    assert_eq!(variable.freshness(&db), TypeVarNonce::NONE);
    assert_eq!(
        Some(variable),
        typing_self(&db, scope, binding, result.class)
    );
    assert_eq!(salsa::plumbing::current_revision(&db), revision);
    assert_no_active_attempt();
}

/// The method-only entry rejects a class body even though general binding can resolve its method.
#[test]
fn validated_method_keeps_wrong_body_refusal() {
    let db = database(SIMPLE);
    let prepared = prepare(&db);
    let seed = seed(&prepared, "target");
    assert!(matches!(
        controlled_member_operation(
            &prepared,
            Producer {
                seed,
                mode: ProducerMode::WrongBody
            },
            &funded()
        ),
        Ok(AnalysisOutcome::Incomplete {
            reason: AnalysisIncomplete::UnavailableOperation(OperationId::TypingSelfBodyScope),
            ..
        })
    ));
    assert_no_active_attempt();
}

/// The method entry rejects a definition from another file before accepting its body scope.
#[test]
fn validated_method_keeps_foreign_file_contract() {
    let mut db = database(SIMPLE);
    db.write_file("src/other.py", SIMPLE).unwrap();
    let prepared = prepare(&db);
    let other_file = system_path_to_file(&db, "src/other.py").unwrap();
    let other_prepared = prepare_file(&db, other_file).unwrap();
    let mut keys = seed(&prepared, "target");
    keys.method = seed(&other_prepared, "target").method;
    assert!(matches!(
        controlled_member_operation(&prepared, Producer { seed: keys, mode: ProducerMode::ValidatedMethod }, &funded()),
        Err(AnalysisFailure::Execution(RunError::Contract("method Self binding belongs to a different file")))
    ));
    assert_no_active_attempt();
}

/// Executes the public special-form dispatcher using structurally selected scope and binding keys.
#[derive(Clone, Copy, Debug)]
struct Annotation<'db> {
    scope: ScopeId<'db>,
    binding: Option<Definition<'db>>,
    flags: InferenceFlags,
}

impl<'db> MemberOperation<'db> for Annotation<'db> {
    type Output = Result<Type<'db>, InvalidTypeExpression<'db>>;

    async fn run<'run, A: SourceAccess<'run, 'db>>(
        self,
        access: &A,
        program: Program<'db>,
    ) -> RunResult<Self::Output>
    where
        'db: 'run,
    {
        let effects = SourceEffects::new(access, program);
        special_form_type_expression_with(
            SpecialFormType::TypingSelf,
            self.scope,
            self.binding,
            self.flags,
            SpecialFormConversionFacts,
            &effects,
        )
        .await
    }
}

/// Identifies the typed error expected from the shared Self validity rules.
#[derive(Clone, Copy, Debug)]
enum InvalidCase {
    StaticMethod,
    Metaclass,
    IncompatibleReceiver,
    TypeAlias,
    NoClass,
}

/// Controlled Self validation preserves each ordinary typed error and exact bound-variable payload.
#[test_case::test_case("class Product:\n    @staticmethod\n    def target(): ...\n", InferenceFlags::empty(), InvalidCase::StaticMethod; "staticmethod")]
#[test_case::test_case("class Product(type):\n    def target(self): ...\n", InferenceFlags::empty(), InvalidCase::Metaclass; "metaclass")]
#[test_case::test_case("class Product:\n    def target(self): ...\n", InferenceFlags::HAS_INCOMPATIBLE_SELF_RECEIVER.union(InferenceFlags::IN_RETURN_TYPE), InvalidCase::IncompatibleReceiver; "incompatible return receiver")]
#[test_case::test_case("class Product:\n    def target(self): ...\n", InferenceFlags::IN_TYPE_ALIAS, InvalidCase::TypeAlias; "type alias")]
#[test_case::test_case("class Product:\n    def target(self): ...\n", InferenceFlags::empty(), InvalidCase::NoClass; "no enclosing class")]
fn typed_validation_errors_match_ordinary(
    source: &str,
    flags: InferenceFlags,
    expected: InvalidCase,
) {
    let db = database(source);
    let prepared = prepare(&db);
    let seed = seed(&prepared, "target");
    let scope = match expected {
        InvalidCase::NoClass => seed.module_scope,
        _ => seed.class_scope,
    };
    let request = Annotation {
        scope,
        binding: Some(seed.method),
        flags,
    };
    let result = controlled_member_operation(&prepared, request, &funded());
    let Ok(AnalysisOutcome::Complete(actual)) = result else {
        panic!("Self annotation: {result:?}");
    };
    match (expected, actual) {
        (InvalidCase::StaticMethod, Err(InvalidTypeExpression::TypingSelfInStaticMethod))
        | (InvalidCase::Metaclass, Err(InvalidTypeExpression::TypingSelfInMetaclass))
        | (
            InvalidCase::IncompatibleReceiver,
            Err(InvalidTypeExpression::TypingSelfWithIncompatibleReceiver(_)),
        )
        | (InvalidCase::TypeAlias, Err(InvalidTypeExpression::TypingSelfInTypeAlias))
        | (
            InvalidCase::NoClass,
            Err(InvalidTypeExpression::InvalidType(
                Type::SpecialForm(SpecialFormType::TypingSelf),
                _,
            )),
        ) => {}
        _ => panic!("Self validity result: {actual:?}"),
    }
    let ordinary = special_form_type_expression_sync(
        SpecialFormType::TypingSelf,
        scope,
        request.binding,
        flags,
        SpecialFormConversionFacts,
        &InlineConversion { db: &db },
    ).unwrap();
    assert_eq!(actual, ordinary);
    assert_no_active_attempt();
}

/// Static `__new__` remains exempt from staticmethod rejection and returns its method-bound Self.
#[test]
fn static_new_retains_self_exemption() {
    let db = database("class Product:\n    @staticmethod\n    def __new__(cls): ...\n");
    let prepared = prepare(&db);
    let seed = seed(&prepared, "__new__");
    let result = controlled_member_operation(
        &prepared,
        Annotation {
            scope: seed.class_scope,
            binding: Some(seed.method),
            flags: InferenceFlags::empty(),
        },
        &funded(),
    );
    let Ok(AnalysisOutcome::Complete(Ok(Type::TypeVar(variable)))) = result else {
        panic!("static __new__ Self: {result:?}");
    };
    assert_eq!(
        variable.binding_context(&db).definition(),
        Some(seed.method)
    );
    assert_no_active_attempt();
}
