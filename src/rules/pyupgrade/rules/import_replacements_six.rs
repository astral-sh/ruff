use std::collections::HashMap;

use once_cell::sync::Lazy;
use rustpython_ast::{AliasData, Located, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::helpers::{get_fromimport_str, ImportFormatting};
use crate::source_code::Locator;
use crate::violations;

static REPLACE_MODS: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("BaseHTTPServer", "http.server");
    m.insert("CGIHTTPServer", "http.server");
    m.insert("SimpleHTTPServer", "http.server");
    m.insert("_dummy_thread", "_dummy_thread");
    m.insert("_thread", "_thread");
    m.insert("builtins", "builtins");
    m.insert("cPickle", "pickle");
    m.insert("collections_abc", "collections.abc");
    m.insert("configparser", "configparser");
    m.insert("copyreg", "copyreg");
    m.insert("dbm_gnu", "dbm.gnu");
    m.insert("dbm_ndbm", "dbm.ndbm");
    m.insert("email_mime_base", "email.mime.base");
    m.insert("email_mime_image", "email.mime.image");
    m.insert("email_mime_multipart", "email.mime.multipart");
    m.insert("email_mime_nonmultipart", "email.mime.nonmultipart");
    m.insert("email_mime_text", "email.mime.text");
    m.insert("html_entities", "html.entities");
    m.insert("html_parser", "html.parser");
    m.insert("http_client", "http.client");
    m.insert("http_cookiejar", "http.cookiejar");
    m.insert("http_cookies", "http.cookies");
    m.insert("queue", "queue");
    m.insert("reprlib", "reprlib");
    m.insert("socketserver", "socketserver");
    m.insert("tkinter", "tkinter");
    m.insert("tkinter_colorchooser", "tkinter.colorchooser");
    m.insert("tkinter_commondialog", "tkinter.commondialog");
    m.insert("tkinter_constants", "tkinter.constants");
    m.insert("tkinter_dialog", "tkinter.dialog");
    m.insert("tkinter_dnd", "tkinter.dnd");
    m.insert("tkinter_filedialog", "tkinter.filedialog");
    m.insert("tkinter_font", "tkinter.font");
    m.insert("tkinter_messagebox", "tkinter.messagebox");
    m.insert("tkinter_scrolledtext", "tkinter.scrolledtext");
    m.insert("tkinter_simpledialog", "tkinter.simpledialog");
    m.insert("tkinter_tix", "tkinter.tix");
    m.insert("tkinter_tkfiledialog", "tkinter.filedialog");
    m.insert("tkinter_tksimpledialog", "tkinter.simpledialog");
    m.insert("tkinter_ttk", "tkinter.ttk");
    m.insert("urllib_error", "urllib.error");
    m.insert("urllib_parse", "urllib.parse");
    m.insert("urllib_robotparser", "urllib.robotparser");
    m.insert("xmlrpc_client", "xmlrpc.client");
    m.insert("xmlrpc_server", "xmlrpc.server");
    m
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
    locator: &Locator,
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

    let formatting = ImportFormatting::new(locator, stmt, names);
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
    final_str.push_str(&format!("\n{new_entries}"));
    if final_str.ends_with('\n') {
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
            final_string =
                refactor_segment(checker.locator, stmt, &REPLACE_MODS, names, module_text);
        } else if module_text == "six.moves.urllib" {
            final_string = refactor_segment(
                checker.locator,
                stmt,
                &REPLACE_MODS_URLLIB,
                names,
                module_text,
            );
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
    let mut diagnostic = Diagnostic::new(violations::ImportReplacementsSix, range);
    if checker.patch(&Rule::ImportReplacementsSix) {
        diagnostic.amend(Fix::replacement(
            final_str,
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
