//! Shared PEP 695 declaration headers with lazy bounds and defaults.

use std::convert::Infallible;

use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::definition::Definition;

use crate::{
    Db,
    types::typevar::{
        TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity,
        TypeVarInstance, TypeVarKind,
    },
};

/// The syntax needed to declare a PEP 695 parameter without evaluating its bound or default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) struct TypeParameterHeaderInput<'a> {
    pub(in crate::types) kind: TypeVarKind,
    pub(in crate::types) name: &'a Name,
    pub(in crate::types) bound: Option<TypeParameterBoundHeader>,
    pub(in crate::types) has_default: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum TypeParameterBoundHeader {
    UpperBound,
    Constraints { len: usize, range: TextRange },
}

impl<'a> From<&'a ast::TypeParam> for TypeParameterHeaderInput<'a> {
    fn from(parameter: &'a ast::TypeParam) -> Self {
        Self::from(ast::TypeParamRef::from(parameter))
    }
}

impl<'a> From<ast::TypeParamRef<'a>> for TypeParameterHeaderInput<'a> {
    fn from(parameter: ast::TypeParamRef<'a>) -> Self {
        match parameter {
            ast::TypeParamRef::TypeVar(node) => Self::from(node),
            ast::TypeParamRef::ParamSpec(node) => Self::from(node),
            ast::TypeParamRef::TypeVarTuple(node) => Self::from(node),
        }
    }
}

impl<'a> From<&'a ast::TypeParamTypeVar> for TypeParameterHeaderInput<'a> {
    fn from(node: &'a ast::TypeParamTypeVar) -> Self {
        Self {
            kind: TypeVarKind::Pep695TypeVar,
            name: &node.name.id,
            bound: node.bound.as_deref().map(|bound| match bound {
                ast::Expr::Tuple(tuple) => TypeParameterBoundHeader::Constraints {
                    len: tuple.elts.len(),
                    range: bound.range(),
                },
                _ => TypeParameterBoundHeader::UpperBound,
            }),
            has_default: node.default.is_some(),
        }
    }
}

impl<'a> From<&'a ast::TypeParamParamSpec> for TypeParameterHeaderInput<'a> {
    fn from(node: &'a ast::TypeParamParamSpec) -> Self {
        Self {
            kind: TypeVarKind::Pep695ParamSpec,
            name: &node.name.id,
            bound: None,
            has_default: node.default.is_some(),
        }
    }
}

impl<'a> From<&'a ast::TypeParamTypeVarTuple> for TypeParameterHeaderInput<'a> {
    fn from(node: &'a ast::TypeParamTypeVarTuple) -> Self {
        Self {
            kind: TypeVarKind::Pep695TypeVarTuple,
            name: &node.name.id,
            bound: None,
            has_default: node.default.is_some(),
        }
    }
}

/// A complete declaration header whose bound and default, when present, remain deferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub(in crate::types) struct TypeParameterHeader<'db> {
    pub(in crate::types) variable: TypeVarInstance<'db>,
    pub(in crate::types) deferred: Option<Definition<'db>>,
    pub(in crate::types) invalid_constraint_count: Option<TextRange>,
}

/// Lazy annotation descriptors and any immediate constraint-count diagnostic.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct TypeParameterHeaderState<'db> {
    bound_or_constraints: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
    default: Option<TypeVarDefaultEvaluation<'db>>,
    invalid_constraint_count: Option<TextRange>,
}

/// Finite decisions that do not inspect or evaluate annotation expressions.
#[derive(Clone, Copy, Debug)]
pub(in crate::types) struct TypeParameterHeaderFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeParameterHeaderEffects)]
    pub(in crate::types) trait TypeParameterHeaderEffects<'db> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn intern_identity(&self, name: &Name, definition: Definition<'db>, kind: TypeVarKind) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(local)]
        async fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Self::Error>;
    }

    #[finite_capability]
    impl TypeParameterHeaderFacts {
        fn annotations<'db>(&self, input: TypeParameterHeaderInput<'_>) -> TypeParameterHeaderState<'db> {
            let (bound_or_constraints, invalid_constraint_count) = match input.bound {
                Some(TypeParameterBoundHeader::Constraints { len, range }) if len < 2 => {
                    (None, Some(range))
                }
                Some(TypeParameterBoundHeader::Constraints { .. }) => (
                    Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
                    None,
                ),
                Some(TypeParameterBoundHeader::UpperBound) => (
                    Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
                    None,
                ),
                None => (None, None),
            };
            TypeParameterHeaderState {
                bound_or_constraints,
                default: input.has_default.then_some(TypeVarDefaultEvaluation::Lazy),
                invalid_constraint_count,
            }
        }

        fn header<'db>(&self, definition: Definition<'db>, variable: TypeVarInstance<'db>, annotations: TypeParameterHeaderState<'db>) -> TypeParameterHeader<'db> {
            TypeParameterHeader {
                variable,
                deferred: (annotations.bound_or_constraints.is_some() || annotations.default.is_some()).then_some(definition),
                invalid_constraint_count: annotations.invalid_constraint_count,
            }
        }
    }

    /// Constructs a parameter header while retaining its unevaluated bounds and default.
    #[synchronous(infer_type_parameter_header_sync)]
    #[capabilities(effects = TypeParameterHeaderEffects, facts = TypeParameterHeaderFacts)]
    #[passive_values()]
    pub(in crate::types) async fn infer_type_parameter_header_with<'db, E: TypeParameterHeaderEffects<'db>>(
        definition: Definition<'db>,
        input: TypeParameterHeaderInput<'_>,
        facts: TypeParameterHeaderFacts,
        effects: &E,
    ) -> Result<TypeParameterHeader<'db>, E::Error> {
        effects.checkpoint().await?;
        let annotations = facts.annotations(input);
        let identity = effects.intern_identity(input.name, definition, input.kind).await?;
        let variable = effects.intern_variable(identity, annotations.bound_or_constraints, annotations.default).await?;
        Ok(facts.header(definition, variable, annotations))
    }
}

