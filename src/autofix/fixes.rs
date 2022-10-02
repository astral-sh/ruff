use libcst_native::ImportNames::Aliases;
use libcst_native::{
    AnnAssign, Annotation, Arg, AsName, Assert, Assign, AssignEqual, AssignTarget,
    AssignTargetExpression, Asynchronous, Attribute, AugAssign, Await, BinaryOp, BinaryOperation,
    BooleanOp, BooleanOperation, Break, Call, ClassDef, Codegen, CompFor, CompIf, CompOp,
    Comparison, ComparisonTarget, CompoundStatement, ConcatenatedString, Continue, Decorator, Del,
    DelTargetExpression, Dict, DictComp, DictElement, Element, Ellipsis, Else, ExceptHandler,
    ExceptStarHandler, Expression, Finally, Float, For, FormattedString, FormattedStringExpression,
    FormattedStringText, FunctionDef, GeneratorExp, Global, If, IfExp, Imaginary, Import,
    ImportAlias, ImportFrom, ImportStar, IndentedBlock, Index, Integer, Lambda, List, ListComp,
    Match, Module, Name, NameItem, NamedExpr, Nonlocal, OrElse, Param, ParamStar, Parameters, Pass,
    Raise, Return, Set, SetComp, SimpleStatementLine, SimpleStatementSuite, SimpleString, Slice,
    SmallStatement, StarredDictElement, StarredElement, Statement, Subscript, SubscriptElement,
    Try, TryStar, Tuple, UnaryOp, UnaryOperation, While, With, WithItem, Yield, YieldValue,
};
use rustpython_parser::ast::{Alias, Expr, Keyword, Location};
use rustpython_parser::lexer;
use rustpython_parser::token::Tok;

use crate::ast::operations::SourceCodeLocator;
use crate::ast::types::Range;
use crate::checks::{Check, Fix};
use crate::cst_visitor;
use crate::cst_visitor::CSTVisitor;

/// Convert a location within a file (relative to `base`) to an absolute position.
fn to_absolute(relative: &Location, base: &Location) -> Location {
    if relative.row() == 1 {
        Location::new(
            relative.row() + base.row() - 1,
            relative.column() + base.column() - 1,
        )
    } else {
        Location::new(relative.row() + base.row() - 1, relative.column())
    }
}

/// Generate a fix to remove a base from a ClassDef statement.
pub fn remove_class_def_base(
    locator: &mut SourceCodeLocator,
    stmt_at: &Location,
    expr_at: Location,
    bases: &[Expr],
    keywords: &[Keyword],
) -> Option<Fix> {
    let content = locator.slice_source_code_at(stmt_at);

    // Case 1: `object` is the only base.
    if bases.len() == 1 && keywords.is_empty() {
        let mut fix_start = None;
        let mut fix_end = None;
        let mut count: usize = 0;
        for (start, tok, end) in lexer::make_tokenizer(content).flatten() {
            if matches!(tok, Tok::Lpar) {
                if count == 0 {
                    fix_start = Some(to_absolute(&start, stmt_at));
                }
                count += 1;
            }

            if matches!(tok, Tok::Rpar) {
                count -= 1;
                if count == 0 {
                    fix_end = Some(to_absolute(&end, stmt_at));
                    break;
                }
            }
        }

        return match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix {
                content: "".to_string(),
                location: start,
                end_location: end,
                applied: false,
            }),
            _ => None,
        };
    }

    if bases
        .iter()
        .map(|node| node.location)
        .chain(keywords.iter().map(|node| node.location))
        .any(|location| location > expr_at)
    {
        // Case 2: `object` is _not_ the last node.
        let mut fix_start: Option<Location> = None;
        let mut fix_end: Option<Location> = None;
        let mut seen_comma = false;
        for (start, tok, end) in lexer::make_tokenizer(content).flatten() {
            let start = to_absolute(&start, stmt_at);
            if seen_comma {
                if matches!(tok, Tok::Newline) {
                    fix_end = Some(end);
                } else {
                    fix_end = Some(start);
                }
                break;
            }
            if start == expr_at {
                fix_start = Some(start);
            }
            if fix_start.is_some() && matches!(tok, Tok::Comma) {
                seen_comma = true;
            }
        }

        match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix {
                content: "".to_string(),
                location: start,
                end_location: end,
                applied: false,
            }),
            _ => None,
        }
    } else {
        // Case 3: `object` is the last node, so we have to find the last token that isn't a comma.
        let mut fix_start: Option<Location> = None;
        let mut fix_end: Option<Location> = None;
        for (start, tok, end) in lexer::make_tokenizer(content).flatten() {
            let start = to_absolute(&start, stmt_at);
            let end = to_absolute(&end, stmt_at);
            if start == expr_at {
                fix_end = Some(end);
                break;
            }
            if matches!(tok, Tok::Comma) {
                fix_start = Some(start);
            }
        }

        match (fix_start, fix_end) {
            (Some(start), Some(end)) => Some(Fix {
                content: "".to_string(),
                location: start,
                end_location: end,
                applied: false,
            }),
            _ => None,
        }
    }
}

