#![allow(clippy::derive_partial_eq_without_eq)]
pub use crate::{builtin::*, text_size::TextSize, ConversionFlag, Node};
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;

pub type Suite<R = TextRange> = Vec<Stmt<R>>;

#[cfg(feature = "all-nodes-with-ranges")]
pub type OptionalRange<R> = R;

#[cfg(not(feature = "all-nodes-with-ranges"))]
pub type OptionalRange<R> = EmptyRange<R>;

#[cfg(not(feature = "all-nodes-with-ranges"))]
impl<R> From<R> for OptionalRange<R> {
    fn from(_: R) -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

#[derive(Eq, PartialEq, Hash, Copy, Clone)]
pub struct EmptyRange<R> {
    phantom: PhantomData<R>,
}

impl<R> EmptyRange<R> {
    #[inline(always)]
    pub fn new(_start: TextSize, _end: TextSize) -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<R> Display for EmptyRange<R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("()")
    }
}

impl<R> Debug for EmptyRange<R> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl<R> Default for EmptyRange<R> {
    fn default() -> Self {
        EmptyRange {
            phantom: PhantomData,
        }
    }
}

impl CmpOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtE => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtE => ">=",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
        }
    }
}

impl<R> Arguments<R> {
    pub fn empty(range: OptionalRange<R>) -> Self {
        Self {
            range,
            posonlyargs: Vec::new(),
            args: Vec::new(),
            vararg: None,
            kwonlyargs: Vec::new(),
            kwarg: None,
        }
    }
}

#[allow(clippy::borrowed_box)] // local utility
fn clone_boxed_expr<R: Clone>(expr: &Box<Expr<R>>) -> Box<Expr<R>> {
    let expr: &Expr<_> = expr.as_ref();
    Box::new(expr.clone())
}

impl<R> ArgWithDefault<R> {
    pub fn from_arg(def: Arg<R>, default: Option<Expr<R>>) -> Self
    where
        R: Clone,
    {
        let range = {
            if cfg!(feature = "all-nodes-with-ranges") {
                todo!("range recovery is not implemented yet") // def.range.start()..default.range.end()
            } else {
                #[allow(clippy::useless_conversion)] // false positive by cfg
                OptionalRange::from(def.range.clone())
            }
        };
        Self {
            range,
            def,
            default: default.map(Box::new),
        }
    }

    pub fn as_arg(&self) -> &Arg<R> {
        &self.def
    }

    pub fn to_arg(&self) -> (Arg<R>, Option<Box<Expr<R>>>)
    where
        R: Clone,
    {
        let ArgWithDefault {
            range: _,
            def,
            default,
        } = self;
        (def.clone(), default.as_ref().map(clone_boxed_expr))
    }
    pub fn into_arg(self) -> (Arg<R>, Option<Box<Expr<R>>>) {
        let ArgWithDefault {
            range: _,
            def,
            default,
        } = self;
        (def, default)
    }
}

impl<R> Arguments<R> {
    pub fn defaults(&self) -> impl std::iter::Iterator<Item = &Expr<R>> {
        self.posonlyargs
            .iter()
            .chain(self.args.iter())
            .filter_map(|arg| arg.default.as_ref().map(|e| e.as_ref()))
    }

    #[allow(clippy::type_complexity)]
    pub fn split_kwonlyargs(&self) -> (Vec<&Arg<R>>, Vec<(&Arg<R>, &Expr<R>)>) {
        let mut args = Vec::new();
        let mut with_defaults = Vec::new();
        for arg in self.kwonlyargs.iter() {
            if let Some(ref default) = arg.default {
                with_defaults.push((arg.as_arg(), &**default));
            } else {
                args.push(arg.as_arg());
            }
        }
        (args, with_defaults)
    }

