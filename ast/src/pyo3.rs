use crate::{source_code::SourceRange, text_size::TextRange, ConversionFlag, Node};
use num_complex::Complex64;
use once_cell::sync::OnceCell;
use pyo3::{
    prelude::*,
    types::{PyBytes, PyList, PyString, PyTuple},
};

pub trait Pyo3Node {
    fn py_type_cache() -> &'static OnceCell<(Py<PyAny>, Py<PyAny>)> {
        {
            static PY_TYPE: OnceCell<(Py<PyAny>, Py<PyAny>)> = OnceCell::new();
            &PY_TYPE
        }
    }
}

pub trait ToPyo3Ast {
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>>;
}

impl<T: ToPyo3Ast> ToPyo3Ast for Box<T> {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        (**self).to_pyo3_ast(py)
    }
}

impl<T: ToPyo3Ast> ToPyo3Ast for Option<T> {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        match self {
            Some(ast) => ast.to_pyo3_ast(py),
            None => Ok(py.None()),
        }
    }
}

impl<T: ToPyo3Ast> ToPyo3Ast for Vec<T> {
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        let list = PyList::empty(py);
        for item in self {
            let py_item = item.to_pyo3_ast(py)?;
            list.append(py_item)?;
        }
        Ok(list.into())
    }
}

impl ToPyo3Ast for crate::Identifier {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.as_str().to_object(py))
    }
}

impl ToPyo3Ast for crate::String {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok(self.as_str().to_object(py))
    }
}

impl ToPyo3Ast for crate::Int {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((self.to_u32()).to_object(py))
    }
}

impl ToPyo3Ast for bool {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((*self as u32).to_object(py))
    }
}

impl ToPyo3Ast for ConversionFlag {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        Ok((*self as i8).to_object(py))
    }
}

impl ToPyo3Ast for crate::Constant {
    #[inline]
    fn to_pyo3_ast(&self, py: Python) -> PyResult<Py<PyAny>> {
        let value = match self {
            crate::Constant::None => py.None(),
            crate::Constant::Bool(bool) => bool.to_object(py),
            crate::Constant::Str(string) => string.to_object(py),
            crate::Constant::Bytes(bytes) => PyBytes::new(py, bytes).into(),
            crate::Constant::Int(int) => int.to_object(py),
            crate::Constant::Tuple(elts) => {
                let elts: PyResult<Vec<_>> = elts.iter().map(|c| c.to_pyo3_ast(py)).collect();
                PyTuple::new(py, elts?).into()
            }
            crate::Constant::Float(f64) => f64.to_object(py),
            crate::Constant::Complex { real, imag } => Complex64::new(*real, *imag).to_object(py),
            crate::Constant::Ellipsis => py.Ellipsis(),
        };
        Ok(value)
    }
}

#[pyclass(module = "rustpython_ast", subclass)]
pub struct AST;

#[pymethods]
impl AST {
    #[new]
    fn new() -> Self {
        Self
    }
}

fn cache_py_type<N: Pyo3Node + Node>(ast_module: &PyAny) -> PyResult<()> {
    let class = ast_module.getattr(N::NAME)?;
    let base = if std::mem::size_of::<N>() == 0 {
        class.call0()?
    } else {
        class.getattr("__new__")?
    };
    N::py_type_cache().get_or_init(|| (class.into(), base.into()));
    Ok(())
}

struct AstKeyCache {
    lineno: Py<PyString>,
    col_offset: Py<PyString>,
    end_lineno: Py<PyString>,
    end_col_offset: Py<PyString>,
}

fn ast_key_cache() -> &'static OnceCell<AstKeyCache> {
    {
        static PY_TYPE: OnceCell<AstKeyCache> = OnceCell::new();
        &PY_TYPE
    }
}

pub fn init(py: Python) -> PyResult<()> {
    ast_key_cache().get_or_init(|| AstKeyCache {
        lineno: pyo3::intern!(py, "lineno").into_py(py),
        col_offset: pyo3::intern!(py, "col_offset").into_py(py),
        end_lineno: pyo3::intern!(py, "end_lineno").into_py(py),
        end_col_offset: pyo3::intern!(py, "end_col_offset").into_py(py),
    });

    init_types(py)
}

include!("gen/to_pyo3.rs");
