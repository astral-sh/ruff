use glob::{glob, Pattern};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::env;
use std::path::{Component, PathBuf};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_macro_input, parse_quote, Error, FnArg, ItemFn, LitStr, Pat, Token};

#[derive(Debug)]
struct FixtureConfiguration {
    pattern: String,
    pattern_span: Span,
}

struct Arg {
    name: syn::Ident,
    _equal_token: Token![=],
    value: LitStr,
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            name: input.parse()?,
            _equal_token: input.parse()?,
            value: input.parse()?,
        })
    }
}

impl Parse for FixtureConfiguration {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let args: Punctuated<_, Token![,]> = input.parse_terminated(Arg::parse)?;

        let mut pattern = None;

        for arg in args {
            if arg.name == "pattern" {
                let raw_pattern = arg.value.value();
                // Validate that it is a valid pattern
                match Pattern::new(&raw_pattern) {
                    Ok(_) => pattern = Some((raw_pattern, arg.value.span())),
                    Err(err) => {
                        return Err(Error::new(
                            arg.value.span(),
                            format!("'{raw_pattern}' is not a valid glob pattern: '{}'", err.msg),
                        ));
                    }
                }
            } else {
                return Err(Error::new(
                    arg.name.span(),
                    format!("Unknown argument {}.", arg.name),
                ));
            }
        }

        match pattern {
            None => Err(Error::new(
                input.span(),
                "'fixture' macro must have a pattern attribute",
            )),
            Some((pattern, pattern_span)) => Ok(Self {
                pattern,
                pattern_span,
            }),
        }
    }
}

#[proc_macro_attribute]
pub fn fixture(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let test_fn = parse_macro_input!(item as ItemFn);
    let configuration = parse_macro_input!(attribute as FixtureConfiguration);

    let result = generate_fixtures(test_fn, &configuration);

    let stream = match result {
        Ok(output) => output,
        Err(err) => err.to_compile_error(),
    };

    TokenStream::from(stream)
}

fn generate_fixtures(
    mut test_fn: ItemFn,
    configuration: &FixtureConfiguration,
) -> syn::Result<proc_macro2::TokenStream> {
    // Remove the fixtures attribute
    test_fn.attrs.retain(|attr| !attr.path.is_ident("fixtures"));

    // Extract the name of the only argument of the test function.
    let last_arg = test_fn.sig.inputs.last();
    let path_ident = match (test_fn.sig.inputs.len(), last_arg) {
        (1, Some(last_arg)) => match last_arg {
            FnArg::Typed(typed) => match typed.pat.as_ref() {
                Pat::Ident(ident) => ident.ident.clone(),
                pat => {
                    return Err(Error::new(
                        pat.span(),
                        "#[fixture] function argument name must be an identifier",
                    ));
                }
            },
            FnArg::Receiver(receiver) => {
                return Err(Error::new(
                    receiver.span(),
                    "#[fixture] function argument name must be an identifier",
                ));
            }
        },
        _ => {
            return Err(Error::new(
                test_fn.sig.inputs.span(),
                "#[fixture] function must have exactly one argument with the type '&Path'",
            ));
        }
    };

    // Remove all arguments
    test_fn.sig.inputs.clear();

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect(
        "#[fixture] requires CARGO_MANIFEST_DIR to be set during the build to resolve the relative paths to the test files.",
    ));

    let pattern = if configuration.pattern.starts_with('/') {
        Cow::from(&configuration.pattern)
    } else {
        Cow::from(format!(
            "{}/{}",
            crate_dir
                .to_str()
                .expect("CARGO_MANIFEST_DIR must point to a directory with a UTF8 path"),
            configuration.pattern
        ))
    };

    let files = glob(&pattern).expect("Pattern to be valid").flatten();
    let mut modules = Modules::default();

    for file in files {
        let mut test_fn = test_fn.clone();

        let test_name = file
            .file_name()
            // SAFETY: Glob only matches on file names.
            .unwrap()
            .to_str()
            .expect("Expected path to be valid UTF8")
            .replace('.', "_");

        test_fn.sig.ident = format_ident!("{test_name}");

        let path = file.as_os_str().to_str().unwrap();

        test_fn.block.stmts.insert(
            0,
            parse_quote!(let #path_ident = std::path::Path::new(#path);),
        );

        modules.push_test(Test {
            path: file,
            test_fn,
        });
    }

    if modules.is_empty() {
        return Err(Error::new(
            configuration.pattern_span,
            "No file matches the specified glob pattern",
        ));
    }

    let root = find_highest_common_ancestor_module(&modules.root);

    root.generate(&test_fn.sig.ident.to_string())
}

fn find_highest_common_ancestor_module(module: &Module) -> &Module {
    let children = &module.children;

    if children.len() == 1 {
        let (_, child) = children.iter().next().unwrap();

        match child {
            Child::Module(common_child) => find_highest_common_ancestor_module(common_child),
            Child::Test(_) => module,
        }
    } else {
        module
    }
}

#[derive(Debug)]
struct Test {
    path: PathBuf,
    test_fn: ItemFn,
}

impl Test {
    fn generate(&self, _: &str) -> proc_macro2::TokenStream {
        let test_fn = &self.test_fn;
        quote!(#test_fn)
    }
}

#[derive(Debug, Default)]
struct Module {
    children: BTreeMap<String, Child>,
}

impl Module {
    fn generate(&self, name: &str) -> syn::Result<proc_macro2::TokenStream> {
        let mut inner = Vec::with_capacity(self.children.len());

        for (name, child) in &self.children {
            inner.push(child.generate(name)?);
        }

        let module_ident = format_ident!("{name}");

        Ok(quote!(
            mod #module_ident {
                use super::*;

                #(#inner)*
            }
        ))
    }
}

#[derive(Debug)]
enum Child {
    Module(Module),
    Test(Test),
}

impl Child {
    fn generate(&self, name: &str) -> syn::Result<proc_macro2::TokenStream> {
        match self {
            Child::Module(module) => module.generate(name),
            Child::Test(test) => Ok(test.generate(name)),
        }
    }
}

#[derive(Debug, Default)]
struct Modules {
    root: Module,
}

impl Modules {
    fn push_test(&mut self, test: Test) {
        let mut components = test
            .path
            .as_path()
            .components()
            .skip_while(|c| matches!(c, Component::RootDir))
            .peekable();

        let mut current = &mut self.root;
        while let Some(component) = components.next() {
            let name = component.as_os_str().to_str().unwrap();
            // A directory
            if components.peek().is_some() {
                let name = component.as_os_str().to_str().unwrap();
                let entry = current.children.entry(name.to_owned());

                match entry.or_insert_with(|| Child::Module(Module::default())) {
                    Child::Module(module) => {
                        current = module;
                    }
                    Child::Test(_) => {
                        unreachable!()
                    }
                }
            } else {
                // We reached the final component, insert the test
                drop(components);
                current.children.insert(name.to_owned(), Child::Test(test));
                break;
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.root.children.is_empty()
    }
}
