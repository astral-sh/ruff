#![cfg(feature = "ext-lint")]
#![cfg_attr(not(test), allow(dead_code))]

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyModule, PyString, PyTuple};
use pyo3::{Bound, PyObject};

use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{ArgOrKeyword, Expr, ExprCall, Stmt};
#[cfg(test)]
use ruff_text_size::TextSize;
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::external::ast::target::{ExprKind, StmtKind};

pub(crate) fn span_tuple(py: Python<'_>, range: TextRange) -> PyResult<PyObject> {
    Ok(
        PyTuple::new(py, [range.start().to_u32(), range.end().to_u32()])?
            .into_any()
            .unbind(),
    )
}

#[derive(Debug)]
pub(crate) struct ProjectionTypes {
    pub node: Py<PyAny>,
    pub call_argument: Py<PyAny>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ModuleTypes {
    pub context: Py<PyAny>,
    pub projection: ProjectionTypes,
}

pub(crate) fn load_module_types(py: Python<'_>) -> PyResult<ModuleTypes> {
    let module = PyModule::new(py, "ruff_external")?;
    module.setattr("__file__", "ruff_external/__init__.py")?;
    ruff_external(py, &module)?;
    let sys = PyModule::import(py, "sys")?;
    let modules_obj = sys.getattr("modules")?;
    let modules = modules_obj.downcast::<PyDict>()?;
    modules.set_item("ruff_external", &module)?;

    let context = module.getattr("Context")?.unbind();
    let node = module.getattr("Node")?.unbind();
    let call_argument = module.getattr("CallArgument")?.unbind();

    Ok(ModuleTypes {
        context,
        projection: ProjectionTypes {
            node,
            call_argument,
        },
    })
}

pub(crate) fn expr_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    expr: &Expr,
    types: &ProjectionTypes,
) -> PyResult<PyObject> {
    let kind = ExprKind::from(expr);
    let range = expr.range();
    let text = locator.slice(range);
    let repr = format!("{expr:?}");

    let mut callee = None;
    let mut function_text = None;
    let mut function_kind = None;
    let mut arguments = Vec::new();

    if let Expr::Call(call) = expr {
        callee = extract_callee(locator, expr, call);
        function_text = Some(locator.slice(call.func.range()).trim().to_string());
        function_kind = Some(ExprKind::from(call.func.as_ref()).as_str().to_owned());
        arguments = call_arguments_to_python(py, locator, call, types)?;
    }

    make_node(
        py,
        types,
        kind.as_str(),
        range,
        text,
        repr,
        callee.as_deref(),
        function_text.as_deref(),
        function_kind.as_deref(),
        arguments,
    )
}

pub(crate) fn stmt_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    stmt: &Stmt,
    types: &ProjectionTypes,
) -> PyResult<PyObject> {
    let kind = StmtKind::from(stmt);
    let range = stmt.range();
    let text = locator.slice(range);
    let repr = format!("{stmt:?}");
    make_node(
        py,
        types,
        kind.as_str(),
        range,
        text,
        repr,
        None,
        None,
        None,
        Vec::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn make_node(
    py: Python<'_>,
    types: &ProjectionTypes,
    kind: &str,
    range: TextRange,
    text: &str,
    repr: String,
    callee: Option<&str>,
    function_text: Option<&str>,
    function_kind: Option<&str>,
    arguments: Vec<PyObject>,
) -> PyResult<PyObject> {
    let arguments_tuple = PyTuple::new(py, arguments)?.into_any().unbind();
    types.node.call1(
        py,
        (
            kind,
            span_tuple(py, range)?,
            text,
            repr,
            optional_str(py, callee),
            optional_str(py, function_text),
            optional_str(py, function_kind),
            arguments_tuple,
        ),
    )
}

fn call_arguments_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    call: &ExprCall,
    types: &ProjectionTypes,
) -> PyResult<Vec<PyObject>> {
    let mut arguments = Vec::with_capacity(call.arguments.len());

    for argument in call.arguments.args.iter().map(ArgOrKeyword::from) {
        arguments.push(call_argument_to_python(py, locator, argument, types)?);
    }

    for keyword in call.arguments.keywords.iter().map(ArgOrKeyword::from) {
        arguments.push(call_argument_to_python(py, locator, keyword, types)?);
    }

    Ok(arguments)
}

fn call_argument_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    argument: ArgOrKeyword,
    types: &ProjectionTypes,
) -> PyResult<PyObject> {
    match argument {
        ArgOrKeyword::Arg(expr) => build_argument(
            py,
            locator,
            types,
            "positional",
            matches!(expr, Expr::Starred(_)),
            None,
            expr,
        ),
        ArgOrKeyword::Keyword(keyword) => {
            let is_unpack = keyword.arg.is_none();
            let name = keyword
                .arg
                .as_ref()
                .map(ruff_python_ast::Identifier::as_str);
            build_argument(
                py,
                locator,
                types,
                "keyword",
                is_unpack,
                name,
                &keyword.value,
            )
        }
    }
}

