mod deserialize;

use crate::deserialize::Fixture;
use ruff_annotate_snippets::{Message, Renderer};
use snapbox::data::DataFormat;
use snapbox::Data;
use std::error::Error;

fn main() {
    #[cfg(not(windows))]
    tryfn::Harness::new("tests/fixtures/", setup, test)
        .select(["*/*.toml"])
        .test();
}

fn setup(input_path: std::path::PathBuf) -> tryfn::Case {
    let parent = input_path
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let file_name = input_path.file_name().unwrap().to_str().unwrap();
    let name = format!("{parent}/{file_name}");
    let expected = Data::read_from(&input_path.with_extension("svg"), None);
    tryfn::Case {
        name,
        fixture: input_path,
        expected,
    }
}

fn test(input_path: &std::path::Path) -> Result<Data, Box<dyn Error>> {
    let src = std::fs::read_to_string(input_path)?;
    let fixture: Fixture = toml::from_str(&src)?;
    let renderer: Renderer = fixture.renderer.into();
    let message: Message<'_> = (&fixture.message).into();

    let actual = renderer.render(message).to_string();
    Ok(Data::from(actual).coerce_to(DataFormat::TermSvg))
}
