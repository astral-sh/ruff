use ruff_text_size::Ranged;

use crate::visitor::source_order::SourceOrderVisitor;
use crate::{
    self as ast, Alias, AnyNodeRef, AnyParameterRef, ArgOrKeyword, MatchCase, PatternArguments,
    PatternKeyword,
};

impl ast::ModModule {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ModModule { body, range: _ } = self;
        visitor.visit_body(body);
    }
}

impl ast::ModExpression {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ModExpression { body, range: _ } = self;
        visitor.visit_expr(body);
    }
}

impl ast::StmtFunctionDef {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtFunctionDef {
            parameters,
            body,
            decorator_list,
            returns,
            type_params,
            range: _,
            is_async: _,
            name,
        } = self;

        for decorator in decorator_list {
            visitor.visit_decorator(decorator);
        }

        visitor.visit_identifier(name);

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        visitor.visit_parameters(parameters);

        if let Some(expr) = returns {
            visitor.visit_annotation(expr);
        }

        visitor.visit_body(body);
    }
}

impl ast::StmtClassDef {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtClassDef {
            arguments,
            body,
            decorator_list,
            type_params,
            name,
            range: _,
        } = self;

        for decorator in decorator_list {
            visitor.visit_decorator(decorator);
        }

        visitor.visit_identifier(name);

        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }

        if let Some(arguments) = arguments {
            visitor.visit_arguments(arguments);
        }

        visitor.visit_body(body);
    }
}

impl ast::StmtReturn {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtReturn { value, range: _ } = self;
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::StmtDelete {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtDelete { targets, range: _ } = self;
        for expr in targets {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::StmtTypeAlias {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtTypeAlias {
            range: _,
            name,
            type_params,
            value,
        } = self;

        visitor.visit_expr(name);
        if let Some(type_params) = type_params {
            visitor.visit_type_params(type_params);
        }
        visitor.visit_expr(value);
    }
}

impl ast::StmtAssign {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAssign {
            targets,
            value,
            range: _,
        } = self;

        for expr in targets {
            visitor.visit_expr(expr);
        }

        visitor.visit_expr(value);
    }
}

impl ast::StmtAugAssign {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAugAssign {
            target,
            op,
            value,
            range: _,
        } = self;

        visitor.visit_expr(target);
        visitor.visit_operator(op);
        visitor.visit_expr(value);
    }
}

impl ast::StmtAnnAssign {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAnnAssign {
            target,
            annotation,
            value,
            range: _,
            simple: _,
        } = self;

        visitor.visit_expr(target);
        visitor.visit_annotation(annotation);
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::StmtFor {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            range: _,
            is_async: _,
        } = self;

        visitor.visit_expr(target);
        visitor.visit_expr(iter);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}

impl ast::StmtWhile {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtWhile {
            test,
            body,
            orelse,
            range: _,
        } = self;

        visitor.visit_expr(test);
        visitor.visit_body(body);
        visitor.visit_body(orelse);
    }
}

impl ast::StmtIf {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
        } = self;

        visitor.visit_expr(test);
        visitor.visit_body(body);
        for clause in elif_else_clauses {
            visitor.visit_elif_else_clause(clause);
        }
    }
}

impl ast::ElifElseClause {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ElifElseClause {
            range: _,
            test,
            body,
        } = self;
        if let Some(test) = test {
            visitor.visit_expr(test);
        }
        visitor.visit_body(body);
    }
}

impl ast::StmtWith {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtWith {
            items,
            body,
            is_async: _,
            range: _,
        } = self;

        for with_item in items {
            visitor.visit_with_item(with_item);
        }
        visitor.visit_body(body);
    }
}

impl ast::StmtMatch {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtMatch {
            subject,
            cases,
            range: _,
        } = self;

        visitor.visit_expr(subject);
        for match_case in cases {
            visitor.visit_match_case(match_case);
        }
    }
}

