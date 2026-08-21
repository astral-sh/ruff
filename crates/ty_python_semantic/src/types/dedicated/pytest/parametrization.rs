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
    match owner.kind(db) {
        DefinitionKind::Function(function) => {
            let module = parsed_module(db, owner.python_file(db)).load(db);
            let decorators = &function.node(&module).decorator_list;
            if !has_call_decorator(decorators) {
                return Parametrizations::default();
            }

            let decorator_types = function_known_decorators(db, owner);
            parse_parametrizations(db, owner, decorators, |expression| {
                decorator_types.expression_type(expression)
            })
        }
        DefinitionKind::Class(class) => {
            let module = parsed_module(db, owner.python_file(db)).load(db);
            let decorators = &class.node(&module).decorator_list;
            if !has_call_decorator(decorators) {
                return Parametrizations::default();
            }

            parse_parametrizations(db, owner, decorators, |expression| {
                Some(definition_expression_type(db, owner, expression))
            })
        }
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
    const fn decorator(&self) -> DecoratorHandle {
        self.decorator
    }

    /// Returns the statically recovered `argnames`.
    pub(crate) const fn argnames(&self) -> &StaticArgnames {
        &self.argnames
    }

    /// Returns whether and where `argvalues` was supplied.
    const fn argvalues(&self) -> ArgumentPresence {
        self.argvalues
    }

    /// Returns the statically recovered `indirect` configuration.
    pub(crate) const fn indirect(&self) -> &StaticIndirect {
        &self.indirect
    }

    /// Returns whether arguments outside the supported pytest signature were supplied.
    const fn has_unsupported_arguments(&self) -> bool {
        self.has_unsupported_arguments
    }
}

/// Locates a decorator within the decorated function or class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct DecoratorHandle(usize);

impl DecoratorHandle {
    /// Resolves this handle against the owner's decorators.
    fn resolve(self, decorators: &[ast::Decorator]) -> Option<&ast::Decorator> {
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
    /// The argument count is known, but individual names may be unknown.
    Names {
        /// Whether pytest receives one string or an iterable of strings.
        form: ArgnamesForm,
        /// The names and their source ranges.
        names: Box<[StaticArgname]>,
    },
}

impl StaticArgnames {
    /// Returns the names in source order, including entries whose values are unknown.
    fn names(&self) -> Option<(ArgnamesForm, &[StaticArgname])> {
        match self {
            Self::Names { form, names } => Some((*form, names)),
            Self::Missing | Self::Unknown => None,
        }
    }

    /// Returns all statically known names, preserving an empty known collection.
    /// Returns `None` if any name is unknown.
    pub(crate) fn known(&self) -> Option<(ArgnamesForm, &[StaticArgname])> {
        let (form, names) = self.names()?;
        names
            .iter()
            .all(|name| name.name().is_some())
            .then_some((form, names))
    }
}

/// The source form of statically known pytest argument names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ArgnamesForm {
    /// A scalar string split on commas, with surrounding whitespace stripped from each name.
    ScalarString,
    /// A statically known fixed-length iterable.
    Sequence,
}

/// One pytest argument name and its source location, even when its value is unknown.
#[derive(Debug, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) struct StaticArgname {
    name: Option<Name>,
    range: TextRange,
}

impl StaticArgname {
    /// Returns the argument name when its value is statically known.
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the source range that supplied the name.
    const fn range(&self) -> TextRange {
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
            Self::Named(names) => {
                Some(names.iter().any(|candidate| candidate.name() == Some(name)))
            }
            Self::Unknown => None,
        }
    }
}

fn has_call_decorator(decorators: &[ast::Decorator]) -> bool {
    decorators
        .iter()
        .any(|decorator| decorator.expression.is_call_expr())
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
        Some((form, names)) => StaticArgnames::Names {
            form,
            names: names.into_boxed_slice(),
        },
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
        .filter(|(_, names)| names.iter().all(|name| name.name().is_some()))
        .map_or(StaticIndirect::Unknown, |(_, names)| {
            StaticIndirect::Named(names.into_boxed_slice())
        })
}

