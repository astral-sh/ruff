use std::path::PathBuf;

use anyhow::Result;
use clap::Parser as ClapParser;
use tree_sitter::Parser;

use ruff::fs;
use ruff::tree_parser::extract_module;

#[derive(Debug, ClapParser)]
struct Cli {
    #[arg(required = true)]
    file: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let src = fs::read_file(&cli.file)?;

    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_python::language())
        .expect("Error loading Python grammar");
    let parse_tree = parser.parse(src.as_bytes(), None);

    if let Some(parse_tree) = &parse_tree {
        // let _ = extract_module(parse_tree.root_node(), src.as_bytes());
        println!(
            "{:#?}",
            extract_module(parse_tree.root_node(), src.as_bytes())
        );
    }

    Ok(())
}