impl ast::StmtRaise {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtRaise {
            exc,
            cause,
            range: _,
        } = self;

        if let Some(expr) = exc {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = cause {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::StmtTry {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
        } = self;

        visitor.visit_body(body);
        for except_handler in handlers {
            visitor.visit_except_handler(except_handler);
        }
        visitor.visit_body(orelse);
        visitor.visit_body(finalbody);
    }
}

impl ast::StmtAssert {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtAssert {
            test,
            msg,
            range: _,
        } = self;
        visitor.visit_expr(test);
        if let Some(expr) = msg {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::StmtImport {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtImport { names, range: _ } = self;

        for alias in names {
            visitor.visit_alias(alias);
        }
    }
}

impl ast::StmtImportFrom {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtImportFrom {
            range: _,
            module,
            names,
            level: _,
        } = self;

        if let Some(module) = module {
            visitor.visit_identifier(module);
        }

        for alias in names {
            visitor.visit_alias(alias);
        }
    }
}

impl ast::StmtGlobal {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtGlobal { range: _, names } = self;

        for name in names {
            visitor.visit_identifier(name);
        }
    }
}

impl ast::StmtNonlocal {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtNonlocal { range: _, names } = self;

        for name in names {
            visitor.visit_identifier(name);
        }
    }
}

impl ast::StmtExpr {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtExpr { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}

impl ast::StmtPass {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtPass { range: _ } = self;
    }
}

impl ast::StmtBreak {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtBreak { range: _ } = self;
    }
}

impl ast::StmtContinue {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtContinue { range: _ } = self;
    }
}

impl ast::StmtIpyEscapeCommand {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::StmtIpyEscapeCommand {
            range: _,
            kind: _,
            value: _,
        } = self;
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

impl ast::ExprNamed {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNamed {
            target,
            value,
            range: _,
        } = self;
        visitor.visit_expr(target);
        visitor.visit_expr(value);
    }
}

impl ast::ExprBinOp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
        } = self;
        visitor.visit_expr(left);
        visitor.visit_operator(op);
        visitor.visit_expr(right);
    }
}

impl ast::ExprUnaryOp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprUnaryOp {
            op,
            operand,
            range: _,
        } = self;

        visitor.visit_unary_op(op);
        visitor.visit_expr(operand);
    }
}

impl ast::ExprLambda {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprLambda {
            parameters,
            body,
            range: _,
        } = self;

        if let Some(parameters) = parameters {
            visitor.visit_parameters(parameters);
        }
        visitor.visit_expr(body);
    }
}

impl ast::ExprIf {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprIf {
            test,
            body,
            orelse,
            range: _,
        } = self;

        // `body if test else orelse`
        visitor.visit_expr(body);
        visitor.visit_expr(test);
        visitor.visit_expr(orelse);
    }
}

impl ast::ExprDict {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDict { items, range: _ } = self;

        for ast::DictItem { key, value } in items {
            if let Some(key) = key {
                visitor.visit_expr(key);
            }
            visitor.visit_expr(value);
        }
    }
}

impl ast::ExprSet {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSet { elts, range: _ } = self;

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::ExprListComp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprListComp {
            elt,
            generators,
            range: _,
        } = self;

        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl ast::ExprSetComp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSetComp {
            elt,
            generators,
            range: _,
        } = self;

        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl ast::ExprDictComp {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprDictComp {
            key,
            value,
            generators,
            range: _,
        } = self;

        visitor.visit_expr(key);
        visitor.visit_expr(value);

        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl ast::ExprGenerator {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprGenerator {
            elt,
            generators,
            range: _,
            parenthesized: _,
        } = self;
        visitor.visit_expr(elt);
        for comprehension in generators {
            visitor.visit_comprehension(comprehension);
        }
    }
}

impl ast::ExprAwait {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprAwait { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}

impl ast::ExprYield {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprYield { value, range: _ } = self;
        if let Some(expr) = value {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::ExprYieldFrom {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprYieldFrom { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}

impl ast::ExprCompare {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
        } = self;

        visitor.visit_expr(left);

        for (op, comparator) in ops.iter().zip(comparators) {
            visitor.visit_cmp_op(op);
            visitor.visit_expr(comparator);
        }
    }
}

impl ast::ExprCall {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprCall {
            func,
            arguments,
            range: _,
        } = self;
        visitor.visit_expr(func);
        visitor.visit_arguments(arguments);
    }
}

impl ast::FStringFormatSpec {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        for element in &self.elements {
            visitor.visit_f_string_element(element);
        }
    }
}

impl ast::FStringExpressionElement {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FStringExpressionElement {
            expression,
            format_spec,
            ..
        } = self;
        visitor.visit_expr(expression);

