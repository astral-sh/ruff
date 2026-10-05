use std::cell::RefCell;
use std::task::Poll;

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::parsed_module;
use ty_python_core::semantic_index;

use super::*;
use crate::db::tests::TestDbBuilder;
use crate::types::signatures::effects::try_poll_immediate;
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy, Default)]
struct Scenario {
    cached: bool,
    named_tuple: bool,
    known: Option<KnownClass>,
    deferred: bool,
    typed_dict: bool,
    new_class: bool,
    generic: bool,
}

#[derive(Debug, Eq, PartialEq)]
struct Validation<'db> {
    name: Option<String>,
    bounds: TypeVarBoundOrConstraints<'db>,
    default: Type<'db>,
    default_node: String,
    bound_nodes: Vec<String>,
}

struct Trace<'db> {
    scenario: Scenario,
    events: RefCell<Vec<String>>,
    constraints: RefCell<Vec<Type<'db>>>,
    validation: RefCell<Option<Validation<'db>>>,
    refuse: Option<usize>,
}

impl Trace<'_> {
    fn record(&self, event: impl Into<String>) -> Result<(), usize> {
        let mut events = self.events.borrow_mut();
        let index = events.len();
        events.push(event.into());
        if self.refuse == Some(index) {
            Err(index)
        } else {
            Ok(())
        }
    }

    fn callee<'db>(&self) -> Type<'db> {
        if self.scenario.named_tuple {
            Type::SpecialForm(SpecialFormType::NamedTuple)
        } else {
            Type::unknown()
        }
    }
}

fn name(expression: &ast::Expr) -> &str {
    expression.as_name_expr().map_or("literal", |name| &name.id)
}

fn infallible<T>(result: Result<T, Infallible>) -> T {
    match result {
        Ok(value) => value,
        Err(never) => match never {},
    }
}

