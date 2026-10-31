use ruff_text_size::Ranged;

use crate::visitor::source_order::SourceOrderVisitor;
use crate::{self as ast, Alias, AnyParameterRef, ArgOrKeyword, PatternKeyword};

impl ast::ElifElseClause {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ElifElseClause {
            range: _,
            node_index: _,
            test,
            body,
        } = self;
        if let Some(test) = test {
            visitor.visit_expr(test);
        }
        visitor.visit_body(body);
    }
}

impl ast::ExprDict {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDict {
            items,
            range: _,
            node_index: _,
        } = self;

        for ast::DictItem { key, value } in items {
            if let Some(key) = key {
                visitor.visit_expr(key);
            }
            visitor.visit_expr(value);
        }
    }
}

impl ast::ExprBoolOp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBoolOp {
            op,
            values,
            range: _,
            node_index: _,
        } = self;
        match values.as_slice() {
            [left, rest @ ..] => {
                visitor.visit_expr(left);
                visitor.visit_bool_op(op);
                for expr in rest {
                    visitor.visit_expr(expr);
                }
            }
            [] => {
                visitor.visit_bool_op(op);
            }
        }
    }
}

impl ast::ExprCompare {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        visitor.visit_expr(self.first_operand());
        for (_, op, right) in self.iter() {
            visitor.visit_cmp_op(op);
            visitor.visit_expr(right);
        }
    }
}

impl ast::InterpolatedStringFormatSpec {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for element in &self.elements {
            visitor.visit_interpolated_string_element(element);
        }
    }
}

impl ast::InterpolatedElement {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::InterpolatedElement {
            expression,
            format_spec,
            ..
        } = self;
        visitor.visit_expr(expression);

        if let Some(format_spec) = format_spec {
            for spec_part in &format_spec.elements {
                visitor.visit_interpolated_string_element(spec_part);
            }
        }
    }
}

impl ast::InterpolatedStringLiteralElement {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::InterpolatedStringLiteralElement {
            range: _,
            node_index: _,
            value: _,
        } = self;
    }
}

impl ast::ExprFString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprFString {
            value,
            range: _,
            node_index: _,
        } = self;

        for f_string_part in value {
            match f_string_part {
                ast::FStringPartRef::Literal(string_literal) => {
                    visitor.visit_string_literal(string_literal);
                }
                ast::FStringPartRef::FString(f_string) => {
                    visitor.visit_f_string(f_string);
                }
            }
        }
    }
}

impl ast::ExprTString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprTString {
            value,
            range: _,
            node_index: _,
        } = self;

        for t_string in value {
            visitor.visit_t_string(t_string);
        }
    }
}

impl ast::ExprStringLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStringLiteral {
            value,
            range: _,
            node_index: _,
        } = self;

        for string_literal in value {
            visitor.visit_string_literal(string_literal);
        }
    }
}

impl ast::ExprBytesLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBytesLiteral {
            value,
            range: _,
            node_index: _,
        } = self;

        for bytes_literal in value {
            visitor.visit_bytes_literal(bytes_literal);
        }
    }
}

impl ast::ExceptHandlerExceptHandler {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExceptHandlerExceptHandler {
            range: _,
            node_index: _,
            type_,
            name,
            body,
        } = self;
        if let Some(expr) = type_ {
            visitor.visit_expr(expr);
        }

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }

        visitor.visit_body(body);
    }
}

impl ast::PatternMatchMapping {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchMapping {
            keys,
            patterns,
            rest,
            range: _,
            node_index: _,
        } = self;

        let mut rest = rest.as_ref();

        for (key, pattern) in keys.iter().zip(patterns) {
            if let Some(rest_identifier) = rest {
                if rest_identifier.start() < key.start() {
                    visitor.visit_identifier(rest_identifier);
                    rest = None;
                }
            }
            visitor.visit_expr(key);
            visitor.visit_pattern(pattern);
        }

        if let Some(rest) = rest {
            visitor.visit_identifier(rest);
        }
    }
}