        if let Some(format_spec) = format_spec {
            for spec_part in &format_spec.elements {
                visitor.visit_f_string_element(spec_part);
            }
        }
    }
}

impl ast::FStringLiteralElement {
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::FStringLiteralElement { range: _, value: _ } = self;
    }
}

impl ast::ExprFString {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprFString { value, range: _ } = self;

        for f_string_part in value {
            match f_string_part {
                ast::FStringPart::Literal(string_literal) => {
                    visitor.visit_string_literal(string_literal);
                }
                ast::FStringPart::FString(f_string) => {
                    visitor.visit_f_string(f_string);
                }
            }
        }
    }
}

impl ast::ExprStringLiteral {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStringLiteral { value, range: _ } = self;

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
        let ast::ExprBytesLiteral { value, range: _ } = self;

        for bytes_literal in value {
            visitor.visit_bytes_literal(bytes_literal);
        }
    }
}

impl ast::ExprNumberLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNumberLiteral { range: _, value: _ } = self;
    }
}

impl ast::ExprBooleanLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprBooleanLiteral { range: _, value: _ } = self;
    }
}

impl ast::ExprNoneLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprNoneLiteral { range: _ } = self;
    }
}

impl ast::ExprEllipsisLiteral {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprEllipsisLiteral { range: _ } = self;
    }
}

impl ast::ExprAttribute {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprAttribute {
            value,
            attr,
            ctx: _,
            range: _,
        } = self;

        visitor.visit_expr(value);
        visitor.visit_identifier(attr);
    }
}

impl ast::ExprSubscript {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _,
        } = self;
        visitor.visit_expr(value);
        visitor.visit_expr(slice);
    }
}

impl ast::ExprStarred {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprStarred {
            value,
            ctx: _,
            range: _,
        } = self;

        visitor.visit_expr(value);
    }
}

impl ast::ExprName {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprName {
            range: _,
            id: _,
            ctx: _,
        } = self;
    }
}

impl ast::ExprList {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprList {
            elts,
            ctx: _,
            range: _,
        } = self;

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::ExprTuple {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprTuple {
            elts,
            ctx: _,
            range: _,
            parenthesized: _,
        } = self;

        for expr in elts {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::ExprSlice {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
        } = self;

        if let Some(expr) = lower {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = upper {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = step {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::ExprIpyEscapeCommand {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, _visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExprIpyEscapeCommand {
            range: _,
            kind: _,
            value: _,
        } = self;
    }
}

impl ast::ExceptHandlerExceptHandler {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::ExceptHandlerExceptHandler {
            range: _,
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

impl ast::PatternMatchValue {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchValue { value, range: _ } = self;
        visitor.visit_expr(value);
    }
}

impl ast::PatternMatchSingleton {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSingleton { value, range: _ } = self;
        visitor.visit_singleton(value);
    }
}

impl ast::PatternMatchSequence {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchSequence { patterns, range: _ } = self;
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
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

impl ast::PatternMatchClass {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchClass {
            cls,
            arguments: parameters,
            range: _,
        } = self;
        visitor.visit_expr(cls);
        visitor.visit_pattern_arguments(parameters);
    }
}

impl ast::PatternMatchStar {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchStar { range: _, name } = self;

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }
    }
}

impl ast::PatternMatchAs {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchAs {
            pattern,
            range: _,
            name,
        } = self;
        if let Some(pattern) = pattern {
            visitor.visit_pattern(pattern);
        }

        if let Some(name) = name {
            visitor.visit_identifier(name);
        }
    }
}

impl ast::PatternMatchOr {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::PatternMatchOr { patterns, range: _ } = self;
        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }
    }
}