impl<'db, 'ast> SynchronousDeferredAssignmentEffects<'db, 'ast> for Trace<'db> {
    type Error = usize;

    fn cached_expression(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, usize> {
        self.record("cached")?;
        Ok(self.scenario.cached.then(|| self.callee()))
    }

    fn expression(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
    ) -> Result<Type<'db>, usize> {
        self.record("callee")?;
        Ok(self.callee())
    }

    fn known_class(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> Result<Option<KnownClass>, usize> {
        self.record("known")?;
        Ok(self.scenario.known)
    }

    fn deferred_definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<Option<Definition<'db>>, usize> {
        self.record("region")?;
        Ok(match builder.region {
            InferenceRegion::Deferred(definition) => Some(definition),
            _ => None,
        })
    }

    fn typed_dict_module(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> Result<Option<TypingModule>, usize> {
        self.record("typed_dict")?;
        Ok(self.scenario.typed_dict.then_some(TypingModule::Typing))
    }

    fn is_new_class(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> Result<bool, usize> {
        self.record("new_class")?;
        Ok(self.scenario.new_class)
    }

    fn child(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _target: &ast::Expr,
        _value: &'ast ast::Expr,
        _call: &'ast ast::ExprCall,
        child: DeferredAssignmentChild<'db>,
    ) -> Result<(), usize> {
        let event = match child {
            DeferredAssignmentChild::NamedTuple => "child:named_tuple",
            DeferredAssignmentChild::NewType => "child:new_type",
            DeferredAssignmentChild::TypeAliasType(definition) => {
                assert!(
                    matches!(builder.region, InferenceRegion::Deferred(region) if region == definition)
                );
                "child:alias"
            }
            DeferredAssignmentChild::BuiltinType(definition) => {
                assert!(
                    matches!(builder.region, InferenceRegion::Deferred(region) if region == definition)
                );
                "child:type"
            }
            DeferredAssignmentChild::TypedDict => "child:typed_dict",
            DeferredAssignmentChild::NewClass(definition) => {
                assert!(
                    matches!(builder.region, InferenceRegion::Deferred(region) if region == definition)
                );
                "child:new_class"
            }
        };
        self.record(event)
    }

    fn new_constraints(&self) -> Result<Vec<Type<'db>>, usize> {
        self.record("constraints:new")?;
        Ok(Vec::new())
    }

    fn next_constraint(
        &self,
        call: &'ast ast::ExprCall,
        cursor: &mut usize,
    ) -> Result<Option<&'ast ast::Expr>, usize> {
        self.record(format!("constraint:{cursor}"))?;
        Ok(infallible(
            OrdinaryDeferredAssignmentEffects.next_constraint(call, cursor),
        ))
    }

    fn push_constraint(
        &self,
        constraints: &mut Vec<Type<'db>>,
        ty: Type<'db>,
    ) -> Result<(), usize> {
        self.record("constraint:push")?;
        constraints.push(ty);
        Ok(())
    }

    fn intern_constraints(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        constraints: Vec<Type<'db>>,
    ) -> Result<TypeVarConstraints<'db>, usize> {
        self.record("constraints:intern")?;
        *self.constraints.borrow_mut() = constraints.clone();
        Ok(infallible(
            OrdinaryDeferredAssignmentEffects.intern_constraints(builder, constraints),
        ))
    }

    fn type_expression(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, usize> {
        self.record(format!("type:{}", name(expression)))?;
        let value = match name(expression) {
            "C1" => 11,
            "C2" => 22,
            "Bound" => 33,
            "LaterBound" => 44,
            "Default" => 55,
            "LaterDefault" => 66,
            other => panic!("unexpected type expression {other}"),
        };
        Ok(Type::int_literal(value))
    }

    fn has_typevar(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> Result<bool, usize> {
        self.record("has_typevar")?;
        Ok(self.scenario.generic)
    }

    fn generic_constraint(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), usize> {
        self.record(format!("generic_constraint:{}", name(expression)))
    }

    fn generic_bound(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        keyword: &ast::Keyword,
    ) -> Result<(), usize> {
        self.record(format!("generic_bound:{}", name(&keyword.value)))
    }

    fn find_keyword(
        &self,
        call: &'ast ast::ExprCall,
        keyword: &str,
    ) -> Result<Option<&'ast ast::Keyword>, usize> {
        self.record(format!("keyword:{keyword}"))?;
        Ok(infallible(
            OrdinaryDeferredAssignmentEffects.find_keyword(call, keyword),
        ))
    }

    fn paramspec_default(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), usize> {
        self.record(format!("paramspec:{}", name(expression)))
    }

    fn typevartuple_default(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> Result<(), usize> {
        self.record(format!("typevartuple:{}", name(expression)))
    }

    fn validate_default(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        name: Option<&str>,
        bounds: Option<TypeVarBoundOrConstraints<'db>>,
        default_ty: Type<'db>,
        default_node: &ast::Expr,
        bound_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) -> Result<(), usize> {
        self.record("validate_default")?;
        validate_typevar_default_sync(
            builder,
            name,
            bounds,
            default_ty,
            default_node,
            bound_nodes,
            self,
        )
    }

    fn bounded_default(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        parameter_name: Option<&str>,
        bounds: TypeVarBoundOrConstraints<'db>,
        default: Type<'db>,
        default_node: &ast::Expr,
        bound_nodes: Option<BoundOrConstraintsNodes<'ast>>,
    ) -> Result<(), usize> {
        self.record("bounded_default")?;
        let bound_nodes = match bound_nodes {
            Some(BoundOrConstraintsNodes::Bound(bound)) => vec![name(bound).to_owned()],
            Some(BoundOrConstraintsNodes::Constraints(constraints)) => constraints
                .iter()
                .map(|constraint| name(constraint).to_owned())
                .collect(),
            None => Vec::new(),
        };
        *self.validation.borrow_mut() = Some(Validation {
            name: parameter_name.map(str::to_owned),
            bounds,
            default,
            default_node: name(default_node).to_owned(),
            bound_nodes,
        });
        Ok(())
    }
}

macro_rules! asynchronous_effects {
    ($(fn $method:ident($($argument:ident: $argument_type:ty),* $(,)?) -> $output:ty;)*) => {
        impl<'db, 'ast> DeferredAssignmentEffects<'db, 'ast> for Trace<'db> {
            type Error = usize;
            async fn validate_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: Option<TypeVarBoundOrConstraints<'db>>, default_ty: Type<'db>, default_node: &ast::Expr, bound_nodes: Option<BoundOrConstraintsNodes<'ast>>) -> Result<(), usize> {
                self.record("validate_default")?;
                validate_typevar_default_with(builder, name, bounds, default_ty, default_node, bound_nodes, self).await
            }
            $(async fn $method(&self, $($argument: $argument_type),*) -> Result<$output, usize> {
                SynchronousDeferredAssignmentEffects::$method(self, $($argument),*)
            })*
        }
    };
}