fn build_argument(
    py: Python<'_>,
    locator: &Locator<'_>,
    types: &ProjectionTypes,
    kind: &str,
    is_unpack: bool,
    name: Option<&str>,
    expr: &Expr,
) -> PyResult<PyObject> {
    let binop_operator = expr_as_binop_operator(expr);
    let call_function_text = expr_as_call_function_text(locator, expr);
    types.call_argument.call1(
        py,
        (
            kind,
            is_unpack,
            span_tuple(py, expr.range())?,
            ExprKind::from(expr).as_str(),
            matches!(expr, Expr::StringLiteral(_)),
            matches!(expr, Expr::FString(_)),
            optional_str(py, binop_operator.as_deref()),
            optional_str(py, call_function_text.as_deref()),
            optional_str(py, name),
        ),
    )
}

fn optional_str(py: Python<'_>, value: Option<&str>) -> PyObject {
    match value {
        Some(value) => py_string(py, value),
        None => py_none(py),
    }
}

fn py_string(py: Python<'_>, value: &str) -> PyObject {
    PyString::new(py, value).into_any().unbind()
}

fn py_bool(py: Python<'_>, value: bool) -> PyObject {
    PyBool::new(py, value).to_owned().into_any().unbind()
}

fn py_none(py: Python<'_>) -> PyObject {
    py.None()
}

fn expr_as_binop_operator(expr: &Expr) -> Option<String> {
    if let Expr::BinOp(bin_op) = expr {
        Some(bin_op.op.as_str().to_string())
    } else {
        None
    }
}

fn expr_as_call_function_text(locator: &Locator<'_>, expr: &Expr) -> Option<String> {
    if let Expr::Call(call) = expr {
        Some(locator.slice(call.func.range()).trim().to_string())
    } else {
        None
    }
}

fn extract_callee(locator: &Locator<'_>, expr: &Expr, call: &ExprCall) -> Option<String> {
    UnqualifiedName::from_expr(call.func.as_ref())
        .map(|name| name.to_string())
        .or_else(|| {
            let text = locator.slice(expr.range());
            let trimmed = text.trim_start();
            trimmed
                .find('(')
                .map(|index| trimmed[..index].trim())
                .filter(|callee| !callee.is_empty())
                .map(ToOwned::to_owned)
        })
}

#[pymodule(gil_used = false)]
pub(crate) fn ruff_external(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<bindings::Node>()?;
    module.add_class::<bindings::CallArgument>()?;
    module.add_class::<bindings::Context>()?;
    let exports = PyTuple::new(module.py(), ["Context", "Node", "CallArgument"])?;
    module.add("__all__", exports)?;
    Ok(())
}

mod bindings {
    #![allow(clippy::used_underscore_binding)]

    use super::{optional_str, py_bool, py_none, py_string};
    use pyo3::exceptions::PyKeyError;
    use pyo3::prelude::*;
    use pyo3::types::{PyAnyMethods, PyTuple};

    #[pyclass(module = "ruff_external", unsendable)]
    pub(crate) struct Node {
        #[pyo3(get)]
        _kind: String,
        #[pyo3(get)]
        _span: PyObject,
        #[pyo3(get)]
        _text: String,
        #[pyo3(get)]
        _repr: String,
        #[pyo3(get)]
        _callee: Option<String>,
        #[pyo3(get)]
        function_text: Option<String>,
        #[pyo3(get)]
        function_kind: Option<String>,
        #[pyo3(get)]
        arguments: PyObject,
    }