impl ast::PatternArguments {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let PatternArguments {
            range: _,
            patterns,
            keywords,
        } = self;

        for pattern in patterns {
            visitor.visit_pattern(pattern);
        }

        for keyword in keywords {
            visitor.visit_pattern_keyword(keyword);
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
        for arg_or_keyword in self.arguments_source_order() {
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
        for parameter in self {
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
            type_params,
        } = self;

        for type_param in type_params {
            visitor.visit_type_param(type_param);
        }
    }
}

impl ast::TypeParamTypeVar {
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVar {
            bound,
            default,
            name,
            range: _,
        } = self;

        visitor.visit_identifier(name);
        if let Some(expr) = bound {
            visitor.visit_expr(expr);
        }
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::TypeParamTypeVarTuple {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamTypeVarTuple {
            range: _,
            name,
            default,
        } = self;
        visitor.visit_identifier(name);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
        }
    }
}

impl ast::TypeParamParamSpec {
    #[inline]
    pub(crate) fn visit_source_order<'a, V>(&'a self, visitor: &mut V)
    where
        V: SourceOrderVisitor<'a> + ?Sized,
    {
        let ast::TypeParamParamSpec {
            range: _,
            name,
            default,
        } = self;
        visitor.visit_identifier(name);
        if let Some(expr) = default {
            visitor.visit_expr(expr);
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
            flags: _,
        } = self;

        for fstring_element in elements {
            visitor.visit_f_string_element(fstring_element);
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
        let ast::Identifier { range: _, id: _ } = self;
    }
}

impl<'a> AnyNodeRef<'a> {
    /// Compares two any node refs by their pointers (referential equality).
    pub fn ptr_eq(self, other: AnyNodeRef) -> bool {
        self.as_ptr().eq(&other.as_ptr()) && self.kind() == other.kind()
    }

    /// In our AST, only some alternative branches are represented as a node. This has historical
    /// reasons, e.g. we added a node for elif/else in if statements which was not originally
    /// present in the parser.
    pub const fn is_alternative_branch_with_node(self) -> bool {
        matches!(
            self,
            AnyNodeRef::ExceptHandlerExceptHandler(_) | AnyNodeRef::ElifElseClause(_)
        )
    }

