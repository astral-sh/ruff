use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use html_escape::encode_double_quoted_attribute_to_string;
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use ty_ide::Docstring;

use crate::model::{
    ClassBaseDoc, ClassDoc, Documentation, FunctionDoc, ModuleDoc, SearchItem, SourceDoc,
    TypeIndexEntry, TypeLinkKind, TypeLinkTarget, VariableDoc, VariableKind,
    is_signature_type_identifier, module_short_name, parent_module, parent_modules,
    sanitize_path_segment,
};
use crate::syntax::{dotted_name_run_end, parse_python_tokens};

pub(crate) fn write_site(documentation: &Documentation, output_dir: &Path) -> Result<PathBuf> {
    let static_dir = output_dir.join("static.files");
    let project_dir = output_dir.join(&documentation.project_slug);

    fs::create_dir_all(&static_dir)
        .with_context(|| format!("Failed to create `{}`", static_dir.display()))?;
    fs::create_dir_all(&project_dir)
        .with_context(|| format!("Failed to create `{}`", project_dir.display()))?;

    write_file(&static_dir.join("tydoc.css"), STYLESHEET)?;
    write_file(&static_dir.join("tydoc.js"), SEARCH_SCRIPT)?;

    let search_index = serde_json::to_string(&search_items(documentation))?;
    write_file(
        &output_dir.join("search-index.js"),
        &format!("window.tyDocSearchIndex = {search_index};\n"),
    )?;

    write_html_file(
        output_dir,
        &project_dir.join("index.html"),
        &render_project_index(documentation),
    )?;
    write_html_file(
        output_dir,
        &project_dir.join("all.html"),
        &render_all_items(documentation),
    )?;
    let libraries = documented_libraries(output_dir)?;
    write_html_file(
        output_dir,
        &output_dir.join("index.html"),
        &render_library_index(documentation, &libraries),
    )?;

    for module in documentation.modules.values() {
        let module_dir = module.name.split('.').fold(
            output_dir.join(&documentation.project_slug),
            |path, component| path.join(sanitize_path_segment(component)),
        );
        fs::create_dir_all(&module_dir)
            .with_context(|| format!("Failed to create `{}`", module_dir.display()))?;

        write_html_file(
            output_dir,
            &module_dir.join("index.html"),
            &render_module_page(documentation, module),
        )?;

        for class in &module.classes {
            write_html_file(
                output_dir,
                &module_dir.join(format!("class.{}.html", sanitize_path_segment(&class.name))),
                &render_class_page(documentation, module, class),
            )?;
        }

        if let Some(source) = &module.source {
            let source_path = output_dir.join(source_doc_path(documentation, &source.path));
            if let Some(parent) = source_path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create `{}`", parent.display()))?;
            }
            write_html_file(
                output_dir,
                &source_path,
                &render_source_page(documentation, module, source),
            )?;
        }
    }

    Ok(project_dir.join("index.html"))
}

fn documented_libraries(output_dir: &Path) -> Result<Vec<String>> {
    let mut libraries = Vec::new();
    for entry in fs::read_dir(output_dir)
        .with_context(|| format!("Failed to read `{}`", output_dir.display()))?
    {
        let entry = entry
            .with_context(|| format!("Failed to read an entry in `{}`", output_dir.display()))?;
        let path = entry.path();
        if !path.is_dir() || !path.join("index.html").is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name == "static.files" {
            continue;
        }
        libraries.push(name.to_string());
    }
    libraries.sort();
    Ok(libraries)
}

fn write_file(path: &Path, content: &str) -> Result<()> {
    fs::write(path, content).with_context(|| format!("Failed to write `{}`", path.display()))
}

fn write_html_file(output_dir: &Path, path: &Path, content: &str) -> Result<()> {
    write_file(path, &compact_internal_hrefs(output_dir, path, content))
}

fn compact_internal_hrefs(output_dir: &Path, page_path: &Path, content: &str) -> String {
    let Some(page_dir) = page_path.parent() else {
        return content.to_string();
    };
    let Some(page_dir) = relative_path_components(output_dir, page_dir) else {
        return content.to_string();
    };

    let mut compacted = String::with_capacity(content.len());
    let mut remainder = content;

    while let Some(href_start) = remainder.find("href=\"") {
        let (before, href_and_after) = remainder.split_at(href_start + "href=\"".len());
        compacted.push_str(before);

        let Some(href_end) = href_and_after.find('"') else {
            compacted.push_str(href_and_after);
            return compacted;
        };
        let (href, after_href) = href_and_after.split_at(href_end);
        let rewritten = compact_internal_href(output_dir, &page_dir, href);
        compacted.push_str(rewritten.as_deref().unwrap_or(href));
        compacted.push('"');
        remainder = &after_href['"'.len_utf8()..];
    }

    compacted.push_str(remainder);
    compacted
}

fn compact_internal_href(output_dir: &Path, page_dir: &[String], href: &str) -> Option<String> {
    if href.is_empty()
        || href.starts_with('#')
        || href.starts_with('/')
        || href.contains("://")
        || href.starts_with("mailto:")
    {
        return None;
    }

    let (path, fragment) = href
        .split_once('#')
        .map_or((href, ""), |(path, fragment)| (path, fragment));
    if path.is_empty() {
        return None;
    }

    let target = resolve_href_components(output_dir, page_dir, path)?;
    let rewritten_path = relative_href(page_dir, &target);
    if rewritten_path.is_empty() {
        return None;
    }

    let rewritten = if fragment.is_empty() {
        rewritten_path
    } else {
        format!("{rewritten_path}#{fragment}")
    };

    (rewritten.len() < href.len()).then_some(rewritten)
}

fn resolve_href_components(
    output_dir: &Path,
    page_dir: &[String],
    href: &str,
) -> Option<Vec<String>> {
    let mut components = page_dir.to_vec();
    for component in href.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop()?;
            }
            component => components.push(component.to_string()),
        }
    }

    let mut candidate = output_dir.to_path_buf();
    for component in &components {
        candidate.push(component);
    }
    candidate.starts_with(output_dir).then_some(components)
}

