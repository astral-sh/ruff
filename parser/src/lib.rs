//! This crate can be used to parse python sourcecode into a so
//! called AST (abstract syntax tree).
//!
//! The stages involved in this process are lexical analysis and
//! parsing. The lexical analysis splits the sourcecode into
//! tokens, and the parsing transforms those tokens into an AST.
//!
//! For example, one could do this:
//!
//! ```
//! use rustpython_parser::{parser, ast};
//!
//! let python_source = "print('Hello world')";
//! let python_ast = parser::parse_expression(python_source, "<embedded>").unwrap();
//!
//! ```

#![doc(html_logo_url = "https://raw.githubusercontent.com/RustPython/RustPython/main/logo.png")]
#![doc(html_root_url = "https://docs.rs/rustpython-parser/")]

#[macro_use]
extern crate log;
pub use rustpython_ast as ast;

pub mod error;
mod function;
pub mod lexer;
pub mod mode;
pub mod parser;
mod string;
#[rustfmt::skip]
mod python;
mod context;
pub mod token;
