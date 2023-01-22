use once_cell::sync::Lazy;
use rustpython_ast::{AliasData, Located, Stmt};
use std::collections::HashMap;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pyupgrade::helpers::{clean_indent, get_fromimport_str};
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

struct Formatting {
    multi_line: bool,
    indent: String,
    short_indent: String,
    start_indent: String,
}

impl Formatting {
    fn new<T>(locator: &Locator, stmt: &Stmt, arg1: &Located<T>) -> Self {
        let module_text = locator.slice_source_code_range(&Range::from_located(stmt));
        let multi_line = module_text.contains('\n');
        let start_indent = clean_indent(locator, stmt);
        let indent = clean_indent(locator, arg1);
        let short_indent = if indent.len() > 3 {
            indent[3..].to_string()
        } else {
            indent.to_string()
        };
        Self {
            multi_line,
            indent,
            short_indent,
            start_indent,
        }
    }
}

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

    let formatting = Formatting::new(locator, stmt, &names.get(0).unwrap());
    for name in names {
        match replace.get(name.node.name.as_str()) {
            None => keep_names.push(name.node.clone()),
            Some(item) => {
                // MAKE SURE TO ADD IF STATEMENTS HERE
                new_entries.push_str(&format!("{}import {item}", formatting.start_indent));
                if let Some(final_name) = &name.node.asname {
                    new_entries.push_str(&format!(" as {}", final_name));
                }
                new_entries.push('\n');
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
    final_str.push_str(&format!("\n{}", new_entries));
    if final_str.ends_with('\n') {
        final_str.pop();
    }
    Some(final_str)
}

/// UP036
pub fn import_replacements_six(
    checker: &mut Checker,
    stmt: &Stmt,
    module: &Option<String>,
    names: &Vec<Located<AliasData>>,
) {
    // Pyupgrade only works with import_from statements, so this linter does that as
    // well

    let final_string: Option<String>;
    if let Some(module_text) = module {
        if module_text == "six.moves" {
            final_string =
                refactor_segment(checker.locator, stmt, &REPLACE_MODS, &names, module_text);
        } else if module_text == "six.moves.urllib" {
            final_string = refactor_segment(
                checker.locator,
                stmt,
                &REPLACE_MODS_URLLIB,
                &names,
                module_text,
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
