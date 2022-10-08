use std::fmt;
use std::string::FromUtf8Error;

use anyhow::Result;
use rustpython_ast::{Excepthandler, ExcepthandlerKind, Suite, Withitem};
use rustpython_common::str;
use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, ConversionFlag, Expr, ExprKind,
    Operator, Stmt, StmtKind,
};

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

pub struct SourceGenerator {
    buffer: Vec<u8>,
    indentation: usize,
    new_lines: usize,
    initial: bool,
}

impl Default for SourceGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceGenerator {
    pub fn new() -> Self {
        SourceGenerator {
            buffer: vec![],
            indentation: 0,
            new_lines: 0,
            initial: true,
        }
    }

    pub fn generate(self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.buffer)
    }

    fn newline(&mut self) -> fmt::Result {
        if self.initial {
            self.initial = false;
        } else {
            self.new_lines = std::cmp::max(self.new_lines, 1);
        }
        Ok(())
    }

    fn newlines(&mut self, extra: usize) -> fmt::Result {
        if self.initial {
            self.initial = false;
        } else {
            self.new_lines = std::cmp::max(self.new_lines, 1 + extra);
        }
        Ok(())
    }

    fn body<U>(&mut self, stmts: &[Stmt<U>]) -> fmt::Result {
        self.indentation += 1;
        for stmt in stmts {
            self.unparse_stmt(stmt)?;
        }
        self.indentation -= 1;
        Ok(())
    }

    fn p(&mut self, s: &str) -> fmt::Result {
        if self.new_lines > 0 {
            for _ in 0..self.new_lines {
                self.buffer.extend("\n".as_bytes());
            }
            self.new_lines = 0;
        }
        self.buffer.extend(s.as_bytes());
        Ok(())
    }

    fn p_if(&mut self, cond: bool, s: &str) -> fmt::Result {
        if cond {
            self.p(s)?;
        }
        Ok(())
    }

    fn p_delim(&mut self, first: &mut bool, s: &str) -> fmt::Result {
        self.p_if(!std::mem::take(first), s)
    }

    fn write_fmt(&mut self, f: fmt::Arguments<'_>) -> fmt::Result {
        self.buffer.extend(format!("{}", f).as_bytes());
        Ok(())
    }

    pub fn unparse_suite<U>(&mut self, suite: &Suite<U>) -> fmt::Result {
        for stmt in suite {
            self.unparse_stmt(stmt)?;
        }
        Ok(())
    }

    fn unparse_stmt<U>(&mut self, ast: &Stmt<U>) -> fmt::Result {
        macro_rules! statement {
            ($body:block) => {{
                self.newline()?;
                self.p(&"    ".repeat(self.indentation))?;
                $body
            }};
        }

        match &ast.node {
            StmtKind::FunctionDef {
                name,
                args,
                body,
                returns,
                ..
            } => {
                // TODO(charlie): Handle decorators.
                self.newlines(if self.indentation == 0 { 2 } else { 1 })?;
                statement!({
                    self.p("def ")?;
                    self.p(name)?;
                    self.p("(")?;
                    self.unparse_args(args)?;
                    self.p(")")?;
                    if let Some(returns) = returns {
                        self.p(" -> ")?;
                        self.unparse_expr(returns, precedence::EXPR)?;
                    }
                    self.p(":")?;
                    self.body(body)?;

                    if self.indentation == 0 {
                        self.newlines(2)?;
                    }
                })
            }
            StmtKind::AsyncFunctionDef {
                name,
                args,
                body,
                returns,
                ..
            } => {
                // TODO(charlie): Handle decorators.
                self.newlines(if self.indentation == 0 { 2 } else { 1 })?;
                statement!({
                    self.p("async def ")?;
                    self.p(name)?;
                    self.p("(")?;
                    self.unparse_args(args)?;
                    self.p(")")?;
                    if let Some(returns) = returns {
                        self.p(" -> ")?;
                        self.unparse_expr(returns, precedence::EXPR)?;
                    }
                    self.p(":")?;
                    self.body(body)?;
                    if self.indentation == 0 {
                        self.newlines(2)?;
                    }
                })
            }
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                body,
                ..
            } => {
                // TODO(charlie): Handle decorators.
                self.newlines(if self.indentation == 0 { 2 } else { 1 })?;
                statement!({
                    self.p("class ")?;
                    self.p(name)?;
                    let mut first = true;
                    for base in bases {
                        self.p_if(first, "(")?;
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_expr(base, precedence::EXPR)?;
                    }
                    for keyword in keywords {
                        self.p_if(first, "(")?;
                        self.p_delim(&mut first, ", ")?;
                        if let Some(arg) = &keyword.node.arg {
                            self.p(arg)?;
                            self.p("=")?;
                        } else {
                            self.p("**")?;
                        }
                        self.unparse_expr(&keyword.node.value, precedence::EXPR)?;
                    }
                    self.p_if(!first, ")")?;
                    self.p(":")?;
                    self.body(body)?;
                    if self.indentation == 0 {
                        self.newlines(2)?;
                    }
                })
            }
            StmtKind::Return { value } => {
                statement!({
                    if let Some(expr) = value {
                        self.p("return ")?;
                        self.unparse_expr(expr, precedence::ATOM)?;
                    } else {
                        self.p("return")?;
                    }
                });
            }
            StmtKind::Delete { targets } => {
                statement!({
                    self.p("del ")?;
                    let mut first = true;
                    for expr in targets {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_expr(expr, precedence::ATOM)?;
                    }
                });
            }
            StmtKind::Assign { targets, value, .. } => {
                statement!({
                    for target in targets {
                        self.unparse_expr(target, precedence::EXPR)?;
                        self.p(" = ")?;
                    }
                    self.unparse_expr(value, precedence::EXPR)?;
                });
            }
            StmtKind::AugAssign { target, op, value } => {
                statement!({
                    self.unparse_expr(target, precedence::EXPR)?;
                    self.p(" ")?;
                    self.p(match op {
                        Operator::Add => "+",
                        Operator::Sub => "-",
                        Operator::Mult => "*",
                        Operator::MatMult => "@",
                        Operator::Div => "/",
                        Operator::Mod => "%",
                        Operator::Pow => "**",
                        Operator::LShift => "<<",
                        Operator::RShift => ">>",
                        Operator::BitOr => "|",
                        Operator::BitXor => "^",
                        Operator::BitAnd => "&",
                        Operator::FloorDiv => "//",
                    })?;
                    self.p("= ")?;
                    self.unparse_expr(value, precedence::EXPR)?;
                })
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => {
                statement!({
                    let need_parens = matches!(target.node, ExprKind::Name { .. }) && simple == &0;
                    self.p_if(need_parens, "(")?;
                    self.unparse_expr(target, precedence::EXPR)?;
                    self.p_if(need_parens, ")")?;
                    self.p(": ")?;
                    self.unparse_expr(annotation, precedence::EXPR)?;
                    if let Some(value) = value {
                        self.p(" = ")?;
                        self.unparse_expr(value, precedence::EXPR)?;
                    }
                })
            }
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
                ..
            } => {
                statement!({
                    self.p("for ")?;
                    self.unparse_expr(target, precedence::TEST)?;
                    self.p(" in ")?;
                    self.unparse_expr(iter, precedence::TEST)?;
                    self.p(":")?;
                    self.body(body)?;
                    if !orelse.is_empty() {
                        statement!({
                            self.p("else:")?;
                            self.body(orelse)?;
                        });
                    }
                })
            }
            StmtKind::AsyncFor {
                target,
                iter,
                body,
                orelse,
                ..
            } => {
                statement!({
                    self.p("async for ")?;
                    self.unparse_expr(target, precedence::TEST)?;
                    self.p(" in ")?;
                    self.unparse_expr(iter, precedence::TEST)?;
                    self.p(":")?;
                    self.body(body)?;
                    if !orelse.is_empty() {
                        statement!({
                            self.p("else:")?;
                            self.body(orelse)?;
                        });
                    }
                })
            }
            StmtKind::While { test, body, orelse } => {
                statement!({
                    self.p("while ")?;
                    self.unparse_expr(test, precedence::TEST)?;
                    self.p(":")?;
                    self.body(body)?;
                    if !orelse.is_empty() {
                        statement!({
                            self.p("else:")?;
                            self.body(orelse)?;
                        });
                    }
                })
            }
            StmtKind::If { test, body, orelse } => {
                statement!({
                    self.p("if ")?;
                    self.unparse_expr(test, precedence::TEST)?;
                    self.p(":")?;
                    self.body(body)?;

                    let mut orelse_: &Vec<Stmt<U>> = orelse;
                    loop {
                        if orelse_.len() == 1 && matches!(orelse_[0].node, StmtKind::If { .. }) {
                            if let StmtKind::If { body, test, orelse } = &orelse_[0].node {
                                statement!({
                                    self.p("elif ")?;
                                    self.unparse_expr(test, precedence::TEST)?;
                                    self.p(":")?;
                                    self.body(body)?;
                                });
                                orelse_ = orelse;
                            }
                        } else {
                            if !orelse_.is_empty() {
                                statement!({
                                    self.p("else:")?;
                                    self.body(orelse_)?;
                                });
                            }
                            break;
                        }
                    }
                });
            }
            StmtKind::With { items, body, .. } => {
                statement!({
                    self.p("with ")?;
                    let mut first = true;
                    for item in items {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_withitem(item)?;
                    }
                    self.p(":")?;
                    self.body(body)?;
                })
            }
            StmtKind::AsyncWith { items, body, .. } => {
                statement!({
                    self.p("async with ")?;
                    let mut first = true;
                    for item in items {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_withitem(item)?;
                    }
                    self.p(":")?;
                    self.body(body)?;
                })
            }
            StmtKind::Match { .. } => {}
            StmtKind::Raise { exc, cause } => {
                statement!({
                    self.p("raise")?;
                    if let Some(exc) = exc {
                        self.p(" ")?;
                        self.unparse_expr(exc, precedence::EXPR)?;
                    }
                    if let Some(cause) = cause {
                        self.p(" from ")?;
                        self.unparse_expr(cause, precedence::EXPR)?;
                    }
                });
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                statement!({
                    self.p("try:")?;
                    self.body(body)?;

                    for handler in handlers {
                        statement!({
                            self.unparse_excepthandler(handler)?;
                        });
                    }

                    if !orelse.is_empty() {
                        statement!({
                            self.p("else:")?;
                            self.body(orelse)?;
                        });
                    }
                    if !finalbody.is_empty() {
                        statement!({
                            self.p("finally:")?;
                            self.body(finalbody)?;
                        });
                    }
                })
            }
            StmtKind::Assert { test, msg } => {
                statement!({
                    self.p("assert ")?;
                    self.unparse_expr(test, precedence::TEST)?;
                    if let Some(msg) = msg {
                        self.p(", ")?;
                        self.unparse_expr(msg, precedence::TEST)?;
                    }
                })
            }
            StmtKind::Import { names } => {
                statement!({
                    self.p("import ")?;
                    let mut first = true;
                    for alias in names {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_alias(alias)?;
                    }
                });
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                statement!({
                    self.p("from ")?;
                    if let Some(level) = level {
                        self.p(&".".repeat(*level))?;
                    }
                    if let Some(module) = module {
                        self.p(module)?;
                    }
                    self.p(" import ")?;
                    let mut first = true;
                    for alias in names {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_alias(alias)?;
                    }
                })
            }
            StmtKind::Global { names } => {
                statement!({
                    self.p("global ")?;
                    let mut first = true;
                    for name in names {
                        self.p_delim(&mut first, ", ")?;
                        self.p(name)?;
                    }
                });
            }
            StmtKind::Nonlocal { names } => {
                statement!({
                    self.p("nonlocal ")?;
                    let mut first = true;
                    for name in names {
                        self.p_delim(&mut first, ", ")?;
                        self.p(name)?;
                    }
                });
            }
            StmtKind::Expr { value } => {
                statement!({
                    self.unparse_expr(value, 0)?;
                });
            }
            StmtKind::Pass => {
                statement!({
                    self.p("pass")?;
                });
            }
            StmtKind::Break => {
                statement!({
                    self.p("break")?;
                });
            }
            StmtKind::Continue => {
                statement!({
                    self.p("continue")?;
                });
            }
        }
        Ok(())
    }

    fn unparse_excepthandler<U>(&mut self, ast: &Excepthandler<U>) -> fmt::Result {
        match &ast.node {
            ExcepthandlerKind::ExceptHandler { type_, name, body } => {
                self.p("except")?;
                if let Some(type_) = type_ {
                    self.p(" ")?;
                    self.unparse_expr(type_, precedence::EXPR)?;
                }
                if let Some(name) = name {
                    self.p(" as ")?;
                    self.p(name)?;
                }
                self.p(":")?;
                self.body(body)?;
            }
        }
        Ok(())
    }

    pub fn unparse_expr<U>(&mut self, ast: &Expr<U>, level: u8) -> fmt::Result {
        macro_rules! opprec {
            ($opty:ident, $x:expr, $enu:path, $($var:ident($op:literal, $prec:ident)),*$(,)?) => {
                match $x {
                    $(<$enu>::$var => (opprec!(@space $opty, $op), precedence::$prec),)*
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
        match &ast.node {
            ExprKind::BoolOp { op, values } => {
                let (op, prec) = opprec!(bin, op, Boolop, And("and", AND), Or("or", OR));
                group_if!(prec, {
                    let mut first = true;
                    for val in values {
                        self.p_delim(&mut first, op)?;
                        self.unparse_expr(val, prec + 1)?;
                    }
                })
            }
            ExprKind::NamedExpr { target, value } => {
                group_if!(precedence::TUPLE, {
                    self.unparse_expr(target, precedence::ATOM)?;
                    self.p(" := ")?;
                    self.unparse_expr(value, precedence::ATOM)?;
                })
            }
            ExprKind::BinOp { left, op, right } => {
                let rassoc = matches!(op, Operator::Pow);
                let (op, prec) = opprec!(
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
                    self.unparse_expr(left, prec + rassoc as u8)?;
                    self.p(op)?;
                    self.unparse_expr(right, prec + !rassoc as u8)?;
                })
            }
            ExprKind::UnaryOp { op, operand } => {
                let (op, prec) = opprec!(
                    un,
                    op,
                    rustpython_ast::Unaryop,
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
            ExprKind::Lambda { args, body } => {
                group_if!(precedence::TEST, {
                    let npos = args.args.len() + args.posonlyargs.len();
                    self.p(if npos > 0 { "lambda " } else { "lambda" })?;
                    self.unparse_args(args)?;
                    write!(self, ": {}", **body)?;
                })
            }
            ExprKind::IfExp { test, body, orelse } => {
                group_if!(precedence::TEST, {
                    self.unparse_expr(body, precedence::TEST + 1)?;
                    self.p(" if ")?;
                    self.unparse_expr(test, precedence::TEST + 1)?;
                    self.p(" else ")?;
                    self.unparse_expr(orelse, precedence::TEST)?;
                })
            }
            ExprKind::Dict { keys, values } => {
                self.p("{")?;
                let mut first = true;
                let (packed, unpacked) = values.split_at(keys.len());
                for (k, v) in keys.iter().zip(packed) {
                    self.p_delim(&mut first, ", ")?;
                    write!(self, "{}: {}", *k, *v)?;
                }
                for d in unpacked {
                    self.p_delim(&mut first, ", ")?;
                    write!(self, "**{}", *d)?;
                }
                self.p("}")?;
            }
            ExprKind::Set { elts } => {
                if elts.is_empty() {
                    self.p("set()")?;
                } else {
                    self.p("{")?;
                    let mut first = true;
                    for v in elts {
                        self.p_delim(&mut first, ", ")?;
                        self.unparse_expr(v, precedence::TEST)?;
                    }
                    self.p("}")?;
                }
            }
            ExprKind::ListComp { elt, generators } => {
                self.p("[")?;
                self.unparse_expr(elt, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p("]")?;
            }
            ExprKind::SetComp { elt, generators } => {
                self.p("{")?;
                self.unparse_expr(elt, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p("}")?;
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.p("{")?;
                self.unparse_expr(key, precedence::TEST)?;
                self.p(": ")?;
                self.unparse_expr(value, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p("}")?;
            }
            ExprKind::GeneratorExp { elt, generators } => {
                self.p("(")?;
                self.unparse_expr(elt, precedence::TEST)?;
                self.unparse_comp(generators)?;
                self.p(")")?;
            }
            ExprKind::Await { value } => {
                group_if!(precedence::AWAIT, {
                    self.p("await ")?;
                    self.unparse_expr(value, precedence::ATOM)?;
                })
            }
            ExprKind::Yield { value } => {
                if let Some(value) = value {
                    write!(self, "(yield {})", **value)?;
                } else {
                    self.p("(yield)")?;
                }
            }
            ExprKind::YieldFrom { value } => {
                write!(self, "(yield from {})", **value)?;
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                group_if!(precedence::CMP, {
                    let new_lvl = precedence::CMP + 1;
                    self.unparse_expr(left, new_lvl)?;
                    for (op, cmp) in ops.iter().zip(comparators) {
                        let op = match op {
                            Cmpop::Eq => " == ",
                            Cmpop::NotEq => " != ",
                            Cmpop::Lt => " < ",
                            Cmpop::LtE => " <= ",
                            Cmpop::Gt => " > ",
                            Cmpop::GtE => " >= ",
                            Cmpop::Is => " is ",
                            Cmpop::IsNot => " is not ",
                            Cmpop::In => " in ",
                            Cmpop::NotIn => " not in ",
                        };
                        self.p(op)?;
                        self.unparse_expr(cmp, new_lvl)?;
                    }
                })
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                self.unparse_expr(func, precedence::ATOM)?;
                self.p("(")?;
                if let (
                    [Expr {
                        node: ExprKind::GeneratorExp { elt, generators },
                        ..
                    }],
                    [],
                ) = (&**args, &**keywords)
                {
                    // make sure a single genexp doesn't get double parens
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
                        if let Some(arg) = &kw.node.arg {
                            self.p(arg)?;
                            self.p("=")?;
                        } else {
                            self.p("**")?;
                        }
                        self.unparse_expr(&kw.node.value, precedence::TEST)?;
                    }
                }
                self.p(")")?;
            }
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => self.unparse_formatted(value, *conversion, format_spec.as_deref())?,
            ExprKind::JoinedStr { values } => self.unparse_joinedstr(values, false)?,
            ExprKind::Constant { value, kind } => {
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
                    _ => self.p(&format!("{}", value))?,
                }
            }
            ExprKind::Attribute { value, attr, .. } => {
                self.unparse_expr(value, precedence::ATOM)?;
                let period = if let ExprKind::Constant {
                    value: Constant::Int(_),
                    ..
                } = &value.node
                {
                    " ."
                } else {
                    "."
                };
                self.p(period)?;
                self.p(attr)?;
            }
            ExprKind::Subscript { value, slice, .. } => {
                self.unparse_expr(value, precedence::ATOM)?;
                let mut lvl = precedence::TUPLE;
                if let ExprKind::Tuple { elts, .. } = &slice.node {
                    if elts
                        .iter()
                        .any(|expr| matches!(expr.node, ExprKind::Starred { .. }))
                    {
                        lvl += 1
                    }
                }
                self.p("[")?;
                self.unparse_expr(slice, lvl)?;
                self.p("]")?;
            }
            ExprKind::Starred { value, .. } => {
                self.p("*")?;
                self.unparse_expr(value, precedence::EXPR)?;
            }
            ExprKind::Name { id, .. } => self.p(id)?,
            ExprKind::List { elts, .. } => {
                self.p("[")?;
                let mut first = true;
                for elt in elts {
                    self.p_delim(&mut first, ", ")?;
                    self.unparse_expr(elt, precedence::TEST)?;
                }
                self.p("]")?;
            }
            ExprKind::Tuple { elts, .. } => {
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
            ExprKind::Slice { lower, upper, step } => {
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

    fn unparse_args<U>(&mut self, args: &Arguments<U>) -> fmt::Result {
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
                write!(self, "={}", default)?;
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
        self.p(&arg.node.arg)?;
        if let Some(ann) = &arg.node.annotation {
            write!(self, ": {}", **ann)?;
        }
        Ok(())
    }

    fn unparse_comp<U>(&mut self, generators: &[Comprehension<U>]) -> fmt::Result {
        for comp in generators {
            self.p(if comp.is_async > 0 {
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
        conversion: usize,
        spec: Option<&Expr<U>>,
    ) -> fmt::Result {
        let mut generator: SourceGenerator = Default::default();
        generator.unparse_expr(val, precedence::TEST + 1)?;
        let brace = if generator.buffer.starts_with("{".as_bytes()) {
            // put a space to avoid escaping the bracket
            "{ "
        } else {
            "{"
        };
        self.p(brace)?;
        self.buffer.extend(generator.buffer);

        if conversion != ConversionFlag::None as usize {
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
        match &expr.node {
            ExprKind::Constant { value, .. } => {
                if let Constant::Str(s) = value {
                    self.unparse_fstring_str(s)
                } else {
                    unreachable!()
                }
            }
            ExprKind::JoinedStr { values } => self.unparse_joinedstr(values, is_spec),
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => self.unparse_formatted(value, *conversion, format_spec.as_deref()),
            _ => unreachable!(),
        }
    }

    fn unparse_fstring_str(&mut self, s: &str) -> fmt::Result {
        let s = s.replace('{', "{{").replace('}', "}}");
        self.p(&s)
    }

    fn unparse_joinedstr<U>(&mut self, values: &[Expr<U>], is_spec: bool) -> fmt::Result {
        if is_spec {
            self.unparse_fstring_body(values, is_spec)?;
        } else {
            self.p("f")?;
            let mut generator: SourceGenerator = Default::default();
            generator.unparse_fstring_body(values, is_spec)?;
            let body = std::str::from_utf8(&generator.buffer).unwrap();
            self.p(&format!("{}", str::repr(body)))?;
        }
        Ok(())
    }

    fn unparse_alias<U>(&mut self, alias: &Alias<U>) -> fmt::Result {
        self.p(&alias.node.name)?;
        if let Some(asname) = &alias.node.asname {
            self.p(" as ")?;
            self.p(asname)?;
        }
        Ok(())
    }

    fn unparse_withitem<U>(&mut self, withitem: &Withitem<U>) -> fmt::Result {
        self.unparse_expr(&withitem.context_expr, precedence::EXPR)?;
        if let Some(optional_vars) = &withitem.optional_vars {
            self.p(" as ")?;
            self.unparse_expr(optional_vars, precedence::EXPR)?;
        }
        Ok(())
    }
}
