#![cfg(feature = "ext-lint")]
#![cfg_attr(not(test), allow(dead_code))]

use self::generated::GENERATED_EXPORTS;
use self::projection::{ProjectionMode, project_typed_node};
use self::store::{AstStoreHandle, current_store};
use crate::Locator;
use crate::external::ast::target::{ExprKind, StmtKind};
use pyo3::IntoPyObject;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyModule, PyString, PyTuple};
use pyo3::{Bound, PyClassInitializer, PyObject};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{AnyNodeRef, Expr, ExprCall, HasNodeIndex, Stmt};
use ruff_text_size::{Ranged, TextRange};

pub(crate) fn span_tuple(py: Python<'_>, range: TextRange) -> PyResult<PyObject> {
    Ok(
        PyTuple::new(py, [range.start().to_u32(), range.end().to_u32()])?
            .into_any()
            .unbind(),
    )
}

#[derive(Debug)]
pub(crate) struct ProjectionTypes;

pub(crate) static PROJECTION_TYPES: ProjectionTypes = ProjectionTypes;

pub(crate) type ProjectionTypesRef = &'static ProjectionTypes;

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ModuleTypes {
    pub context: Py<PyAny>,
    pub projection: ProjectionTypesRef,
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

    let projection = &PROJECTION_TYPES;

    Ok(ModuleTypes {
        context,
        projection,
    })
}

pub(crate) fn expr_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    expr: &Expr,
    types: ProjectionTypesRef,
) -> PyResult<PyObject> {
    project_or_raw(py, locator, AnyNodeRef::from(expr), types, || {
        let store = current_store();
        let node_id = ensure_node_id(expr, &store);
        let kind = ExprKind::from(expr).as_str().to_string();
        let range = expr.range();
        let text = locator.slice(range);
        let repr = format!("{expr:?}");
        build_raw_node(py, kind, range, text.to_string(), repr, node_id, store)
    })
}

pub(crate) fn stmt_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    stmt: &Stmt,
    types: ProjectionTypesRef,
) -> PyResult<PyObject> {
    project_or_raw(py, locator, AnyNodeRef::from(stmt), types, || {
        let store = current_store();
        let node_id = ensure_node_id(stmt, &store);
        let kind = StmtKind::from(stmt).as_str().to_string();
        let range = stmt.range();
        let text = locator.slice(range);
        let repr = format!("{stmt:?}");
        build_raw_node(py, kind, range, text.to_string(), repr, node_id, store)
    })
}

pub(crate) fn node_to_python(
    py: Python<'_>,
    locator: &Locator<'_>,
    node: AnyNodeRef<'_>,
    types: ProjectionTypesRef,
) -> PyResult<PyObject> {
    project_or_raw(py, locator, node, types, || {
        let store = current_store();
        let node_id = ensure_node_id(&node, &store);
        let range = node.range();
        let text = locator.slice(range).to_string();
        let repr = format!("{node:?}");
        let kind = format!("{:?}", node.kind());

        build_raw_node(py, kind, range, text, repr, node_id, store)
    })
}

fn build_raw_node(
    py: Python<'_>,
    kind: String,
    range: TextRange,
    text: String,
    repr: String,
    node_id: u32,
    store: AstStoreHandle,
) -> PyResult<PyObject> {
    RawNode::new_instance(py, kind, span_tuple(py, range)?, text, repr, node_id, store)
}

