//! Generate the `CheckCodePrefix` enum.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::OpenOptions;
use std::io::Write;

use anyhow::Result;
use clap::Parser;
use codegen::{Scope, Type, Variant};
use itertools::Itertools;
use ruff::checks::CheckCode;
use strum::IntoEnumIterator;

const FILE: &str = "src/checks_gen.rs";

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Write the generated source code to stdout (rather than to
    /// `src/checks_gen.rs`).
    #[arg(long)]
    dry_run: bool,
}

pub fn main(cli: &Cli) -> Result<()> {
    // Build up a map from prefix to matching CheckCodes.
    let mut prefix_to_codes: BTreeMap<String, BTreeSet<CheckCode>> = BTreeMap::default();
    for check_code in CheckCode::iter() {
        let as_ref: String = check_code.as_ref().to_string();
        let prefix_len = as_ref
            .chars()
            .take_while(|char| char.is_alphabetic())
            .count();
        for i in prefix_len..=as_ref.len() {
            let prefix = as_ref[..i].to_string();
            let entry = prefix_to_codes.entry(prefix).or_default();
            entry.insert(check_code.clone());
        }
    }

    let mut scope = Scope::new();

    // Create the `CheckCodePrefix` definition.
    let mut gen = scope
        .new_enum("CheckCodePrefix")
        .vis("pub")
        .derive("EnumString")
        .derive("Debug")
        .derive("PartialEq")
        .derive("Eq")
        .derive("PartialOrd")
        .derive("Ord")
        .derive("Clone")
        .derive("Serialize")
        .derive("Deserialize");
    for prefix in prefix_to_codes.keys() {
        gen = gen.push_variant(Variant::new(prefix.to_string()));
    }

    // Create the `PrefixSpecificity` definition.
    scope
        .new_enum("PrefixSpecificity")
        .vis("pub")
        .derive("PartialEq")
        .derive("Eq")
        .derive("PartialOrd")
        .derive("Ord")
        .push_variant(Variant::new("Category"))
        .push_variant(Variant::new("Hundreds"))
        .push_variant(Variant::new("Tens"))
        .push_variant(Variant::new("Explicit"));

    // Create the `match` statement, to map from definition to relevant codes.
    let mut gen = scope
        .new_impl("CheckCodePrefix")
        .new_fn("codes")
        .arg_ref_self()
        .ret(Type::new("Vec<CheckCode>"))
        .vis("pub")
        .line("#[allow(clippy::match_same_arms)]")
        .line("match self {");
    for (prefix, codes) in &prefix_to_codes {
        gen = gen.line(format!(
            "CheckCodePrefix::{prefix} => vec![{}],",
            codes
                .iter()
                .map(|code| format!("CheckCode::{}", code.as_ref()))
                .join(", ")
        ));
    }
    gen.line("}");

    // Create the `match` statement, to map from definition to specificity.
    let mut gen = scope
        .new_impl("CheckCodePrefix")
        .new_fn("specificity")
        .arg_ref_self()
        .ret(Type::new("PrefixSpecificity"))
        .vis("pub")
        .line("#[allow(clippy::match_same_arms)]")
        .line("match self {");
    for prefix in prefix_to_codes.keys() {
        let num_numeric = prefix.chars().filter(|char| char.is_numeric()).count();
        let specificity = match num_numeric {
            3 => "Explicit",
            2 => "Tens",
            1 => "Hundreds",
            0 => "Category",
            _ => panic!("Invalid prefix: {prefix}"),
        };
        gen = gen.line(format!(
            "CheckCodePrefix::{prefix} => PrefixSpecificity::{},",
            specificity
        ));
    }
    gen.line("}");

    // Construct the output contents.
    let mut output = String::new();
    output
        .push_str("//! File automatically generated by `examples/generate_check_code_prefix.rs`.");
    output.push('\n');
    output.push('\n');
    output.push_str("use serde::{{Serialize, Deserialize}};");
    output.push('\n');
    output.push_str("use strum_macros::EnumString;");
    output.push('\n');
    output.push('\n');
    output.push_str("use crate::checks::CheckCode;");
    output.push('\n');
    output.push('\n');
    output.push_str(&scope.to_string());
    output.push('\n');
    output.push('\n');

    // Add the list of output categories (not generated).
    output.push_str("pub const CATEGORIES: &[CheckCodePrefix] = &[");
    output.push('\n');
    for prefix in prefix_to_codes.keys() {
        if prefix.chars().all(char::is_alphabetic) {
            output.push_str(&format!("CheckCodePrefix::{prefix},"));
            output.push('\n');
        }
    }
    output.push_str("];");
    output.push('\n');
    output.push('\n');

    // Write the output to `src/checks_gen.rs` (or stdout).
    if cli.dry_run {
        println!("{output}");
    } else {
        let mut f = OpenOptions::new().write(true).truncate(true).open(FILE)?;
        write!(f, "{output}")?;
    }

    Ok(())
}
