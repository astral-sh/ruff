use std::collections::HashMap;

use once_cell::sync::Lazy;
use ruff_macros::derive_message_formats;
use rustpython_ast::{AliasData, Located, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::helpers::{get_fromimport_str, ImportFormatting};
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ImportReplacementsSix;
);
impl AlwaysAutofixableViolation for ImportReplacementsSix {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace old formatting imports with their new versions")
    }

    fn autofix_title(&self) -> String {
        "Updated the import".to_string()
    }
}

static REPLACE_MODS: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
    HashMap::from([
        ("BaseHTTPServer", "http.server"),
        ("CGIHTTPServer", "http.server"),
        ("SimpleHTTPServer", "http.server"),
        ("_dummy_thread", "_dummy_thread"),
        ("_thread", "_thread"),
        ("builtins", "builtins"),
        ("cPickle", "pickle"),
        ("collections_abc", "collections.abc"),
        ("configparser", "configparser"),
        ("copyreg", "copyreg"),
        ("dbm_gnu", "dbm.gnu"),
        ("dbm_ndbm", "dbm.ndbm"),
        ("email_mime_base", "email.mime.base"),
        ("email_mime_image", "email.mime.image"),
        ("email_mime_multipart", "email.mime.multipart"),
        ("email_mime_nonmultipart", "email.mime.nonmultipart"),
        ("email_mime_text", "email.mime.text"),
        ("html_entities", "html.entities"),
        ("html_parser", "html.parser"),
        ("http_client", "http.client"),
        ("http_cookiejar", "http.cookiejar"),
        ("http_cookies", "http.cookies"),
        ("queue", "queue"),
        ("reprlib", "reprlib"),
        ("socketserver", "socketserver"),
        ("tkinter", "tkinter"),
        ("tkinter_colorchooser", "tkinter.colorchooser"),
        ("tkinter_commondialog", "tkinter.commondialog"),
        ("tkinter_constants", "tkinter.constants"),
        ("tkinter_dialog", "tkinter.dialog"),
        ("tkinter_dnd", "tkinter.dnd"),
        ("tkinter_filedialog", "tkinter.filedialog"),
        ("tkinter_font", "tkinter.font"),
        ("tkinter_messagebox", "tkinter.messagebox"),
        ("tkinter_scrolledtext", "tkinter.scrolledtext"),
        ("tkinter_simpledialog", "tkinter.simpledialog"),
        ("tkinter_tix", "tkinter.tix"),
        ("tkinter_tkfiledialog", "tkinter.filedialog"),
        ("tkinter_tksimpledialog", "tkinter.simpledialog"),
        ("tkinter_ttk", "tkinter.ttk"),
        ("urllib_error", "urllib.error"),
        ("urllib_parse", "urllib.parse"),
        ("urllib_robotparser", "urllib.robotparser"),
        ("xmlrpc_client", "xmlrpc.client"),
        ("xmlrpc_server", "xmlrpc.server"),
    ])
});

static REPLACE_MODS_URLLIB: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("error", "urllib.error");
    m.insert("parse", "urllib.parse");
    m.insert("request", "urllib.request");
    m.insert("response", "urllib.response");
    m.insert("robotparser", "urllib.robotparser");
    m
});

fn refactor_segment(
    checker: &Checker,
    stmt: &Stmt,
    replace: &Lazy<HashMap<&str, &str>>,
    names: &[Located<AliasData>],
    module: &str,
) -> Option<String> {
    let mut new_entries = String::new();
    let mut keep_names: Vec<AliasData> = vec![];
    let mut clean_names: Vec<AliasData> = vec![];
    for name in names {
        clean_names.push(name.node.clone());
    }

    let formatting = ImportFormatting::new(checker.locator, stmt, names);
    for name in names {
        let import_name = name.node.name.as_str();
        match replace.get(import_name) {
            None => keep_names.push(name.node.clone()),
            Some(item) => {
                // Only replace if the name remains the name, or if there is an as on the import
                let new_name = item.split('.').last().unwrap_or_default();
                if name.node.asname.is_some() || import_name == new_name {
                    new_entries.push_str(&format!("{}import {item}", formatting.start_indent));
                    if let Some(final_name) = &name.node.asname {
                        new_entries.push_str(&format!(" as {final_name}"));
                    }
                    new_entries.push('\n');
                }
            }
        }
    }
    // If nothing was different, there is no need to change
    if new_entries.is_empty() {
        return None;
    }
    let mut final_str = get_fromimport_str(
        &keep_names,
        module,
        formatting.multi_line,
        &formatting.indent,
        &formatting.short_indent,
    );
    let nl = checker.stylist.line_ending().as_str();
    final_str.push_str(&format!("{nl}{new_entries}"));
    if final_str.ends_with(nl) {
        final_str.pop();
    }
    Some(final_str)
}

/// If the entire replace is before the import, we can use this to quickly make
/// the change
fn replace_from_only(
    module: &str,
    locator: &Locator,
    stmt: &Stmt,
    replace: &Lazy<HashMap<&str, &str>>,
    replace_str: &str,
) -> Option<String> {
    let new_module_text = module.replace(&format!("{replace_str}."), "");
    if let Some(item) = replace.get(new_module_text.as_str()) {
        let original = locator.slice_source_code_range(&Range::from_located(stmt));
        let new_str = original.replace(module, item);
        return Some(new_str);
    }
    None
}

/// UP036
pub fn import_replacements_six(
    checker: &mut Checker,
    stmt: &Stmt,
    module: &Option<String>,
    names: &[Located<AliasData>],
) {
    // Pyupgrade only works with import_from statements, so this linter does that as
    // well
    let final_string: Option<String>;
    if let Some(module_text) = module {
        if module_text == "six.moves" {
            final_string = refactor_segment(checker, stmt, &REPLACE_MODS, names, module_text);
        } else if module_text == "six.moves.urllib" {
            final_string =
                refactor_segment(checker, stmt, &REPLACE_MODS_URLLIB, names, module_text);
        } else if module_text.contains("six.moves.urllib") {
            final_string = replace_from_only(
                module_text,
                checker.locator,
                stmt,
                &REPLACE_MODS_URLLIB,
                "six.moves.urllib",
            );
        } else if module_text.contains("six.moves") {
            final_string = replace_from_only(
                module_text,
                checker.locator,
                stmt,
                &REPLACE_MODS,
                "six.moves",
            );
        } else {
            return;
        }
    } else {
        return;
    }
    let final_str = match final_string {
        Some(s) => s,
        None => return,
    };
    let range = Range::from_located(stmt);
    let mut diagnostic = Diagnostic::new(ImportReplacementsSix, range);
    if checker.patch(&Rule::ImportReplacementsSix) {
        diagnostic.amend(Fix::replacement(
            final_str,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
