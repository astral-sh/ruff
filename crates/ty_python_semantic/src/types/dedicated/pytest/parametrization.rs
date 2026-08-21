//! A shared semantic model for `pytest.mark.parametrize` decorators.

use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, name::Name};
use ruff_text_size::{Ranged, TextRange};
use ty_python_core::{
    ExpressionNodeKey,
    definition::{Definition, DefinitionKind},
};

use crate::Db;
use crate::types::infer::function_known_decorators;
use crate::types::{
    KnownClass, ProgramEnvironment, Type, definition_expression_type,
    extract_fixed_length_iterable_element_types,
};

/// Returns the canonical pytest parametrizations applied to a function or class.
#[salsa::tracked(returns(ref), heap_size = ruff_memory_usage::heap_size)]
pub(crate) fn parametrizations<'db>(db: &'db dyn Db, owner: Definition<'db>) -> Parametrizations {
    let module = parsed_module(db, owner.python_file(db)).load(db);

    match owner.kind(db) {
        DefinitionKind::Function(function) => {
            let decorators = function_known_decorators(db, owner);
            parse_parametrizations(
                db,
                owner,
                &function.node(&module).decorator_list,
                |expression| decorators.expression_type(expression),
            )
        }
        DefinitionKind::Class(class) => parse_parametrizations(
            db,
            owner,
            &class.node(&module).decorator_list,
            |expression| Some(definition_expression_type(db, owner, expression)),
        ),
        _ => Parametrizations::default(),
    }
}

/// The canonical pytest parametrizations applied to one owner.
#[derive(Debug, Default, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct Parametrizations(Box<[Parametrization]>);

impl Parametrizations {
    /// Iterates over the owner's parametrizations in decorator order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Parametrization> {
        self.0.iter()
    }
}

/// Static facts recovered from one canonical `pytest.mark.parametrize` decorator.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct Parametrization {
    decorator: DecoratorHandle,
    argnames: StaticArgnames,
    argvalues: ArgumentPresence,
    indirect: StaticIndirect,
    has_unsupported_arguments: bool,
}

impl Parametrization {
    /// Returns a handle to the decorator in its owner's AST.
    pub(crate) const fn decorator(&self) -> DecoratorHandle {
        self.decorator
    }

    /// Returns the statically recovered `argnames`.
    pub(crate) const fn argnames(&self) -> &StaticArgnames {
        &self.argnames
    }

    /// Returns whether and where `argvalues` was supplied.
    pub(crate) const fn argvalues(&self) -> ArgumentPresence {
        self.argvalues
    }

    /// Returns the statically recovered `indirect` configuration.
    pub(crate) const fn indirect(&self) -> &StaticIndirect {
        &self.indirect
    }

    /// Returns whether arguments outside the supported pytest signature were supplied.
    pub(crate) const fn has_unsupported_arguments(&self) -> bool {
        self.has_unsupported_arguments
    }
}

/// Locates a decorator within the decorated function or class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DecoratorHandle(usize);

impl DecoratorHandle {
    /// Resolves this handle against the owner's decorators.
    pub(crate) fn resolve(self, decorators: &[ast::Decorator]) -> Option<&ast::Decorator> {
        decorators.get(self.0)
    }
}

/// Whether an argument was explicitly supplied and, if so, its AST node key.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ArgumentPresence {
    /// The argument was not supplied directly.
    Missing,
    /// The argument was supplied at this expression.
    Present(ExpressionNodeKey),
}

/// Static information about the `argnames` argument.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum StaticArgnames {
    /// The argument was not supplied directly.
    Missing,
    /// The argument was supplied but cannot be interpreted statically.
    Unknown,
    /// Every argument name is statically known.
    Known {
        /// Whether pytest receives one string or an iterable of strings.
        form: ArgnamesForm,
        /// The names and their source ranges.
        names: Box<[StaticArgname]>,
    },
}

impl StaticArgnames {
    /// Returns all statically known names, preserving an empty known collection.
    pub(crate) fn known(&self) -> Option<(ArgnamesForm, &[StaticArgname])> {
        match self {
            Self::Known { form, names } => Some((*form, names)),
            Self::Missing | Self::Unknown => None,
        }
    }
}

/// The source form of statically known pytest argument names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ArgnamesForm {
    /// A scalar string split on commas or whitespace.
    ScalarString,
    /// A statically known fixed-length iterable of strings.
    Sequence,
}

/// One statically known pytest argument name.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct StaticArgname {
    name: Name,
    range: TextRange,
}

