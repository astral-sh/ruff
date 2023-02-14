use anyhow::{bail, Result};
use libcst_native::{Codegen, CodegenState, CompoundStatement, Statement, Suite, With};
use rustpython_parser::ast::Location;

use crate::ast::types::Range;
use crate::ast::whitespace;
use crate::cst::matchers::match_module;
use crate::fix::Fix;
use crate::source_code::{Locator, Stylist};

/// (SIM117) Convert `with a: with b:` to `with a, b:`.
pub(crate) fn fix_multiple_with_statements(
    locator: &Locator,
    stylist: &Stylist,
    stmt: &rustpython_parser::ast::Stmt,
) -> Result<Fix> {
    // Infer the indentation of the outer block.
    let Some(outer_indent) = whitespace::indentation(locator, stmt) else {
        bail!("Unable to fix multiline statement");
    };

    // Extract the module text.
    let contents = locator.slice_source_code_range(&Range::new(
        Location::new(stmt.location.row(), 0),
        Location::new(stmt.end_location.unwrap().row() + 1, 0),
    ));

    // If the block is indented, "embed" it in a function definition, to preserve
    // indentation while retaining valid source code. (We'll strip the prefix later
    // on.)
    let module_text = if outer_indent.is_empty() {
        contents.to_string()
    } else {
        format!("def f():{}{contents}", stylist.line_ending().as_str())
    };

    // Parse the CST.
    let mut tree = match_module(&module_text)?;

    let statements = if outer_indent.is_empty() {
        &mut *tree.body
    } else {
        let [Statement::Compound(CompoundStatement::FunctionDef(embedding))] = &mut *tree.body else {
            bail!("Expected statement to be embedded in a function definition")
        };

        let Suite::IndentedBlock(indented_block) = &mut embedding.body else {
            bail!("Expected indented block")
        };
        indented_block.indent = Some(outer_indent);

        &mut *indented_block.body
    };

    let [Statement::Compound(CompoundStatement::With(outer_with))] = statements else {
        bail!("Expected one outer with statement")
    };

    let With {
        body: Suite::IndentedBlock(ref mut outer_body),
        ..
    } = outer_with else {
        bail!("Expected outer with to have indented body")
    };

    let [Statement::Compound(CompoundStatement::With(inner_with))] =
        &mut *outer_body.body
    else {
        bail!("Expected one inner with statement");
    };

    outer_with.items.append(&mut inner_with.items);
    if outer_with.lpar.is_none() {
        outer_with.lpar = inner_with.lpar.clone();
        outer_with.rpar = inner_with.rpar.clone();
    }
    outer_with.body = inner_with.body.clone();

    let mut state = CodegenState {
        default_newline: stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    // Reconstruct and reformat the code.
    let module_text = state.to_string();
    let contents = if outer_indent.is_empty() {
        module_text
    } else {
        module_text
            .strip_prefix(&format!("def f():{}", stylist.line_ending().as_str()))
            .unwrap()
            .to_string()
    };

    Ok(Fix::replacement(
        contents,
        Location::new(stmt.location.row(), 0),
        Location::new(stmt.end_location.unwrap().row() + 1, 0),
    ))
}
