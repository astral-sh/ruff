//! Generate Python source code from an expression's abstract syntax tree (AST).

use crate::precedence;
use ruff_python_ast::str::Quote;
use ruff_python_ast::visitor::source_order::TraversalSignal;
use ruff_python_ast::ParameterWithDefault;
use ruff_python_ast::{
    self as ast, ArgOrKeyword, BoolOp, CmpOp, Comprehension, ConversionFlag, DebugText, Expr,
    Identifier, Operator, Parameter, Parameters,
};
use ruff_python_literal::escape::{AsciiEscape, Escape, UnicodeEscape};

pub trait ExpressionGenerator<'a>
where
    Self: Sized,
{
    fn buffer(&mut self) -> &mut String;

    fn quote(&self) -> Quote;

    #[must_use]
    fn subgenerator(&self) -> Self;

    #[must_use]
    fn subgenerator_with_opposite_quotes(&self) -> Self;

    fn p(&mut self, s: &str) {
        *self.buffer() += s;
    }

    fn p_id(&mut self, s: &Identifier) {
        self.p(s.as_str());
    }

    fn p_bytes_repr(&mut self, s: &[u8]) {
        let escape = AsciiEscape::with_preferred_quote(s, self.quote());
        let buffer = self.buffer();
        if let Some(len) = escape.layout().len {
            buffer.reserve(len);
        }
        escape.bytes_repr().write(buffer).unwrap(); // write to string doesn't fail
    }

    fn p_str_repr(&mut self, s: &str) {
        let escape = UnicodeEscape::with_preferred_quote(s, self.quote());
        let buffer = self.buffer();
        if let Some(len) = escape.layout().len {
            buffer.reserve(len);
        }
        escape.str_repr().write(buffer).unwrap(); // write to string doesn't fail
    }

    fn p_if(&mut self, cond: bool, s: &str) {
        if cond {
            self.p(s);
        }
    }

    fn p_delim(&mut self, first: &mut bool, s: &str) {
        self.p_if(!std::mem::take(first), s);
    }

    fn enter_expr(&mut self, _expr: &Expr, _level: u8) -> TraversalSignal {
        TraversalSignal::Traverse
    }

    fn leave_expr(&mut self, _expr: &Expr, _level: u8) {}

    fn unparse_expr(&mut self, ast: &Expr, level: u8) {
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
                self.p_if(group, "(");
                let ret = $body;
                self.p_if(group, ")");
                ret
            }};
        }
        if !self.enter_expr(ast, level).is_traverse() {
            return;
        }
        match ast {
            Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _,
            }) => {
                let (op, prec) = opprec!(bin, op, BoolOp, And("and", AND), Or("or", OR));
                group_if!(prec, {
                    let mut first = true;
                    for val in values {
                        self.p_delim(&mut first, op);
                        self.unparse_expr(val, prec + 1);
                    }
                });
            }
            Expr::Named(ast::ExprNamed {
                target,
                value,
                range: _,
            }) => {
                group_if!(precedence::NAMED_EXPR, {
                    self.unparse_expr(target, precedence::NAMED_EXPR);
                    self.p(" := ");
                    self.unparse_expr(value, precedence::NAMED_EXPR + 1);
                });
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range: _,
            }) => {
                let rassoc = matches!(op, Operator::Pow);
                let (op, prec) = opprec!(
                    bin,
                    op,
                    Operator,
                    Add("+", ADD),
                    Sub("-", SUB),
                    Mult("*", MULT),
                    MatMult("@", MAT_MULT),
                    Div("/", DIV),
                    Mod("%", MOD),
                    Pow("**", POW),
                    LShift("<<", LSHIFT),
                    RShift(">>", RSHIFT),
                    BitOr("|", BIT_OR),
                    BitXor("^", BIT_XOR),
                    BitAnd("&", BIT_AND),
                    FloorDiv("//", FLOORDIV),
                );
                group_if!(prec, {
                    self.unparse_expr(left, prec + u8::from(rassoc));
                    self.p(op);
                    self.unparse_expr(right, prec + u8::from(!rassoc));
                });
            }
            Expr::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _,
            }) => {
                let (op, prec) = opprec!(
                    un,
                    op,
                    ruff_python_ast::UnaryOp,
                    Invert("~", INVERT),
                    Not("not ", NOT),
                    UAdd("+", UADD),
                    USub("-", USUB)
                );
                group_if!(prec, {
                    self.p(op);
                    self.unparse_expr(operand, prec);
                });
            }
            Expr::Lambda(ast::ExprLambda {
                parameters,
                body,
                range: _,
            }) => {
                group_if!(precedence::LAMBDA, {
                    self.p("lambda");
                    if let Some(parameters) = parameters {
                        self.p(" ");
                        self.unparse_parameters(parameters);
                    }
                    self.p(": ");
                    self.unparse_expr(body, precedence::LAMBDA);
                });
            }
            Expr::If(ast::ExprIf {
                test,
                body,
                orelse,
                range: _,
            }) => {
                group_if!(precedence::IF_EXP, {
                    self.unparse_expr(body, precedence::IF_EXP + 1);
                    self.p(" if ");
                    self.unparse_expr(test, precedence::IF_EXP + 1);
                    self.p(" else ");
                    self.unparse_expr(orelse, precedence::IF_EXP);
                });
            }
            Expr::Dict(dict) => {
                self.p("{");
                let mut first = true;
                for ast::DictItem { key, value } in dict {
                    self.p_delim(&mut first, ", ");
                    if let Some(key) = key {
                        self.unparse_expr(key, precedence::COMMA);
                        self.p(": ");
                        self.unparse_expr(value, precedence::COMMA);
                    } else {
                        self.p("**");
                        self.unparse_expr(value, precedence::MAX);
                    }
                }
                self.p("}");
            }
            Expr::Set(set) => {
                if set.is_empty() {
                    self.p("set()");
                } else {
                    self.p("{");
                    let mut first = true;
                    for item in set {
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(item, precedence::COMMA);
                    }
                    self.p("}");
                }
            }
            Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
            }) => {
                self.p("[");
                self.unparse_expr(elt, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p("]");
            }
            Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
            }) => {
                self.p("{");
                self.unparse_expr(elt, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p("}");
            }
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => {
                self.p("{");
                self.unparse_expr(key, precedence::COMPREHENSION_ELEMENT);
                self.p(": ");
                self.unparse_expr(value, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p("}");
            }
            Expr::Generator(ast::ExprGenerator {
                elt,
                generators,
                parenthesized: _,
                range: _,
            }) => {
                self.p("(");
                self.unparse_expr(elt, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p(")");
            }
            Expr::Await(ast::ExprAwait { value, range: _ }) => {
                group_if!(precedence::AWAIT, {
                    self.p("await ");
                    self.unparse_expr(value, precedence::MAX);
                });
            }
            Expr::Yield(ast::ExprYield { value, range: _ }) => {
                group_if!(precedence::YIELD, {
                    self.p("yield");
                    if let Some(value) = value {
                        self.p(" ");
                        self.unparse_expr(value, precedence::YIELD + 1);
                    }
                });
            }
            Expr::YieldFrom(ast::ExprYieldFrom { value, range: _ }) => {
                group_if!(precedence::YIELD_FROM, {
                    self.p("yield from ");
                    self.unparse_expr(value, precedence::MAX);
                });
            }
            Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _,
            }) => {
                group_if!(precedence::CMP, {
                    let new_lvl = precedence::CMP + 1;
                    self.unparse_expr(left, new_lvl);
                    for (op, cmp) in ops.iter().zip(comparators) {
                        let op = match op {
                            CmpOp::Eq => " == ",
                            CmpOp::NotEq => " != ",
                            CmpOp::Lt => " < ",
                            CmpOp::LtE => " <= ",
                            CmpOp::Gt => " > ",
                            CmpOp::GtE => " >= ",
                            CmpOp::Is => " is ",
                            CmpOp::IsNot => " is not ",
                            CmpOp::In => " in ",
                            CmpOp::NotIn => " not in ",
                        };
                        self.p(op);
                        self.unparse_expr(cmp, new_lvl);
                    }
                });
            }
            Expr::Call(ast::ExprCall {
                func,
                arguments,
                range: _,
            }) => {
                self.unparse_expr(func, precedence::MAX);
                self.p("(");
                if let (
                    [Expr::Generator(ast::ExprGenerator {
                        elt,
                        generators,
                        range: _,
                        parenthesized: _,
                    })],
                    [],
                ) = (arguments.args.as_ref(), arguments.keywords.as_ref())
                {
                    // Ensure that a single generator doesn't get double-parenthesized.
                    self.unparse_expr(elt, precedence::COMMA);
                    self.unparse_comp(generators);
                } else {
                    let mut first = true;

                    for arg_or_keyword in arguments.arguments_source_order() {
                        match arg_or_keyword {
                            ArgOrKeyword::Arg(arg) => {
                                self.p_delim(&mut first, ", ");
                                self.unparse_expr(arg, precedence::COMMA);
                            }
                            ArgOrKeyword::Keyword(keyword) => {
                                self.p_delim(&mut first, ", ");
                                if let Some(arg) = &keyword.arg {
                                    self.p_id(arg);
                                    self.p("=");
                                    self.unparse_expr(&keyword.value, precedence::COMMA);
                                } else {
                                    self.p("**");
                                    self.unparse_expr(&keyword.value, precedence::MAX);
                                }
                            }
                        }
                    }
                }
                self.p(")");
            }
            Expr::FString(ast::ExprFString { value, .. }) => {
                self.unparse_f_string_value(value, false);
            }
            Expr::StringLiteral(literal) => {
                self.unparse_string_literal_value(&literal.value);
            }
            Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
                let mut first = true;
                for bytes_literal in value {
                    self.p_delim(&mut first, " ");
                    self.p_bytes_repr(&bytes_literal.value);
                }
            }
            Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => {
                static INF_STR: &str = "1e309";
                assert_eq!(f64::MAX_10_EXP, 308);

                match value {
                    ast::Number::Int(i) => {
                        self.p(&format!("{i}"));
                    }
                    ast::Number::Float(fp) => {
                        if fp.is_infinite() {
                            self.p(INF_STR);
                        } else {
                            self.p(&ruff_python_literal::float::to_string(*fp));
                        }
                    }
                    ast::Number::Complex { real, imag } => {
                        let value = if *real == 0.0 {
                            format!("{imag}j")
                        } else {
                            format!("({real}{imag:+}j)")
                        };
                        if real.is_infinite() || imag.is_infinite() {
                            self.p(&value.replace("inf", INF_STR));
                        } else {
                            self.p(&value);
                        }
                    }
                }
            }
            Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => {
                self.p(if *value { "True" } else { "False" });
            }
            Expr::NoneLiteral(_) => {
                self.p("None");
            }
            Expr::EllipsisLiteral(_) => {
                self.p("...");
            }
            Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                if let Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(_),
                    ..
                }) = value.as_ref()
                {
                    self.p("(");
                    self.unparse_expr(value, precedence::MAX);
                    self.p(").");
                } else {
                    self.unparse_expr(value, precedence::MAX);
                    self.p(".");
                };
                self.p_id(attr);
            }
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                self.unparse_expr(value, precedence::MAX);
                self.p("[");
                self.unparse_expr(slice, precedence::SUBSCRIPT);
                self.p("]");
            }
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.p("*");
                self.unparse_expr(value, precedence::MAX);
            }
            Expr::Name(ast::ExprName { id, .. }) => self.p(id.as_str()),
            Expr::List(list) => {
                self.p("[");
                let mut first = true;
                for item in list {
                    self.p_delim(&mut first, ", ");
                    self.unparse_expr(item, precedence::COMMA);
                }
                self.p("]");
            }
            Expr::Tuple(tuple) => {
                if tuple.is_empty() {
                    self.p("()");
                } else {
                    group_if!(precedence::TUPLE, {
                        let mut first = true;
                        for item in tuple {
                            self.p_delim(&mut first, ", ");
                            self.unparse_expr(item, precedence::COMMA);
                        }
                        self.p_if(tuple.len() == 1, ",");
                    });
                }
            }
            Expr::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: _,
            }) => {
                if let Some(lower) = lower {
                    self.unparse_expr(lower, precedence::SLICE);
                }
                self.p(":");
                if let Some(upper) = upper {
                    self.unparse_expr(upper, precedence::SLICE);
                }
                if let Some(step) = step {
                    self.p(":");
                    self.unparse_expr(step, precedence::SLICE);
                }
            }
            Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand { kind, value, .. }) => {
                self.p(&format!("{kind}{value}"));
            }
        }
        self.leave_expr(ast, level);
    }

    fn unparse_parameters(&mut self, parameters: &Parameters) {
        let mut first = true;
        for (i, parameter_with_default) in parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .enumerate()
        {
            self.p_delim(&mut first, ", ");
            self.unparse_parameter_with_default(parameter_with_default);
            self.p_if(i + 1 == parameters.posonlyargs.len(), ", /");
        }
        if parameters.vararg.is_some() || !parameters.kwonlyargs.is_empty() {
            self.p_delim(&mut first, ", ");
            self.p("*");
        }
        if let Some(vararg) = &parameters.vararg {
            self.unparse_parameter(vararg);
        }
        for kwarg in &parameters.kwonlyargs {
            self.p_delim(&mut first, ", ");
            self.unparse_parameter_with_default(kwarg);
        }
        if let Some(kwarg) = &parameters.kwarg {
            self.p_delim(&mut first, ", ");
            self.p("**");
            self.unparse_parameter(kwarg);
        }
    }

    fn unparse_parameter(&mut self, parameter: &Parameter) {
        self.p_id(&parameter.name);
        if let Some(ann) = &parameter.annotation {
            self.p(": ");
            self.unparse_expr(ann, precedence::COMMA);
        }
    }

    fn unparse_parameter_with_default(&mut self, parameter_with_default: &ParameterWithDefault) {
        self.unparse_parameter(&parameter_with_default.parameter);
        if let Some(default) = &parameter_with_default.default {
            self.p("=");
            self.unparse_expr(default, precedence::COMMA);
        }
    }

    fn unparse_comp(&mut self, generators: &[Comprehension]) {
        for comp in generators {
            self.p(if comp.is_async {
                " async for "
            } else {
                " for "
            });
            self.unparse_expr(&comp.target, precedence::COMPREHENSION_TARGET);
            self.p(" in ");
            self.unparse_expr(&comp.iter, precedence::COMPREHENSION);
            for cond in &comp.ifs {
                self.p(" if ");
                self.unparse_expr(cond, precedence::COMPREHENSION);
            }
        }
    }

    fn unparse_string_literal(&mut self, string_literal: &ast::StringLiteral) {
        let ast::StringLiteral { value, flags, .. } = string_literal;
        if flags.prefix().is_unicode() {
            self.p("u");
        }
        self.p_str_repr(value);
    }

    fn unparse_string_literal_value(&mut self, value: &ast::StringLiteralValue) {
        let mut first = true;
        for string_literal in value {
            self.p_delim(&mut first, " ");
            self.unparse_string_literal(string_literal);
        }
    }

    fn unparse_f_string_value(&mut self, value: &ast::FStringValue, is_spec: bool) {
        let mut first = true;
        for f_string_part in value {
            self.p_delim(&mut first, " ");
            match f_string_part {
                ast::FStringPart::Literal(string_literal) => {
                    self.unparse_string_literal(string_literal);
                }
                ast::FStringPart::FString(f_string) => {
                    self.unparse_f_string(&f_string.elements, is_spec);
                }
            }
        }
    }

    fn unparse_f_string_body(&mut self, values: &[ast::FStringElement]) {
        for value in values {
            self.unparse_f_string_element(value);
        }
    }

    fn unparse_f_string_expression_element(
        &mut self,
        val: &Expr,
        debug_text: Option<&DebugText>,
        conversion: ConversionFlag,
        spec: Option<&ast::FStringFormatSpec>,
    ) {
        let mut generator = self.subgenerator();
        generator.unparse_expr(val, precedence::FORMATTED_VALUE);
        let val_buffer = generator.buffer();
        let brace = if val_buffer.starts_with('{') {
            // put a space to avoid escaping the bracket
            "{ "
        } else {
            "{"
        };
        self.p(brace);
        let buffer = self.buffer();

        if let Some(debug_text) = debug_text {
            *buffer += debug_text.leading.as_str();
        }

        *buffer += val_buffer;

        if let Some(debug_text) = debug_text {
            *buffer += debug_text.trailing.as_str();
        }

        if !conversion.is_none() {
            self.p("!");
            #[allow(clippy::cast_possible_truncation)]
            self.p(&format!("{}", conversion as u8 as char));
        }

        if let Some(spec) = spec {
            self.p(":");
            self.unparse_f_string(&spec.elements, true);
        }

        self.p("}");
    }

    fn unparse_f_string_element(&mut self, element: &ast::FStringElement) {
        match element {
            ast::FStringElement::Literal(ast::FStringLiteralElement { value, .. }) => {
                self.unparse_f_string_literal_element(value);
            }
            ast::FStringElement::Expression(ast::FStringExpressionElement {
                expression,
                debug_text,
                conversion,
                format_spec,
                range: _,
            }) => self.unparse_f_string_expression_element(
                expression,
                debug_text.as_ref(),
                *conversion,
                format_spec.as_deref(),
            ),
        }
    }

    fn unparse_f_string_literal_element(&mut self, s: &str) {
        let s = s.replace('{', "{{").replace('}', "}}");
        self.p(&s);
    }

    fn unparse_f_string(&mut self, values: &[ast::FStringElement], is_spec: bool) {
        if is_spec {
            self.unparse_f_string_body(values);
        } else {
            self.p("f");
            let mut generator = self.subgenerator_with_opposite_quotes();
            generator.unparse_f_string_body(values);
            let body = &generator.buffer();
            self.p_str_repr(body);
        }
    }
}