pub fn remove_super_arguments(locator: &mut SourceCodeLocator, expr: &Expr) -> Option<Fix> {
    let contents = locator.slice_source_code_range(&expr.location, &expr.end_location);

    let mut tree = match libcst_native::parse_module(contents, None) {
        Ok(m) => m,
        Err(_) => return None,
    };

    if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
            if let Expression::Call(body) = &mut body.value {
                body.args = vec![];
                body.whitespace_before_args = Default::default();
                body.whitespace_after_func = Default::default();

                let mut state = Default::default();
                tree.codegen(&mut state);

                return Some(Fix {
                    content: state.to_string(),
                    location: expr.location,
                    end_location: expr.end_location,
                    applied: false,
                });
            }
        }
    }

    None
}

// struct ImportRemover {}
//
// impl CSTVisitor for ImportRemover {
//     fn visit_SmallStatement<'a>(&mut self, node: &'a mut SmallStatement<'a>) {
//         match node {
//             SmallStatement::Import(body) => {
//                 body.names = vec![];
//             }
//             SmallStatement::ImportFrom(_) => {}
//             _ => {
//                 cst_visitor::walk_SmallStatement(self, node);
//             }
//         }
//     }
// }
//
// pub fn modify(contents: &str) -> Module {
//     // First, generate the CST.
//     let mut tree = match libcst_native::parse_module(contents, None) {
//         Ok(m) => m,
//         Err(_) => panic!("Oops"),
//     };
//
//     let mut visitor = ImportRemover {};
//     visitor.visit_Module(&mut tree);
//
//     tree
// }

// pub fn remove_unused_imports(contents: &str, checks: &[Check]) {
//     // // // First, generate the CST.
//     // // let mut tree = match libcst_native::parse_module(contents, None) {
//     // //     Ok(m) => m,
//     // //     Err(_) => return,
//     // // };
//     // //
//     // // let mut visitor = ImportRemover {};
//     // // visitor.visit_Module(&mut tree);
//     // let mut tree = modify(contents);
//     //
//     // let mut state = Default::default();
//     // tree.codegen(&mut state);
// }

pub fn remove_unused_import(location: &Location, end_location: &Location) -> Option<Fix> {
    Some(Fix {
        start: location.clone(),
        end: end_location.clone(),
        content: "".to_string(),
        applied: false,
    })
}

pub fn remove_unused_import_from(
    locator: &mut SourceCodeLocator,
    full_name: &str,
    location: &Range,
) -> Option<Fix> {
    let contents = locator.slice_source_code_range(&location, &end_location);

    let mut tree = match libcst_native::parse_module(contents, None) {
        Ok(m) => m,
        Err(_) => return None,
    };

    if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        if let Some(SmallStatement::ImportFrom(body)) = body.body.first_mut() {
            if let Aliases(aliases) = &mut body.names {
                // let index = aliases.iter().position(|alias| alias.name)
                aliases.remove(0);
            }

            let mut state = Default::default();
            tree.codegen(&mut state);

            return Some(Fix {
                content: state.to_string(),
                start: expr.location,
                end: expr.end_location,
                applied: false,
            });
        }
    }

    None
}