fn relative_href(from: &[String], to: &[String]) -> String {
    let common = from
        .iter()
        .zip(to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut components = vec![".."; from.len() - common];
    components.extend(to[common..].iter().map(String::as_str));
    components.join("/")
}

fn relative_path_components(output_dir: &Path, path: &Path) -> Option<Vec<String>> {
    let relative = path.strip_prefix(output_dir).ok()?;
    Some(
        relative
            .components()
            .filter_map(|component| component.as_os_str().to_str().map(ToString::to_string))
            .collect(),
    )
}

fn source_doc_path(documentation: &Documentation, source_path: &str) -> String {
    let mut path = format!("{}/src/", documentation.project_slug);
    let components = source_path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();

    if let Some((file_name, directories)) = components.split_last() {
        for directory in directories {
            path.push_str(&sanitize_path_segment(directory));
            path.push('/');
        }
        path.push_str(&sanitize_path_segment(file_name));
    } else {
        path.push_str("source");
    }

    path.push_str(".html");
    path
}

fn render_project_index(documentation: &Documentation) -> String {
    let mut body = String::new();
    write_heading(&mut body, "Project", &documentation.project_name, None);

    if documentation.modules.is_empty() {
        body.push_str("<p>No Python modules were found.</p>");
    } else {
        render_project_documentation(&mut body, documentation);
        render_module_table(
            &mut body,
            "Modules",
            documentation.top_level_modules(),
            documentation,
            "../",
            ModuleLabelStyle::Short,
        );
    }

    render_page(
        documentation,
        "../",
        &format!("{} - ty doc", documentation.project_name),
        None,
        None,
        &body,
    )
}

fn render_library_index(documentation: &Documentation, libraries: &[String]) -> String {
    let mut body = String::new();
    body.push_str(
        "<div class=\"main-heading\"><h1 id=\"page-title\"><a class=\"pm\" href=\"#page-title\" aria-label=\"Permalink to this page\">§</a>Libraries</h1></div>",
    );

    if libraries.is_empty() {
        body.push_str("<p>No libraries were found.</p>");
    } else {
        body.push_str("<ul class=\"library-list\">");
        for library in libraries {
            render_link_list_item(&mut body, &format!("{library}/index.html"), library);
        }
        body.push_str("</ul>");
    }

    let mut page = String::new();
    page.push_str("<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"generator\" content=\"ty doc ");
    render_attr(&mut page, &documentation.generator_version);
    page.push_str("\"><title>Libraries - ty doc</title><link rel=\"stylesheet\" href=\"static.files/tydoc.css\"></head><body><a class=\"skip\" href=\"#main-content\">Skip to main content</a><header class=\"topbar\"><h2><a href=\"index.html\">Libraries</a></h2></header><nav class=\"sidebar\"><div class=\"sidebar-crate\"><h2><a href=\"index.html\">Libraries</a></h2></div><ul class=\"module-list\">");
    for library in libraries {
        render_link_list_item(&mut page, &format!("{library}/index.html"), library);
    }
    page.push_str(
        "</ul></nav><main><section id=\"main-content\" class=\"content\" tabindex=\"-1\">",
    );
    page.push_str(&body);
    page.push_str("</section></main></body></html>");
    page
}

fn render_project_documentation(body: &mut String, documentation: &Documentation) {
    let modules = documentation
        .top_level_modules()
        .filter(|module| module.docstring.is_some())
        .collect::<Vec<_>>();
    if modules.is_empty() {
        return;
    }

    write_section_heading(body, "library-documentation", "Library Documentation");
    for module in modules {
        body.push_str("<h3>");
        render_link(
            body,
            None,
            &module_href("../", documentation, &module.name),
            &module.name,
        );
        body.push_str("</h3>");
        render_docblock(body, module.docstring.as_deref().unwrap());
    }
}

fn render_all_items(documentation: &Documentation) -> String {
    let mut body = String::new();
    write_heading(&mut body, "All Items", &documentation.project_name, None);

    render_module_table(
        &mut body,
        "Modules",
        documentation.modules.values(),
        documentation,
        "../",
        ModuleLabelStyle::Full,
    );

    render_item_table(
        &mut body,
        "Classes",
        "classes",
        documentation.modules.values().flat_map(|module| {
            module
                .classes
                .iter()
                .filter(|class| module.public_items.contains(&class.name))
                .map(move |class| (module, class))
        }),
        |(module, class), rows| {
            render_item_table_row(
                rows,
                "class",
                &class_page_href("../", documentation, &module.name, &class.name),
                &format!("{}.{}", module.name, class.name),
                class.summary(),
            );
        },
    );

    render_item_table(
        &mut body,
        "Functions",
        "functions",
        documentation.modules.values().flat_map(|module| {
            module
                .functions
                .iter()
                .filter(|function| module.public_items.contains(&function.name))
                .map(move |function| (module, function))
        }),
        |(module, function), rows| {
            render_item_table_row(
                rows,
                "fn",
                &anchored_href(
                    &module_href("../", documentation, &module.name),
                    "fn",
                    &function.name,
                ),
                &format!("{}.{}", module.name, function.name),
                function.summary(),
            );
        },
    );

    render_all_variable_items(
        &mut body,
        "Variables",
        documentation,
        VariableKind::Variable,
    );
    render_all_variable_items(
        &mut body,
        "Type Aliases",
        documentation,
        VariableKind::TypeAlias,
    );

    render_page(
        documentation,
        "../",
        &format!("All Items - {}", documentation.project_name),
        None,
        None,
        &body,
    )
}

fn render_module_page(documentation: &Documentation, module: &ModuleDoc) -> String {
    let root = root_prefix_for_module(&module.name);
    let mut body = String::new();
    let source_line = module
        .source
        .as_ref()
        .filter(|source| !source.text.is_empty())
        .map(|_| "1");

    write_breadcrumbs(&mut body, documentation, &root, parent_module(&module.name));
    write_heading(
        &mut body,
        "Module",
        module_short_name(&module.name),
        source_href_for(&root, documentation, module.source.as_ref(), source_line),
    );

    if let Some(docstring) = &module.docstring {
        render_docblock(&mut body, docstring);
    }

    render_module_table(
        &mut body,
        "Submodules",
        module
            .submodules
            .iter()
            .filter_map(|name| documentation.modules.get(name)),
        documentation,
        &root,
        ModuleLabelStyle::Short,
    );
    render_item_table(
        &mut body,
        "Classes",
        "classes",
        &module.classes,
        |class, rows| {
            render_item_table_row(
                rows,
                "class",
                &class_page_href(&root, documentation, &module.name, &class.name),
                &class.name,
                class.summary(),
            );
        },
    );
    render_function_sections(
        &mut body,
        FunctionSections {
            documentation,
            root: &root,
            module,
            section_anchor: "functions",
            title: "Functions",
            item_anchor_prefix: "fn",
        },
        module.functions.iter(),
        |_| None,
    );
    render_variable_table(
        &mut body,
        "Variables",
        &module.variables,
        documentation,
        &root,
        module,
    );

    render_page(
        documentation,
        &root,
        &format!("{} - ty doc", module.name),
        Some(module),
        None,
        &body,
    )
}

fn render_class_page(
    documentation: &Documentation,
    module: &ModuleDoc,
    class: &ClassDoc,
) -> String {
    let root = root_prefix_for_module(&module.name);
    let mut body = String::new();
    let enum_members = class_enum_members(class);
    let attributes = local_class_attributes(documentation, class);
    let methods = local_class_methods(documentation, class);

    write_breadcrumbs(&mut body, documentation, &root, Some(&module.name));
    write_heading(
        &mut body,
        "Class",
        &class.name,
        source_href_for(
            &root,
            documentation,
            module.source.as_ref(),
            Some(&class.source_line),
        ),
    );
    if enum_members.is_empty() {
        render_signature(
            &mut body,
            documentation,
            &root,
            &module.name,
            &class.signature,
            &class.signature_links,
        );
    } else {
        render_enum_signature(
            &mut body,
            documentation,
            &root,
            &module.name,
            class,
            &enum_members,
        );
    }
    if let Some(docstring) = &class.docstring {
        render_docblock(&mut body, docstring);
    }

    render_attribute_sections(
        &mut body,
        "Attributes",
        &attributes,
        documentation,
        &root,
        module,
        class,
    );
    render_function_sections(
        &mut body,
        FunctionSections {
            documentation,
            root: &root,
            module,
            section_anchor: "methods",
            title: "Methods",
            item_anchor_prefix: "method",
        },
        methods.iter().copied(),
        |function| method_override_note(documentation, &root, class, function),
    );
    render_inherited_members(&mut body, documentation, &root, module, class);

    render_page(
        documentation,
        &root,
        &format!("{}.{} - ty doc", module.name, class.name),
        Some(module),
        Some(ActiveItem::Class(class)),
        &body,
    )
}

fn class_enum_members(class: &ClassDoc) -> Vec<&VariableDoc> {
    class
        .attributes
        .iter()
        .filter(|attribute| class.enum_member_names.contains(&attribute.name))
        .collect()
}

fn class_attributes(class: &ClassDoc) -> Vec<&VariableDoc> {
    class
        .attributes
        .iter()
        .filter(|attribute| !class.enum_member_names.contains(&attribute.name))
        .collect()
}

fn local_class_attributes<'a>(
    documentation: &Documentation,
    class: &'a ClassDoc,
) -> Vec<&'a VariableDoc> {
    class_attributes(class)
        .into_iter()
        .filter(|attribute| {
            find_inherited_attribute(documentation, class, &attribute.name).is_none()
        })
        .collect()
}

fn local_class_methods<'a>(
    documentation: &Documentation,
    class: &'a ClassDoc,
) -> Vec<&'a FunctionDoc> {
    class
        .methods
        .iter()
        .filter(|method| find_inherited_function(documentation, class, &method.name).is_none())
        .collect()
}

fn render_source_page(
    documentation: &Documentation,
    module: &ModuleDoc,
    source: &SourceDoc,
) -> String {
    let root = root_prefix_for_source(&source.path);
    let mut body = String::new();

    write_breadcrumbs(&mut body, documentation, &root, Some(&module.name));
    write_heading(&mut body, "Source", &source.path, None);

    let highlighted_lines = highlight_python_source_lines(&source.text, &source.tokens);
    body.push_str("<pre class=\"source-code\">");
    for (index, highlighted) in highlighted_lines.into_iter().enumerate() {
        let line_number = index + 1;
        let line_id = format!("L{line_number}");
        body.push_str("<span id=\"");
        render_attr(&mut body, &line_id);
        body.push_str("\"><a href=\"#");
        render_attr(&mut body, &line_id);
        body.push_str("\">");
        body.push_str(&line_number.to_string());
        body.push_str("</a><code>");
        body.push_str(&highlighted);
        body.push_str("</code></span>");
    }
    body.push_str("</pre>");

    render_page(
        documentation,
        &root,
        &format!("{} source - ty doc", module.name),
        Some(module),
        None,
        &body,
    )
}

#[derive(Copy, Clone)]
enum ActiveItem<'a> {
    Class(&'a ClassDoc),
}

fn render_page(
    documentation: &Documentation,
    root: &str,
    title: &str,
    active_module: Option<&ModuleDoc>,
    active_item: Option<ActiveItem>,
    body: &str,
) -> String {
    Page {
        documentation,
        root,
        title,
        active_module,
        active_item,
        body,
    }
    .render()
}

struct Page<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    title: &'a str,
    active_module: Option<&'a ModuleDoc>,
    active_item: Option<ActiveItem<'a>>,
    body: &'a str,
}

impl Render for Page<'_> {
    fn render_to(&self, output: &mut String) {
        output.push_str("<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><meta name=\"generator\" content=\"ty doc ");
        render_attr(output, &self.documentation.generator_version);
        output.push_str("\"><title>");
        self.title.render_to(output);
        output.push_str("</title><link rel=\"stylesheet\" href=\"");
        render_attr(output, &format!("{}static.files/tydoc.css", self.root));
        output.push_str("\"><script defer src=\"");
        render_attr(output, &format!("{}static.files/tydoc.js", self.root));
        output.push_str("\"></script></head><body data-tydoc-root=\"");
        render_attr(output, self.root);
        output.push_str("\"><a class=\"skip\" href=\"#main-content\">Skip to main content</a>");
        Topbar {
            documentation: self.documentation,
            root: self.root,
            active_module: self.active_module,
        }
        .render_to(output);
        Sidebar {
            documentation: self.documentation,
            root: self.root,
            active_module: self.active_module,
            active_item: self.active_item,
        }
        .render_to(output);
        output.push_str("<main><section id=\"main-content\" class=\"content\" tabindex=\"-1\">");
        output.push_str(self.body);
        output.push_str("</section></main></body></html>");
    }
}

struct Topbar<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    active_module: Option<&'a ModuleDoc>,
}

impl Render for Topbar<'_> {
    fn render_to(&self, output: &mut String) {
        let (name, href) = self.active_module.map_or_else(
            || {
                (
                    self.documentation.project_name.as_str(),
                    project_index_href(self.root, self.documentation),
                )
            },
            |module| {
                (
                    module_short_name(&module.name),
                    module_href(self.root, self.documentation, &module.name),
                )
            },
        );
        output.push_str("<header class=\"topbar\"><h2>");
        render_link(output, None, &href, name);
        output.push_str("</h2><div class=\"search topbar-search\"><label for=\"tydoc-search\">Search</label><input id=\"tydoc-search\" type=\"search\" autocomplete=\"off\" spellcheck=\"false\" placeholder=\"Search docs\"><div id=\"tydoc-search-results\" class=\"search-results\" hidden></div></div></header>");
    }
}

struct Sidebar<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    active_module: Option<&'a ModuleDoc>,
    active_item: Option<ActiveItem<'a>>,
}

