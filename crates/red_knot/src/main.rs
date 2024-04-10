use std::path::PathBuf;
use std::sync::Arc;

use rustc_hash::FxHashSet;

use red_knot::db::{check_physical_lines, check_syntax, dependencies, parse, Database, Db};
use red_knot::{files, Workspace};

fn main() -> anyhow::Result<()> {
    let files = files::Files::default();
    let mut workspace = Workspace::new(PathBuf::from("/home/micha/astral/test/"));

    let file_id = files.intern(&workspace.root().join("test.py"));
    workspace.open_file(file_id);

    // For now, discover all python files in the root directory and mark them as open.

    // for entry in fs::read_dir(workspace.root())?
    //     .filter_map(|entry| entry.ok())
    //     .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "py"))
    // {
    //     let file_id = files.intern(&entry.path());
    //     dbg!(file_id, &entry.path());
    //

    //     workspace.open_file(file_id);
    // }

    // TODO: discover all python files and intern the file ids?

    println!("start analysis for {workspace:#?}");

    let db = Database::new(Arc::new(files));
    let mut queue: Vec<_> = workspace.open_files().collect();
    let mut queued: FxHashSet<_> = queue.iter().copied().collect();
    // Should we use an accumulator for this?
    let mut diagnostics = Vec::new();

    // TODO we could now consider spawning the analysis of the dependencies into their own threads.
    while let Some(file) = queue.pop() {
        let content = db.source_text(file).unwrap();
        // TODO this looks weird: dependencies.files. Let's figure out a better naming and structure.
        let dependencies = dependencies(&db, content).files(&db);

        // We know that we need to analyse all dependencies, but we don't need to check them.
        for file in dependencies {
            if queued.insert(file) {
                queue.push(file);
            }
        }

        let parsed = parse(&db, content);

        // If this is an open file
        if workspace.is_file_open(file) {
            // * run source text, logical line, and path based rules.
            // * build the semantic model
            // * run the semantic rules
            // * run type checking
            // Some of the steps could run together

            // TODO check_tokens(&db, parsed.tokens(&db));

            // I think we can run the syntax checks and the item tree construction in a single traversal?
            // Probably not, because we actually want to visit the nodes in a different order (breath first vs depth first, at least for some nodes).
            diagnostics.extend(check_physical_lines(&db, content).diagnostics(&db));
            diagnostics.extend(check_syntax(&db, parsed).diagnostics(&db));
        }

        // This is the HIR
        // I forgot how rust-analyzer reference from the HIR to the AST.
        // let item_tree = build_item_tree(&db, parsed.ast(&db)); // construct the item tree from the AST (the item tree is location agnostic)
        // The bindings should only resolve internally. Imports should be resolved to full qualified paths
        // but not resolved to bindings to ensure that result can be caculated on a per-file basis.
        // let bindings = binder(&db, item_tree); // Run the item tree through the binder

        // let types = type_inference(&db, bindings); // Run the type checker on the bindings

        // We need to build the symbol table here. What rust-analyzer does is it first transforms
        // the AST into a HIR that only contains the definitions. Each HIR node gets a unique where
        // it first assigns IDs to the top-level elements before their children (to ensure that changes
        // in the function body remain local). The idea of the HIR is to make the analysis location independent.

        // Run the syntax only rules for the file and perform some binding?

        // dbg!(parsed.module(&db));
    }

    dbg!(&diagnostics);
    // TODO let's trigger a re-check down here. Not sure how to do this or how to model it but that's kind of what this
    // is all about.

    // Oh dear, fitting this all into the fix loop will be fun.

    Ok(())
}
