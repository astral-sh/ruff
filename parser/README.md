# RustPython/parser

This directory has the code for python lexing, parsing and generating Abstract Syntax Trees (AST).

The steps are:
- Lexical analysis: splits the source code into tokens.
- Parsing and generating the AST: transforms those tokens into an AST. Uses `LALRPOP`, a Rust parser generator framework.

This crate is published on [https://docs.rs/rustpython-parser](https://docs.rs/rustpython-parser).

We wrote [a blog post](https://rustpython.github.io/2020/04/02/thing-explainer-parser.html) with screenshots and an explanation to help you understand the steps by seeing them in action.

For more information on LALRPOP, here is a link to the [LALRPOP book](https://github.com/lalrpop/lalrpop).

There is a readme in the `src` folder with the details of each file.


## Directory content

`build.rs`: The build script.
`Cargo.toml`: The config file.

The `src` directory has:

**lib.rs**   
This is the crate's root.

**lexer.rs**   
This module takes care of lexing python source text. This means source code is translated into separate tokens.

**parser.rs**   
A python parsing module. Use this module to parse python code into an AST. There are three ways to parse python code. You could parse a whole program, a single statement, or a single expression.

**ast.rs**   
 Implements abstract syntax tree (AST) nodes for the python language. Roughly equivalent to [the python AST](https://docs.python.org/3/library/ast.html).

**python.lalrpop**   
Python grammar.

**token.rs**   
Different token definitions. Loosely based on token.h from CPython source.

**errors.rs**   
Define internal parse error types. The goal is to provide a matching and a safe error API, masking errors from LALR.

**fstring.rs**   
Format strings.

**function.rs**   
Collection of functions for parsing parameters, arguments.

**location.rs**   
Datatypes to support source location information.

**mode.rs**   
Execution mode check. Allowed modes are `exec`, `eval` or `single`.


## How to use

For example, one could do this:
```
  use rustpython_parser::{parser, ast};
  let python_source = "print('Hello world')";
  let python_ast = parser::parse_expression(python_source).unwrap();
```