impl Render for Sidebar<'_> {
    fn render_to(&self, output: &mut String) {
        let mut context = String::new();

        if let Some(active_module) = self.active_module {
            write_module_context(&mut context, self.documentation, self.root, active_module);
            if let Some(active_item) = self.active_item {
                write_item_context(&mut context, self.documentation, active_item);
            }
        } else {
            write_module_list(
                &mut context,
                "Modules",
                self.documentation.top_level_modules(),
                self.documentation,
                self.root,
                None,
            );
        }

        output.push_str("<nav class=\"sidebar\"><div class=\"sidebar-crate\"><h2>");
        render_link(
            output,
            None,
            &project_index_href(self.root, self.documentation),
            &self.documentation.project_name,
        );
        output.push_str("</h2></div><ul class=\"block\"><li>");
        render_link(
            output,
            None,
            &format!("{}index.html", self.root),
            "Libraries",
        );
        output.push_str("</li><li>");
        render_link(
            output,
            None,
            &format!(
                "{}all.html",
                project_href_prefix(self.root, self.documentation)
            ),
            "All Items",
        );
        output.push_str("</li></ul>");
        output.push_str(&context);
        output.push_str("</nav>");
    }
}

fn write_item_context(page: &mut String, documentation: &Documentation, active_item: ActiveItem) {
    match active_item {
        ActiveItem::Class(class) => {
            let attributes = local_class_attributes(documentation, class);
            if !attributes.is_empty() {
                let anchors =
                    unique_item_anchors("attr", attributes.iter().map(|attr| attr.name.as_str()));
                page.push_str(
                    "<h3><a href=\"#attributes\">Attributes</a></h3><ul class=\"block item-list\">",
                );
                for (attribute, anchor) in attributes.iter().zip(anchors) {
                    render_link_list_item(page, &format!("#{anchor}"), &attribute.name);
                }
                page.push_str("</ul>");
            }

            let methods = local_class_methods(documentation, class);
            if !methods.is_empty() {
                let anchors = unique_item_anchors(
                    "method",
                    methods.iter().map(|method| method.name.as_str()),
                );
                page.push_str(
                    "<h3><a href=\"#methods\">Methods</a></h3><ul class=\"block item-list\">",
                );
                for (method, anchor) in methods.iter().zip(anchors) {
                    render_link_list_item(page, &format!("#{anchor}"), &method.name);
                }
                page.push_str("</ul>");
            }

            let inherited_groups = collect_inherited_groups(documentation, class);
            if !inherited_groups.is_empty() {
                page.push_str(
                    "<h3><a href=\"#inherited\">Inherited</a></h3><ul class=\"block item-list\">",
                );
                for (group_index, group) in inherited_groups.into_iter().enumerate() {
                    render_link_list_item(
                        page,
                        &format!("#{}", inherited_group_anchor(group_index)),
                        &group.class.name,
                    );
                }
                page.push_str("</ul>");
            }
        }
    }
}

fn write_module_context(
    page: &mut String,
    documentation: &Documentation,
    root: &str,
    active_module: &ModuleDoc,
) {
    write_module_list(
        page,
        "Parent Modules",
        parent_modules(&active_module.name)
            .into_iter()
            .rev()
            .filter_map(|module| documentation.modules.get(&module)),
        documentation,
        root,
        None,
    );
    write_module_list(
        page,
        "Current Module",
        std::iter::once(active_module),
        documentation,
        root,
        Some(active_module),
    );

    write_module_list(
        page,
        "Submodules",
        active_module
            .submodules
            .iter()
            .filter_map(|module| documentation.modules.get(module)),
        documentation,
        root,
        None,
    );
}

fn write_module_list<'a>(
    page: &mut String,
    title: &str,
    modules: impl Iterator<Item = &'a ModuleDoc>,
    documentation: &Documentation,
    root: &str,
    active_module: Option<&ModuleDoc>,
) {
    let modules = modules.collect::<Vec<_>>();
    if modules.is_empty() {
        return;
    }

    page.push_str("<h3>");
    title.render_to(page);
    page.push_str("</h3><ul class=\"module-list\">");
    for module in modules {
        ModuleListItem {
            documentation,
            root,
            module,
            active_module,
        }
        .render_to(page);
    }
    page.push_str("</ul>");
}

#[derive(Copy, Clone)]
enum ModuleLabelStyle {
    Full,
    Short,
}

struct ModuleListItem<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    module: &'a ModuleDoc,
    active_module: Option<&'a ModuleDoc>,
}

impl Render for ModuleListItem<'_> {
    fn render_to(&self, output: &mut String) {
        let active = self
            .active_module
            .is_some_and(|active| active.name == self.module.name);

        if active {
            output.push_str("<li class=\"active\">");
        } else {
            output.push_str("<li>");
        }
        render_link(
            output,
            None,
            &module_href(self.root, self.documentation, &self.module.name),
            module_short_name(&self.module.name),
        );
        output.push_str("</li>");
    }
}

fn write_breadcrumbs(
    body: &mut String,
    documentation: &Documentation,
    root: &str,
    parent: Option<&str>,
) {
    body.push_str("<div class=\"breadcrumbs\">");
    render_link(
        body,
        None,
        &project_index_href(root, documentation),
        &documentation.project_name,
    );
    if let Some(parent) = parent {
        body.push_str(" / ");
        ModulePathLinks {
            root,
            documentation,
            module: parent,
        }
        .render_to(body);
    }
    body.push_str("</div>");
}

fn write_heading(body: &mut String, kind: &str, name: &str, source: Option<String>) {
    body.push_str("<div class=\"main-heading\"><div class=\"main-heading-row\"><h1 id=\"page-title\"><a class=\"pm\" href=\"#page-title\" aria-label=\"Permalink to this page\">§</a>");
    kind.render_to(body);
    body.push(' ');
    body.push_str("<span>");
    name.render_to(body);
    body.push_str("</span></h1>");
    if let Some(source) = source {
        body.push_str("<a class=\"src heading-source\" href=\"");
        render_attr(body, &source);
        body.push_str("\">Source</a>");
    }
    body.push_str("</div></div>");
}

struct ModulePathLinks<'a> {
    root: &'a str,
    documentation: &'a Documentation,
    module: &'a str,
}

impl Render for ModulePathLinks<'_> {
    fn render_to(&self, rendered: &mut String) {
        let mut current = String::new();

        for (index, component) in self.module.split('.').enumerate() {
            if index > 0 {
                rendered.push('.');
                current.push('.');
            }
            current.push_str(component);
            render_link(
                rendered,
                None,
                &module_href(self.root, self.documentation, &current),
                component,
            );
        }
    }
}

fn render_docblock(body: &mut String, docstring: &str) {
    let markdown = Docstring::new(docstring.to_string()).render_markdown();
    body.push_str("<section class=\"doc\">");
    DocMarkdown(&markdown).render_to(body);
    body.push_str("</section>");
}

struct DocMarkdown<'a>(&'a str);

impl Render for DocMarkdown<'_> {
    fn render_to(&self, body: &mut String) {
        let mut paragraph = Vec::new();
        let mut unordered_list = Vec::new();
        let mut ordered_list = Vec::new();
        let mut code_block = Vec::new();
        let mut in_code_block = false;

        for raw_line in self.0.lines() {
            let line = raw_line.trim_end();
            let trimmed = line.trim();

            if trimmed.starts_with("```") {
                if in_code_block {
                    flush_code_block(body, &mut code_block);
                    in_code_block = false;
                } else {
                    flush_paragraph(body, &mut paragraph);
                    flush_unordered_list(body, &mut unordered_list);
                    flush_ordered_list(body, &mut ordered_list);
                    in_code_block = true;
                }
                continue;
            }

            if in_code_block {
                code_block.push(line);
                continue;
            }

            if trimmed.is_empty() {
                flush_paragraph(body, &mut paragraph);
                flush_unordered_list(body, &mut unordered_list);
                flush_ordered_list(body, &mut ordered_list);
            } else if let Some((level, heading)) = doc_heading(trimmed) {
                flush_paragraph(body, &mut paragraph);
                flush_unordered_list(body, &mut unordered_list);
                flush_ordered_list(body, &mut ordered_list);
                DocHeading { level, heading }.render_to(body);
            } else if let Some(item) = unordered_doc_list_item(trimmed) {
                flush_paragraph(body, &mut paragraph);
                flush_ordered_list(body, &mut ordered_list);
                unordered_list.push(item);
            } else if let Some(item) = ordered_doc_list_item(trimmed) {
                flush_paragraph(body, &mut paragraph);
                flush_unordered_list(body, &mut unordered_list);
                ordered_list.push(item);
            } else {
                flush_unordered_list(body, &mut unordered_list);
                flush_ordered_list(body, &mut ordered_list);
                paragraph.push(trimmed);
            }
        }

        flush_paragraph(body, &mut paragraph);
        flush_unordered_list(body, &mut unordered_list);
        flush_ordered_list(body, &mut ordered_list);
        flush_code_block(body, &mut code_block);
    }
}

fn flush_paragraph(body: &mut String, paragraph: &mut Vec<&str>) {
    if paragraph.is_empty() {
        return;
    }

    body.push_str("<p>");
    DocInline(&paragraph.join(" ")).render_to(body);
    body.push_str("</p>");
    paragraph.clear();
}

struct DocHeading<'a> {
    level: usize,
    heading: &'a str,
}

impl Render for DocHeading<'_> {
    fn render_to(&self, output: &mut String) {
        if let 2..=6 = self.level {
            output.push_str("<h");
            output.push_str(&self.level.to_string());
            output.push('>');
            DocInline(self.heading).render_to(output);
            output.push_str("</h");
            output.push_str(&self.level.to_string());
            output.push('>');
        }
    }
}

fn flush_unordered_list(body: &mut String, list: &mut Vec<&str>) {
    flush_list(body, list, "ul");
}

