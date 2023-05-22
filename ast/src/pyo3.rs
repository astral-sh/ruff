use crate::{source_code::SourceRange, text_size::TextRange, ConversionFlag, Node};
use num_complex::Complex64;
use once_cell::sync::OnceCell;
use pyo3::{
    prelude::*,
    types::{PyBool, PyBytes, PyList, PyString, PyTuple},
    ToPyObject,
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
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny>;
}

impl<T: ToPyo3Ast> ToPyo3Ast for Box<T> {
    #[inline]
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        (**self).to_pyo3_ast(py)
    }
}

impl<T: ToPyo3Ast> ToPyo3Ast for Option<T> {
    #[inline]
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        match self {
            Some(ast) => ast.to_pyo3_ast(py),
            None => Ok(ast_cache().none_ref(py)),
        }
    }
}

impl<T: ToPyo3Ast> ToPyo3Ast for Vec<T> {
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let elts = self
            .iter()
            .map(|item| item.to_pyo3_ast(py))
            .collect::<Result<Vec<_>, _>>()?;
        let list = PyList::new(py, elts);
        Ok(list.into())
    }
}

impl ToPyo3Ast for crate::Identifier {
    #[inline]
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(PyString::new(py, self.as_str()).into())
    }
}

impl ToPyo3Ast for crate::String {
    #[inline]
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(PyString::new(py, self.as_str()).into())
    }
}

impl ToPyo3Ast for bool {
    #[inline]
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(ast_cache().bool_int(py, *self))
    }
}

impl ToPyo3Ast for ConversionFlag {
    #[inline]
    fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(ast_cache().conversion_flag(py, *self))
    }
}

impl ToPyObject for crate::Constant {
    fn to_object(&self, py: Python) -> PyObject {
        let cache = ast_cache();
        match self {
            crate::Constant::None => cache.none.clone_ref(py),
            crate::Constant::Bool(bool) => cache.bool(py, *bool).into(),
            crate::Constant::Str(string) => string.to_object(py),
            crate::Constant::Bytes(bytes) => PyBytes::new(py, bytes).into(),
            crate::Constant::Int(int) => int.to_object(py),
            crate::Constant::Tuple(elts) => {
                let elts: Vec<_> = elts.iter().map(|c| c.to_object(py)).collect();
                PyTuple::new(py, elts).into()
            }
            crate::Constant::Float(f64) => f64.to_object(py),
            crate::Constant::Complex { real, imag } => Complex64::new(*real, *imag).to_object(py),
            crate::Constant::Ellipsis => py.Ellipsis(),
        }
    }
}

// impl ToPyo3Ast for crate::Constant {
//     #[inline]
//     fn to_pyo3_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
//         let cache = ast_cache();
//         let value = match self {
//             crate::Constant::None => cache.none_ref(py),
//             crate::Constant::Bool(bool) => cache.bool_ref(py),
//             crate::Constant::Str(string) => string.to_object(py),
//             crate::Constant::Bytes(bytes) => PyBytes::new(py, bytes).into(),
//             crate::Constant::Int(int) => int.to_object(py),
//             crate::Constant::Tuple(elts) => {
//                 let elts: PyResult<Vec<_>> = elts.iter().map(|c| c.to_pyo3_ast(py)).collect();
//                 PyTuple::new(py, elts?).into()
//             }
//             crate::Constant::Float(f64) => f64.to_object(py),
//             crate::Constant::Complex { real, imag } => Complex64::new(*real, *imag).to_object(py),
//             crate::Constant::Ellipsis => py.Ellipsis(),
//         };
//         Ok(value)
//     }
// }

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

// TODO: This cache must be bound to 'py
struct AstCache {
    lineno: Py<PyString>,
    col_offset: Py<PyString>,
    end_lineno: Py<PyString>,
    end_col_offset: Py<PyString>,
    none: Py<PyAny>,
    bool_values: (Py<PyBool>, Py<PyBool>),
    bool_int_values: (Py<PyAny>, Py<PyAny>),
    conversion_flags: (Py<PyAny>, Py<PyAny>, Py<PyAny>, Py<PyAny>),
}

impl AstCache {
    #[inline]
    fn none_ref<'py>(&'static self, py: Python<'py>) -> &'py PyAny {
        Py::<PyAny>::as_ref(&self.none, py)
    }
    #[inline]
    fn bool_int<'py>(&'static self, py: Python<'py>, value: bool) -> &'py PyAny {
        let v = &self.bool_int_values;
        Py::<PyAny>::as_ref(if value { &v.1 } else { &v.0 }, py)
    }
    #[inline]
    fn bool(&'static self, py: Python, value: bool) -> Py<PyBool> {
        let v = &self.bool_values;
        (if value { &v.1 } else { &v.0 }).clone_ref(py)
    }
    fn conversion_flag<'py>(&'static self, py: Python<'py>, value: ConversionFlag) -> &'py PyAny {
        let v = &self.conversion_flags;
        match value {
            ConversionFlag::None => v.0.as_ref(py),
            ConversionFlag::Str => v.1.as_ref(py),
            ConversionFlag::Ascii => v.2.as_ref(py),
            ConversionFlag::Repr => v.3.as_ref(py),
        }
    }
}

fn ast_cache_cell() -> &'static OnceCell<AstCache> {
    {
        static PY_TYPE: OnceCell<AstCache> = OnceCell::new();
        &PY_TYPE
    }
}

fn ast_cache() -> &'static AstCache {
    ast_cache_cell().get().unwrap()
}

pub fn init(py: Python) -> PyResult<()> {
    ast_cache_cell().get_or_init(|| AstCache {
        lineno: pyo3::intern!(py, "lineno").into_py(py),
        col_offset: pyo3::intern!(py, "col_offset").into_py(py),
        end_lineno: pyo3::intern!(py, "end_lineno").into_py(py),
        end_col_offset: pyo3::intern!(py, "end_col_offset").into_py(py),
        none: py.None(),
        bool_values: (PyBool::new(py, false).into(), PyBool::new(py, true).into()),
        bool_int_values: ((0).to_object(py), (1).to_object(py)),
        conversion_flags: (
            (-1).to_object(py),
            (b's').to_object(py),
            (b'a').to_object(py),
            (b'r').to_object(py),
        ),
    });

    init_types(py)
}

include!("gen/to_pyo3.rs");