    pub fn to_python_arguments(&self) -> PythonArguments<R>
    where
        R: Clone,
    {
        let Arguments {
            range,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        let mut pos_only = Vec::with_capacity(posonlyargs.len());
        let mut pos_args = Vec::with_capacity(args.len());
        let mut defaults = Vec::new();
        for arg in posonlyargs {
            let (arg, default) = arg.to_arg();
            if let Some(default) = default {
                defaults.push(*default);
            }
            pos_only.push(arg);
        }
        for arg in args {
            let (arg, default) = arg.to_arg();
            if let Some(default) = default {
                defaults.push(*default);
            }
            pos_args.push(arg);
        }

        let mut kw_only = Vec::with_capacity(kwonlyargs.len());
        let mut kw_defaults = Vec::new();
        for arg in kwonlyargs {
            let (arg, default) = arg.to_arg();
            if let Some(default) = default {
                kw_defaults.push(*default);
            }
            kw_only.push(arg);
        }

        PythonArguments {
            range: range.clone(),
            posonlyargs: pos_only,
            args: pos_args,
            defaults,
            vararg: vararg.clone(),
            kwonlyargs: kw_only,
            kw_defaults,
            kwarg: kwarg.clone(),
        }
    }

    pub fn into_python_arguments(self) -> PythonArguments<R> {
        let Arguments {
            range,
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
        } = self;

        let mut pos_only = Vec::with_capacity(posonlyargs.len());
        let mut pos_args = Vec::with_capacity(args.len());
        let mut defaults = Vec::new();
        for arg in posonlyargs {
            let (arg, default) = arg.into_arg();
            if let Some(default) = default {
                defaults.push(*default);
            }
            pos_only.push(arg);
        }
        for arg in args {
            let (arg, default) = arg.into_arg();
            if let Some(default) = default {
                defaults.push(*default);
            }
            pos_args.push(arg);
        }

        let mut kw_only = Vec::with_capacity(kwonlyargs.len());
        let mut kw_defaults = Vec::new();
        for arg in kwonlyargs {
            let (arg, default) = arg.into_arg();
            if let Some(default) = default {
                kw_defaults.push(*default);
            }
            kw_only.push(arg);
        }

        PythonArguments {
            range,
            posonlyargs: pos_only,
            args: pos_args,
            defaults,
            vararg,
            kwonlyargs: kw_only,
            kw_defaults,
            kwarg,
        }
    }
}

impl<R> PythonArguments<R> {
    pub fn into_arguments(self) -> Arguments<R>
    where
        R: Clone,
    {
        let PythonArguments {
            range,
            posonlyargs,
            args,
            defaults,
            vararg,
            kwonlyargs,
            kw_defaults,
            kwarg,
        } = self;

        let mut pos_only = Vec::with_capacity(posonlyargs.len());
        let mut pos_args = Vec::with_capacity(args.len());
        let args_len = posonlyargs.len() + args.len();
        // not optimal
        let mut defaults: Vec<_> = std::iter::repeat_with(|| None)
            .take(args_len - defaults.len())
            .chain(defaults.into_iter().map(Some))
            .collect();
        debug_assert_eq!(args_len, defaults.len());

        for (arg, default) in std::iter::zip(args, defaults.drain(posonlyargs.len()..)) {
            let arg = ArgWithDefault::from_arg(arg, default);
            pos_args.push(arg);
        }

        for (arg, default) in std::iter::zip(posonlyargs, defaults.drain(..)) {
            let arg = ArgWithDefault::from_arg(arg, default);
            pos_only.push(arg);
        }

        let mut kw_only = Vec::with_capacity(kwonlyargs.len());
        let kw_defaults: Vec<_> = std::iter::repeat_with(|| None)
            .take(kw_only.len().saturating_sub(kw_defaults.len()))
            .chain(kw_defaults.into_iter().map(Some))
            .collect();
        for (arg, default) in std::iter::zip(kwonlyargs, kw_defaults) {
            let arg = ArgWithDefault::from_arg(arg, default);
            kw_only.push(arg);
        }

        Arguments {
            range,
            posonlyargs: pos_only,
            args: pos_args,
            vararg,
            kwonlyargs: kw_only,
            kwarg,
        }
    }
}

impl<R> From<Arguments<R>> for PythonArguments<R> {
    fn from(arguments: Arguments<R>) -> Self {
        arguments.into_python_arguments()
    }
}

include!("gen/generic.rs");