fn flush_ordered_list(body: &mut String, list: &mut Vec<&str>) {
    flush_list(body, list, "ol");
}

fn flush_list(body: &mut String, list: &mut Vec<&str>, tag: &str) {
    if list.is_empty() {
        return;
    }

    if !matches!(tag, "ul" | "ol") {
        return;
    }
    body.push('<');
    body.push_str(tag);
    body.push('>');
    for item in list.drain(..) {
        body.push_str("<li>");
        DocInline(item).render_to(body);
        body.push_str("</li>");
    }
    body.push_str("</");
    body.push_str(tag);
    body.push('>');
}

fn flush_code_block(body: &mut String, code_block: &mut Vec<&str>) {
    if code_block.is_empty() {
        return;
    }

    let code = code_block.join("\n");
    body.push_str("<pre class=\"doc-code\"><code>");
    PythonCode(&code).render_to(body);
    body.push_str("</code></pre>");
    code_block.clear();
}

fn doc_heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=5).contains(&hashes) || !line[hashes..].starts_with(' ') {
        return None;
    }

    Some((hashes + 1, line[hashes..].trim()))
}

fn unordered_doc_list_item(line: &str) -> Option<&str> {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .map(str::trim)
}

fn ordered_doc_list_item(line: &str) -> Option<&str> {
    let (number, item) = line.split_once(". ")?;
    (!number.is_empty() && number.chars().all(|character| character.is_ascii_digit()))
        .then_some(item.trim())
}

struct DocInline<'a>(&'a str);

impl Render for DocInline<'_> {
    fn render_to(&self, body: &mut String) {
        let mut remaining = self.0;

        while !remaining.is_empty() {
            if let Some(after_start) = remaining.strip_prefix('`') {
                if let Some(end) = after_start.find('`') {
                    let (code, after_end) = after_start.split_at(end);
                    body.push_str("<code>");
                    code.render_to(body);
                    body.push_str("</code>");
                    remaining = &after_end[1..];
                    continue;
                }
            }

            if let Some(after_start) = remaining.strip_prefix("**") {
                if let Some(end) = after_start.find("**") {
                    let (strong, after_end) = after_start.split_at(end);
                    body.push_str("<strong>");
                    strong.render_to(body);
                    body.push_str("</strong>");
                    remaining = &after_end[2..];
                    continue;
                }
            }

            if let Some(after_start) = remaining.strip_prefix('*') {
                if let Some(end) = after_start.find('*') {
                    let (emphasis, after_end) = after_start.split_at(end);
                    body.push_str("<em>");
                    emphasis.render_to(body);
                    body.push_str("</em>");
                    remaining = &after_end[1..];
                    continue;
                }
            }

            if let Some(after_label_start) = remaining.strip_prefix('[')
                && let Some(label_end) = after_label_start.find("](")
            {
                let (label, after_label) = after_label_start.split_at(label_end);
                let after_url_start = &after_label[2..];
                if let Some(url_end) = after_url_start.find(')') {
                    let (url, after_url) = after_url_start.split_at(url_end);
                    if is_safe_doc_href(url) {
                        render_link(body, None, url, label);
                        remaining = &after_url[1..];
                        continue;
                    }
                }
            }

            if let Some(after_slash) = remaining.strip_prefix('\\')
                && let Some(character) = after_slash.chars().next()
                && is_markdown_escaped_character(character)
            {
                character.render_to(body);
                remaining = &after_slash[character.len_utf8()..];
                continue;
            }

            let character = remaining.chars().next().unwrap();
            character.render_to(body);
            remaining = &remaining[character.len_utf8()..];
        }
    }
}

fn is_markdown_escaped_character(character: char) -> bool {
    matches!(
        character,
        '\\' | '`'
            | '*'
            | '_'
            | '{'
            | '}'
            | '['
            | ']'
            | '<'
            | '>'
            | '('
            | ')'
            | '#'
            | '+'
            | '-'
            | '.'
            | '!'
            | '|'
    )
}

fn is_safe_doc_href(url: &str) -> bool {
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with('#')
        || url.starts_with('/')
        || url.starts_with("./")
        || url.starts_with("../")
        || (!url.starts_with("//") && !url.contains(':'))
}

struct PythonCode<'a>(&'a str);

impl Render for PythonCode<'_> {
    fn render_to(&self, output: &mut String) {
        let tokens = parse_python_tokens(self.0);
        PythonTokens {
            source: self.0,
            tokens: &tokens,
            signature_links: None,
        }
        .render_to(output);
    }
}

struct PythonSignature<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    current_module: &'a str,
    signature: &'a str,
    signature_links: &'a BTreeMap<String, String>,
}

impl Render for PythonSignature<'_> {
    fn render_to(&self, output: &mut String) {
        let tokens = parse_python_tokens(self.signature);
        PythonTokens {
            source: self.signature,
            tokens: &tokens,
            signature_links: Some(SignatureLinkContext {
                documentation: self.documentation,
                root: self.root,
                current_module: self.current_module,
                signature_links: self.signature_links,
            }),
        }
        .render_to(output);
    }
}

fn highlight_python_source_lines(source: &str, tokens: &Tokens) -> Vec<String> {
    if source.is_empty() {
        return Vec::new();
    }

    let mut lines = vec![String::new()];
    let mut cursor = TextSize::default();
    for token in tokens
        .iter()
        .copied()
        .take_while(|token| !token.kind().is_eof())
    {
        push_source_fragment(
            &mut lines,
            &source[TextRange::new(cursor, token.start())],
            None,
        );
        push_source_fragment(
            &mut lines,
            &source[token.range()],
            token_syntax_class(token.kind()),
        );
        cursor = token.end();
    }
    push_source_fragment(
        &mut lines,
        &source[TextRange::new(cursor, source.text_len())],
        None,
    );
    lines
}

#[derive(Copy, Clone)]
struct SignatureLinkContext<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    current_module: &'a str,
    signature_links: &'a BTreeMap<String, String>,
}

impl SignatureLinkContext<'_> {
    fn resolve_identifier(self, token: &str) -> Option<ResolvedSignatureLink> {
        if let Some(href) = self.signature_links.get(token) {
            return Some(ResolvedSignatureLink {
                href: href.clone(),
                class: "bi",
            });
        }

        if !is_signature_type_identifier(token) {
            return None;
        }

        let mut module_name = Some(self.current_module);
        while let Some(name) = module_name {
            if let Some(module) = self.documentation.modules.get(name)
                && let Some(target) = find_type_target_in_module(module, token)
            {
                return Some(ResolvedSignatureLink {
                    href: type_link_href(self.root, self.documentation, &target),
                    class: "ty",
                });
            }

            module_name = parent_module(name);
        }

        match self.documentation.type_index.get(token) {
            Some(TypeIndexEntry::Unique(target)) => Some(ResolvedSignatureLink {
                href: type_link_href(self.root, self.documentation, target),
                class: "ty",
            }),
            Some(TypeIndexEntry::Ambiguous) | None => None,
        }
    }
}

struct ResolvedSignatureLink {
    href: String,
    class: &'static str,
}

struct PythonTokens<'a> {
    source: &'a str,
    tokens: &'a Tokens,
    signature_links: Option<SignatureLinkContext<'a>>,
}

impl Render for PythonTokens<'_> {
    fn render_to(&self, rendered: &mut String) {
        let tokens = self
            .tokens
            .iter()
            .copied()
            .take_while(|token| !token.kind().is_eof())
            .collect::<Vec<_>>();
        let mut cursor = TextSize::default();
        let mut index = 0;

        while let Some(token) = tokens.get(index).copied() {
            self.source[TextRange::new(cursor, token.start())].render_to(rendered);

            if token.kind() == TokenKind::Name
                && let Some(links) = self.signature_links
            {
                if let Some(run_end) = dotted_name_run_end(&tokens, index) {
                    let last = tokens[run_end - 1];
                    render_signature_dotted_identifier(
                        rendered,
                        links,
                        &self.source[TextRange::new(token.start(), last.end())],
                    );
                    cursor = last.end();
                    index = run_end;
                    continue;
                }
                render_signature_identifier(rendered, links, &self.source[token.range()]);
            } else {
                SyntaxFragment {
                    source: &self.source[token.range()],
                    class: token_syntax_class(token.kind()),
                }
                .render_to(rendered);
            }

            cursor = token.end();
            index += 1;
        }

        self.source[TextRange::new(cursor, self.source.text_len())].render_to(rendered);
    }
}

struct SignatureLink<'a> {
    text: &'a str,
    link: &'a ResolvedSignatureLink,
}

impl Render for SignatureLink<'_> {
    fn render_to(&self, output: &mut String) {
        output.push_str("<a class=\"");
        self.link.class.render_to(output);
        output.push_str("\" href=\"");
        self.link.href.render_to(output);
        output.push_str("\">");
        self.text.render_to(output);
        output.push_str("</a>");
    }
}

struct SyntaxFragment<'a> {
    source: &'a str,
    class: Option<&'a str>,
}

impl Render for SyntaxFragment<'_> {
    fn render_to(&self, output: &mut String) {
        if let Some(class) = self.class {
            output.push_str("<span class=\"");
            class.render_to(output);
            output.push_str("\">");
            self.source.render_to(output);
            output.push_str("</span>");
        } else {
            self.source.render_to(output);
        }
    }
}

fn render_signature_dotted_identifier(
    output: &mut String,
    links: SignatureLinkContext<'_>,
    path: &str,
) {
    let mut prefix = String::new();
    for (index, component) in path.split('.').enumerate() {
        if index > 0 {
            output.push('.');
            prefix.push('.');
        }
        prefix.push_str(component);

        if let Some(link) = links
            .resolve_identifier(&prefix)
            .or_else(|| links.resolve_identifier(component))
        {
            SignatureLink {
                text: component,
                link: &link,
            }
            .render_to(output);
        } else {
            component.render_to(output);
        }
    }
}

