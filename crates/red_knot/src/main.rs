use red_knot::{files, Workspace};
use std::fs;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut files = files::Files::default();
    let mut workspace = Workspace::new(PathBuf::from("/home/micha/astral/test"));

    // For now, discover all python files in the root directory and mark them as open.

    for entry in fs::read_dir(workspace.root())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "py"))
    {
        let file_id = files.intern(&entry.path());

        let content = fs::read_to_string(&entry.path())?;
        workspace.open_file(file_id, content);
    }

    println!("start analysis for {workspace:#?}");

    workspace.check(&files);

    // TODO we need a way to model changed files.

    // for (id, open_file) in workspace.open_files() {
    //     let path = files.path(open_file);
    //
    //     println!("analyzing {path}");
    // }
    //
    Ok(())
}
