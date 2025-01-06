use anyhow::{bail, Result};
use libcst_native::{CompoundStatement, Statement, Suite, With};

use ruff_diagnostics::Edit;
use ruff_python_ast as ast;
use ruff_python_ast::whitespace;
use ruff_python_codegen::Stylist;
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::cst::matchers::{match_function_def, match_indented_block, match_statement, match_with};
use crate::fix::codemods::CodegenStylist;
use crate::Locator;

/// (SIM117) Convert `with a: with b:` to `with a, b:`.
pub(crate) fn fix_multiple_with_statements(
    locator: &Locator,
    stylist: &Stylist,
    with_stmt: &ast::StmtWith,
) -> Result<Edit> {
    // Infer the indentation of the outer block.
    let Some(outer_indent) = whitespace::indentation(locator.contents(), with_stmt) else {
        bail!("Unable to fix multiline statement");
    };

    // Extract the module text.
    let contents = locator.lines_str(with_stmt.range());

    // If the block is indented, "embed" it in a function definition, to preserve
    // indentation while retaining valid source code. (We'll strip the prefix later
    // on.)
    let module_text = if outer_indent.is_empty() {
        contents.to_string()
    } else {
        format!("def f():{}{contents}", stylist.line_ending().as_str())
    };

    // Parse the CST.
    let mut tree = match_statement(&module_text)?;

    let statement = if outer_indent.is_empty() {
        &mut tree
    } else {
        let embedding = match_function_def(&mut tree)?;

        let indented_block = match_indented_block(&mut embedding.body)?;
        indented_block.indent = Some(outer_indent);

        let Some(statement) = indented_block.body.first_mut() else {
            bail!("Expected indented block to have at least one statement")
        };
        statement
    };

    let outer_with = match_with(statement)?;

    let With {
        body: Suite::IndentedBlock(ref mut outer_body),
        ..
    } = outer_with
    else {
        bail!("Expected outer with to have indented body")
    };

    let [Statement::Compound(CompoundStatement::With(inner_with))] = &mut *outer_body.body else {
        bail!("Expected one inner with statement");
    };

    outer_with.items.append(&mut inner_with.items);
    if outer_with.lpar.is_none() {
        outer_with.lpar.clone_from(&inner_with.lpar);
        outer_with.rpar.clone_from(&inner_with.rpar);
    }
    outer_with.body = inner_with.body.clone();

    // Reconstruct and reformat the code.
    let module_text = tree.codegen_stylist(stylist);
    let contents = if outer_indent.is_empty() {
        module_text
    } else {
        module_text
            .strip_prefix(&format!("def f():{}", stylist.line_ending().as_str()))
            .unwrap()
            .to_string()
    };

    let range = locator.lines_range(with_stmt.range());

    Ok(Edit::range_replacement(contents, range))
}
