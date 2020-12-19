use rustpython_bytecode::CodeObject;
use rustpython_compiler_core::{compile, symboltable};
use rustpython_parser::{ast::Location, parser};
use std::fmt;

pub use compile::{CompileOpts, Mode};
pub use symboltable::{Symbol, SymbolScope, SymbolTable, SymbolTableType};

#[derive(Debug, thiserror::Error)]
pub enum CompileErrorType {
    #[error(transparent)]
    Compile(#[from] rustpython_compiler_core::error::CompileErrorType),
    #[error(transparent)]
    Parse(#[from] rustpython_parser::error::ParseErrorType),
}

#[derive(Debug, thiserror::Error)]
pub struct CompileError {
    pub error: CompileErrorType,
    pub statement: Option<String>,
    pub source_path: String,
    pub location: Location,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let loc = self.location;
        if let Some(ref stmt) = self.statement {
            // visualize the error when location and statement are provided
            write!(
                f,
                "{}",
                loc.visualize(stmt, &format_args!("{} at {}", self.error, loc))
            )
        } else {
            write!(f, "{} at {}", self.error, loc)
        }
    }
}

impl CompileError {
    fn from_compile(error: rustpython_compiler_core::error::CompileError, source: &str) -> Self {
        CompileError {
            error: error.error.into(),
            location: error.location,
            source_path: error.source_path,
            statement: get_statement(source, error.location),
        }
    }
    fn from_parse(
        error: rustpython_parser::error::ParseError,
        source: &str,
        source_path: String,
    ) -> Self {
        CompileError {
            error: error.error.into(),
            location: error.location,
            source_path,
            statement: get_statement(source, error.location),
        }
    }
    fn from_symtable(
        error: symboltable::SymbolTableError,
        source: &str,
        source_path: String,
    ) -> Self {
        Self::from_compile(error.into_compile_error(source_path), source)
    }
}

/// Compile a given sourcecode into a bytecode object.
pub fn compile(
    source: &str,
    mode: compile::Mode,
    source_path: String,
    opts: CompileOpts,
) -> Result<CodeObject, CompileError> {
    macro_rules! try_parse {
        ($x:expr) => {
            match $x {
                Ok(x) => x,
                Err(e) => return Err(CompileError::from_parse(e, source, source_path)),
            }
        };
    }
    let res = match mode {
        compile::Mode::Exec => {
            let ast = try_parse!(parser::parse_program(source));
            compile::compile_program(ast, source_path, opts)
        }
        compile::Mode::Eval => {
            let statement = try_parse!(parser::parse_statement(source));
            compile::compile_statement_eval(statement, source_path, opts)
        }
        compile::Mode::Single => {
            let ast = try_parse!(parser::parse_program(source));
            compile::compile_program_single(ast, source_path, opts)
        }
    };
    res.map_err(|e| CompileError::from_compile(e, source))
}

pub fn compile_symtable(
    source: &str,
    mode: compile::Mode,
    source_path: &str,
) -> Result<symboltable::SymbolTable, CompileError> {
    macro_rules! try_parse {
        ($x:expr) => {
            match $x {
                Ok(x) => x,
                Err(e) => return Err(CompileError::from_parse(e, source, source_path.to_owned())),
            }
        };
    }
    let res = match mode {
        compile::Mode::Exec | compile::Mode::Single => {
            let ast = try_parse!(parser::parse_program(source));
            symboltable::make_symbol_table(&ast)
        }
        compile::Mode::Eval => {
            let statement = try_parse!(parser::parse_statement(source));
            symboltable::statements_to_symbol_table(&statement)
        }
    };
    res.map_err(|e| CompileError::from_symtable(e, source, source_path.to_owned()))
}

fn get_statement(source: &str, loc: Location) -> Option<String> {
    if loc.column() == 0 || loc.row() == 0 {
        return None;
    }
    let line = source.split('\n').nth(loc.row() - 1)?.to_owned();
    Some(line + "\n")
}
