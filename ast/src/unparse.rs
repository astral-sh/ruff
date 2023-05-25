use crate::{
    Arg, ArgWithDefault, Arguments, BoolOp, Comprehension, Constant, ConversionFlag, Expr,
    Identifier, Operator, PythonArguments,
};
use std::fmt;

mod precedence {
    macro_rules! precedence {
        ($($op:ident,)*) => {
            precedence!(@0, $($op,)*);
        };
        (@$i:expr, $op1:ident, $($op:ident,)*) => {
            pub const $op1: u8 = $i;
            precedence!(@$i + 1, $($op,)*);
        };
        (@$i:expr,) => {};
    }
    precedence!(
        TUPLE, TEST, OR, AND, NOT, CMP, // "EXPR" =
        BOR, BXOR, BAND, SHIFT, ARITH, TERM, FACTOR, POWER, AWAIT, ATOM,
    );
    pub const EXPR: u8 = BOR;
}

#[repr(transparent)]
struct Unparser<'a> {
    f: fmt::Formatter<'a>,
}
impl<'a> Unparser<'a> {
    fn new<'b>(f: &'b mut fmt::Formatter<'a>) -> &'b mut Unparser<'a> {
        unsafe { &mut *(f as *mut fmt::Formatter<'a> as *mut Unparser<'a>) }
    }

    fn p(&mut self, s: &str) -> fmt::Result {
        self.f.write_str(s)
    }
    fn p_id(&mut self, s: &Identifier) -> fmt::Result {
        self.f.write_str(s.as_str())
    }
    fn p_if(&mut self, cond: bool, s: &str) -> fmt::Result {
        if cond {
            self.f.write_str(s)?;
        }
        Ok(())
    }
    fn p_delim(&mut self, first: &mut bool, s: &str) -> fmt::Result {
        self.p_if(!std::mem::take(first), s)
    }
    fn write_fmt(&mut self, f: fmt::Arguments<'_>) -> fmt::Result {
        self.f.write_fmt(f)
    }

    fn unparse_expr<U>(&mut self, ast: &Expr<U>, level: u8) -> fmt::Result {
        macro_rules! op_prec {
            ($op_ty:ident, $x:expr, $enu:path, $($var:ident($op:literal, $prec:ident)),*$(,)?) => {
                match $x {
                    $(<$enu>::$var => (op_prec!(@space $op_ty, $op), precedence::$prec),)*
                }
            };
            (@space bin, $op:literal) => {
                concat!(" ", $op, " ")
            };
            (@space un, $op:literal) => {
                $op
            };
        }
        macro_rules! group_if {
            ($lvl:expr, $body:block) => {{
                let group = level > $lvl;
                self.p_if(group, "(")?;
                let ret = $body;
                self.p_if(group, ")")?;
                ret
            }};
        }
        match &ast {
            Expr::BoolOp(crate::ExprBoolOp {
                op,
                values,
                range: _range,
            }) => {
                let (op, prec) = op_prec!(bin, op, BoolOp, And("and", AND), Or("or", OR));
                group_if!(prec, {
                    let mut first = true;
                    for val in values {
                        self.p_delim(&mut first, op)?;
                        self.unparse_expr(val, prec + 1)?;
                    }
                })
            }
            Expr::NamedExpr(crate::ExprNamedExpr {
                target,
                value,
                range: _range,
            }) => {
                group_if!(precedence::TUPLE, {
                    self.unparse_expr(target, precedence::ATOM)?;
                    self.p(" := ")?;
                    self.unparse_expr(value, precedence::ATOM)?;
                })
            }
            Expr::BinOp(crate::ExprBinOp {
                left,
                op,
                right,
                range: _range,
            }) => {
                let right_associative = matches!(op, Operator::Pow);
                let (op, prec) = op_prec!(
                    bin,
                    op,
                    Operator,
                    Add("+", ARITH),
                    Sub("-", ARITH),
                    Mult("*", TERM),
                    MatMult("@", TERM),
                    Div("/", TERM),
                    Mod("%", TERM),
                    Pow("**", POWER),
                    LShift("<<", SHIFT),
                    RShift(">>", SHIFT),
                    BitOr("|", BOR),
                    BitXor("^", BXOR),
                    BitAnd("&", BAND),
                    FloorDiv("//", TERM),
                );
                group_if!(prec, {
                    self.unparse_expr(left, prec + right_associative as u8)?;
                    self.p(op)?;
                    self.unparse_expr(right, prec + !right_associative as u8)?;
                })
            }
            Expr::UnaryOp(crate::ExprUnaryOp {
                op,
                operand,
                range: _range,
            }) => {
                let (op, prec) = op_prec!(
                    un,
                    op,
                    crate::UnaryOp,
                    Invert("~", FACTOR),
                    Not("not ", NOT),
                    UAdd("+", FACTOR),
                    USub("-", FACTOR)
                );
                group_if!(prec, {
                    self.p(op)?;
                    self.unparse_expr(operand, prec)?;
                })
            }
            Expr::Lambda(crate::ExprLambda {
                args,
                body,
                range: _range,
            }) => {
                group_if!(precedence::TEST, {
                    let pos = args.args.len() + args.posonlyargs.len();
                    self.p(if pos > 0 { "lambda " } else { "lambda" })?;
                    self.unparse_arguments(args)?;
                    write!(self, ": {}", **body)?;
                })
            }
            Expr::IfExp(crate::ExprIfExp {
                test,
                body,
                orelse,
                range: _range,
            }) => {
                group_if!(precedence::TEST, {
                    self.unparse_expr(body, precedence::TEST + 1)?;
                    self.p(" if ")?;
                    self.unparse_expr(test, precedence::TEST + 1)?;
                    self.p(" else ")?;
                    self.unparse_expr(orelse, precedence::TEST)?;
                })
            }
            Expr::Dict(crate::ExprDict {
                keys,
                values,
                range: _range,
            }) => {
                self.p("{")?;
                let mut first = true;
                let (packed, unpacked) = values.split_at(keys.len());
                for (k, v) in keys.iter().zip(packed) {
                    self.p_delim(&mut first, ", ")?;
                    if let Some(k) = k {
                        write!(self, "{}: {}", *k, *v)?;
                    } else {
                        write!(self, "**{}", *v)?;
                    }
                }
                for d in unpacked {
                    self.p_delim(&mut first, ", ")?;
                    write!(self, "**{}", *d)?;
                }
                self.p("}")?;
            }
            Expr::Set(crate::ExprSet {
                elts,
                range: _range,
            }) => {
                self.p("{")?;
                let mut first = true;
                for v in elts {
                    self.p_delim(&mut first, ", ")?;
                    self.unparse_expr(v, precedence::TEST)?;
                }
                self.p("}")?;
            }
            Expr::ListComp(crate::ExprListComp {
                elt,
                generators,
                range: _range,
            }) => {
                self.p("[")?;
                self.unparse_expr(elt, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p("]")?;
            }
            Expr::SetComp(crate::ExprSetComp {
                elt,
                generators,
                range: _range,
            }) => {
                self.p("{")?;
                self.unparse_expr(elt, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p("}")?;
            }
            Expr::DictComp(crate::ExprDictComp {
                key,
                value,
                generators,
                range: _range,
            }) => {
                self.p("{")?;
                self.unparse_expr(key, precedence::TEST)?;
                self.p(": ")?;
                self.unparse_expr(value, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p("}")?;
            }
            Expr::GeneratorExp(crate::ExprGeneratorExp {
                elt,
                generators,
                range: _range,
            }) => {
                self.p("(")?;
                self.unparse_expr(elt, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p(")")?;
            }
            Expr::Await(crate::ExprAwait {
                value,
                range: _range,
            }) => {
                group_if!(precedence::AWAIT, {
                    self.p("await ")?;
                    self.unparse_expr(value, precedence::ATOM)?;
                })
            }
            Expr::Yield(crate::ExprYield {
                value,
                range: _range,
            }) => {
                if let Some(value) = value {
                    write!(self, "(yield {})", **value)?;
                } else {
                    self.p("(yield)")?;
                }
            }
            Expr::YieldFrom(crate::ExprYieldFrom {
                value,
                range: _range,
            }) => {
                write!(self, "(yield from {})", **value)?;
            }
            Expr::Compare(crate::ExprCompare {
                left,
                ops,
                comparators,
                range: _range,
            }) => {
                group_if!(precedence::CMP, {
                    let new_lvl = precedence::CMP + 1;
                    self.unparse_expr(left, new_lvl)?;
                    for (op, cmp) in ops.iter().zip(comparators) {
                        self.p(" ")?;
                        self.p(op.as_str())?;
                        self.p(" ")?;
                        self.unparse_expr(cmp, new_lvl)?;
                    }
                })
            }
            Expr::Call(crate::ExprCall {
                func,
                args,
                keywords,
                range: _range,
            }) => {
                self.unparse_expr(func, precedence::ATOM)?;
                self.p("(")?;
                if let (
                    [Expr::GeneratorExp(crate::ExprGeneratorExp {
                        elt,
                        generators,
                        range: _range,
                    })],
                    [],
                ) = (&**args, &**keywords)
                {
                    // make sure a single genexpr doesn't get double parens
                    self.unparse_expr(elt, precedence::TEST)?;
                    self.unparse_comp(generators)?;
                } else {
                    let mut first = true;
                    for arg in args {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_expr(arg, precedence::TEST)?;
                    }
                    for kw in keywords {
                        self.p_delim(&mut first, ", ")?;
                        if let Some(arg) = &kw.arg {
                            self.p_id(arg)?;
                            self.p("=")?;
                        } else {
                            self.p("**")?;
                        }
                        self.unparse_expr(&kw.value, precedence::TEST)?;
                    }
                }
                self.p(")")?;
            }
            Expr::FormattedValue(crate::ExprFormattedValue {
                value,
                conversion,
                format_spec,
                range: _range,
            }) => self.unparse_formatted(value, *conversion, format_spec.as_deref())?,
            Expr::JoinedStr(crate::ExprJoinedStr {
                values,
                range: _range,
            }) => self.unparse_joined_str(values, false)?,
            Expr::Constant(crate::ExprConstant {
                value,
                kind,
                range: _range,
            }) => {
                if let Some(kind) = kind {
                    self.p(kind)?;
                }
                assert_eq!(f64::MAX_10_EXP, 308);
                let inf_str = "1e309";
                match value {
                    Constant::Float(f) if f.is_infinite() => self.p(inf_str)?,
                    Constant::Complex { real, imag }
                        if real.is_infinite() || imag.is_infinite() =>
                    {
                        self.p(&value.to_string().replace("inf", inf_str))?
                    }
                    _ => fmt::Display::fmt(value, &mut self.f)?,
                }
            }
            Expr::Attribute(crate::ExprAttribute { value, attr, .. }) => {
                self.unparse_expr(value, precedence::ATOM)?;
                let period = if let Expr::Constant(crate::ExprConstant {
                    value: Constant::Int(_),
                    ..
                }) = value.as_ref()
                {
                    " ."
                } else {
                    "."
                };
                self.p(period)?;
                self.p_id(attr)?;
            }
            Expr::Subscript(crate::ExprSubscript { value, slice, .. }) => {
                self.unparse_expr(value, precedence::ATOM)?;
                let mut lvl = precedence::TUPLE;
                if let Expr::Tuple(crate::ExprTuple { elts, .. }) = slice.as_ref() {
                    if elts.iter().any(|expr| expr.is_starred_expr()) {
                        lvl += 1
                    }
                }
                self.p("[")?;
                self.unparse_expr(slice, lvl)?;
                self.p("]")?;
            }
            Expr::Starred(crate::ExprStarred { value, .. }) => {
                self.p("*")?;
                self.unparse_expr(value, precedence::EXPR)?;
            }
            Expr::Name(crate::ExprName { id, .. }) => self.p_id(id)?,
            Expr::List(crate::ExprList { elts, .. }) => {
                self.p("[")?;
                let mut first = true;
                for elt in elts {
                    self.p_delim(&mut first, ", ")?;
                    self.unparse_expr(elt, precedence::TEST)?;
                }
                self.p("]")?;
            }
            Expr::Tuple(crate::ExprTuple { elts, .. }) => {
                if elts.is_empty() {
                    self.p("()")?;
                } else {
                    group_if!(precedence::TUPLE, {
                        let mut first = true;
                        for elt in elts {
                            self.p_delim(&mut first, ", ")?;
                            self.unparse_expr(elt, precedence::TEST)?;
                        }
                        self.p_if(elts.len() == 1, ",")?;
                    })
                }
            }
            Expr::Slice(crate::ExprSlice {
                lower,
                upper,
                step,
                range: _range,
            }) => {
                if let Some(lower) = lower {
                    self.unparse_expr(lower, precedence::TEST)?;
                }
                self.p(":")?;
                if let Some(upper) = upper {
                    self.unparse_expr(upper, precedence::TEST)?;
                }
                if let Some(step) = step {
                    self.p(":")?;
                    self.unparse_expr(step, precedence::TEST)?;
                }
            }
        }
        Ok(())
    }

    fn unparse_arguments<U>(&mut self, args: &Arguments<U>) -> fmt::Result {
        let mut first = true;
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            self.p_delim(&mut first, ", ")?;
            self.unparse_function_arg(arg)?;
            self.p_if(i + 1 == args.posonlyargs.len(), ", /")?;
        }
        if args.vararg.is_some() || !args.kwonlyargs.is_empty() {
            self.p_delim(&mut first, ", ")?;
            self.p("*")?;
        }
        if let Some(vararg) = &args.vararg {
            self.unparse_arg(vararg)?;
        }
        for kwarg in args.kwonlyargs.iter() {
            self.p_delim(&mut first, ", ")?;
            self.unparse_function_arg(kwarg)?;
        }
        if let Some(kwarg) = &args.kwarg {
            self.p_delim(&mut first, ", ")?;
            self.p("**")?;
            self.unparse_arg(kwarg)?;
        }
        Ok(())
    }
    fn unparse_function_arg<U>(&mut self, arg: &ArgWithDefault<U>) -> fmt::Result {
        self.p_id(&arg.def.arg)?;
        if let Some(ann) = &arg.def.annotation {
            write!(self, ": {}", **ann)?;
        }
        if let Some(default) = &arg.default {
            write!(self, "={}", default)?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn unparse_python_arguments<U>(&mut self, args: &PythonArguments<U>) -> fmt::Result {
        let mut first = true;
        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            self.p_delim(&mut first, ", ")?;
            self.unparse_arg(arg)?;
            if let Some(i) = i.checked_sub(defaults_start) {
                write!(self, "={}", &args.defaults[i])?;
            }
            self.p_if(i + 1 == args.posonlyargs.len(), ", /")?;
        }
        if args.vararg.is_some() || !args.kwonlyargs.is_empty() {
            self.p_delim(&mut first, ", ")?;
            self.p("*")?;
        }
        if let Some(vararg) = &args.vararg {
            self.unparse_arg(vararg)?;
        }
        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            self.p_delim(&mut first, ", ")?;
            self.unparse_arg(kwarg)?;
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.kw_defaults.get(i))
            {
                write!(self, "={default}")?;
            }
        }
        if let Some(kwarg) = &args.kwarg {
            self.p_delim(&mut first, ", ")?;
            self.p("**")?;
            self.unparse_arg(kwarg)?;
        }
        Ok(())
    }
    fn unparse_arg<U>(&mut self, arg: &Arg<U>) -> fmt::Result {
        self.p_id(&arg.arg)?;
        if let Some(ann) = &arg.annotation {
            write!(self, ": {}", **ann)?;
        }
        Ok(())
    }

    fn unparse_comp<U>(&mut self, generators: &[Comprehension<U>]) -> fmt::Result {
        for comp in generators {
            self.p(if comp.is_async {
                " async for "
            } else {
                " for "
            })?;
            self.unparse_expr(&comp.target, precedence::TUPLE)?;
            self.p(" in ")?;
            self.unparse_expr(&comp.iter, precedence::TEST + 1)?;
            for cond in &comp.ifs {
                self.p(" if ")?;
                self.unparse_expr(cond, precedence::TEST + 1)?;
            }
        }
        Ok(())
    }

    fn unparse_fstring_body<U>(&mut self, values: &[Expr<U>], is_spec: bool) -> fmt::Result {
        for value in values {
            self.unparse_fstring_elem(value, is_spec)?;
        }
        Ok(())
    }

    fn unparse_formatted<U>(
        &mut self,
        val: &Expr<U>,
        conversion: ConversionFlag,
        spec: Option<&Expr<U>>,
    ) -> fmt::Result {
        let buffered = to_string_fmt(|f| Unparser::new(f).unparse_expr(val, precedence::TEST + 1));
        let brace = if buffered.starts_with('{') {
            // put a space to avoid escaping the bracket
            "{ "
        } else {
            "{"
        };
        self.p(brace)?;
        self.p(&buffered)?;
        drop(buffered);

        if conversion != ConversionFlag::None {
            self.p("!")?;
            let buf = &[conversion as u8];
            let c = std::str::from_utf8(buf).unwrap();
            self.p(c)?;
        }

        if let Some(spec) = spec {
            self.p(":")?;
            self.unparse_fstring_elem(spec, true)?;
        }

        self.p("}")?;

        Ok(())
    }

    fn unparse_fstring_elem<U>(&mut self, expr: &Expr<U>, is_spec: bool) -> fmt::Result {
        match &expr {
            Expr::Constant(crate::ExprConstant { value, .. }) => {
                if let Constant::Str(s) = value {
                    self.unparse_fstring_str(s)
                } else {
                    unreachable!()
                }
            }
            Expr::JoinedStr(crate::ExprJoinedStr {
                values,
                range: _range,
            }) => self.unparse_joined_str(values, is_spec),
            Expr::FormattedValue(crate::ExprFormattedValue {
                value,
                conversion,
                format_spec,
                range: _range,
            }) => self.unparse_formatted(value, *conversion, format_spec.as_deref()),
            _ => unreachable!(),
        }
    }

    fn unparse_fstring_str(&mut self, s: &str) -> fmt::Result {
        let s = s.replace('{', "{{").replace('}', "}}");
        self.p(&s)
    }

    fn unparse_joined_str<U>(&mut self, values: &[Expr<U>], is_spec: bool) -> fmt::Result {
        if is_spec {
            self.unparse_fstring_body(values, is_spec)
        } else {
            self.p("f")?;
            let body = to_string_fmt(|f| Unparser::new(f).unparse_fstring_body(values, is_spec));
            rustpython_literal::escape::UnicodeEscape::new_repr(&body)
                .str_repr()
                .write(&mut self.f)
        }
    }
}

impl<U> fmt::Display for Expr<U> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Unparser::new(f).unparse_expr(self, precedence::TEST)
    }
}

fn to_string_fmt(f: impl FnOnce(&mut fmt::Formatter) -> fmt::Result) -> String {
    use std::cell::Cell;
    struct Fmt<F>(Cell<Option<F>>);
    impl<F: FnOnce(&mut fmt::Formatter) -> fmt::Result> fmt::Display for Fmt<F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.take().unwrap()(f)
        }
    }
    Fmt(Cell::new(Some(f))).to_string()
}