impl StaticArgname {
    /// Returns the argument name.
    pub(crate) fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the source range that supplied the name.
    pub(crate) const fn range(&self) -> TextRange {
        self.range
    }
}

/// Static information about the `indirect` argument.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum StaticIndirect {
    /// `indirect` is absent or statically false.
    False,
    /// `indirect` is statically true for every name.
    True,
    /// `indirect` names a statically known collection of parameters.
    Named(Box<[StaticArgname]>),
    /// `indirect` was supplied but cannot be interpreted statically.
    Unknown,
}

impl StaticIndirect {
    /// Returns whether `name` is definitely indirect.
    pub(crate) fn is_indirect(&self, name: &str) -> Option<bool> {
        match self {
            Self::False => Some(false),
            Self::True => Some(true),
            Self::Named(names) => Some(names.iter().any(|candidate| candidate.name() == name)),
            Self::Unknown => None,
        }
    }
}

fn parse_parametrizations<'db>(
    db: &'db dyn Db,
    owner: Definition<'db>,
    decorators: &[ast::Decorator],
    expression_type: impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Parametrizations {
    let environment = ProgramEnvironment::from_file(owner.program_file(db));
    Parametrizations(
        decorators
            .iter()
            .enumerate()
            .filter_map(|(index, decorator)| {
                parse_parametrization(
                    db,
                    &environment,
                    DecoratorHandle(index),
                    decorator,
                    &expression_type,
                )
            })
            .collect(),
    )
}

/// Interprets one decorator without emitting diagnostics.
fn parse_parametrization<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    handle: DecoratorHandle,
    decorator: &ast::Decorator,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<Parametrization> {
    let call = decorator.expression.as_call_expr()?;
    if !expression_type(&call.func)
        .is_some_and(|ty| ty.is_instance_of(db, KnownClass::PytestParametrizeMarkDecorator))
    {
        return None;
    }

    let argnames = call
        .arguments
        .find_argument_value("argnames", 0)
        .map_or(StaticArgnames::Missing, |expression| {
            parse_argnames(db, environment, expression, expression_type)
        });
    let argvalues = call
        .arguments
        .find_argument_value("argvalues", 1)
        .map_or(ArgumentPresence::Missing, |expression| {
            ArgumentPresence::Present(expression.into())
        });
    let indirect = call
        .arguments
        .find_argument_value("indirect", 2)
        .map_or(StaticIndirect::False, |expression| {
            parse_indirect(db, environment, expression, expression_type)
        });

    Some(Parametrization {
        decorator: handle,
        argnames,
        argvalues,
        indirect,
        has_unsupported_arguments: has_unsupported_arguments(&call.arguments),
    })
}

fn parse_argnames<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> StaticArgnames {
    match statically_known_names(db, environment, expression, expression_type) {
        Some((form, names)) => StaticArgnames::Known { form, names },
        None => StaticArgnames::Unknown,
    }
}

fn parse_indirect<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> StaticIndirect {
    let Some(ty) = expression_type(expression) else {
        return StaticIndirect::Unknown;
    };
    if ty == Type::bool_literal(false) {
        return StaticIndirect::False;
    }
    if ty == Type::bool_literal(true) {
        return StaticIndirect::True;
    }

    statically_known_names_from_type(db, environment, expression, ty, expression_type)
        .map_or(StaticIndirect::Unknown, |(_, names)| {
            StaticIndirect::Named(names)
        })
}

fn statically_known_names<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<(ArgnamesForm, Box<[StaticArgname]>)> {
    let ty = expression_type(expression)?;
    statically_known_names_from_type(db, environment, expression, ty, expression_type)
}

fn statically_known_names_from_type<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    ty: Type<'db>,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<(ArgnamesForm, Box<[StaticArgname]>)> {
    if let Some(string) = ty.as_string_literal() {
        let names = string
            .value(db)
            .split(|character: char| character == ',' || character.is_whitespace())
            .filter(|name| !name.is_empty())
            .map(|name| StaticArgname {
                name: Name::new(name),
                range: expression.range(),
            })
            .collect();
        return Some((ArgnamesForm::ScalarString, names));
    }

    let names = fixed_length_elements(db, environment, expression, expression_type)?
        .into_iter()
        .map(|(ty, range)| {
            ty.as_string_literal().map(|string| StaticArgname {
                name: Name::new(string.value(db)),
                range,
            })
        })
        .collect::<Option<Box<[_]>>>()?;
    Some((ArgnamesForm::Sequence, names))
}

