use num_complex::Complex64;
use num_traits::cast::ToPrimitive;
use once_cell::sync::OnceCell;
use pyo3::{
    prelude::*,
    types::{PyBool, PyBytes, PyList, PyString, PyTuple},
    ToPyObject,
};
use rustpython_ast::{
    self as ast, source_code::SourceRange, text_size::TextRange, ConversionFlag, Node,
};

pub trait PyNode {
    fn py_type_cache() -> &'static OnceCell<(Py<PyAny>, Py<PyAny>)> {
        {
            static PY_TYPE: OnceCell<(Py<PyAny>, Py<PyAny>)> = OnceCell::new();
            &PY_TYPE
        }
    }
}

pub trait ToPyAst {
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny>;
}

impl<T: ToPyAst> ToPyAst for Box<T> {
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        (**self).to_py_ast(py)
    }
}

impl<T: ToPyAst> ToPyAst for Option<T> {
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        match self {
            Some(ast) => ast.to_py_ast(py),
            None => Ok(ast_cache().none_ref(py)),
        }
    }
}

impl<T: ToPyAst> ToPyAst for Vec<T> {
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let elts = self
            .iter()
            .map(|item| item.to_py_ast(py))
            .collect::<Result<Vec<_>, _>>()?;
        let list = PyList::new(py, elts);
        Ok(list.into())
    }
}

impl ToPyAst for ast::Identifier {
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(PyString::new(py, self.as_str()).into())
    }
}

impl ToPyAst for ast::String {
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(PyString::new(py, self.as_str()).into())
    }
}

impl ToPyAst for bool {
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(ast_cache().bool_int(py, *self))
    }
}

impl ToPyAst for ConversionFlag {
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        Ok(ast_cache().conversion_flag(py, *self))
    }
}

impl<R> ToPyAst for ast::Arguments<R>
where
    R: Clone,
    ast::PythonArguments<R>: ToPyAst,
{
    #[inline]
    fn to_py_ast<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let arguments = self.to_python_arguments();
        arguments.to_py_ast(py)
    }
}

fn constant_to_object(constant: &ast::Constant, py: Python) -> PyObject {
    let cache = ast_cache();
    match constant {
        ast::Constant::None => cache.none.clone_ref(py),
        ast::Constant::Bool(bool) => cache.bool(py, *bool).into(),
        ast::Constant::Str(string) => string.to_object(py),
        ast::Constant::Bytes(bytes) => PyBytes::new(py, bytes).into(),
        ast::Constant::Int(int) => match int.to_i64() {
            Some(small_int) => small_int.to_object(py),
            None => int.to_object(py),
        },
        ast::Constant::Tuple(elts) => {
            let elts: Vec<_> = elts.iter().map(|c| constant_to_object(c, py)).collect();
            PyTuple::new(py, elts).into()
        }
        ast::Constant::Float(f64) => f64.to_object(py),
        ast::Constant::Complex { real, imag } => Complex64::new(*real, *imag).to_object(py),
        ast::Constant::Ellipsis => py.Ellipsis(),
    }
}

#[pyclass(module = "rustpython_ast", subclass)]
pub struct Ast;

#[pymethods]
impl Ast {
    #[new]
    fn new() -> Self {
        Self
    }
}

fn cache_py_type<N: PyNode + Node>(ast_module: &PyAny) -> PyResult<()> {
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
    // fn location_vec<'py>(&'static self, py: Python<'py>, range: &SourceRange) -> &'py PyDict {
    //     let attributes = PyDict::new(py);
    //     attributes.set_item(self.lineno.as_ref(py), range.start.row.get()).unwrap();
    //     attributes.set_item(self.col_offset.as_ref(py), range.start.column.to_zero_indexed()).unwrap();
    //     if let Some(end) = range.end {
    //         attributes.set_item(self.end_lineno.as_ref(py), end.row.get()).unwrap();
    //         attributes.set_item(
    //             self.end_col_offset.as_ref(py),
    //             end.column.to_zero_indexed(),
    //         ).unwrap();
    //     }
    //     attributes
    // }
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

include!("gen/to_py_ast.rs");
