use crate::pyo3::{Pyo3Node, AST};
use crate::{source_code::SourceRange, text_size::TextRange, ConversionFlag, Node};
use num_complex::Complex64;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};

pub trait ToPyo3Wrapper {
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>>;
}

impl<T: ToPyo3Wrapper> ToPyo3Wrapper for Box<T> {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        (**self).to_pyo3_wrapper(py)
    }
}

impl<T: ToPyo3Wrapper> ToPyo3Wrapper for Option<T> {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        match self {
            Some(ast) => ast.to_pyo3_wrapper(py),
            None => Ok(py.None()),
        }
    }
}

impl ToPyo3Wrapper for crate::Identifier {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.as_str().to_object(py))
    }
}

impl ToPyo3Wrapper for crate::String {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.as_str().to_object(py))
    }
}

impl ToPyo3Wrapper for crate::Int {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((self.to_u32()).to_object(py))
    }
}

impl ToPyo3Wrapper for bool {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((*self as u32).to_object(py))
    }
}

impl ToPyo3Wrapper for ConversionFlag {
    #[inline]
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((*self as i8).to_object(py))
    }
}

impl ToPyo3Wrapper for crate::Constant {
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        let value = match self {
            crate::Constant::None => py.None(),
            crate::Constant::Bool(bool) => bool.to_object(py),
            crate::Constant::Str(string) => string.to_object(py),
            crate::Constant::Bytes(bytes) => PyBytes::new(py, bytes).into(),
            crate::Constant::Int(int) => int.to_object(py),
            crate::Constant::Tuple(elts) => {
                let elts: PyResult<Vec<_>> = elts.iter().map(|c| c.to_pyo3_wrapper(py)).collect();
                PyTuple::new(py, elts?).into()
            }
            crate::Constant::Float(f64) => f64.to_object(py),
            crate::Constant::Complex { real, imag } => Complex64::new(*real, *imag).to_object(py),
            crate::Constant::Ellipsis => py.Ellipsis(),
        };
        Ok(value)
    }
}

impl<T: ToPyo3Wrapper> ToPyo3Wrapper for Vec<T> {
    fn to_pyo3_wrapper(&'static self, py: Python) -> PyResult<Py<PyAny>> {
        let list = PyList::empty(py);
        for item in self {
            let py_item = item.to_pyo3_wrapper(py)?;
            list.append(py_item)?;
        }
        Ok(list.into())
    }
}

pub mod located {
    use super::*;
    pub use crate::pyo3::AST;
    include!("gen/pyo3_wrapper_located.rs");
}

pub mod ranged {
    use super::*;
    pub use crate::pyo3::AST;
    include!("gen/pyo3_wrapper_ranged.rs");
}

fn init_type<P: pyo3::PyClass, N: Pyo3Node + Node>(py: Python, m: &PyModule) -> PyResult<()> {
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
    m.add_class::<AST>()?;

    let ast = m.getattr("AST")?;
    let fields = PyTuple::empty(py);
    ast.setattr("_fields", fields)?;

    Ok(())
}
