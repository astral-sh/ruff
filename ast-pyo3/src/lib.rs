mod py_ast;
#[cfg(feature = "wrapper")]
pub mod wrapper;

pub use py_ast::{init, PyNode, ToPyAst};
use pyo3::prelude::*;
use rustpython_parser::ast::{source_code::LinearLocator, Fold};

#[pyfunction]
#[pyo3(signature = (source, filename="<unknown>", *, type_comments=false, locate=true))]
pub fn parse<'py>(
    source: &str,
    filename: &str,
    type_comments: bool,
    locate: bool,
    py: Python<'py>,
) -> PyResult<&'py PyAny> {
    if type_comments {
        todo!("'type_comments' is not implemented yet");
    }
    let parsed = rustpython_parser::parse(source, rustpython_parser::Mode::Module, filename)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PySyntaxError, _>(e.to_string()))?;
    if locate {
        let parsed = LinearLocator::new(source).fold(parsed).unwrap();
        parsed.module().unwrap().to_py_ast(py)
    } else {
        parsed.module().unwrap().to_py_ast(py)
    }
}

#[pymodule]
fn rustpython_ast(py: Python, m: &PyModule) -> PyResult<()> {
    py_ast::init(py)?;

    #[cfg(feature = "wrapper")]
    {
        let ast = PyModule::new(py, "ast")?;
        wrapper::located::add_to_module(py, ast)?;
        m.add_submodule(ast)?;

        let ast = PyModule::new(py, "unlocated_ast")?;
        wrapper::ranged::add_to_module(py, ast)?;
        m.add_submodule(ast)?;
    }

    m.add_function(wrap_pyfunction!(parse, m)?)?;

    Ok(())
}