impl ast::PatternArguments {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for pattern_or_keyword in self.iter_source_order() {
            match pattern_or_keyword {
                crate::PatternOrKeyword::Pattern(pattern) => visitor.visit_pattern(pattern),
                crate::PatternOrKeyword::Keyword(keyword) => {
                    visitor.visit_pattern_keyword(keyword);
                }
            }
        }
    }
}

impl ast::PatternKeyword {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternKeyword {
            range: _,
            node_index: _,
            attr,
            pattern,
        } = self;

        visitor.visit_identifier(attr);
        visitor.visit_pattern(pattern);
    }
}

impl ast::Comprehension {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Comprehension {
            range: _,
            node_index: _,
            target,
            iter,
            ifs,
            is_async: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(iter);

        for expr in ifs {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::Arguments {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for arg_or_keyword in self.iter_source_order() {
            match arg_or_keyword {
                ArgOrKeyword::Arg(arg) => visitor.visit_expr(arg),
                ArgOrKeyword::Keyword(keyword) => visitor.visit_keyword(keyword),
            }
        }
    }
}

impl ast::Parameters {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for parameter in self.iter_source_order() {
            match parameter {
                AnyParameterRef::NonVariadic(parameter_with_default) => {
                    visitor.visit_parameter_with_default(parameter_with_default);
                }
                AnyParameterRef::Variadic(parameter) => visitor.visit_parameter(parameter),
            }
        }
    }
}

impl ast::Parameter {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Parameter {
            range: _,
            node_index: _,
            name,
            annotation,
        } = self;

        visitor.visit_identifier(name);
        if let Some(expr) = annotation {
            visitor.visit_annotation(expr);
        }
    }
}

impl ast::ParameterWithDefault {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ParameterWithDefault {
            range: _,
            node_index: _,
            parameter,
            default,
        } = self;
        visitor.visit_parameter(parameter);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::Keyword {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Keyword {
            range: _,
            node_index: _,
            arg,
            value,
        } = self;

        if let Some(arg) = arg {
            visitor.visit_identifier(arg);
        }
        visitor.visit_expr(value);
    }
}

impl Alias {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Alias {
            range: _,
            node_index: _,
            name,
            asname,
        } = self;

        visitor.visit_identifier(name);
        if let Some(asname) = asname {
            visitor.visit_identifier(asname);
        }
    }
}

impl ast::WithItem {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::WithItem {
            range: _,
            node_index: _,
            context_expr,
            optional_vars,
        } = self;

        visitor.visit_expr(context_expr);

        if let Some(expr) = optional_vars {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::MatchCase {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::MatchCase {
            range: _,
            node_index: _,
            pattern,
            guard,
            body,
        } = self;

        visitor.visit_pattern(pattern);
        if let Some(expr) = guard {
            visitor.visit_expr(expr);
        }
        visitor.visit_body(body);
    }
}

impl ast::Decorator {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Decorator {
            range: _,
            node_index: _,
            expression,
        } = self;

        visitor.visit_expr(expression);
    }
}

impl ast::TypeParams {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParams {
            range: _,
            node_index: _,
            type_params,
        } = self;

        for type_param in type_params {
            visitor.visit_type_param(type_param);
        }
    }
}

impl ast::FString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FString {
            elements,
            range: _,
            node_index: _,
            flags: _,
        } = self;

        for fstring_element in elements {
            visitor.visit_interpolated_string_element(fstring_element);
        }
    }
}

impl ast::TString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TString {
            elements,
            range: _,
            node_index: _,
            flags: _,
        } = self;

        for tstring_element in elements {
            visitor.visit_interpolated_string_element(tstring_element);
        }
    }
}

impl ast::StringLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StringLiteral {
            range: _,
            node_index: _,
            value: _,
            flags: _,
        } = self;
    }
}

impl ast::BytesLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::BytesLiteral {
            range: _,
            node_index: _,
            value: _,
            flags: _,
        } = self;
    }
}

impl ast::Identifier {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::Identifier {
            range: _,
            node_index: _,
            id: _,
        } = self;
    }
}