fn render_signature_identifier(
    output: &mut String,
    links: SignatureLinkContext<'_>,
    identifier: &str,
) {
    if let Some(link) = links.resolve_identifier(identifier) {
        SignatureLink {
            text: identifier,
            link: &link,
        }
        .render_to(output);
    } else {
        identifier.render_to(output);
    }
}

fn token_syntax_class(kind: TokenKind) -> Option<&'static str> {
    if kind == TokenKind::Comment {
        Some("com")
    } else if kind.is_keyword() || kind.is_singleton() {
        Some("kw")
    } else if matches!(
        kind,
        TokenKind::String
            | TokenKind::FStringStart
            | TokenKind::FStringMiddle
            | TokenKind::FStringEnd
            | TokenKind::TStringStart
            | TokenKind::TStringMiddle
            | TokenKind::TStringEnd
    ) {
        Some("str")
    } else if matches!(kind, TokenKind::Int | TokenKind::Float | TokenKind::Complex) {
        Some("num")
    } else {
        None
    }
}

fn push_source_fragment(lines: &mut Vec<String>, source: &str, class: Option<&str>) {
    let mut fragments = source.split('\n').peekable();
    while let Some(fragment) = fragments.next() {
        let fragment = fragment.strip_suffix('\r').unwrap_or(fragment);
        if let Some(line) = lines.last_mut() {
            SyntaxFragment {
                source: fragment,
                class,
            }
            .render_to(line);
        }
        if fragments.peek().is_some() {
            lines.push(String::new());
        }
    }
}

fn find_type_target_in_module(module: &ModuleDoc, name: &str) -> Option<TypeLinkTarget> {
    if let Some(class) = module.classes.iter().find(|class| class.name == name) {
        return Some(TypeLinkTarget {
            module: module.name.clone(),
            kind: TypeLinkKind::Class,
            name: class.name.clone(),
        });
    }

    module
        .variables
        .iter()
        .find(|variable| {
            variable.name == name
                && (variable.kind == VariableKind::TypeAlias
                    || is_signature_type_identifier(&variable.name))
        })
        .map(|variable| TypeLinkTarget {
            module: module.name.clone(),
            kind: match variable.kind {
                VariableKind::Variable => TypeLinkKind::Variable,
                VariableKind::TypeAlias => TypeLinkKind::TypeAlias,
            },
            name: variable.name.clone(),
        })
}

fn type_link_href(root: &str, documentation: &Documentation, target: &TypeLinkTarget) -> String {
    match target.kind {
        TypeLinkKind::Class => class_page_href(root, documentation, &target.module, &target.name),
        TypeLinkKind::TypeAlias => anchored_href(
            &module_href(root, documentation, &target.module),
            "type",
            &target.name,
        ),
        TypeLinkKind::Variable => anchored_href(
            &module_href(root, documentation, &target.module),
            "var",
            &target.name,
        ),
    }
}

fn render_signature(
    body: &mut String,
    documentation: &Documentation,
    root: &str,
    module: &str,
    signature: &str,
    signature_links: &BTreeMap<String, String>,
) {
    body.push_str("<pre class=\"signature\"><code>");
    PythonSignature {
        documentation,
        root,
        current_module: module,
        signature,
        signature_links,
    }
    .render_to(body);
    body.push_str("</code></pre>");
}

fn render_enum_signature(
    body: &mut String,
    documentation: &Documentation,
    root: &str,
    module: &str,
    class: &ClassDoc,
    members: &[&VariableDoc],
) {
    let anchors = unique_item_anchors("member", members.iter().map(|member| member.name.as_str()));
    body.push_str("<pre class=\"signature enum-signature\"><code>");
    PythonSignature {
        documentation,
        root,
        current_module: module,
        signature: &class.signature,
        signature_links: &class.signature_links,
    }
    .render_to(body);
    body.push(':');
    for (member, anchor) in members.iter().zip(anchors) {
        body.push_str("<span id=\"");
        render_attr(body, &anchor);
        body.push_str("\" class=\"enum-member-line\">    ");
        member.name.render_to(body);
        PythonSignature {
            documentation,
            root,
            current_module: module,
            signature: &member.signature,
            signature_links: &member.signature_links,
        }
        .render_to(body);
        body.push_str("</span>");
    }
    body.push_str("</code></pre>");
}

fn collapsed_definition_signature(signature: &str) -> Option<String> {
    if !signature.contains('\n') {
        return None;
    }

    let open = signature.find('(')?;
    let close = matching_close_parenthesis(signature, open)?;
    let mut collapsed = String::with_capacity(signature.len());
    collapsed.push_str(&signature[..=open]);
    collapsed.push_str("...");
    collapsed.push_str(&collapse_whitespace(&signature[close..]));
    Some(collapsed)
}

fn matching_close_parenthesis(signature: &str, open: usize) -> Option<usize> {
    let mut depth = 0_u32;
    for (index, character) in signature
        .char_indices()
        .skip_while(|(index, _)| *index < open)
    {
        match character {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }

    None
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct CollapsibleSignature<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    module: &'a str,
    signature: &'a str,
    signature_links: &'a BTreeMap<String, String>,
}

impl Render for CollapsibleSignature<'_> {
    fn render_to(&self, output: &mut String) {
        let expanded = PythonSignature {
            documentation: self.documentation,
            root: self.root,
            current_module: self.module,
            signature: self.signature,
            signature_links: self.signature_links,
        };
        let Some(collapsed) = collapsed_definition_signature(self.signature) else {
            output.push_str("<code class=\"sig\">");
            expanded.render_to(output);
            output.push_str("</code>");
            return;
        };

        output.push_str("<code class=\"sig sgx\">");
        expanded.render_to(output);
        output.push_str("</code><code class=\"sig sgc\">");
        PythonSignature {
            documentation: self.documentation,
            root: self.root,
            current_module: self.module,
            signature: &collapsed,
            signature_links: self.signature_links,
        }
        .render_to(output);
        output.push_str("</code>");
    }
}

fn collapsed_attribute_signature(name: &str, signature: &str) -> Option<String> {
    if !signature.contains('\n') && name.chars().count() + signature.chars().count() <= 88 {
        return None;
    }

    let signature = signature.trim();
    if signature.is_empty() {
        return None;
    }

    if let Some(index) = signature.find('=') {
        let prefix = signature[..index].trim_end();
        return Some(format!("{name}{prefix} = ..."));
    }

    if signature.starts_with(':') {
        Some(format!("{name}: ..."))
    } else {
        Some(format!("{name} ..."))
    }
}

struct CollapsibleAttributeSignature<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    module: &'a str,
    variable: &'a VariableDoc,
}

impl Render for CollapsibleAttributeSignature<'_> {
    fn render_to(&self, output: &mut String) {
        let expanded = format!("{}{}", self.variable.name, self.variable.signature);
        let expanded = PythonSignature {
            documentation: self.documentation,
            root: self.root,
            current_module: self.module,
            signature: &expanded,
            signature_links: &self.variable.signature_links,
        };
        let Some(collapsed) =
            collapsed_attribute_signature(&self.variable.name, &self.variable.signature)
        else {
            output.push_str("<code class=\"asg var sig\">");
            expanded.render_to(output);
            output.push_str("</code>");
            return;
        };

        output.push_str("<code class=\"asg var sig sgx\">");
        expanded.render_to(output);
        output.push_str("</code><code class=\"asg var sig sgc\">");
        PythonSignature {
            documentation: self.documentation,
            root: self.root,
            current_module: self.module,
            signature: &collapsed,
            signature_links: &self.variable.signature_links,
        }
        .render_to(output);
        output.push_str("</code>");
    }
}

fn render_module_table<'a>(
    body: &mut String,
    title: &str,
    modules: impl Iterator<Item = &'a ModuleDoc>,
    documentation: &Documentation,
    root: &str,
    label_style: ModuleLabelStyle,
) {
    render_item_table(
        body,
        title,
        &title.to_ascii_lowercase(),
        modules,
        |module, rows| {
            let label = match label_style {
                ModuleLabelStyle::Full => &module.name,
                ModuleLabelStyle::Short => module_short_name(&module.name),
            };
            render_item_table_row(
                rows,
                "mod",
                &module_href(root, documentation, &module.name),
                label,
                module.summary(),
            );
        },
    );
}

fn render_all_variable_items(
    body: &mut String,
    title: &str,
    documentation: &Documentation,
    kind: VariableKind,
) {
    render_item_table(
        body,
        title,
        &title.to_ascii_lowercase().replace(' ', "-"),
        documentation.modules.values().flat_map(|module| {
            module
                .variables
                .iter()
                .filter(|variable| {
                    variable.kind == kind && module.public_items.contains(&variable.name)
                })
                .map(move |variable| (module, variable))
        }),
        |(module, variable), rows| {
            render_item_table_row(
                rows,
                kind.anchor_prefix(),
                &anchored_href(
                    &module_href("../", documentation, &module.name),
                    kind.anchor_prefix(),
                    &variable.name,
                ),
                &format!("{}.{}", module.name, variable.name),
                variable.summary(),
            );
        },
    );
}

fn render_item_table_row(rows: &mut String, class: &str, href: &str, label: &str, summary: &str) {
    rows.push_str("<dt>");
    render_link(rows, Some(class), href, label);
    rows.push_str("</dt><dd>");
    DocInline(summary).render_to(rows);
    rows.push_str("</dd>");
}

fn render_item_table<T>(
    body: &mut String,
    title: &str,
    section_id: &str,
    items: impl IntoIterator<Item = T>,
    mut render_item: impl FnMut(T, &mut String),
) {
    let items = items.into_iter().collect::<Vec<_>>();
    if items.is_empty() {
        return;
    }

    write_section_heading(body, section_id, title);
    body.push_str("<dl class=\"item-table\">");
    for item in items {
        render_item(item, body);
    }
    body.push_str("</dl>");
}