/// Interns ordinary declaration headers without evaluating their annotation expressions.
struct OrdinaryTypeParameterHeaderEffects<'db>(&'db dyn Db);

impl<'db> SynchronousTypeParameterHeaderEffects<'db> for OrdinaryTypeParameterHeaderEffects<'db> {
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn intern_identity(
        &self,
        name: &Name,
        definition: Definition<'db>,
        kind: TypeVarKind,
    ) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(TypeVarIdentity::new(self.0, name, Some(definition), kind))
    }

    fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(TypeVarInstance::new(self.0, identity, bounds, None, default))
    }
}

/// Creates an ordinary declaration header through the shared lazy-header algorithm.
pub(in crate::types) fn infer_type_parameter_header<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    input: TypeParameterHeaderInput<'_>,
) -> TypeParameterHeader<'db> {
    match infer_type_parameter_header_sync(
        definition,
        input,
        TypeParameterHeaderFacts,
        &OrdinaryTypeParameterHeaderEffects(db),
    ) {
        Ok(header) => header,
        Err(never) => match never {},
    }
}

#[cfg(test)]
mod tests {
    use ruff_db::{
        files::system_path_to_file, parsed::parsed_module, system::DbWithWritableSystem,
        testing::assert_function_query_was_not_run_by_name,
    };
    use ty_python_core::semantic_index;

    use super::*;
    use crate::db::tests::setup_db;

    #[test]
    fn headers_preserve_lazy_annotations_and_constraint_ranges() -> anyhow::Result<()> {
        let mut db = setup_db();
        let source = "\
def plain[T](): ...
def bounded[T: MissingBound[tuple[int, ...]]](): ...
def constrained[T: (MissingA[int], MissingB[str])](): ...
def defaulted[T = MissingDefault[list[int]]](): ...
def invalid_empty[T: ()](): ...
def invalid_single[T: (Missing,)](): ...
def invalid_default[T: (Missing,) = MissingDefault[int]](): ...
def paramspec[**P](): ...
def paramspec_default[**P = [Missing[int]]](): ...
def typevartuple[*Ts](): ...
def typevartuple_default[*Ts = *tuple[Missing[int], ...]](): ...
";
        db.write_file("src/header.py", source)?;
        let file = system_path_to_file(&db, "src/header.py")?;
        let program_file = db.program_file(file);
        let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
        let index = semantic_index(&db, program_file);
        let expected = [
            (TypeVarKind::Pep695TypeVar, None, false, None),
            (
                TypeVarKind::Pep695TypeVar,
                Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
                false,
                None,
            ),
            (
                TypeVarKind::Pep695TypeVar,
                Some(TypeVarBoundOrConstraintsEvaluation::LazyConstraints),
                false,
                None,
            ),
            (TypeVarKind::Pep695TypeVar, None, true, None),
            (TypeVarKind::Pep695TypeVar, None, false, Some("()")),
            (TypeVarKind::Pep695TypeVar, None, false, Some("(Missing,)")),
            (TypeVarKind::Pep695TypeVar, None, true, Some("(Missing,)")),
            (TypeVarKind::Pep695ParamSpec, None, false, None),
            (TypeVarKind::Pep695ParamSpec, None, true, None),
            (TypeVarKind::Pep695TypeVarTuple, None, false, None),
            (TypeVarKind::Pep695TypeVarTuple, None, true, None),
        ];
        assert_eq!(module.suite().len(), expected.len());
        for (statement, (kind, bound, has_default, invalid_constraints)) in
            module.suite().iter().zip(expected)
        {
            let ast::Stmt::FunctionDef(function) = statement else {
                anyhow::bail!("expected a function declaration");
            };
            let Some(type_params) = &function.type_params else {
                anyhow::bail!("expected type parameters");
            };
            let [parameter] = type_params.type_params.as_slice() else {
                anyhow::bail!("expected one type parameter");
            };
            let definition = match parameter {
                ast::TypeParam::TypeVar(node) => index.expect_single_definition(node),
                ast::TypeParam::ParamSpec(node) => index.expect_single_definition(node),
                ast::TypeParam::TypeVarTuple(node) => index.expect_single_definition(node),
            };
            let input = TypeParameterHeaderInput::from(parameter);
            let header = infer_type_parameter_header(&db, definition, input);
            let identity = TypeVarIdentity::new(&db, input.name, Some(definition), kind);
            assert_eq!(
                header.variable,
                TypeVarInstance::new(
                    &db,
                    identity,
                    bound,
                    None,
                    has_default.then_some(TypeVarDefaultEvaluation::Lazy),
                ),
            );
            assert_eq!(
                header.deferred,
                (bound.is_some() || has_default).then_some(definition),
            );
            assert_eq!(
                header.invalid_constraint_count.map(|range| &source[range]),
                invalid_constraints,
            );
        }

        let events = db.take_salsa_events();
        for query in [
            "infer_definition_types",
            "infer_deferred_types",
            "infer_scope_types_impl",
            "infer_expression_types_impl",
        ] {
            assert_function_query_was_not_run_by_name(&db, query, None, &events);
        }
        Ok(())
    }
}