fn project_or_raw<F>(
    py: Python<'_>,
    locator: &Locator<'_>,
    node: AnyNodeRef<'_>,
    types: ProjectionTypesRef,
    fallback: F,
) -> PyResult<PyObject>
where
    F: FnOnce() -> PyResult<PyObject>,
{
    if let Some(typed) = project_typed_node(py, locator, node, ProjectionMode::Typed, types)? {
        return Ok(typed);
    }

    fallback()
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

fn py_int(py: Python<'_>, value: u32) -> PyObject {
    value
        .into_pyobject(py)
        .expect("u32 to PyObject")
        .into_any()
        .unbind()
}

fn py_none(py: Python<'_>) -> PyObject {
    py.None()
}

fn ensure_node_id(node: &impl HasNodeIndex, store: &AstStoreHandle) -> u32 {
    store.assign_id(node.node_index())
}

fn extract_callee(locator: &Locator<'_>, range: TextRange, call: &ExprCall) -> Option<String> {
    UnqualifiedName::from_expr(call.func.as_ref())
        .map(|name| name.to_string())
        .or_else(|| {
            let text = locator.slice(range);
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
    module.add_class::<RawNode>()?;
    module.add_class::<bindings::Context>()?;
    generated::add_generated_classes(module)?;
    let mut exports = vec!["Context", "Node", "RawNode"];
    exports.extend_from_slice(GENERATED_EXPORTS);
    let exports = PyTuple::new(module.py(), exports)?;
    module.add("__all__", exports)?;
    Ok(())
}

mod generated;
mod projection;
pub(crate) mod source;
pub(crate) mod store;

#[pyclass(module = "ruff_external", extends = bindings::Node, unsendable)]
pub(crate) struct RawNode;

#[pymethods]
impl RawNode {}

impl RawNode {
    fn new_instance(
        py: Python<'_>,
        kind: String,
        span: PyObject,
        text: String,
        repr_value: String,
        node_id: u32,
        store: AstStoreHandle,
    ) -> PyResult<PyObject> {
        let node = bindings::Node::new_inner(py, kind, span, text, repr_value, node_id, store);
        let initializer = PyClassInitializer::from(node).add_subclass(RawNode);
        Ok(Py::new(py, initializer)?.into_any())
    }
}

mod bindings {
    #![allow(clippy::used_underscore_binding)]

    use super::store::AstStoreHandle;
    use super::{py_int, py_none, py_string};
    use pyo3::exceptions::PyKeyError;
    use pyo3::prelude::*;
    use pyo3::types::PyAnyMethods;

    #[pyclass(module = "ruff_external", unsendable, subclass)]
    pub(crate) struct Node {
        #[pyo3(get)]
        _kind: String,
        #[pyo3(get)]
        _span: PyObject,
        #[pyo3(get)]
        _text: String,
        #[pyo3(get)]
        _repr: String,
        #[pyo3(get, name = "node_id")]
        py_id: u32,
        #[allow(dead_code)]
        store: AstStoreHandle,
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
            node_id,
        )
        )]
        fn new(
            _py: Python<'_>,
            kind: String,
            span: PyObject,
            text: String,
            repr_value: String,
            node_id: u32,
        ) -> Self {
            Self::new_inner(
                _py,
                kind,
                span,
                text,
                repr_value,
                node_id,
                AstStoreHandle::new(),
            )
        }

        fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
            let span_repr: String = self._span.bind(py).repr()?.extract()?;
            Ok(format!("Node(kind={:?}, span={})", self._kind, span_repr))
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

    impl Node {
        pub(crate) fn new_inner(
            _py: Python<'_>,
            kind: String,
            span: PyObject,
            text: String,
            repr_value: String,
            node_id: u32,
            store: AstStoreHandle,
        ) -> Self {
            Self {
                _kind: kind,
                _span: span,
                _text: text,
                _repr: repr_value,
                py_id: node_id,
                store,
            }
        }

        pub(crate) const fn store(&self) -> &AstStoreHandle {
            &self.store
        }

        pub(crate) const fn node_id(&self) -> u32 {
            self.py_id
        }
    }

    fn attribute_lookup(py: Python<'_>, key: &str, node: &Node) -> Option<PyObject> {
        match key {
            "_kind" => Some(py_string(py, &node._kind)),
            "_span" => Some(node._span.clone_ref(py)),
            "_text" => Some(py_string(py, &node._text)),
            "_repr" => Some(py_string(py, &node._repr)),
            "node_id" => Some(py_int(py, node.py_id)),
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
        config: PyObject,
        #[pyo3(get)]
        _report: Py<PyAny>,
    }

    #[pymethods]
    impl Context {
        #[new]
        fn new(code: String, name: String, config: PyObject, reporter: Py<PyAny>) -> Self {
            Self {
                code,
                name,
                config,
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
    use super::source::with_source_file;
    use super::store::{AstStoreHandle, with_store};
    use super::*;
    use anyhow::anyhow;
    use pyo3::types::{PyAnyMethods, PyTuple as PyTupleType, PyTuple};
    use ruff_python_ast::{Expr, Stmt};
    use ruff_python_parser::{parse_expression, parse_module};
    use ruff_source_file::SourceFileBuilder;
    use ruff_text_size::{TextRange, TextSize};

    fn with_python_fixture<R>(
        source: &str,
        f: impl FnOnce(Python<'_>, ProjectionTypesRef, &Locator<'_>) -> R,
    ) -> R {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let module_types = load_module_types(py).expect("module types");
            let projection = module_types.projection;
            let locator = Locator::new(source);
            let source_file = SourceFileBuilder::new("test.py", source).finish();
            with_store(AstStoreHandle::new(), || {
                with_source_file(&source_file, || f(py, projection, &locator))
            })
        })
    }

    fn with_python_fixture_result<R>(
        source: &str,
        f: impl FnOnce(Python<'_>, ProjectionTypesRef, &Locator<'_>) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let module_types = load_module_types(py).expect("module types");
            let projection = module_types.projection;
            let locator = Locator::new(source);
            let source_file = SourceFileBuilder::new("test.py", source).finish();
            with_store(AstStoreHandle::new(), || {
                with_source_file(&source_file, || f(py, projection, &locator))
            })
        })
    }

    #[test]
    fn call_expr_arguments_are_cached() {
        with_python_fixture("foo(value)", |py, projection, locator| {
            let parsed = parse_expression(locator.contents()).expect("parse expression");
            let mod_expr = parsed.into_syntax();
            let expr = *mod_expr.body;

            let py_call = expr_to_python(py, locator, &expr, projection).expect("convert call");
            let call_bound = py_call.bind(py);
            let first = call_bound.getattr("arguments").expect("first arguments");
            let second = call_bound.getattr("arguments").expect("second arguments");

            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "lazy loader should cache the PyTuple instance"
            );

            let args = first.getattr("args").expect("Arguments.args attribute");
            let args_tuple = args
                .downcast::<PyTuple>()
                .expect("Arguments.args should be a tuple");
            assert_eq!(args_tuple.len(), 1);
        });
    }

    #[test]
    fn if_expr_test_is_eager() {
        with_python_fixture("value if cond else fallback", |py, projection, locator| {
            let parsed = parse_expression(locator.contents()).expect("parse expression");
            let mod_expr = parsed.into_syntax();
            let expr = *mod_expr.body;
            let Expr::If(_) = expr else {
                panic!("expected IfExpr")
            };

            let py_if = expr_to_python(py, locator, &expr, projection).expect("convert if expr");

            let if_bound = py_if.bind(py);
            let first = if_bound.getattr("test").expect("first test attribute");
            let second = if_bound.getattr("test").expect("second test attribute");

            assert_eq!(
                first.as_ptr(),
                second.as_ptr(),
                "eager field should be cached"
            );
        });
    }

    #[test]
    fn call_projection_includes_callee() -> anyhow::Result<()> {
        with_python_fixture_result("logging.info('x')", |py, projection, locator| {
            let expr = {
                let parsed = ruff_python_parser::parse_expression(locator.contents())?;
                *parsed.into_syntax().body
            };
            let node = expr_to_python(py, locator, &expr, projection)?;
            let node = node.bind(py);
            let callee: String = node.getattr("callee")?.extract()?;
            assert_eq!(callee, "logging.info");
            Ok(())
        })
    }

    #[test]
    fn call_projection_includes_arguments() -> anyhow::Result<()> {
        with_python_fixture_result(
            "logging.info('static', msg='template {}'.format(value))",
            |py, projection, locator| {
                let expr = {
                    let parsed = ruff_python_parser::parse_expression(locator.contents())?;
                    *parsed.into_syntax().body
                };

                let node = expr_to_python(py, locator, &expr, projection)?;
                let node = node.bind(py);

                let function_text: String = node.getattr("function_text")?.extract()?;
                assert_eq!(function_text, "logging.info");

                let arguments = node.getattr("arguments")?;
                let args_obj = arguments.getattr("args")?;
                let args = args_obj
                    .downcast::<PyTupleType>()
                    .map_err(|err| anyhow!(err.to_string()))?;
                assert_eq!(args.len(), 1);

                let first = args.get_item(0)?;
                let first_kind: String = first.getattr("_kind")?.extract()?;
                assert_eq!(first_kind, "StringLiteral");

                let keywords_obj = arguments.getattr("keywords")?;
                let keywords = keywords_obj
                    .downcast::<PyTupleType>()
                    .map_err(|err| anyhow!(err.to_string()))?;
                assert_eq!(keywords.len(), 1);

                let keyword = keywords.get_item(0)?;
                let name = keyword.getattr("arg")?;
                let name: String = name.extract()?;
                assert_eq!(name, "msg");

                let call_function_text: String = keyword
                    .getattr("value")?
                    .getattr("function_text")?
                    .extract()?;
                assert!(
                    call_function_text.ends_with(".format"),
                    "unexpected call_function_text: {call_function_text}"
                );

                Ok(())
            },
        )
    }

    #[test]
    fn stmt_projection_reports_kind() -> anyhow::Result<()> {
        with_python_fixture_result("pass\n", |py, projection, locator| {
            let stmt = ruff_python_ast::Stmt::Pass(ruff_python_ast::StmtPass {
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
                range: TextRange::new(TextSize::new(0), TextSize::new(4)),
            });

            let node = stmt_to_python(py, locator, &stmt, projection)?;
            let node = node.bind(py);
            let kind: String = node.getattr("_kind")?.extract()?;
            assert_eq!(kind, "Pass");
            Ok(())
        })
    }

    #[test]
    fn function_def_projects_decorators() -> anyhow::Result<()> {
        with_python_fixture_result(
            "@decorator\ndef func():\n    pass\n",
            |py, projection, locator| {
                let parsed = parse_module(locator.contents())?;
                let module = parsed.into_syntax();
                let stmt = module
                    .body
                    .first()
                    .ok_or_else(|| anyhow!("missing statement"))?;
                let Stmt::FunctionDef(_) = stmt else {
                    return Err(anyhow!("expected FunctionDef"));
                };

                let node = stmt_to_python(py, locator, stmt, projection)?;
                let node = node.bind(py);
                let decorators = node.getattr("decorator_list")?;
                let decorators = decorators
                    .downcast::<PyTupleType>()
                    .map_err(|err| anyhow!(err.to_string()))?;
                assert_eq!(decorators.len(), 1);
                Ok(())
            },
        )
    }

    #[test]
    fn with_stmt_projects_items() -> anyhow::Result<()> {
        with_python_fixture_result(
            "with open('x') as f:\n    pass\n",
            |py, projection, locator| {
                let parsed = parse_module(locator.contents())?;
                let module = parsed.into_syntax();
                let stmt = module
                    .body
                    .first()
                    .ok_or_else(|| anyhow!("missing statement"))?;
                let Stmt::With(_) = stmt else {
                    return Err(anyhow!("expected With"));
                };

                let node = stmt_to_python(py, locator, stmt, projection)?;
                let node = node.bind(py);
                let items = node.getattr("items")?;
                let items = items
                    .downcast::<PyTupleType>()
                    .map_err(|err| anyhow!(err.to_string()))?;
                assert_eq!(items.len(), 1);
                Ok(())
            },
        )
    }

    #[test]
    fn import_from_projects_names_and_level() -> anyhow::Result<()> {
        with_python_fixture_result("from os import path as p\n", |py, projection, locator| {
            let parsed = parse_module(locator.contents())?;
            let module = parsed.into_syntax();
            let stmt = module
                .body
                .first()
                .ok_or_else(|| anyhow!("missing statement"))?;
            let Stmt::ImportFrom(_) = stmt else {
                return Err(anyhow!("expected ImportFrom"));
            };

            let node = stmt_to_python(py, locator, stmt, projection)?;
            let node = node.bind(py);
            let names = node.getattr("names")?;
            let names = names
                .downcast::<PyTupleType>()
                .map_err(|err| anyhow!(err.to_string()))?;
            assert_eq!(names.len(), 1);

            let level: u32 = node.getattr("level")?.extract()?;
            assert_eq!(level, 0);
            Ok(())
        })
    }

    #[test]
    fn try_stmt_projects_handlers() -> anyhow::Result<()> {
        with_python_fixture_result(
            "try:\n    pass\nexcept Exception:\n    pass\n",
            |py, projection, locator| {
                let parsed = parse_module(locator.contents())?;
                let module = parsed.into_syntax();
                let stmt = module
                    .body
                    .first()
                    .ok_or_else(|| anyhow!("missing statement"))?;
                let Stmt::Try(_) = stmt else {
                    return Err(anyhow!("expected Try"));
                };

                let node = stmt_to_python(py, locator, stmt, projection)?;
                let node = node.bind(py);
                let handlers = node.getattr("handlers")?;
                let handlers = handlers
                    .downcast::<PyTupleType>()
                    .map_err(|err| anyhow!(err.to_string()))?;
                assert_eq!(handlers.len(), 1);
                Ok(())
            },
        )
    }

    #[test]
    fn comprehension_projects_generators() -> anyhow::Result<()> {
        with_python_fixture_result("[x for x in values]", |py, projection, locator| {
            let parsed = parse_expression(locator.contents())?;
            let expr = parsed.into_syntax().body;

            let node = expr_to_python(py, locator, &expr, projection)?;
            let node = node.bind(py);
            let generators = node.getattr("generators")?;
            let generators = generators
                .downcast::<PyTupleType>()
                .map_err(|err| anyhow!(err.to_string()))?;
            assert_eq!(generators.len(), 1);
            Ok(())
        })
    }

    #[test]
    fn for_stmt_projects_is_async() -> anyhow::Result<()> {
        with_python_fixture_result(
            "async for x in y:\n    pass\n",
            |py, projection, locator| {
                let parsed = parse_module(locator.contents())?;
                let module = parsed.into_syntax();
                let stmt = module
                    .body
                    .first()
                    .ok_or_else(|| anyhow!("missing statement"))?;
                let Stmt::For(_) = stmt else {
                    return Err(anyhow!("expected For"));
                };

                let node = stmt_to_python(py, locator, stmt, projection)?;
                let node = node.bind(py);
                let is_async: bool = node.getattr("is_async")?.extract()?;
                assert!(is_async);
                Ok(())
            },
        )
    }

    #[test]
    fn aug_assign_projects_op() -> anyhow::Result<()> {
        with_python_fixture_result("x += 1\n", |py, projection, locator| {
            let parsed = parse_module(locator.contents())?;
            let module = parsed.into_syntax();
            let stmt = module
                .body
                .first()
                .ok_or_else(|| anyhow!("missing statement"))?;
            let Stmt::AugAssign(_) = stmt else {
                return Err(anyhow!("expected AugAssign"));
            };

            let node = stmt_to_python(py, locator, stmt, projection)?;
            let node = node.bind(py);
            let op: String = node.getattr("op")?.extract()?;
            assert_eq!(op, "+");
            Ok(())
        })
    }

    #[test]
    fn global_stmt_projects_names() -> anyhow::Result<()> {
        with_python_fixture_result("global a, b\n", |py, projection, locator| {
            let parsed = parse_module(locator.contents())?;
            let module = parsed.into_syntax();
            let stmt = module
                .body
                .first()
                .ok_or_else(|| anyhow!("missing statement"))?;
            let Stmt::Global(_) = stmt else {
                return Err(anyhow!("expected Global"));
            };

            let node = stmt_to_python(py, locator, stmt, projection)?;
            let node = node.bind(py);
            let names = node.getattr("names")?;
            let names = names
                .downcast::<PyTupleType>()
                .map_err(|err| anyhow!(err.to_string()))?;
            assert_eq!(names.len(), 2);
            Ok(())
        })
    }
}