struct InheritedMemberDoc<'a, T> {
    module: &'a ModuleDoc,
    class: &'a ClassDoc,
    member: &'a T,
    override_member: Option<&'a T>,
}

type InheritedFunctionDoc<'a> = InheritedMemberDoc<'a, FunctionDoc>;
type InheritedAttributeDoc<'a> = InheritedMemberDoc<'a, VariableDoc>;

struct InheritedGroup<'a> {
    module: &'a ModuleDoc,
    class: &'a ClassDoc,
    attributes: Vec<InheritedAttributeDoc<'a>>,
    methods: Vec<InheritedFunctionDoc<'a>>,
}

fn inherited_group_anchor(index: usize) -> String {
    format!("inherited.{index}")
}

fn inherited_member_anchor(group_index: usize, kind: &str, member_index: usize) -> String {
    format!("inherited.{group_index}.{kind}.{member_index}")
}

fn render_inherited_members(
    body: &mut String,
    documentation: &Documentation,
    root: &str,
    current_module: &ModuleDoc,
    class: &ClassDoc,
) {
    let inherited_groups = collect_inherited_groups(documentation, class);
    if inherited_groups.is_empty() {
        return;
    }

    write_section_heading(body, "inherited", "Inherited");
    let mut groups = String::new();

    for (group_index, group) in inherited_groups.into_iter().enumerate() {
        let group_anchor = inherited_group_anchor(group_index);
        let mut attribute_sections = String::new();
        let mut method_sections = String::new();

        for (attribute_index, attribute) in group.attributes.into_iter().enumerate() {
            let variable = attribute.override_member.unwrap_or(attribute.member);
            let attribute_module = if attribute.override_member.is_some() {
                current_module
            } else {
                attribute.module
            };
            let anchor = attribute.override_member.map_or_else(
                || inherited_member_anchor(group_index, "a", attribute_index),
                |_| item_anchor("attr", &variable.name),
            );
            let documentation_doc = variable.docstring.as_deref().or(attribute
                .override_member
                .and(attribute.member.docstring.as_deref()));
            render_attribute_section(
                &mut attribute_sections,
                documentation,
                root,
                &attribute_module.name,
                attribute_module.source.as_ref(),
                variable,
                documentation_doc,
                &anchor,
                None,
            );
        }

        for (method_index, method) in group.methods.into_iter().enumerate() {
            let function = method.override_member.unwrap_or(method.member);
            let function_module = if method.override_member.is_some() {
                current_module
            } else {
                method.module
            };
            let anchor = method.override_member.map_or_else(
                || inherited_member_anchor(group_index, "m", method_index),
                |_| item_anchor("method", &function.name),
            );
            let documentation_doc = function.documentation().or_else(|| {
                method
                    .override_member
                    .and_then(|_| method.member.documentation())
            });
            render_function_section(
                &mut method_sections,
                documentation,
                root,
                FunctionSection {
                    function,
                    module: function_module,
                    anchor,
                    documentation: documentation_doc,
                    override_note: None,
                    source_opens_details: true,
                },
            );
        }

        groups.push_str("<section id=\"");
        render_attr(&mut groups, &group_anchor);
        groups.push_str("\" class=\"ig col open\"><div class=\"isum\"><button class=\"tog ht\" aria-label=\"Toggle inherited members from ");
        render_attr(&mut groups, &group.class.name);
        groups.push_str("\" aria-expanded=\"true\"></button><a class=\"pm\" href=\"#");
        render_attr(&mut groups, &group_anchor);
        groups.push_str("\" aria-label=\"Permalink to inherited members from ");
        render_attr(&mut groups, &group.class.name);
        groups.push_str("\">§</a><span class=\"inherited-title\">Inherited from ");
        render_link(
            &mut groups,
            Some("class ibl"),
            &class_page_href(root, documentation, &group.module.name, &group.class.name),
            &group.class.name,
        );
        groups.push_str("</span></div><div class=\"dc\">");
        if !attribute_sections.is_empty() {
            groups.push_str("<h3 class=\"ish\">Attributes</h3><div class=\"iss ats\">");
            groups.push_str(&attribute_sections);
            groups.push_str("</div>");
        }
        if !method_sections.is_empty() {
            groups.push_str("<h3 class=\"ish\">Methods</h3><div class=\"iss\">");
            groups.push_str(&method_sections);
            groups.push_str("</div>");
        }
        groups.push_str("</div></section>");
    }

    body.push_str("<div class=\"inherited-impls\">");
    body.push_str(&groups);
    body.push_str("</div>");
}

fn method_override_note<'a>(
    documentation: &'a Documentation,
    root: &'a str,
    class: &'a ClassDoc,
    function: &'a FunctionDoc,
) -> Option<OverrideNote<'a>> {
    find_inherited_function(documentation, class, &function.name).map(|base| OverrideNote {
        href: anchored_href(
            &class_page_href(root, documentation, &base.module.name, &base.class.name),
            "method",
            &base.member.name,
        ),
        class_name: &base.class.name,
        member_name: &base.member.name,
    })
}

fn attribute_override_note<'a>(
    documentation: &'a Documentation,
    root: &'a str,
    class: &'a ClassDoc,
    variable: &'a VariableDoc,
) -> Option<OverrideNote<'a>> {
    find_inherited_attribute(documentation, class, &variable.name).map(|base| OverrideNote {
        href: anchored_href(
            &class_page_href(root, documentation, &base.module.name, &base.class.name),
            "attr",
            &base.member.name,
        ),
        class_name: &base.class.name,
        member_name: &base.member.name,
    })
}

struct OverrideNote<'a> {
    href: String,
    class_name: &'a str,
    member_name: &'a str,
}

impl Render for OverrideNote<'_> {
    fn render_to(&self, output: &mut String) {
        output.push_str("<div class=\"item-meta\">Overrides <a href=\"");
        render_attr(output, &self.href);
        output.push_str("\">");
        self.class_name.render_to(output);
        output.push('.');
        self.member_name.render_to(output);
        output.push_str("</a></div>");
    }
}

struct FunctionOverloads<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    module: &'a str,
    source: Option<&'a SourceDoc>,
    function: &'a FunctionDoc,
}

impl Render for FunctionOverloads<'_> {
    fn render_to(&self, output: &mut String) {
        let overloads = self.function.overloads_to_render();
        if overloads.is_empty() {
            return;
        }

        output.push_str("<div class=\"overload-list\"><h3>Overload signatures</h3>");
        for overload in overloads {
            let source_link = source_href_for(
                self.root,
                self.documentation,
                self.source,
                Some(&overload.source_line),
            );
            output.push_str("<div class=\"overload-entry\"><div class=\"overload-signature\"><code class=\"sig\">");
            PythonSignature {
                documentation: self.documentation,
                root: self.root,
                current_module: self.module,
                signature: &overload.signature,
                signature_links: &overload.signature_links,
            }
            .render_to(output);
            output.push_str("</code>");
            if let Some(source_link) = source_link {
                render_source_action(output, &source_link);
            }
            output.push_str("</div>");
            if let Some(docstring) = overload.docstring() {
                render_docblock(output, docstring);
            }
            output.push_str("</div>");
        }
        output.push_str("</div>");
    }
}

struct FunctionSection<'a> {
    function: &'a FunctionDoc,
    module: &'a ModuleDoc,
    anchor: String,
    documentation: Option<&'a str>,
    override_note: Option<OverrideNote<'a>>,
    source_opens_details: bool,
}

fn render_function_section(
    body: &mut String,
    documentation: &Documentation,
    root: &str,
    section: FunctionSection<'_>,
) {
    let FunctionSection {
        function,
        module,
        anchor,
        documentation: documentation_doc,
        override_note,
        source_opens_details,
    } = section;
    let source = source_href_for(
        root,
        documentation,
        module.source.as_ref(),
        Some(&function.source_line),
    );
    let has_details = override_note.is_some()
        || !function.overloads_to_render().is_empty()
        || documentation_doc.is_some()
        || collapsed_definition_signature(&function.signature).is_some()
        || source_opens_details && source.is_some();
    let item_actions = has_details.then_some(source).flatten();
    let item_permalink = Permalink { anchor: &anchor };
    let overload_count = function.overloads.len();
    let signature = CollapsibleSignature {
        documentation,
        root,
        module: &module.name,
        signature: &function.signature,
        signature_links: &function.signature_links,
    };
    let overloads = FunctionOverloads {
        documentation,
        root,
        module: &module.name,
        source: module.source.as_ref(),
        function,
    };
    let documentation_doc = documentation_doc.filter(|docstring| {
        function.docstring.is_some()
            || !function
                .overloads_to_render()
                .iter()
                .any(|overload| overload.docstring() == Some(*docstring))
    });

    render_item_section(
        body,
        &anchor,
        "itm",
        has_details,
        |body| {
            item_permalink.render_to(body);
            if has_details {
                body.push_str("<span class=\"sc\">");
                signature.render_to(body);
                if overload_count > 0 {
                    render_overload_count(body, overload_count);
                }
                body.push_str("</span>");
            } else {
                signature.render_to(body);
            }
            if let Some(item_actions) = &item_actions {
                render_source_action(body, item_actions);
            }
        },
        |body| {
            if let Some(override_note) = &override_note {
                override_note.render_to(body);
            }
            overloads.render_to(body);
            if let Some(docstring) = documentation_doc {
                render_docblock(body, docstring);
            }
        },
    );
}

fn render_item_section(
    body: &mut String,
    anchor: &str,
    item_class: &str,
    collapsible: bool,
    render_summary: impl FnOnce(&mut String),
    render_details: impl FnOnce(&mut String),
) {
    body.push_str("<section id=\"");
    render_attr(body, anchor);
    body.push_str("\" class=\"");
    render_attr(body, item_class);
    if collapsible {
        body.push_str("\"><div class=\"det col open\"><div class=\"sum\"><button class=\"tog itog\" aria-label=\"Toggle details\" aria-expanded=\"true\"></button>");
        render_summary(body);
        body.push_str("</div><div class=\"dc\">");
        render_details(body);
        body.push_str("</div></div></section>");
    } else {
        body.push_str("\"><div class=\"sum sst\">");
        render_summary(body);
        body.push_str("</div>");
        render_details(body);
        body.push_str("</section>");
    }
}