    #[pymethods]
    impl Node {
        #[new]
        #[allow(clippy::too_many_arguments)]
        #[pyo3(
            signature = (
                kind,
                span,
                text,
                repr_value,
                callee=None,
                function_text=None,
                function_kind=None,
                arguments=None
            )
        )]
        fn new(
            py: Python<'_>,
            kind: String,
            span: PyObject,
            text: String,
            repr_value: String,
            callee: Option<String>,
            function_text: Option<String>,
            function_kind: Option<String>,
            arguments: Option<PyObject>,
        ) -> Self {
            let arguments = arguments.unwrap_or_else(|| PyTuple::empty(py).into_any().unbind());
            Self {
                _kind: kind,
                _span: span,
                _text: text,
                _repr: repr_value,
                _callee: callee,
                function_text,
                function_kind,
                arguments,
            }
        }

        fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
            let span_repr: String = self._span.bind(py).repr()?.extract()?;
            Ok(format!(
                "Node(kind={:?}, span={}, callee={:?}, function_text={:?})",
                self._kind, span_repr, self._callee, self.function_text
            ))
        }

        fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
            attribute_lookup(py, key, self).ok_or_else(|| PyKeyError::new_err(key.to_string()))
        }

        #[pyo3(signature = (key, default=None))]
        fn get(&self, py: Python<'_>, key: &str, default: Option<PyObject>) -> PyObject {
            attribute_lookup(py, key, self)
                .unwrap_or_else(|| default.unwrap_or_else(|| py_none(py)))
        }
    }

    fn attribute_lookup(py: Python<'_>, key: &str, node: &Node) -> Option<PyObject> {
        match key {
            "_kind" => Some(py_string(py, &node._kind)),
            "_span" => Some(node._span.clone_ref(py)),
            "_text" => Some(py_string(py, &node._text)),
            "_repr" => Some(py_string(py, &node._repr)),
            "_callee" => Some(optional_str(py, node._callee.as_deref())),
            "function_text" => Some(optional_str(py, node.function_text.as_deref())),
            "function_kind" => Some(optional_str(py, node.function_kind.as_deref())),
            "arguments" => Some(node.arguments.clone_ref(py)),
            _ => None,
        }
    }

    #[pyclass(module = "ruff_external", unsendable)]
    pub(crate) struct CallArgument {
        #[pyo3(get)]
        kind: String,
        #[pyo3(get)]
        is_unpack: bool,
        #[pyo3(get)]
        name: Option<String>,
        #[pyo3(get)]
        span: PyObject,
        #[pyo3(get)]
        expr_kind: String,
        #[pyo3(get)]
        is_string_literal: bool,
        #[pyo3(get)]
        is_fstring: bool,
        #[pyo3(get)]
        binop_operator: Option<String>,
        #[pyo3(get)]
        call_function_text: Option<String>,
    }

    #[pymethods]
    impl CallArgument {
        #[new]
        #[allow(clippy::too_many_arguments)]
        #[pyo3(
            signature = (
                kind,
                is_unpack,
                span,
                expr_kind,
                is_string_literal,
                is_fstring,
                binop_operator=None,
                call_function_text=None,
                name=None
            )
        )]
        fn new(
            kind: String,
            is_unpack: bool,
            span: PyObject,
            expr_kind: String,
            is_string_literal: bool,
            is_fstring: bool,
            binop_operator: Option<String>,
            call_function_text: Option<String>,
            name: Option<String>,
        ) -> Self {
            Self {
                kind,
                is_unpack,
                name,
                span,
                expr_kind,
                is_string_literal,
                is_fstring,
                binop_operator,
                call_function_text,
            }
        }

        fn __repr__(&self) -> String {
            format!(
                "CallArgument(kind={:?}, name={:?}, expr_kind={:?})",
                self.kind, self.name, self.expr_kind
            )
        }

        fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
            argument_lookup(py, key, self).ok_or_else(|| PyKeyError::new_err(key.to_string()))
        }

        #[pyo3(signature = (key, default=None))]
        fn get(&self, py: Python<'_>, key: &str, default: Option<PyObject>) -> PyObject {
            argument_lookup(py, key, self).unwrap_or_else(|| default.unwrap_or_else(|| py_none(py)))
        }
    }

    fn argument_lookup(py: Python<'_>, key: &str, arg: &CallArgument) -> Option<PyObject> {
        match key {
            "kind" => Some(py_string(py, &arg.kind)),
            "is_unpack" => Some(py_bool(py, arg.is_unpack)),
            "name" => Some(optional_str(py, arg.name.as_deref())),
            "span" => Some(arg.span.clone_ref(py)),
            "expr_kind" => Some(py_string(py, &arg.expr_kind)),
            "is_string_literal" => Some(py_bool(py, arg.is_string_literal)),
            "is_fstring" => Some(py_bool(py, arg.is_fstring)),
            "binop_operator" => Some(optional_str(py, arg.binop_operator.as_deref())),
            "call_function_text" => Some(optional_str(py, arg.call_function_text.as_deref())),
            _ => None,
        }
    }

    #[pyclass(module = "ruff_external", unsendable)]
    pub(crate) struct Context {
        #[pyo3(get)]
        code: String,
        #[pyo3(get)]
        name: String,
        #[pyo3(get)]
        _report: Py<PyAny>,
    }

    #[pymethods]
    impl Context {
        #[new]
        fn new(code: String, name: String, reporter: Py<PyAny>) -> Self {
            Self {
                code,
                name,
                _report: reporter,
            }
        }

        #[pyo3(signature = (message, span=None))]
        fn report(&self, py: Python<'_>, message: &str, span: Option<(u32, u32)>) -> PyResult<()> {
            self._report.bind(py).call1((message, span))?;
            Ok(())
        }

        fn __repr__(&self) -> String {
            format!("Context(code={:?}, name={:?})", self.code, self.name)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use pyo3::types::{PyAnyMethods, PyTuple as PyTupleType};

    #[test]
    fn call_projection_includes_callee() -> anyhow::Result<()> {
        pyo3::prepare_freethreaded_python();
        let source = "logging.info('x')";
        let expr = {
            let parsed = ruff_python_parser::parse_expression(source)?;
            *parsed.into_syntax().body
        };

        let locator = Locator::new(source);
        Python::with_gil(|py| {
            let module_types = load_module_types(py)?;
            let node = expr_to_python(py, &locator, &expr, &module_types.projection)?;
            let node = node.bind(py);
            let callee: String = node.getattr("_callee")?.extract()?;
            assert_eq!(callee, "logging.info");
            Ok(())
        })
    }

    #[test]
    fn call_projection_includes_arguments() -> anyhow::Result<()> {
        pyo3::prepare_freethreaded_python();
        let source = "logging.info('static', msg='template {}'.format(value))";
        let expr = {
            let parsed = ruff_python_parser::parse_expression(source)?;
            *parsed.into_syntax().body
        };

        let locator = Locator::new(source);
        Python::with_gil(|py| {
            let module_types = load_module_types(py)?;
            let node = expr_to_python(py, &locator, &expr, &module_types.projection)?;
            let node = node.bind(py);

            let function_text: String = node.getattr("function_text")?.extract()?;
            assert_eq!(function_text, "logging.info");

            let arguments = node.getattr("arguments")?;
            let arguments = arguments
                .downcast::<PyTupleType>()
                .map_err(|err| anyhow!(err.to_string()))?;
            assert_eq!(arguments.len(), 2);

            let first = arguments.get_item(0)?;
            let first_kind: String = first.getattr("kind")?.extract()?;
            assert_eq!(first_kind, "positional");
            let first_is_string: bool = first.getattr("is_string_literal")?.extract()?;
            assert!(first_is_string);

            let second = arguments.get_item(1)?;
            let second_kind: String = second.getattr("kind")?.extract()?;
            assert_eq!(second_kind, "keyword");
            let name = second.getattr("name")?;
            if !name.is_none() {
                let name: String = name.extract()?;
                assert_eq!(name, "msg");
            } else {
                panic!("expected keyword argument to have a name");
            }

            let call_function_text: String = second.getattr("call_function_text")?.extract()?;
            assert!(
                call_function_text.ends_with(".format"),
                "unexpected call_function_text: {call_function_text}"
            );

            Ok(())
        })
    }

    #[test]
    fn stmt_projection_reports_kind() -> anyhow::Result<()> {
        pyo3::prepare_freethreaded_python();
        let source = "pass\n";
        let stmt = ruff_python_ast::Stmt::Pass(ruff_python_ast::StmtPass {
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            range: TextRange::new(TextSize::new(0), TextSize::new(4)),
        });

        let locator = Locator::new(source);
        Python::with_gil(|py| {
            let module_types = load_module_types(py)?;
            let node = stmt_to_python(py, &locator, &stmt, &module_types.projection)?;
            let node = node.bind(py);
            let kind: String = node.getattr("_kind")?.extract()?;
            assert_eq!(kind, "Pass");
            Ok(())
        })
    }
}