fn fixed_length_elements<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<Vec<(Type<'db>, TextRange)>> {
    let elements: Option<&[ast::Expr]> = match expression {
        ast::Expr::List(list) => Some(list.elts.as_ref()),
        ast::Expr::Tuple(tuple) => Some(tuple.elts.as_ref()),
        _ => None,
    };

    if let Some(elements) = elements {
        let mut result = Vec::with_capacity(elements.len());
        for element in elements {
            if let ast::Expr::Starred(starred) = element {
                result.extend(fixed_length_elements(
                    db,
                    environment,
                    &starred.value,
                    expression_type,
                )?);
            } else {
                result.push((
                    expression_type(element).unwrap_or_else(Type::unknown),
                    element.range(),
                ));
            }
        }
        return Some(result);
    }

    extract_fixed_length_iterable_element_types(db, environment, expression, |element| {
        expression_type(element).unwrap_or_else(Type::unknown)
    })
    .map(|types| {
        types
            .into_vec()
            .into_iter()
            .map(|ty| (ty, expression.range()))
            .collect()
    })
}

fn has_unsupported_arguments(arguments: &ast::Arguments) -> bool {
    const SUPPORTED_KEYWORDS: [&str; 5] = ["argnames", "argvalues", "indirect", "ids", "scope"];

    let mut seen = [false; SUPPORTED_KEYWORDS.len()];
    for (position, argument) in arguments.args.iter().enumerate() {
        if argument.is_starred_expr() || position >= SUPPORTED_KEYWORDS.len() {
            return true;
        }
        seen[position] = true;
    }

    for keyword in &arguments.keywords {
        let Some(name) = keyword.arg.as_ref() else {
            return true;
        };
        let Some(position) = SUPPORTED_KEYWORDS
            .iter()
            .position(|supported| name.as_str() == *supported)
        else {
            return true;
        };
        if seen[position] {
            return true;
        }
        seen[position] = true;
    }

    false
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_python_ast as ast;
    use ruff_text_size::Ranged;
    use ty_python_core::{
        ExpressionNodeKey,
        definition::{Definition, DefinitionKind},
        semantic_index,
    };

    use super::{
        ArgnamesForm, ArgumentPresence, StaticArgname, StaticArgnames, StaticIndirect,
        parametrizations,
    };
    use crate::Db as _;
    use crate::db::tests::{TestDb, TestDbBuilder};

    #[test]
    fn models_function_parametrizations() {
        let source = r#"
import pytest

dynamic_names: str
dynamic_indirect: bool

def unrelated(function): ...

@unrelated
@pytest.mark.parametrize(argvalues=[1])
@pytest.mark.parametrize(dynamic_names, [1])
@pytest.mark.parametrize("first, second third", [1])
@pytest.mark.parametrize(("first", "second"), [(1, 2)], indirect=("second",))
@pytest.mark.parametrize("first", [1], indirect=False)
@pytest.mark.parametrize("first", [1], indirect=True)
@pytest.mark.parametrize("first", [1], indirect=dynamic_indirect)
@pytest.mark.parametrize("first", [1], ids=None, scope="function")
@pytest.mark.parametrize("first", [1], unexpected=True)
@pytest.mark.parametrize("first")
def test_example(first, second, third): ...
"#;
        let db = pytest_db("/src/test_example.py", source);
        let owner = function_owner(&db, "/src/test_example.py", "test_example");
        let module = parsed_module(&db, owner.python_file(&db)).load(&db);
        let DefinitionKind::Function(function) = owner.kind(&db) else {
            panic!("owner should be a function");
        };
        let function = function.node(&module);
        let modeled = parametrizations(&db, owner);
        let parametrizations = modeled.iter().collect::<Vec<_>>();

        assert_eq!(parametrizations.len(), 10);

        assert_eq!(parametrizations[0].argnames(), &StaticArgnames::Missing);
        assert!(matches!(
            parametrizations[0].argvalues(),
            ArgumentPresence::Present(_)
        ));
        assert_eq!(parametrizations[0].indirect(), &StaticIndirect::False);

        assert_eq!(parametrizations[1].argnames(), &StaticArgnames::Unknown);

        let scalar_decorator = parametrizations[2]
            .decorator()
            .resolve(&function.decorator_list)
            .expect("decorator handle resolves");
        let scalar_call = scalar_decorator
            .expression
            .as_call_expr()
            .expect("parametrize decorator is a call");
        let scalar_expression = scalar_call
            .arguments
            .find_argument_value("argnames", 0)
            .expect("argnames is present");
        let scalar_argvalues = scalar_call
            .arguments
            .find_argument_value("argvalues", 1)
            .expect("argvalues is present");
        let Some((form, names)) = parametrizations[2].argnames().known() else {
            panic!("argnames should be statically known");
        };
        assert_eq!(form, ArgnamesForm::ScalarString);
        assert_eq!(
            names.iter().map(StaticArgname::name).collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert!(
            names
                .iter()
                .all(|name| name.range() == scalar_expression.range())
        );

        let sequence_decorator = parametrizations[3]
            .decorator()
            .resolve(&function.decorator_list)
            .expect("decorator handle resolves");
        let sequence_call = sequence_decorator
            .expression
            .as_call_expr()
            .expect("parametrize decorator is a call");
        let sequence = sequence_call
            .arguments
            .find_argument_value("argnames", 0)
            .and_then(ast::Expr::as_tuple_expr)
            .expect("argnames is a tuple");
        let Some((form, names)) = parametrizations[3].argnames().known() else {
            panic!("argnames should be statically known");
        };
        assert_eq!(form, ArgnamesForm::Sequence);
        assert_eq!(names[0].range(), sequence.elts[0].range());
        assert_eq!(names[1].range(), sequence.elts[1].range());
        assert!(matches!(
            parametrizations[3].indirect(),
            StaticIndirect::Named(names) if names.len() == 1 && names[0].name() == "second"
        ));
        assert_eq!(
            parametrizations[3].indirect().is_indirect("first"),
            Some(false)
        );
        assert_eq!(
            parametrizations[3].indirect().is_indirect("second"),
            Some(true)
        );

        assert_eq!(parametrizations[4].indirect(), &StaticIndirect::False);
        assert_eq!(parametrizations[5].indirect(), &StaticIndirect::True);
        assert_eq!(parametrizations[6].indirect(), &StaticIndirect::Unknown);
        assert!(!parametrizations[7].has_unsupported_arguments());
        assert!(parametrizations[8].has_unsupported_arguments());
        assert_eq!(parametrizations[9].argvalues(), ArgumentPresence::Missing);

        assert_eq!(
            parametrizations[2].argvalues(),
            ArgumentPresence::Present(ExpressionNodeKey::from(scalar_argvalues))
        );
    }

    #[test]
    fn models_class_parametrizations() {
        let db = pytest_db(
            "/src/test_example.py",
            r#"
import pytest

ARGNAMES = ("first", "second")

@pytest.mark.parametrize(ARGNAMES, [(1, 2)])
class TestExample: ...
"#,
        );
        let owner = class_owner(&db, "/src/test_example.py", "TestExample");
        let modeled = parametrizations(&db, owner);
        let parametrization = modeled.iter().next().expect("parametrization is modeled");
        let Some((form, names)) = parametrization.argnames().known() else {
            panic!("argnames should be statically known");
        };

        assert_eq!(form, ArgnamesForm::Sequence);
        assert_eq!(
            names.iter().map(StaticArgname::name).collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    fn function_owner<'db>(db: &'db TestDb, path: &str, name: &str) -> Definition<'db> {
        let file = system_path_to_file(db, path).expect("test file exists");
        let file = db.program_file(file);
        let module = parsed_module(db, file.python_file(db)).load(db);
        let function = module
            .suite()
            .iter()
            .find_map(|statement| {
                statement
                    .as_function_def_stmt()
                    .filter(|function| function.name.as_str() == name)
            })
            .expect("test function exists");
        semantic_index(db, file).expect_single_definition(function)
    }

    fn class_owner<'db>(db: &'db TestDb, path: &str, name: &str) -> Definition<'db> {
        let file = system_path_to_file(db, path).expect("test file exists");
        let file = db.program_file(file);
        let module = parsed_module(db, file.python_file(db)).load(db);
        let class = module
            .suite()
            .iter()
            .find_map(|statement| {
                statement
                    .as_class_def_stmt()
                    .filter(|class| class.name.as_str() == name)
            })
            .expect("test class exists");
        semantic_index(db, file).expect_single_definition(class)
    }

    fn pytest_db(path: &'static str, source: &'static str) -> TestDb {
        TestDbBuilder::new()
            .with_third_party_packages()
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/__init__.pyi",
                "",
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/mark/__init__.pyi",
                "",
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/mark/structures.pyi",
                r#"
class MarkDecorator:
    def __call__(self, *args: object, **kwargs: object) -> object: ...

class _ParametrizeMarkDecorator(MarkDecorator): ...

class MarkGenerator:
    parametrize: _ParametrizeMarkDecorator
"#,
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/pytest/__init__.pyi",
                r#"
from _pytest.mark.structures import MarkGenerator

mark: MarkGenerator
"#,
            )
            .with_file(path, source)
            .build()
            .expect("valid pytest test database")
    }
}
