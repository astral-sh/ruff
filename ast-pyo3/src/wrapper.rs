use crate::PyNode;
use num_complex::Complex64;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};
use rustpython_ast::{
    self as ast, source_code::SourceRange, text_size::TextRange, ConversionFlag, Node,
};

pub trait ToPyWrapper {
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>>;
}

impl<T: ToPyWrapper> ToPyWrapper for Box<T> {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        (**self).to_py_wrapper(py)
    }
}

impl<T: ToPyWrapper> ToPyWrapper for Option<T> {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        match self {
            Some(ast) => ast.to_py_wrapper(py),
            None => Ok(py.None()),
        }
    }
}

impl ToPyWrapper for ast::Identifier {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.as_str().to_object(py))
    }
}

impl ToPyWrapper for ast::String {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.as_str().to_object(py))
    }
}

impl ToPyWrapper for ast::Int {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((self.to_u32()).to_object(py))
    }
}

impl ToPyWrapper for bool {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((*self as u32).to_object(py))
    }
}

impl ToPyWrapper for ConversionFlag {
    #[inline]
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((*self as i8).to_object(py))
    }
}

impl ToPyWrapper for ast::Constant {
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        let value = match self {
            ast::Constant::None => py.None(),
            ast::Constant::Bool(bool) => bool.to_object(py),
            ast::Constant::Str(string) => string.to_object(py),
            ast::Constant::Bytes(bytes) => PyBytes::new(py, bytes).into(),
            ast::Constant::Int(int) => int.to_object(py),
            ast::Constant::Tuple(elts) => {
                let elts: PyResult<Vec<_>> = elts.iter().map(|c| c.to_py_wrapper(py)).collect();
                PyTuple::new(py, elts?).into()
            }
            ast::Constant::Float(f64) => f64.to_object(py),
            ast::Constant::Complex { real, imag } => Complex64::new(*real, *imag).to_object(py),
            ast::Constant::Ellipsis => py.Ellipsis(),
        };
        Ok(value)
    }
}

impl<T: ToPyWrapper> ToPyWrapper for Vec<T> {
    fn to_py_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        let list = PyList::empty(py);
        for item in self {
            let py_item = item.to_py_wrapper(py)?;
            list.append(py_item)?;
        }
        Ok(list.into())
    }
}

impl<R> ToPyWrapper for ast::Arguments<R>
where
    Self: Clone,
    ast::PythonArguments<R>: ToPyWrapper,
{
    #[inline]
    fn to_py_wrapper(&'static self, _py: Python) -> PyResult<Py<PyAny>> {
        todo!()
        // Ok(FunctionArguments(self).to_object(py))
    }
}

#[pyclass(module = "rustpython_ast", name = "AST", subclass)]
pub struct Ast;

#[pymethods]
impl Ast {
    #[new]
    fn new() -> Self {
        Self
    }
}

pub mod located {
    pub use super::Ast;
    use super::*;
    include!("gen/wrapper_located.rs");
}

pub mod ranged {
    pub use super::Ast;
    use super::*;
    include!("gen/wrapper_ranged.rs");
}

fn init_type<P: pyo3::PyClass, N: PyNode + Node>(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<P>()?;
    let node = m.getattr(P::NAME)?;
    if P::NAME != N::NAME {
        // TODO: no idea how to escape rust keyword on #[pyclass]
        m.setattr(P::NAME, node)?;
    }
    let names: Vec<&'static str> = N::FIELD_NAMES.to_vec();
    let fields = PyTuple::new(py, names);
    node.setattr("_fields", fields)?;
    Ok(())
}

/// A Python module implemented in Rust.
fn init_module(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Ast>()?;

    let ast = m.getattr("AST")?;
    let fields = PyTuple::empty(py);
    ast.setattr("_fields", fields)?;

    Ok(())
}