    /// The last child of the last branch, if the node has multiple branches.
    pub fn last_child_in_body(&self) -> Option<AnyNodeRef<'a>> {
        let body = match self {
            AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
            | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. })
            | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
            | AnyNodeRef::MatchCase(MatchCase { body, .. })
            | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
                body,
                ..
            })
            | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. }) => body,
            AnyNodeRef::StmtIf(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => elif_else_clauses.last().map_or(body, |clause| &clause.body),

            AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
                if orelse.is_empty() {
                    body
                } else {
                    orelse
                }
            }

            AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
                return cases.last().map(AnyNodeRef::from);
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                if finalbody.is_empty() {
                    if orelse.is_empty() {
                        if handlers.is_empty() {
                            body
                        } else {
                            return handlers.last().map(AnyNodeRef::from);
                        }
                    } else {
                        orelse
                    }
                } else {
                    finalbody
                }
            }

            // Not a node that contains an indented child node.
            _ => return None,
        };

        body.last().map(AnyNodeRef::from)
    }

    /// Check if the given statement is the first statement after the colon of a branch, be it in if
    /// statements, for statements, after each part of a try-except-else-finally or function/class
    /// definitions.
    ///
    ///
    /// ```python
    /// if True:    <- has body
    ///     a       <- first statement
    ///     b
    /// elif b:     <- has body
    ///     c       <- first statement
    ///     d
    /// else:       <- has body
    ///     e       <- first statement
    ///     f
    ///
    /// class:      <- has body
    ///     a: int  <- first statement
    ///     b: int
    ///
    /// ```
    ///
    /// For nodes with multiple bodies, we check all bodies that don't have their own node. For
    /// try-except-else-finally, each except branch has it's own node, so for the `StmtTry`, we check
    /// the `try:`, `else:` and `finally:`, bodies, while `ExceptHandlerExceptHandler` has it's own
    /// check. For for-else and while-else, we check both branches for the whole statement.
    ///
    /// ```python
    /// try:        <- has body (a)
    ///     6/8     <- first statement (a)
    ///     1/0
    /// except:     <- has body (b)
    ///     a       <- first statement (b)
    ///     b
    /// else:
    ///     c       <- first statement (a)
    ///     d
    /// finally:
    ///     e       <- first statement (a)
    ///     f
    /// ```
    pub fn is_first_statement_in_body(&self, body: AnyNodeRef) -> bool {
        match body {
            AnyNodeRef::StmtFor(ast::StmtFor { body, orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { body, orelse, .. }) => {
                are_same_optional(*self, body.first()) || are_same_optional(*self, orelse.first())
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                body,
                orelse,
                finalbody,
                ..
            }) => {
                are_same_optional(*self, body.first())
                    || are_same_optional(*self, orelse.first())
                    || are_same_optional(*self, finalbody.first())
            }

            AnyNodeRef::StmtIf(ast::StmtIf { body, .. })
            | AnyNodeRef::ElifElseClause(ast::ElifElseClause { body, .. })
            | AnyNodeRef::StmtWith(ast::StmtWith { body, .. })
            | AnyNodeRef::ExceptHandlerExceptHandler(ast::ExceptHandlerExceptHandler {
                body,
                ..
            })
            | AnyNodeRef::MatchCase(MatchCase { body, .. })
            | AnyNodeRef::StmtFunctionDef(ast::StmtFunctionDef { body, .. })
            | AnyNodeRef::StmtClassDef(ast::StmtClassDef { body, .. }) => {
                are_same_optional(*self, body.first())
            }

            AnyNodeRef::StmtMatch(ast::StmtMatch { cases, .. }) => {
                are_same_optional(*self, cases.first())
            }

            _ => false,
        }
    }

    /// Returns `true` if `statement` is the first statement in an alternate `body` (e.g. the else of an if statement)
    pub fn is_first_statement_in_alternate_body(&self, body: AnyNodeRef) -> bool {
        match body {
            AnyNodeRef::StmtFor(ast::StmtFor { orelse, .. })
            | AnyNodeRef::StmtWhile(ast::StmtWhile { orelse, .. }) => {
                are_same_optional(*self, orelse.first())
            }

            AnyNodeRef::StmtTry(ast::StmtTry {
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                are_same_optional(*self, handlers.first())
                    || are_same_optional(*self, orelse.first())
                    || are_same_optional(*self, finalbody.first())
            }

            AnyNodeRef::StmtIf(ast::StmtIf {
                elif_else_clauses, ..
            }) => are_same_optional(*self, elif_else_clauses.first()),
            _ => false,
        }
    }
}

/// Returns `true` if `right` is `Some` and `left` and `right` are referentially equal.
fn are_same_optional<'a, T>(left: AnyNodeRef, right: Option<T>) -> bool
where
    T: Into<AnyNodeRef<'a>>,
{
    right.is_some_and(|right| left.ptr_eq(right.into()))
}
