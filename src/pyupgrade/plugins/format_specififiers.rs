use libcst_native::{Arg, Codegen, CodegenState, Expression};
use num_bigint::{BigInt, Sign};
use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Constant, Expr, ExprKind};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::cst::matchers::{match_call, match_expression};

// The regex documentation says to do this because creating regexs is expensive:
// https://docs.rs/regex/latest/regex/#example-avoid-compiling-the-same-regex-in-a-loop
static FORMAT_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<int>\d+)?(?P<fmt>.*?)\}").unwrap());

/// Convert a python integer to a unsigned 32 but integer. We are assuming this
/// will never overflow because people will probbably never have more than 2^32
/// arguments to a format string. I am also ignoring the signed, I personally
/// checked and negative numbers are not allowed in format strings
fn convert_big_int(bigint: BigInt) -> Option<u32> {
    let (sign, digits) = bigint.to_u32_digits();
    match sign {
        Sign::Plus => digits.get(0).copied(),
        Sign::Minus => None,
        Sign::NoSign => Some(0),
    }
}

fn get_new_args(old_args: Vec<Arg>, correct_order: Vec<u32>) -> Vec<Arg> {
    let mut new_args: Vec<Arg> = Vec::new();
    for (i, given_idx) in correct_order.iter().enumerate() {
        // We need to keep the formatting in the same order but move the values
        let values = old_args.get(given_idx.to_owned() as usize).unwrap();
        let formatting = old_args.get(i).unwrap();
        let new_arg = Arg {
            value: values.value.clone(),
            comma: formatting.comma.clone(),
            // Kwargs are NOT allowed in .format (I checked)
            equal: None,
            keyword: None,
            star: values.star,
            whitespace_after_star: formatting.whitespace_after_star.clone(),
            whitespace_after_arg: formatting.whitespace_after_arg.clone(),
        };
        new_args.push(new_arg);
    }
    new_args
}

fn get_new_call(module_text: &str, correct_order: Vec<u32>) -> Option<String> {
    println!("MINI 1");
    println!("{:?}", module_text);
    let mut expression = match match_expression(&module_text) {
        Err(_) => return None,
        Ok(item) => item,
    };
    println!("MINI 2");
    let mut call = match match_call(&mut expression) {
        Err(_) => return None,
        Ok(item) => item,
    };
    println!("MINI 3");
    call.args = get_new_args(call.args.clone(), correct_order);
    // Create the new function
    if let Expression::Attribute(item) = &*call.func {
        // Converting the struct to a struct and then back is not very efficient, but
        // regexs were the simplest way I could find to remove the specifiers
        println!("MINI 3.5");
        let mut state = CodegenState::default();
        item.codegen(&mut state);
        let cleaned = remove_specifiers(&state.to_string());
        println!("MINI 4");
        match match_expression(&cleaned) {
            Err(_) => return None,
            Ok(item) => call.func = Box::new(item),
        };
        println!("MINI 5");
        // Create the string
        let mut final_state = CodegenState::default();
        expression.codegen(&mut final_state);
        return Some(final_state.to_string());
    }
    None
}

fn get_specifier_order(value_str: &str) -> Vec<u32> {
    let mut specifier_ints: Vec<u32> = vec![];
    // Whether the previous character was a Lbrace. If this is true and the next
    // character is an integer than this integer gets added to the list of
    // constants
    let mut prev_l_brace = false;
    for (_, tok, _) in lexer::make_tokenizer(value_str).flatten() {
        if Tok::Lbrace == tok {
            prev_l_brace = true;
        } else if let Tok::Int { value } = tok {
            if prev_l_brace {
                if let Some(int_val) = convert_big_int(value) {
                    specifier_ints.push(int_val);
                }
            }
            prev_l_brace = false;
        } else {
            prev_l_brace = false;
        }
    }
    specifier_ints
}

/// Returns a string without the format specifiers. Ex. "Hello {0} {1}" ->
/// "Hello {} {}"
fn remove_specifiers(raw_specifiers: &str) -> String {
    println!("BEFORE: {}", raw_specifiers);
    let new_str = FORMAT_SPECIFIER
        .replace_all(raw_specifiers, "{$fmt}")
        .to_string();
    println!("AFTER: {}\n", new_str);
    new_str
}

/// Checks if there is a single specifier in the string. The string must either
/// have all formatterts or no formatters (or else an error will be thrown), so
/// this will work as long as the python code is valid
fn has_specifiers(raw_specifiers: &str) -> bool {
    FORMAT_SPECIFIER.is_match(raw_specifiers)
}

/// UP030
pub fn format_specifiers(checker: &mut Checker, expr: &Expr, func: &Expr) {
    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if let ExprKind::Constant {
            value: cons_value, ..
        } = &value.node
        {
            println!("STARTING");
            if let Constant::Str(provided_string) = cons_value {
                if attr == "format" && has_specifiers(provided_string) {
                    let as_ints = get_specifier_order(provided_string);
                    let call_range = Range::from_located(expr);
                    println!("Checkpoit 1");
                    let call_text = checker.locator.slice_source_code_range(&call_range);
                    println!("Call text: {}", call_text);
                    println!("Checkpoit 2");
                    let new_call = match get_new_call(&call_text, as_ints) {
                        None => return,
                        Some(item) => item,
                    };
                    println!("Checkpoit 3");
                    let mut check =
                        Check::new(CheckKind::FormatSpecifiers, Range::from_located(expr));
                    if checker.patch(check.kind.code()) {
                        check.amend(Fix::replacement(
                            new_call,
                            expr.location,
                            expr.end_location.unwrap(),
                        ));
                    }
                    checker.add_check(check);
                }
            }
        }
    }
}