fn render_overload_count(output: &mut String, count: usize) {
    output.push_str("<span class=\"overload-count\">");
    output.push_str(&count.to_string());
    output.push_str(" overload");
    if count != 1 {
        output.push('s');
    }
    output.push_str("</span>");
}

#[derive(Copy, Clone)]
struct FunctionSections<'a> {
    documentation: &'a Documentation,
    root: &'a str,
    module: &'a ModuleDoc,
    section_anchor: &'static str,
    title: &'static str,
    item_anchor_prefix: &'static str,
}

fn render_function_sections<'a>(
    body: &mut String,
    section: FunctionSections<'a>,
    functions: impl IntoIterator<Item = &'a FunctionDoc>,
    mut override_note: impl FnMut(&'a FunctionDoc) -> Option<OverrideNote<'a>>,
) {
    let functions = functions.into_iter().collect::<Vec<_>>();
    if functions.is_empty() {
        return;
    }

    write_section_heading(body, section.section_anchor, section.title);
    let mut sections = String::new();

    for (function, anchor) in functions.iter().copied().zip(unique_item_anchors(
        section.item_anchor_prefix,
        functions.iter().map(|function| function.name.as_str()),
    )) {
        render_function_section(
            &mut sections,
            section.documentation,
            section.root,
            FunctionSection {
                function,
                module: section.module,
                anchor,
                documentation: function.documentation(),
                override_note: override_note(function),
                source_opens_details: false,
            },
        );
    }
    body.push_str("<div class=\"iss\">");
    body.push_str(&sections);
    body.push_str("</div>");
}

fn render_attribute_sections(
    body: &mut String,
    title: &str,
    variables: &[&VariableDoc],
    documentation: &Documentation,
    root: &str,
    module: &ModuleDoc,
    class: &ClassDoc,
) {
    if variables.is_empty() {
        return;
    }

    write_section_heading(body, &title.to_ascii_lowercase(), title);
    let mut sections = String::new();

    for (variable, anchor) in variables.iter().zip(unique_item_anchors(
        "attr",
        variables.iter().map(|variable| variable.name.as_str()),
    )) {
        let override_note = attribute_override_note(documentation, root, class, variable);
        render_attribute_section(
            &mut sections,
            documentation,
            root,
            &module.name,
            module.source.as_ref(),
            variable,
            variable.docstring.as_deref(),
            &anchor,
            override_note.as_ref(),
        );
    }

    body.push_str("<div class=\"iss ats\">");
    body.push_str(&sections);
    body.push_str("</div>");
}

#[expect(
    clippy::too_many_arguments,
    reason = "Attribute rendering mirrors the method renderer's explicit inputs."
)]
fn render_attribute_section<'a>(
    body: &mut String,
    documentation: &'a Documentation,
    root: &'a str,
    module: &'a str,
    source: Option<&'a SourceDoc>,
    variable: &'a VariableDoc,
    documentation_doc: Option<&'a str>,
    anchor: &str,
    override_note: Option<&OverrideNote<'a>>,
) {
    let item_actions = source_href_for(root, documentation, source, Some(&variable.source_line));
    let has_collapsed_signature =
        collapsed_attribute_signature(&variable.name, &variable.signature).is_some();
    let item_permalink = Permalink { anchor };
    let signature = CollapsibleAttributeSignature {
        documentation,
        root,
        module,
        variable,
    };
    render_item_section(
        body,
        anchor,
        "itm",
        has_collapsed_signature,
        |body| {
            item_permalink.render_to(body);
            body.push_str("<span class=\"sc\">");
            signature.render_to(body);
            body.push_str("</span>");
            if let Some(item_actions) = &item_actions {
                render_source_action(body, item_actions);
            }
        },
        |body| {
            if let Some(override_note) = override_note {
                override_note.render_to(body);
            }
            if let Some(docstring) = documentation_doc {
                render_docblock(body, docstring);
            }
        },
    );
}

fn render_variable_table(
    body: &mut String,
    title: &str,
    variables: &[VariableDoc],
    documentation: &Documentation,
    root: &str,
    module: &ModuleDoc,
) {
    if variables.is_empty() {
        return;
    }

    write_section_heading(body, &title.to_ascii_lowercase(), title);
    let mut rows = String::new();

    for variable in variables {
        let class = match variable.kind {
            VariableKind::Variable => "var",
            VariableKind::TypeAlias => "type",
        };
        let anchor = item_anchor(variable.kind.anchor_prefix(), &variable.name);
        let source_link = source_href_for(
            root,
            documentation,
            module.source.as_ref(),
            Some(&variable.source_line),
        );
        let has_details = !variable.summary().is_empty() || source_link.is_some();
        rows.push_str("<dt id=\"");
        render_attr(&mut rows, &anchor);
        rows.push_str("\" class=\"ve\"><a class=\"pm vpm\" href=\"#");
        render_attr(&mut rows, &anchor);
        rows.push_str("\" aria-label=\"Permalink to ");
        render_attr(&mut rows, class);
        rows.push(' ');
        render_attr(&mut rows, &variable.name);
        rows.push_str("\">§</a><code class=\"vs ");
        render_attr(&mut rows, class);
        rows.push_str("\">");
        variable.name.render_to(&mut rows);
        PythonSignature {
            documentation,
            root,
            current_module: &module.name,
            signature: &variable.signature,
            signature_links: &variable.signature_links,
        }
        .render_to(&mut rows);
        rows.push_str("</code></dt>");
        if has_details {
            rows.push_str("<dd class=\"vd\">");
            if !variable.summary().is_empty() {
                rows.push_str("<p>");
                DocInline(variable.summary()).render_to(&mut rows);
                rows.push_str("</p>");
            }
            if let Some(source_link) = source_link {
                render_source_action(&mut rows, &source_link);
            }
            rows.push_str("</dd>");
        }
    }
    body.push_str("<dl class=\"vl\">");
    body.push_str(&rows);
    body.push_str("</dl>");
}

fn root_prefix_for_module(module: &str) -> String {
    "../".repeat(module.split('.').count() + 1)
}

fn root_prefix_for_source(source_path: &str) -> String {
    let component_count = source_path
        .split('/')
        .filter(|component| !component.is_empty())
        .count()
        .max(1);
    "../".repeat(component_count + 1)
}

fn project_href_prefix(root: &str, documentation: &Documentation) -> String {
    format!("{}{}/", root, documentation.project_slug)
}

fn project_index_href(root: &str, documentation: &Documentation) -> String {
    format!("{}index.html", project_href_prefix(root, documentation))
}

fn module_href_prefix(root: &str, documentation: &Documentation, module: &str) -> String {
    let mut href = project_href_prefix(root, documentation);
    for component in module.split('.') {
        href.push_str(&sanitize_path_segment(component));
        href.push('/');
    }
    href
}

fn module_href(root: &str, documentation: &Documentation, module: &str) -> String {
    format!(
        "{}index.html",
        module_href_prefix(root, documentation, module)
    )
}

fn item_href(root: &str, documentation: &Documentation, module: &str, file: &str) -> String {
    format!("{}{file}", module_href_prefix(root, documentation, module))
}

fn source_href(
    root: &str,
    documentation: &Documentation,
    source_path: &str,
    line: Option<&str>,
) -> String {
    let mut href = format!("{}{}", root, source_doc_path(documentation, source_path));
    if let Some(line) = line {
        href.push_str("#L");
        href.push_str(line);
    }
    href
}

fn source_href_for(
    root: &str,
    documentation: &Documentation,
    source: Option<&SourceDoc>,
    line: Option<&str>,
) -> Option<String> {
    let source = source?;
    Some(source_href(root, documentation, &source.path, line))
}

fn item_anchor(prefix: &str, name: &str) -> String {
    format!("{}.{}", prefix, sanitize_path_segment(name))
}

fn unique_item_anchors<'a>(prefix: &str, names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut counts = BTreeMap::new();
    names
        .into_iter()
        .map(|name| {
            let base = item_anchor(prefix, name);
            let count = counts.entry(base.clone()).or_insert(0);
            let anchor = if *count == 0 {
                base
            } else {
                format!("{base}-{count}", count = *count)
            };
            *count += 1;
            anchor
        })
        .collect()
}

fn write_section_heading(body: &mut String, anchor: &str, title: &str) {
    body.push_str("<h2 id=\"");
    render_attr(body, anchor);
    body.push_str("\">");
    Permalink { anchor }.render_to(body);
    title.render_to(body);
    body.push_str("</h2>");
}

fn render_source_action(output: &mut String, href: &str) {
    render_link(output, Some("src"), href, "Source");
}

struct Permalink<'a> {
    anchor: &'a str,
}

impl Render for Permalink<'_> {
    fn render_to(&self, output: &mut String) {
        output.push_str("<a class=\"pm\" href=\"#");
        render_attr(output, self.anchor);
        output.push_str("\" aria-label=\"Permalink\">§</a>");
    }
}

fn class_page_href(root: &str, documentation: &Documentation, module: &str, class: &str) -> String {
    item_href(
        root,
        documentation,
        module,
        &format!("class.{}.html", sanitize_path_segment(class)),
    )
}

fn anchored_href(page_href: &str, anchor_prefix: &str, item_name: &str) -> String {
    format!("{page_href}#{}", item_anchor(anchor_prefix, item_name))
}

fn find_documented_class<'a>(
    documentation: &'a Documentation,
    class_ref: &ClassBaseDoc,
) -> Option<(&'a ModuleDoc, &'a ClassDoc)> {
    let module = documentation.modules.get(&class_ref.module)?;
    let class = module
        .classes
        .iter()
        .find(|class| class.name == class_ref.name)?;
    Some((module, class))
}

