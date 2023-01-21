use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_import, match_import_from, match_module};
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::violations;
use libcst_native::{
    AsName, AssignTargetExpression, Codegen, CodegenState, ImportAlias, ImportNames,
    NameOrAttribute,
};
use once_cell::sync::Lazy;
use rustpython_ast::Stmt;
use std::collections::HashMap;

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
    m.insert("urllib.error", "urllib.error");
    m.insert("urllib.parse", "urllib.parse");
    m.insert("urllib.request", "urllib.request");
    m.insert("urllib.response", "urllib.response");
    m.insert("urllib.robotparser", "urllib.robotparser");
    m.insert("urllib_error", "urllib.error");
    m.insert("urllib_parse", "urllib.parse");
    m.insert("urllib_robotparser", "urllib.robotparser");
    m.insert("xmlrpc_client", "xmlrpc.client");
    m.insert("xmlrpc_server", "xmlrpc.server");
    m
});

fn get_asname(asname: &AsName) -> Option<String> {
    if let AssignTargetExpression::Name(item) = &asname.name {
        return Some(item.value.to_string());
    }
    None
}

/// UP036
pub fn import_replacements_six(checker: &mut Checker, stmt: &Stmt, module: &Option<String>) {
    // Pyupgrade only works with import_from statements, so this linter does that as
    // well

    // This only applies to six.moves libraries
    if let Some(module_text) = module {
        if module_text != "six.moves" {
            return;
        }
    } else {
        return;
    }
    let module_text = checker
        .locator
        .slice_source_code_range(&Range::from_located(stmt));
    let mut tree = match_module(&module_text).unwrap();
    let mut import = match_import_from(&mut tree).unwrap();
    let mut new_entries = String::new();
    let mut keep_names: Vec<ImportAlias<'_>> = vec![];
    if let ImportNames::Aliases(item_names) = &import.names {
        for name in item_names {
            if let NameOrAttribute::N(the_name) = &name.name {
                match REPLACE_MODS.get(the_name.value) {
                    Some(raw_name) => {
                        new_entries.push_str(&format!("import {}", raw_name));
                        if let Some(asname) = &name.asname {
                            if let Some(final_name) = get_asname(asname) {
                                new_entries.push_str(&format!(" as {}", final_name));
                            }
                        }
                        new_entries.push('\n');
                    }
                    None => keep_names.push(name.clone()),
                }
            } else {
                keep_names.push(name.clone())
            }
        }
    }
    // If nothing was different, there is no need to change
    if new_entries.is_empty() {
        return;
    }
    import.names = ImportNames::Aliases(keep_names);
    let mut state = CodegenState::default();
    import.codegen(&mut state);
    let mut final_str = state.to_string();
    final_str.push_str(&format!("\n{}", new_entries));
    if final_str.chars().last() == Some('\n') {
        final_str.pop();
    }
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