asynchronous_effects! {
    fn cached_expression(builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Option<Type<'db>>;
    fn expression(builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Type<'db>;
    fn known_class(builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Option<KnownClass>;
    fn deferred_definition(builder: &TypeInferenceBuilder<'db, 'ast>) -> Option<Definition<'db>>;
    fn typed_dict_module(builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> Option<TypingModule>;
    fn is_new_class(builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> bool;
    fn child(builder: &mut TypeInferenceBuilder<'db, 'ast>, target: &ast::Expr, value: &'ast ast::Expr, call: &'ast ast::ExprCall, child: DeferredAssignmentChild<'db>) -> ();
    fn new_constraints() -> Vec<Type<'db>>;
    fn next_constraint(call: &'ast ast::ExprCall, cursor: &mut usize) -> Option<&'ast ast::Expr>;
    fn push_constraint(constraints: &mut Vec<Type<'db>>, ty: Type<'db>) -> ();
    fn intern_constraints(builder: &TypeInferenceBuilder<'db, 'ast>, constraints: Vec<Type<'db>>) -> TypeVarConstraints<'db>;
    fn type_expression(builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> Type<'db>;
    fn has_typevar(builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> bool;
    fn generic_constraint(builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> ();
    fn generic_bound(builder: &TypeInferenceBuilder<'db, 'ast>, keyword: &ast::Keyword) -> ();
    fn find_keyword(call: &'ast ast::ExprCall, name: &str) -> Option<&'ast ast::Keyword>;
    fn paramspec_default(builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> ();
    fn typevartuple_default(builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> ();
    fn bounded_default(builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: TypeVarBoundOrConstraints<'db>, default_ty: Type<'db>, default_node: &ast::Expr, bound_nodes: Option<BoundOrConstraintsNodes<'ast>>) -> ();
}

fn check(source: &str, scenario: Scenario, expected: &[&str], expected_bound: Option<bool>) {
    let db = TestDbBuilder::new()
        .with_file("/src/main.py", source)
        .build()
        .unwrap();
    let file = db.program_file(system_path_to_file(&db, "/src/main.py").unwrap());
    let module = parsed_module(&db, file.python_file(&db)).load(&db);
    let [ast::Stmt::Assign(assignment)] = module.suite().as_slice() else {
        panic!("fixture contains one assignment");
    };
    let target = &assignment.targets[0];
    let index = semantic_index(&db, file);
    let definition = index.expect_single_definition(target.as_name_expr().unwrap());
    let env = ProgramEnvironment::from_file(file);
    for asynchronous in [false, true] {
        for refuse in std::iter::once(None).chain((0..expected.len()).map(Some)) {
            let region = if scenario.deferred {
                InferenceRegion::Deferred(definition)
            } else {
                InferenceRegion::Definition(definition)
            };
            let mut builder =
                TypeInferenceBuilder::new(&db, &env, region, file.file(&db), file, index, &module);
            builder.context.defuse();
            let effects = Trace {
                scenario,
                events: RefCell::default(),
                constraints: RefCell::default(),
                validation: RefCell::default(),
                refuse,
            };
            let result = if asynchronous {
                try_poll_immediate(infer_assignment_deferred_with(
                    &mut builder,
                    target,
                    &assignment.value,
                    DeferredAssignmentFacts,
                    &effects,
                ))
            } else {
                Poll::Ready(infer_assignment_deferred_sync(
                    &mut builder,
                    target,
                    &assignment.value,
                    DeferredAssignmentFacts,
                    &effects,
                ))
            };
            assert_eq!(result, Poll::Ready(refuse.map_or(Ok(()), Err)));
            assert_eq!(
                effects.events.borrow().as_slice(),
                &expected[..refuse.map_or(expected.len(), |index| index + 1)],
                "{source}"
            );
            if refuse.is_none() {
                if expected.contains(&"constraints:intern") {
                    assert_eq!(
                        effects.constraints.borrow().as_slice(),
                        &[Type::int_literal(11), Type::int_literal(22)]
                    );
                }
                if let Some(upper_bound) = expected_bound {
                    let validation = effects.validation.borrow();
                    let validation = validation.as_ref().unwrap();
                    assert_eq!(validation.name.as_deref(), Some("T"));
                    assert_eq!(validation.default, Type::int_literal(55));
                    assert_eq!(validation.default_node, "Default");
                    if upper_bound {
                        assert_eq!(
                            validation.bounds,
                            TypeVarBoundOrConstraints::UpperBound(Type::int_literal(33))
                        );
                        assert_eq!(validation.bound_nodes, ["Bound"]);
                    } else {
                        let TypeVarBoundOrConstraints::Constraints(constraints) = validation.bounds
                        else {
                            panic!("constraints retained for default validation");
                        };
                        assert_eq!(
                            constraints.elements(&db).as_ref(),
                            &[Type::int_literal(11), Type::int_literal(22)]
                        );
                        assert_eq!(validation.bound_nodes, ["C1", "C2"]);
                    }
                } else {
                    assert!(effects.validation.borrow().is_none());
                }
            }
            assert!(!builder.is_deferred());
            assert!(builder.typevar_binding_context.is_none());
        }
    }
}

#[test]
fn constructor_dispatch_preserves_precedence_and_child_refusal_order() {
    check("T = 1\n", Scenario::default(), &[], None);
    check(
        "T = factory(\"T\")\n",
        Scenario {
            named_tuple: true,
            known: Some(KnownClass::NewType),
            typed_dict: true,
            new_class: true,
            ..Scenario::default()
        },
        &["cached", "callee", "child:named_tuple"],
        None,
    );
    for (known, child) in [
        (KnownClass::NewType, "child:new_type"),
        (KnownClass::TypeAliasType, "child:alias"),
        (KnownClass::ExtensionsTypeAliasType, "child:alias"),
        (KnownClass::Type, "child:type"),
    ] {
        check(
            "T = factory(\"T\")\n",
            Scenario {
                cached: true,
                known: Some(known),
                deferred: true,
                typed_dict: true,
                new_class: true,
                ..Scenario::default()
            },
            &["cached", "known", "region", child],
            None,
        );
    }
    check(
        "T = factory(\"T\")\n",
        Scenario {
            deferred: true,
            typed_dict: true,
            new_class: true,
            ..Scenario::default()
        },
        &[
            "cached",
            "callee",
            "known",
            "region",
            "typed_dict",
            "child:typed_dict",
        ],
        None,
    );
    check(
        "T = factory(\"T\")\n",
        Scenario {
            deferred: true,
            new_class: true,
            ..Scenario::default()
        },
        &[
            "cached",
            "callee",
            "known",
            "region",
            "typed_dict",
            "new_class",
            "child:new_class",
        ],
        None,
    );
    for known in [
        None,
        Some(KnownClass::Type),
        Some(KnownClass::TypeAliasType),
    ] {
        check(
            "T = factory(\"T\")\n",
            Scenario {
                known,
                new_class: true,
                ..Scenario::default()
            },
            &[
                "cached",
                "callee",
                "known",
                "region",
                "typed_dict",
                "constraints:new",
                "constraint:1",
                "keyword:bound",
                "keyword:default",
            ],
            None,
        );
    }
}

#[test]
fn constraints_precede_bound_override_and_first_keywords_choose_validation_inputs() {
    check(
        "T = factory(\"T\", C1, C2, bound=Bound, bound=LaterBound, default=Default, default=LaterDefault)\n",
        Scenario {
            deferred: true,
            generic: true,
            ..Scenario::default()
        },
        &[
            "cached",
            "callee",
            "known",
            "region",
            "typed_dict",
            "new_class",
            "constraints:new",
            "constraint:1",
            "type:C1",
            "constraint:push",
            "has_typevar",
            "generic_constraint:C1",
            "constraint:2",
            "type:C2",
            "constraint:push",
            "has_typevar",
            "generic_constraint:C2",
            "constraint:3",
            "constraints:intern",
            "keyword:bound",
            "type:Bound",
            "has_typevar",
            "generic_bound:Bound",
            "keyword:default",
            "type:Default",
            "keyword:bound",
            "validate_default",
            "bounded_default",
        ],
        Some(true),
    );
    check(
        "T = factory(\"T\", C1, C2, default=Default)\n",
        Scenario::default(),
        &[
            "cached",
            "callee",
            "known",
            "region",
            "typed_dict",
            "constraints:new",
            "constraint:1",
            "type:C1",
            "constraint:push",
            "has_typevar",
            "constraint:2",
            "type:C2",
            "constraint:push",
            "has_typevar",
            "constraint:3",
            "constraints:intern",
            "keyword:bound",
            "keyword:default",
            "type:Default",
            "keyword:bound",
            "validate_default",
            "bounded_default",
        ],
        Some(false),
    );
}

#[test]
fn default_families_preserve_unbounded_validation_short_circuit() {
    for known in [None, Some(KnownClass::TypeVar)] {
        check(
            "T = factory(\"T\", default=Default, default=LaterDefault)\n",
            Scenario {
                known,
                ..Scenario::default()
            },
            &[
                "cached",
                "callee",
                "known",
                "region",
                "typed_dict",
                "constraints:new",
                "constraint:1",
                "keyword:bound",
                "keyword:default",
                "type:Default",
                "keyword:bound",
                "validate_default",
            ],
            None,
        );
    }
    for (known, child) in [
        (KnownClass::ParamSpec, "paramspec:Default"),
        (KnownClass::ExtensionsParamSpec, "paramspec:Default"),
        (KnownClass::TypeVarTuple, "typevartuple:Default"),
        (KnownClass::ExtensionsTypeVarTuple, "typevartuple:Default"),
    ] {
        check(
            "T = factory(\"T\", default=Default)\n",
            Scenario {
                known: Some(known),
                ..Scenario::default()
            },
            &[
                "cached",
                "callee",
                "known",
                "region",
                "typed_dict",
                "constraints:new",
                "constraint:1",
                "keyword:bound",
                "keyword:default",
                child,
            ],
            None,
        );
    }
}