fn documented_base_classes<'a>(
    documentation: &'a Documentation,
    class: &ClassDoc,
) -> Vec<(&'a ModuleDoc, &'a ClassDoc)> {
    class
        .base_classes
        .iter()
        .filter_map(|base| find_documented_class(documentation, base))
        .collect()
}

fn documented_base_class_chain<'a>(
    documentation: &'a Documentation,
    class: &ClassDoc,
) -> Vec<(&'a ModuleDoc, &'a ClassDoc)> {
    let mut chain = Vec::new();
    let mut visited = BTreeSet::new();
    collect_documented_base_class_chain(documentation, class, &mut visited, &mut chain);
    chain
}

fn collect_documented_base_class_chain<'a>(
    documentation: &'a Documentation,
    class: &ClassDoc,
    visited: &mut BTreeSet<String>,
    chain: &mut Vec<(&'a ModuleDoc, &'a ClassDoc)>,
) {
    for (module, base_class) in documented_base_classes(documentation, class) {
        if visited.insert(class_key(module, base_class)) {
            chain.push((module, base_class));
            collect_documented_base_class_chain(documentation, base_class, visited, chain);
        }
    }
}

fn find_inherited_member<'a, T, F>(
    documentation: &'a Documentation,
    class: &ClassDoc,
    visited: &mut BTreeSet<String>,
    member: F,
) -> Option<(&'a ModuleDoc, &'a ClassDoc, &'a T)>
where
    F: Copy + Fn(&'a ClassDoc) -> Option<&'a T>,
{
    for (module, base_class) in documented_base_classes(documentation, class) {
        if !visited.insert(class_key(module, base_class)) {
            continue;
        }

        if let Some(member) = member(base_class) {
            return Some((module, base_class, member));
        }

        if let Some(member) = find_inherited_member(documentation, base_class, visited, member) {
            return Some(member);
        }
    }

    None
}

fn find_inherited_function<'a>(
    documentation: &'a Documentation,
    class: &ClassDoc,
    name: &str,
) -> Option<InheritedFunctionDoc<'a>> {
    let mut visited = BTreeSet::new();
    let (module, class, function) =
        find_inherited_member(documentation, class, &mut visited, |base_class| {
            base_class.methods.iter().find(|method| method.name == name)
        })?;
    Some(InheritedFunctionDoc {
        module,
        class,
        member: function,
        override_member: None,
    })
}

fn find_inherited_attribute<'a>(
    documentation: &'a Documentation,
    class: &ClassDoc,
    name: &str,
) -> Option<InheritedAttributeDoc<'a>> {
    let mut visited = BTreeSet::new();
    let (module, class, variable) =
        find_inherited_member(documentation, class, &mut visited, |base_class| {
            base_class
                .attributes
                .iter()
                .find(|attribute| attribute.name == name)
        })?;
    Some(InheritedAttributeDoc {
        module,
        class,
        member: variable,
        override_member: None,
    })
}

fn collect_inherited_members<'a, T>(
    documentation: &'a Documentation,
    class: &'a ClassDoc,
    base_members: impl Fn(&'a ClassDoc) -> &'a [T],
    member_name: impl Fn(&T) -> &str,
    override_member: impl Fn(&'a ClassDoc, &T) -> Option<&'a T>,
) -> Vec<InheritedMemberDoc<'a, T>> {
    let mut inherited = Vec::new();
    let mut seen_names = BTreeSet::new();
    for (module, base_class) in documented_base_class_chain(documentation, class) {
        for member in base_members(base_class) {
            if seen_names.insert(member_name(member).to_string()) {
                inherited.push(InheritedMemberDoc {
                    module,
                    class: base_class,
                    member,
                    override_member: override_member(class, member),
                });
            }
        }
    }
    inherited
}

fn collect_inherited_functions<'a>(
    documentation: &'a Documentation,
    class: &'a ClassDoc,
) -> Vec<InheritedFunctionDoc<'a>> {
    collect_inherited_members(
        documentation,
        class,
        |base_class| &base_class.methods,
        |function| &function.name,
        |class, function| {
            class
                .methods
                .iter()
                .find(|method| method.name == function.name)
        },
    )
}

fn collect_inherited_attributes<'a>(
    documentation: &'a Documentation,
    class: &'a ClassDoc,
) -> Vec<InheritedAttributeDoc<'a>> {
    collect_inherited_members(
        documentation,
        class,
        |base_class| &base_class.attributes,
        |variable| &variable.name,
        |class, variable| {
            class
                .attributes
                .iter()
                .find(|attribute| attribute.name == variable.name)
        },
    )
}

fn collect_inherited_groups<'a>(
    documentation: &'a Documentation,
    class: &'a ClassDoc,
) -> Vec<InheritedGroup<'a>> {
    let mut groups = Vec::new();
    let mut group_indexes = BTreeMap::new();

    for attribute in collect_inherited_attributes(documentation, class) {
        let key = class_key(attribute.module, attribute.class);
        let group_index = *group_indexes.entry(key).or_insert_with(|| {
            groups.push(InheritedGroup {
                module: attribute.module,
                class: attribute.class,
                attributes: Vec::new(),
                methods: Vec::new(),
            });
            groups.len() - 1
        });
        groups[group_index].attributes.push(attribute);
    }

    for method in collect_inherited_functions(documentation, class) {
        let key = class_key(method.module, method.class);
        let group_index = *group_indexes.entry(key).or_insert_with(|| {
            groups.push(InheritedGroup {
                module: method.module,
                class: method.class,
                attributes: Vec::new(),
                methods: Vec::new(),
            });
            groups.len() - 1
        });
        groups[group_index].methods.push(method);
    }

    groups
}

fn class_key(module: &ModuleDoc, class: &ClassDoc) -> String {
    format!("{}::{}", module.name, class.name)
}

fn search_items(documentation: &Documentation) -> Vec<SearchItem> {
    let mut items = Vec::new();

    for module in documentation.modules.values() {
        items.push(SearchItem(
            "module",
            module.name.clone(),
            module.name.clone(),
            module_href("", documentation, &module.name),
            module.summary().to_string(),
        ));

        for class in &module.classes {
            let class_href = class_page_href("", documentation, &module.name, &class.name);
            items.push(SearchItem(
                "class",
                class.name.clone(),
                format!("{}.{}", module.name, class.name),
                class_href.clone(),
                class.summary().to_string(),
            ));

            for (method, anchor) in class.methods.iter().zip(unique_item_anchors(
                "method",
                class.methods.iter().map(|method| method.name.as_str()),
            )) {
                items.push(SearchItem(
                    "method",
                    method.name.clone(),
                    format!("{}.{}.{}", module.name, class.name, method.name),
                    format!("{class_href}#{anchor}"),
                    method.summary().to_string(),
                ));
            }

            let attributes = class_attributes(class);
            for (attribute, anchor) in attributes.iter().zip(unique_item_anchors(
                "attr",
                attributes.iter().map(|attribute| attribute.name.as_str()),
            )) {
                items.push(SearchItem(
                    "attribute",
                    attribute.name.clone(),
                    format!("{}.{}.{}", module.name, class.name, attribute.name),
                    format!("{class_href}#{anchor}"),
                    attribute.summary().to_string(),
                ));
            }

            let enum_members = class_enum_members(class);
            for (member, anchor) in enum_members.iter().zip(unique_item_anchors(
                "member",
                enum_members.iter().map(|member| member.name.as_str()),
            )) {
                items.push(SearchItem(
                    "enum member",
                    member.name.clone(),
                    format!("{}.{}.{}", module.name, class.name, member.name),
                    format!("{class_href}#{anchor}"),
                    member.summary().to_string(),
                ));
            }
        }

        for function in &module.functions {
            items.push(SearchItem(
                "function",
                function.name.clone(),
                format!("{}.{}", module.name, function.name),
                anchored_href(
                    &module_href("", documentation, &module.name),
                    "fn",
                    &function.name,
                ),
                function.summary().to_string(),
            ));
        }

        for variable in &module.variables {
            items.push(SearchItem(
                variable.kind.search_kind(),
                variable.name.clone(),
                format!("{}.{}", module.name, variable.name),
                anchored_href(
                    &module_href("", documentation, &module.name),
                    variable.kind.anchor_prefix(),
                    &variable.name,
                ),
                variable.summary().to_string(),
            ));
        }

        if let Some(source) = &module.source {
            items.push(SearchItem(
                "source",
                source.path.clone(),
                module.name.clone(),
                source_doc_path(documentation, &source.path),
                format!("Source for {}", module.name),
            ));
        }
    }

    items
}

trait Render {
    fn render_to(&self, output: &mut String);

    fn render(&self) -> String {
        let mut output = String::new();
        self.render_to(&mut output);
        output
    }
}

impl Render for str {
    fn render_to(&self, output: &mut String) {
        encode_double_quoted_attribute_to_string(self, output);
    }
}

impl Render for String {
    fn render_to(&self, output: &mut String) {
        self.as_str().render_to(output);
    }
}

impl Render for char {
    fn render_to(&self, output: &mut String) {
        match self {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            character => output.push(*character),
        }
    }
}

fn render_attr(output: &mut String, value: &str) {
    encode_double_quoted_attribute_to_string(value, output);
}

fn render_link(output: &mut String, class: Option<&str>, href: &str, label: &str) {
    output.push_str("<a");
    if let Some(class) = class {
        output.push_str(" class=\"");
        render_attr(output, class);
        output.push('"');
    }
    output.push_str(" href=\"");
    render_attr(output, href);
    output.push_str("\">");
    label.render_to(output);
    output.push_str("</a>");
}

fn render_link_list_item(output: &mut String, href: &str, label: &str) {
    output.push_str("<li>");
    render_link(output, None, href, label);
    output.push_str("</li>");
}

const SEARCH_SCRIPT: &str = include_str!("../assets/tydoc.js");
const STYLESHEET: &str = include_str!("../assets/tydoc.css");