fn statically_known_names<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<(ArgnamesForm, Vec<StaticArgname>)> {
    let ty = expression_type(expression)?;
    statically_known_names_from_type(db, environment, expression, ty, expression_type)
}

fn statically_known_names_from_type<'db>(
    db: &'db dyn Db,
    environment: &ProgramEnvironment<'db>,
    expression: &ast::Expr,
    ty: Type<'db>,
    expression_type: &impl Fn(&ast::Expr) -> Option<Type<'db>>,
) -> Option<(ArgnamesForm, Vec<StaticArgname>)> {
    if let Some(string) = ty.as_string_literal() {
        let names = string
            .value(db)
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| StaticArgname {
                name: Some(Name::new(name)),
                range: expression.range(),
            })
            .collect();
        return Some((ArgnamesForm::ScalarString, names));
    }

    let names = fixed_length_elements(db, environment, expression, expression_type)?
        .into_iter()
        .map(|(ty, range)| StaticArgname {
            name: ty
                .as_string_literal()
                .map(|string| Name::new(string.value(db))),
            range,
        })
        .collect();
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
        // Inspect literal elements before their container type erases individual literal values.
        // Expanding fixed-length starred elements also preserves their source locations.
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
        ArgnamesForm, ArgumentPresence, Parametrization, StaticArgname, StaticArgnames,
        StaticIndirect, parametrizations,
    };
    use crate::Db as _;
    use crate::db::tests::{TestDb, TestDbBuilder};

    #[test]
    fn models_scalar_argnames() {
        let test = ParametrizationTest::new(
            r#"
import pytest

@pytest.mark.parametrize("first, second, third", [1])
def test_example(first, second, third): ...
"#,
        );

        assert_known_names(
            test.only("test_example").argnames(),
            ArgnamesForm::ScalarString,
            &["first", "second", "third"],
        );
    }

    #[test]
    fn models_class_parametrizations() {
        let test = ParametrizationTest::new(
            r#"
import pytest

ARGNAMES = ("first", "second")

@pytest.mark.parametrize(ARGNAMES, [(1, 2)])
class TestExample: ...
"#,
        );

        assert_known_names(
            test.only("TestExample").argnames(),
            ArgnamesForm::Sequence,
            &["first", "second"],
        );
    }

    #[test]
    fn distinguishes_missing_and_unknown_arguments() {
        let test = ParametrizationTest::new(
            r#"
import pytest

dynamic_names: str

@pytest.mark.parametrize(argvalues=[1])
def missing_names(first): ...

@pytest.mark.parametrize(dynamic_names, [1])
def unknown_names(first): ...

@pytest.mark.parametrize("first")
def missing_values(first): ...
"#,
        );

        assert_eq!(
            test.only("missing_names").argnames(),
            &StaticArgnames::Missing
        );
        assert!(matches!(
            test.only("missing_names").argvalues(),
            ArgumentPresence::Present(_)
        ));
        assert_eq!(
            test.only("unknown_names").argnames(),
            &StaticArgnames::Unknown
        );
        assert_eq!(
            test.only("missing_values").argvalues(),
            ArgumentPresence::Missing
        );
    }

    #[test]
    fn models_boolean_indirect_configuration() {
        let test = ParametrizationTest::new(
            r#"
import pytest

dynamic_indirect: bool

@pytest.mark.parametrize("first", [1])
def absent(first): ...

@pytest.mark.parametrize("first", [1], indirect=False)
def explicitly_false(first): ...

@pytest.mark.parametrize("first", [1], indirect=True)
def explicitly_true(first): ...

@pytest.mark.parametrize("first", [1], indirect=dynamic_indirect)
def unknown(first): ...
"#,
        );

        assert_eq!(test.only("absent").indirect(), &StaticIndirect::False);
        assert_eq!(
            test.only("explicitly_false").indirect(),
            &StaticIndirect::False
        );
        assert_eq!(
            test.only("explicitly_true").indirect(),
            &StaticIndirect::True
        );
        assert_eq!(test.only("unknown").indirect(), &StaticIndirect::Unknown);
    }

    #[test]
    fn models_named_indirect_configuration() {
        let test = ParametrizationTest::new(
            r#"
import pytest

@pytest.mark.parametrize(("first", "second"), [(1, 2)], indirect=("second",))
def test_example(first, second): ...
"#,
        );
        let parametrization = test.only("test_example");

        assert_known_names(
            parametrization.argnames(),
            ArgnamesForm::Sequence,
            &["first", "second"],
        );
        assert!(matches!(
            parametrization.indirect(),
            StaticIndirect::Named(names) if names.len() == 1 && names[0].name() == Some("second")
        ));
        assert_eq!(parametrization.indirect().is_indirect("first"), Some(false));
        assert_eq!(parametrization.indirect().is_indirect("second"), Some(true));
    }

    #[test]
    fn preserves_whitespace_within_scalar_names() {
        let test = ParametrizationTest::new(
            r#"
import pytest

NAMES = " first second , third\tfourth, , fifth , "

@pytest.mark.parametrize(NAMES, [])
def spaces_and_tabs(): ...

@pytest.mark.parametrize("first\nsecond", [])
def newline(): ...
"#,
        );

        assert_known_names(
            test.only("spaces_and_tabs").argnames(),
            ArgnamesForm::ScalarString,
            &["first second", "third\tfourth", "fifth"],
        );
        assert_known_names(
            test.only("newline").argnames(),
            ArgnamesForm::ScalarString,
            &["first\nsecond"],
        );
    }

    #[test]
    fn preserves_sequence_names_verbatim() {
        let test = ParametrizationTest::new(
            r#"
import pytest

@pytest.mark.parametrize([" first ", "second,third", ""], [])
def test_example(): ...
"#,
        );

        assert_known_names(
            test.only("test_example").argnames(),
            ArgnamesForm::Sequence,
            &[" first ", "second,third", ""],
        );
    }

    #[test]
    fn keeps_empty_names_known() {
        let test = ParametrizationTest::new(
            r#"
import pytest

@pytest.mark.parametrize(" , \t, ", [])
def scalar(): ...

@pytest.mark.parametrize([], [])
def sequence(): ...
"#,
        );

        assert_known_names(
            test.only("scalar").argnames(),
            ArgnamesForm::ScalarString,
            &[],
        );
        assert_known_names(
            test.only("sequence").argnames(),
            ArgnamesForm::Sequence,
            &[],
        );
    }

    #[test]
    fn preserves_partially_known_names_and_ranges() {
        let test = ParametrizationTest::new(
            r#"
import pytest

dynamic_name: str

@pytest.mark.parametrize(["request", dynamic_name, "last"], [])
def list_names(): ...

@pytest.mark.parametrize(("request", dynamic_name, "last"), [])
def tuple_names(): ...

@pytest.mark.parametrize([*("request", dynamic_name), "last"], [])
def unpacked_names(): ...

@pytest.mark.parametrize(["request", dynamic_name, "last"], [])
class TestExample: ...
"#,
        );

        for owner in ["list_names", "tuple_names", "unpacked_names", "TestExample"] {
            let argnames = test.only(owner).argnames();
            test.assert_name_sources(
                argnames,
                ArgnamesForm::Sequence,
                &[
                    (Some("request"), r#""request""#),
                    (None, "dynamic_name"),
                    (Some("last"), r#""last""#),
                ],
            );
            assert!(argnames.known().is_none(), "{owner} has an unknown name");
        }
    }

    #[test]
    fn preserves_partially_known_names_from_a_binding() {
        let test = ParametrizationTest::new(
            r#"
import pytest

dynamic_name: str
NAMES = ("request", dynamic_name)

@pytest.mark.parametrize(NAMES, [])
def test_example(): ...
"#,
        );
        let argnames = test.only("test_example").argnames();

        test.assert_name_sources(
            argnames,
            ArgnamesForm::Sequence,
            &[(Some("request"), "NAMES"), (None, "NAMES")],
        );
        assert!(argnames.known().is_none());
    }

    #[test]
    fn keeps_unknown_lengths_uncertain() {
        let test = ParametrizationTest::new(
            r#"
import pytest

dynamic_names: list[str]

@pytest.mark.parametrize(["request", *dynamic_names], [])
def test_example(): ...
"#,
        );

        assert_eq!(
            test.only("test_example").argnames(),
            &StaticArgnames::Unknown
        );
    }

    #[test]
    fn keeps_partially_known_indirect_names_uncertain() {
        let test = ParametrizationTest::new(
            r#"
import pytest

dynamic_name: str

@pytest.mark.parametrize("request", [], indirect=["request", dynamic_name])
def test_example(): ...
"#,
        );
        let indirect = test.only("test_example").indirect();

        assert_eq!(indirect, &StaticIndirect::Unknown);
        assert_eq!(indirect.is_indirect("request"), None);
    }

    #[test]
    fn flags_unsupported_arguments_without_discarding_marks() {
        let test = ParametrizationTest::new(
            r#"
import pytest

arguments = ("value", [1])
options = {"indirect": True}

@pytest.mark.parametrize("value", [1], False, None, "function")
def supported_positional(value): ...

@pytest.mark.parametrize("value", [1], ids=None, scope="function")
def supported_keywords(value): ...

@pytest.mark.parametrize("value", [1], False, None, "function", True)
def extra_positional(value): ...

@pytest.mark.parametrize("value", [1], unexpected=True)
def extra_keyword(value): ...

@pytest.mark.parametrize("value", [1], argnames="other")
def duplicate_argument(value): ...

@pytest.mark.parametrize(*arguments)
def unpacked_positional(value): ...

@pytest.mark.parametrize("value", [1], **options)
def unpacked_keywords(value): ...
"#,
        );

        assert!(
            !test
                .only("supported_positional")
                .has_unsupported_arguments()
        );
        assert!(!test.only("supported_keywords").has_unsupported_arguments());
        assert!(test.only("extra_positional").has_unsupported_arguments());
        assert!(test.only("extra_keyword").has_unsupported_arguments());
        assert!(test.only("duplicate_argument").has_unsupported_arguments());
        assert!(test.only("unpacked_positional").has_unsupported_arguments());
        assert!(test.only("unpacked_keywords").has_unsupported_arguments());
    }

    #[test]
    fn preserves_decorator_order_and_provenance() {
        let test = ParametrizationTest::new(
            r#"
import pytest

def unrelated(function): ...

@unrelated
@pytest.mark.parametrize("first, second", [(1, 2)])
@unrelated()
@pytest.mark.parametrize(("third", "fourth"), [(3, 4)])
def test_example(first, second, third, fourth): ...
"#,
        );

        test.assert_function_provenance("test_example", &[1, 3]);
    }

    struct ParametrizationTest {
        db: TestDb,
        source: &'static str,
    }

    impl ParametrizationTest {
        const PATH: &str = "/src/test_example.py";

        fn new(source: &'static str) -> Self {
            Self {
                db: pytest_db(Self::PATH, source),
                source,
            }
        }

        #[track_caller]
        fn only(&self, owner_name: &str) -> &Parametrization {
            let mut modeled = parametrizations(&self.db, self.owner(owner_name)).iter();
            let only = modeled.next().expect("owner has a parametrization");
            assert!(
                modeled.next().is_none(),
                "expected one parametrization on {owner_name}"
            );
            only
        }

        #[track_caller]
        fn assert_name_sources(
            &self,
            argnames: &StaticArgnames,
            expected_form: ArgnamesForm,
            expected: &[(Option<&str>, &str)],
        ) {
            let (form, names) = argnames.names().expect("argument count is known");
            assert_eq!(form, expected_form);
            assert_eq!(
                names
                    .iter()
                    .map(|name| (name.name(), &self.source[name.range()]))
                    .collect::<Vec<_>>(),
                expected
            );
        }

        /// Checks handles and exact argument locations against independently selected AST nodes.
        #[track_caller]
        fn assert_function_provenance(&self, owner_name: &str, decorator_indices: &[usize]) {
            let owner = self.owner(owner_name);
            let module = parsed_module(&self.db, owner.python_file(&self.db)).load(&self.db);
            let DefinitionKind::Function(function) = owner.kind(&self.db) else {
                panic!("provenance owner should be a function");
            };
            let decorators = &function.node(&module).decorator_list;
            let modeled = parametrizations(&self.db, owner);
            assert_eq!(modeled.iter().count(), decorator_indices.len());

            for (parametrization, &index) in modeled.iter().zip(decorator_indices) {
                let expected = &decorators[index];
                let resolved = parametrization
                    .decorator()
                    .resolve(decorators)
                    .expect("valid handle");
                assert_eq!(resolved.range(), expected.range());

                let call = expected
                    .expression
                    .as_call_expr()
                    .expect("decorator is a call");
                let argnames = call
                    .arguments
                    .find_argument_value("argnames", 0)
                    .expect("argnames exists");
                let (_, names) = parametrization.argnames().known().expect("names are known");
                let expected_ranges = match argnames {
                    ast::Expr::Tuple(tuple) => tuple.elts.iter().map(Ranged::range).collect(),
                    _ => vec![argnames.range(); names.len()],
                };
                assert_eq!(
                    names.iter().map(StaticArgname::range).collect::<Vec<_>>(),
                    expected_ranges
                );

                let argvalues = call
                    .arguments
                    .find_argument_value("argvalues", 1)
                    .expect("argvalues exists");
                assert_eq!(
                    parametrization.argvalues(),
                    ArgumentPresence::Present(ExpressionNodeKey::from(argvalues))
                );
            }
        }

        fn owner(&self, name: &str) -> Definition<'_> {
            let file = system_path_to_file(&self.db, Self::PATH).expect("test file exists");
            let file = self.db.program_file(file);
            let module = parsed_module(&self.db, file.python_file(&self.db)).load(&self.db);
            let index = semantic_index(&self.db, file);
            module
                .suite()
                .iter()
                .find_map(|statement| match statement {
                    ast::Stmt::FunctionDef(function) if function.name.as_str() == name => {
                        Some(index.expect_single_definition(function))
                    }
                    ast::Stmt::ClassDef(class) if class.name.as_str() == name => {
                        Some(index.expect_single_definition(class))
                    }
                    _ => None,
                })
                .expect("test function or class exists")
        }
    }

    #[track_caller]
    fn assert_known_names(
        argnames: &StaticArgnames,
        expected_form: ArgnamesForm,
        expected: &[&str],
    ) {
        let (form, names) = argnames.known().expect("all argument names are known");
        assert_eq!(form, expected_form);
        assert_eq!(
            names.iter().map(StaticArgname::name).collect::<Vec<_>>(),
            expected.iter().copied().map(Some).collect::<Vec<_>>()
        );
    }

    fn pytest_db(path: &'static str, source: &'static str) -> TestDb {
        TestDbBuilder::new()
            .with_third_party_packages()
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/__init__.pyi",
                "
",
            )
            .with_file(
                "/.venv/lib/python3.13/site-packages/_pytest/mark/__init__.pyi",
                "
",
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
