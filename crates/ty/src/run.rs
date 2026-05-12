use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use anyhow::{Context, bail};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::system::OsSystem;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_python_ast as ast;
use ruff_python_ast::{BoolOp, CmpOp, Expr, Number, Operator, Stmt, UnaryOp};
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_python_semantic::types::{KnownClass, Type};
use ty_python_semantic::{HasType, SemanticModel};
use wasm_encoder::{
    BlockType, CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection,
    ImportSection, Instruction, Module, TypeSection, ValType,
};
use wasmtime::{Caller, Engine, Linker, Store};

use crate::args::RunCommand;

pub(crate) fn run_file(args: RunCommand) -> anyhow::Result<()> {
    let cwd = std::env::current_dir().context("Failed to get the current working directory")?;
    let cwd = SystemPathBuf::from_path_buf(cwd).map_err(|path| {
        anyhow::anyhow!(
            "The current working directory `{}` contains non-Unicode characters. ty only supports Unicode paths.",
            path.display()
        )
    })?;
    let display_path = args.path.clone();
    let path = SystemPath::absolute(&args.path, &cwd);
    let system = OsSystem::new(&cwd);
    let mut project_metadata = ProjectMetadata::discover(&cwd, &system)?;
    project_metadata.apply_configuration_files(&system)?;
    let db = ProjectDatabase::fallible(project_metadata, system)?;
    let file = system_path_to_file(&db, &path)
        .with_context(|| format!("Failed to load `{display_path}` into ty"))?;

    let diagnostics = db.check_file(file);
    if !diagnostics.is_empty() {
        bail!(
            "Failed to type-check `{display_path}` before compilation: found {} diagnostic{}",
            diagnostics.len(),
            if diagnostics.len() == 1 { "" } else { "s" }
        );
    }

    let parsed = parsed_module(&db, file);
    let parsed = parsed.load(&db);
    let model = SemanticModel::new(&db, file);
    let (imported_modules, import_specs) =
        load_imported_function_modules(&db, parsed.suite(), &model)?;
    let imported_items = imported_definitions(&imported_modules, &import_specs)?;
    let program = Compiler::compile(parsed.suite(), &model, imported_items)
        .with_context(|| format!("Failed to compile `{display_path}`"))?;

    if args.print_wasm {
        let wat = wasmprinter::print_bytes(&program.wasm)
            .context("Failed to pretty-print generated WebAssembly")?;
        print!("{wat}");
        if !wat.ends_with('\n') {
            println!();
        }
    }

    if let Some(output) = args.emit_wasm.as_ref() {
        let output = SystemPath::absolute(output, &cwd);
        write_wasm(&output, &program.wasm)?;
    }

    if let Some(output) = args.emit_web.as_ref() {
        if program.uses_filesystem {
            bail!(
                "Browser artifacts are not available for programs that call local filesystem intrinsics yet"
            );
        }
        let output = SystemPath::absolute(output, &cwd);
        write_web_artifacts(&output, &program)?;
    }

    if args.no_execute {
        Ok(())
    } else {
        execute(program)
    }
}

struct LoadedModule<'db> {
    parsed: ParsedModuleRef,
    model: SemanticModel<'db>,
}

struct ImportedFunctionSpec {
    module_index: usize,
    source_name: String,
    binding_name: String,
}

enum ImportedDefinition<'a, 'db> {
    Function(FunctionDefinition<'a, 'db>),
    Constant(ConstantDefinition<'a, 'db>),
    Class(ClassDefinition<'a, 'db>),
}

struct ConstantDefinition<'a, 'db> {
    binding_name: String,
    value: &'a Expr,
    model: &'a SemanticModel<'db>,
}

struct ClassDefinition<'a, 'db> {
    binding_name: String,
    class: &'a ast::StmtClassDef,
    model: &'a SemanticModel<'db>,
}

fn load_imported_function_modules<'db>(
    db: &'db ProjectDatabase,
    body: &[Stmt],
    model: &SemanticModel<'db>,
) -> anyhow::Result<(Vec<LoadedModule<'db>>, Vec<ImportedFunctionSpec>)> {
    let mut modules = Vec::new();
    let mut import_specs = Vec::new();
    let mut modules_by_file = HashMap::new();
    let mut queued_imports = VecDeque::new();

    enqueue_imports(body, model.file(), &mut queued_imports)?;
    while let Some(queued_import) = queued_imports.pop_front() {
        let importing_model = SemanticModel::new(db, queued_import.importing_file);
        let module = importing_model
            .resolve_module(Some(&queued_import.module_name), queued_import.level)
            .with_context(|| {
                format!(
                    "`ty run` could not resolve imported module `{}`",
                    queued_import.module_name
                )
            })?;
        let module_file = module.file(db).with_context(|| {
            format!(
                "`ty run` cannot compile namespace package `{}` as an imported function module",
                queued_import.module_name
            )
        })?;
        let module_index = if let Some(module_index) = modules_by_file.get(&module_file) {
            *module_index
        } else {
            let diagnostics = db.check_file(module_file);
            if !diagnostics.is_empty() {
                bail!(
                    "Failed to type-check imported module `{}` before compilation: found {} diagnostic{}",
                    queued_import.module_name,
                    diagnostics.len(),
                    if diagnostics.len() == 1 { "" } else { "s" }
                );
            }

            let parsed = parsed_module(db, module_file).load(db);
            let loaded_module = LoadedModule {
                parsed,
                model: SemanticModel::new(db, module_file),
            };
            enqueue_imports(
                loaded_module.parsed.suite(),
                loaded_module.model.file(),
                &mut queued_imports,
            )?;
            let module_index = modules.len();
            modules.push(loaded_module);
            modules_by_file.insert(module_file, module_index);
            module_index
        };
        import_specs.extend(
            queued_import
                .names
                .into_iter()
                .map(|name| ImportedFunctionSpec {
                    module_index,
                    source_name: name.source_name,
                    binding_name: name.binding_name,
                }),
        );
    }

    Ok((modules, import_specs))
}

struct QueuedImportedFunctions {
    importing_file: File,
    module_name: String,
    level: u32,
    names: Vec<QueuedImportedFunctionName>,
}

struct QueuedImportedFunctionName {
    source_name: String,
    binding_name: String,
}

fn enqueue_imports(
    body: &[Stmt],
    importing_file: File,
    queued_imports: &mut VecDeque<QueuedImportedFunctions>,
) -> anyhow::Result<()> {
    for statement in body {
        let Stmt::ImportFrom(import_from) = statement else {
            continue;
        };
        let Some(module_name) = import_from.module.as_ref() else {
            continue;
        };
        if import_from.level == 0 && module_name.id.as_str() == "ty_extensions" {
            continue;
        }
        if import_from
            .names
            .iter()
            .any(|alias| alias.name.id.as_str() == "*")
        {
            bail!(
                "`ty run` project-module imports currently support only named functions, not `*` imports"
            );
        }
        queued_imports.push_back(QueuedImportedFunctions {
            importing_file,
            module_name: module_name.id.as_str().to_string(),
            level: import_from.level,
            names: import_from
                .names
                .iter()
                .map(|alias| QueuedImportedFunctionName {
                    source_name: alias.name.id.as_str().to_string(),
                    binding_name: alias
                        .asname
                        .as_ref()
                        .map_or_else(|| alias.name.id.as_str(), |name| name.id.as_str())
                        .to_string(),
                })
                .collect(),
        });
    }

    Ok(())
}

fn imported_definitions<'a, 'db>(
    modules: &'a [LoadedModule<'db>],
    import_specs: &[ImportedFunctionSpec],
) -> anyhow::Result<Vec<ImportedDefinition<'a, 'db>>> {
    let mut definitions = Vec::new();
    let mut seen_functions = HashSet::new();

    for import_spec in import_specs {
        if !seen_functions.insert((import_spec.module_index, import_spec.binding_name.clone())) {
            continue;
        }
        let module = modules
            .get(import_spec.module_index)
            .context("Imported module index escaped its loader")?;
        let definition = module
            .parsed
            .suite()
            .iter()
            .find_map(|statement| match statement {
                Stmt::FunctionDef(function)
                    if function.name.as_str() == import_spec.source_name.as_str() =>
                {
                    Some(ImportedDefinition::Function(FunctionDefinition {
                        key: import_spec.binding_name.clone(),
                        function,
                        model: &module.model,
                    }))
                }
                Stmt::Assign(assign)
                    if assign.targets.len() == 1
                        && matches!(
                            assign.targets.first(),
                            Some(Expr::Name(name))
                                if name.id.as_str() == import_spec.source_name.as_str()
                        ) =>
                {
                    Some(ImportedDefinition::Constant(ConstantDefinition {
                        binding_name: import_spec.binding_name.clone(),
                        value: assign.value.as_ref(),
                        model: &module.model,
                    }))
                }
                Stmt::AnnAssign(assign)
                    if matches!(
                        assign.target.as_ref(),
                        Expr::Name(name)
                            if name.id.as_str() == import_spec.source_name.as_str()
                    ) =>
                {
                    assign.value.as_deref().map(|value| {
                        ImportedDefinition::Constant(ConstantDefinition {
                            binding_name: import_spec.binding_name.clone(),
                            value,
                            model: &module.model,
                        })
                    })
                }
                Stmt::ClassDef(class)
                    if class.name.as_str() == import_spec.source_name.as_str() =>
                {
                    Some(ImportedDefinition::Class(ClassDefinition {
                        binding_name: import_spec.binding_name.clone(),
                        class,
                        model: &module.model,
                    }))
                }
                _ => None,
            })
            .with_context(|| {
                format!(
                    "`ty run` currently imports only top-level functions, initialized scalar constants, and supported classes; `{}` was not found as one",
                    import_spec.source_name
                )
            })?;
        definitions.push(definition);
    }

    Ok(definitions)
}

fn execute(program: CompiledProgram) -> anyhow::Result<()> {
    let engine = Engine::default();
    let module = wasmtime::Module::new(&engine, &program.wasm)
        .map_err(|error| anyhow::anyhow!("Failed to load generated wasm: {error:?}"))?;
    let mut linker = Linker::<Runtime>::new(&engine);
    linker
        .func_wrap("ty", "print_i64", |value: i64| {
            println!("{value}");
        })
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap("ty", "print_f64", |value: f64| {
            println!("{value:?}");
        })
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "print_ref",
            |caller: Caller<'_, Runtime>, handle: i64| {
                println!("{}", caller.data().display(handle));
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap("ty", "list_new", |mut caller: Caller<'_, Runtime>| -> i64 {
            caller.data_mut().list_new()
        })
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_str_new",
            |mut caller: Caller<'_, Runtime>| -> i64 { caller.data_mut().list_str_new() },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_push_i64",
            |mut caller: Caller<'_, Runtime>, handle: i64, value: i64| {
                caller.data_mut().list_push_i64(handle, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_get_i64",
            |caller: Caller<'_, Runtime>, handle: i64, index: i64| -> i64 {
                caller.data().list_get_i64(handle, index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_set_i64",
            |mut caller: Caller<'_, Runtime>, handle: i64, index: i64, value: i64| {
                caller.data_mut().list_set_i64(handle, index, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_push_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, value: i64| {
                caller.data_mut().list_push_ref(handle, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_get_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, index: i64| -> i64 {
                caller.data_mut().list_get_ref(handle, index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_set_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, index: i64, value: i64| {
                caller.data_mut().list_set_ref(handle, index, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_obj_new",
            |mut caller: Caller<'_, Runtime>| -> i64 { caller.data_mut().list_obj_new() },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_push_obj",
            |mut caller: Caller<'_, Runtime>, handle: i64, value: i64| {
                caller.data_mut().list_push_obj(handle, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_get_obj",
            |caller: Caller<'_, Runtime>, handle: i64, index: i64| -> i64 {
                caller.data().list_get_obj(handle, index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "list_set_obj",
            |mut caller: Caller<'_, Runtime>, handle: i64, index: i64, value: i64| {
                caller.data_mut().list_set_obj(handle, index, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "tuple_new",
            |mut caller: Caller<'_, Runtime>| -> i64 { caller.data_mut().tuple_new() },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "tuple_push_i64",
            |mut caller: Caller<'_, Runtime>, handle: i64, value: i64| {
                caller.data_mut().tuple_push_i64(handle, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "tuple_get_i64",
            |caller: Caller<'_, Runtime>, handle: i64, index: i64| -> i64 {
                caller.data().tuple_get_i64(handle, index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "tuple_str_new",
            |mut caller: Caller<'_, Runtime>| -> i64 { caller.data_mut().tuple_str_new() },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "tuple_push_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, value: i64| {
                caller.data_mut().tuple_push_ref(handle, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "tuple_get_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, index: i64| -> i64 {
                caller.data_mut().tuple_get_ref(handle, index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap("ty", "dict_new", |mut caller: Caller<'_, Runtime>| -> i64 {
            caller.data_mut().dict_new()
        })
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "dict_str_new",
            |mut caller: Caller<'_, Runtime>| -> i64 { caller.data_mut().dict_str_new() },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "dict_set_str_i64",
            |mut caller: Caller<'_, Runtime>, handle: i64, key: i64, value: i64| {
                caller.data_mut().dict_set_str_i64(handle, key, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "dict_get_str_i64",
            |caller: Caller<'_, Runtime>, handle: i64, key: i64| -> i64 {
                caller.data().dict_get_str_i64(handle, key)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "dict_set_str_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, key: i64, value: i64| {
                caller.data_mut().dict_set_str_ref(handle, key, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "dict_get_str_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, key: i64| -> i64 {
                caller.data_mut().dict_get_str_ref(handle, key)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "dict_key_ref",
            |mut caller: Caller<'_, Runtime>, handle: i64, index: i64| -> i64 {
                caller.data_mut().dict_key_ref(handle, index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "str_const",
            |mut caller: Caller<'_, Runtime>, index: i32| -> i64 {
                caller.data_mut().string_constant(index)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "str_concat",
            |mut caller: Caller<'_, Runtime>, left: i64, right: i64| -> i64 {
                caller.data_mut().string_concat(left, right)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "str_compare",
            |caller: Caller<'_, Runtime>, left: i64, right: i64, operator: i64| -> i32 {
                caller.data().string_compare(left, right, operator)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "str_from_i64",
            |mut caller: Caller<'_, Runtime>, value: i64| -> i64 {
                caller.data_mut().string_from_i64(value)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "str_from_f64",
            |mut caller: Caller<'_, Runtime>, value: f64| -> i64 {
                caller.data_mut().string_from_f64(value)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "str_from_bool",
            |mut caller: Caller<'_, Runtime>, value: i64| -> i64 {
                caller.data_mut().string_from_bool(value)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "object_new",
            |mut caller: Caller<'_, Runtime>| -> i64 { caller.data_mut().object_new() },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "object_set_i64",
            |mut caller: Caller<'_, Runtime>, handle: i64, key: i64, value: i64| {
                caller.data_mut().object_set_i64(handle, key, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "object_get_i64",
            |caller: Caller<'_, Runtime>, handle: i64, key: i64| -> i64 {
                caller.data().object_get_i64(handle, key)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "object_set_f64",
            |mut caller: Caller<'_, Runtime>, handle: i64, key: i64, value: f64| {
                caller.data_mut().object_set_f64(handle, key, value);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "object_get_f64",
            |caller: Caller<'_, Runtime>, handle: i64, key: i64| -> f64 {
                caller.data().object_get_f64(handle, key)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "ref_len",
            |caller: Caller<'_, Runtime>, handle: i64| -> i64 { caller.data().ref_len(handle) },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "read_text",
            |mut caller: Caller<'_, Runtime>, handle: i64| -> i64 {
                caller.data_mut().read_text(handle)
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;
    linker
        .func_wrap(
            "ty",
            "write_text",
            |mut caller: Caller<'_, Runtime>, path: i64, contents: i64| {
                caller.data_mut().write_text(path, contents);
            },
        )
        .map_err(|error| {
            anyhow::anyhow!("Failed to install the ty runtime host functions: {error}")
        })?;

    let mut store = Store::new(&engine, Runtime::new(program.strings));
    let instance = linker
        .instantiate(&mut store, &module)
        .map_err(|error| anyhow::anyhow!("Failed to instantiate generated wasm: {error}"))?;
    let start = instance
        .get_typed_func::<(), ()>(&mut store, "_start")
        .map_err(|error| anyhow::anyhow!("Generated wasm did not export `_start`: {error}"))?;
    start
        .call(&mut store, ())
        .map_err(|error| anyhow::anyhow!("Generated wasm trapped during execution: {error}"))?;
    Ok(())
}

fn write_wasm(path: &SystemPath, wasm: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.as_std_path().parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create `{}`", parent.display()))?;
    }
    fs::write(path.as_std_path(), wasm).with_context(|| format!("Failed to write `{path}`"))
}

fn write_web_artifacts(directory: &SystemPath, program: &CompiledProgram) -> anyhow::Result<()> {
    fs::create_dir_all(directory.as_std_path())
        .with_context(|| format!("Failed to create `{directory}`"))?;
    fs::write(directory.join("program.wasm").as_std_path(), &program.wasm)
        .with_context(|| format!("Failed to write `{directory}/program.wasm`"))?;
    let runtime = WEB_RUNTIME.replace(
        "__TY_STRING_CONSTANTS__",
        &serde_json::to_string(&program.strings).context("Failed to encode web string table")?,
    );
    fs::write(
        directory.join("runtime.js").as_std_path(),
        runtime.as_bytes(),
    )
    .with_context(|| format!("Failed to write `{directory}/runtime.js`"))?;
    fs::write(
        directory.join("index.html").as_std_path(),
        WEB_INDEX.as_bytes(),
    )
    .with_context(|| format!("Failed to write `{directory}/index.html`"))
}

const WEB_RUNTIME: &str = r##"const output = document.querySelector("#output");
const stringConstants = __TY_STRING_CONSTANTS__;
const heap = [];

function writeLine(value) {
  output.textContent += `${value}\n`;
}

function formatFloat(value) {
  return Number.isInteger(value) ? `${value}.0` : `${value}`;
}

function store(value) {
  heap.push(value);
  return BigInt(heap.length - 1);
}

function load(handle) {
  return heap[Number(handle)];
}

const imports = {
  ty: {
    print_i64(value) {
      writeLine(value);
    },
    print_f64(value) {
      writeLine(formatFloat(value));
    },
    print_ref(handle) {
      writeLine(load(handle));
    },
    str_const(index) {
      return store(stringConstants[index]);
    },
    str_concat(left, right) {
      return store(`${load(left)}${load(right)}`);
    },
    str_compare(left, right, operator) {
      const lhs = load(left);
      const rhs = load(right);
      switch (Number(operator)) {
        case 0:
          return Number(lhs === rhs);
        case 1:
          return Number(lhs !== rhs);
        case 2:
          return Number(lhs < rhs);
        case 3:
          return Number(lhs <= rhs);
        case 4:
          return Number(lhs > rhs);
        case 5:
          return Number(lhs >= rhs);
        default:
          return 0;
      }
    },
    str_from_i64(value) {
      return store(`${value}`);
    },
    str_from_f64(value) {
      return store(formatFloat(value));
    },
    str_from_bool(value) {
      return store(value === 0n ? "False" : "True");
    },
    list_new() {
      return store([]);
    },
    list_str_new() {
      return store([]);
    },
    list_push_i64(handle, value) {
      load(handle).push(value);
    },
    list_get_i64(handle, index) {
      return load(handle)[Number(index)] ?? 0n;
    },
    list_set_i64(handle, index, value) {
      load(handle)[Number(index)] = value;
    },
    list_push_ref(handle, value) {
      load(handle).push(load(value));
    },
    list_get_ref(handle, index) {
      return store(load(handle)[Number(index)] ?? "");
    },
    list_set_ref(handle, index, value) {
      load(handle)[Number(index)] = load(value);
    },
    list_obj_new() {
      return store([]);
    },
    list_push_obj(handle, value) {
      load(handle).push(value);
    },
    list_get_obj(handle, index) {
      return load(handle)[Number(index)] ?? 0n;
    },
    list_set_obj(handle, index, value) {
      load(handle)[Number(index)] = value;
    },
    tuple_new() {
      return store([]);
    },
    tuple_push_i64(handle, value) {
      load(handle).push(value);
    },
    tuple_get_i64(handle, index) {
      return load(handle)[Number(index)] ?? 0n;
    },
    tuple_str_new() {
      return store([]);
    },
    tuple_push_ref(handle, value) {
      load(handle).push(load(value));
    },
    tuple_get_ref(handle, index) {
      return store(load(handle)[Number(index)] ?? "");
    },
    dict_new() {
      return store(new Map());
    },
    dict_str_new() {
      return store(new Map());
    },
    dict_set_str_i64(handle, key, value) {
      load(handle).set(load(key), value);
    },
    dict_get_str_i64(handle, key) {
      return load(handle).get(load(key)) ?? 0n;
    },
    dict_set_str_ref(handle, key, value) {
      load(handle).set(load(key), load(value));
    },
    dict_get_str_ref(handle, key) {
      return store(load(handle).get(load(key)) ?? "");
    },
    dict_key_ref(handle, index) {
      return store([...load(handle).keys()][Number(index)] ?? "");
    },
    object_new() {
      return store(new Map());
    },
    object_set_i64(handle, key, value) {
      load(handle).set(load(key), value);
    },
    object_get_i64(handle, key) {
      return load(handle).get(load(key)) ?? 0n;
    },
    object_set_f64(handle, key, value) {
      load(handle).set(load(key), value);
    },
    object_get_f64(handle, key) {
      return load(handle).get(load(key)) ?? 0;
    },
    ref_len(handle) {
      const value = load(handle);
      if (value instanceof Map) {
        return BigInt(value.size);
      }
      return BigInt(value.length ?? 0);
    },
    read_text(_handle) {
      throw new Error("read_text is unavailable in browser artifacts");
    },
    write_text(_path, _contents) {
      throw new Error("write_text is unavailable in browser artifacts");
    },
  },
};

const response = await fetch("./program.wasm");
const bytes = await response.arrayBuffer();
const { instance } = await WebAssembly.instantiate(bytes, imports);
instance.exports._start();
"##;

const WEB_INDEX: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>ty wasm program</title>
    <style>
      :root {
        color-scheme: light dark;
        font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
      }

      body {
        margin: 0;
        min-height: 100vh;
        display: grid;
        place-items: center;
        background: Canvas;
        color: CanvasText;
      }

      main {
        width: min(960px, calc(100vw - 48px));
      }

      pre {
        min-height: 320px;
        margin: 0;
        padding: 20px;
        border: 1px solid color-mix(in srgb, CanvasText 22%, transparent);
        border-radius: 8px;
        overflow: auto;
        background: color-mix(in srgb, Canvas 92%, CanvasText 8%);
      }
    </style>
  </head>
  <body>
    <main>
      <pre id="output"></pre>
    </main>
    <script type="module" src="./runtime.js"></script>
  </body>
</html>
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueType {
    Int,
    Bool,
    Float,
    String,
    ListInt,
    ListString,
    ListObject,
    TupleInt,
    TupleString,
    DictStrInt,
    DictStrString,
    Object,
}

impl ValueType {
    const fn name(self) -> &'static str {
        match self {
            ValueType::Int => "int",
            ValueType::Bool => "bool",
            ValueType::Float => "float",
            ValueType::String => "str",
            ValueType::ListInt => "list[int]",
            ValueType::ListString => "list[str]",
            ValueType::ListObject => "list[object]",
            ValueType::TupleInt => "tuple[int, ...]",
            ValueType::TupleString => "tuple[str, ...]",
            ValueType::DictStrInt => "dict[str, int]",
            ValueType::DictStrString => "dict[str, str]",
            ValueType::Object => "object",
        }
    }

    const fn wasm(self) -> ValType {
        match self {
            ValueType::Int
            | ValueType::Bool
            | ValueType::String
            | ValueType::ListInt
            | ValueType::ListString
            | ValueType::ListObject
            | ValueType::TupleInt
            | ValueType::TupleString
            | ValueType::DictStrInt
            | ValueType::DictStrString
            | ValueType::Object => ValType::I64,
            ValueType::Float => ValType::F64,
        }
    }
}

struct CompiledProgram {
    wasm: Vec<u8>,
    strings: Vec<String>,
    uses_filesystem: bool,
}

#[derive(Default)]
struct Runtime {
    string_constants: Vec<String>,
    heap: Vec<RuntimeObject>,
    cwd: PathBuf,
}

impl Runtime {
    fn new(string_constants: Vec<String>) -> Self {
        Self {
            string_constants,
            heap: Vec::new(),
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }

    fn string_constant(&mut self, index: i32) -> i64 {
        let value = usize::try_from(index)
            .ok()
            .and_then(|index| self.string_constants.get(index))
            .cloned()
            .unwrap_or_default();
        self.store(RuntimeObject::String(value))
    }

    fn string_concat(&mut self, left: i64, right: i64) -> i64 {
        let left = self.string(left).unwrap_or_default();
        let right = self.string(right).unwrap_or_default();
        self.store(RuntimeObject::String(format!("{left}{right}")))
    }

    fn string_compare(&self, left: i64, right: i64, operator: i64) -> i32 {
        let left = self.string(left).unwrap_or_default();
        let right = self.string(right).unwrap_or_default();
        i32::from(match operator {
            0 => left == right,
            1 => left != right,
            2 => left < right,
            3 => left <= right,
            4 => left > right,
            5 => left >= right,
            _ => false,
        })
    }

    fn string_from_i64(&mut self, value: i64) -> i64 {
        self.store(RuntimeObject::String(value.to_string()))
    }

    fn string_from_f64(&mut self, value: f64) -> i64 {
        self.store(RuntimeObject::String(format!("{value:?}")))
    }

    fn string_from_bool(&mut self, value: i64) -> i64 {
        self.store(RuntimeObject::String(
            if value == 0 { "False" } else { "True" }.to_string(),
        ))
    }

    fn display(&self, handle: i64) -> String {
        match self
            .heap
            .get(usize::try_from(handle).ok().unwrap_or(usize::MAX))
        {
            Some(RuntimeObject::String(value)) => value.clone(),
            Some(RuntimeObject::ListInt(values)) => format!(
                "[{}]",
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::ListString(values)) => format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| format!("'{value}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::ListObject(values)) => format!(
                "[{}]",
                values
                    .iter()
                    .map(|value| self.display(*value))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::TupleInt(values)) => format!(
                "({})",
                values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::TupleString(values)) => format!(
                "({})",
                values
                    .iter()
                    .map(|value| format!("'{value}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::DictStrInt(values)) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| format!("'{key}': {value}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::DictStrString(values)) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| format!("'{key}': '{value}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some(RuntimeObject::Object(values)) => format!(
                "object({})",
                values
                    .iter()
                    .map(|(key, value)| format!("{key}={}", value.display()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => "<invalid-ref>".to_string(),
        }
    }

    fn string(&self, handle: i64) -> Option<&str> {
        match self.heap.get(usize::try_from(handle).ok()?)? {
            RuntimeObject::String(value) => Some(value),
            RuntimeObject::ListInt(_)
            | RuntimeObject::ListString(_)
            | RuntimeObject::ListObject(_)
            | RuntimeObject::TupleInt(_)
            | RuntimeObject::TupleString(_)
            | RuntimeObject::DictStrInt(_)
            | RuntimeObject::DictStrString(_)
            | RuntimeObject::Object(_) => None,
        }
    }

    fn list_new(&mut self) -> i64 {
        self.store(RuntimeObject::ListInt(Vec::new()))
    }

    fn list_str_new(&mut self) -> i64 {
        self.store(RuntimeObject::ListString(Vec::new()))
    }

    fn list_obj_new(&mut self) -> i64 {
        self.store(RuntimeObject::ListObject(Vec::new()))
    }

    fn list_push_i64(&mut self, handle: i64, value: i64) {
        if let Some(RuntimeObject::ListInt(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.push(value);
        }
    }

    fn list_get_i64(&self, handle: i64, index: i64) -> i64 {
        match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::ListInt(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index))
                .copied()
                .unwrap_or_default(),
            _ => 0,
        }
    }

    fn list_set_i64(&mut self, handle: i64, index: i64, value: i64) {
        if let Some(existing) = usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get_mut(handle))
            .and_then(|object| match object {
                RuntimeObject::ListInt(values) => usize::try_from(index)
                    .ok()
                    .and_then(|index| values.get_mut(index)),
                _ => None,
            })
        {
            *existing = value;
        }
    }

    fn list_push_ref(&mut self, handle: i64, value: i64) {
        let value = self.string(value).unwrap_or_default().to_string();
        if let Some(RuntimeObject::ListString(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.push(value);
        }
    }

    fn list_get_ref(&mut self, handle: i64, index: i64) -> i64 {
        let value = match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::ListString(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index))
                .cloned()
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.store(RuntimeObject::String(value))
    }

    fn list_set_ref(&mut self, handle: i64, index: i64, value: i64) {
        let value = self.string(value).unwrap_or_default().to_string();
        if let Some(existing) = usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get_mut(handle))
            .and_then(|object| match object {
                RuntimeObject::ListString(values) => usize::try_from(index)
                    .ok()
                    .and_then(|index| values.get_mut(index)),
                _ => None,
            })
        {
            *existing = value;
        }
    }

    fn list_push_obj(&mut self, handle: i64, value: i64) {
        if let Some(RuntimeObject::ListObject(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.push(value);
        }
    }

    fn list_get_obj(&self, handle: i64, index: i64) -> i64 {
        match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::ListObject(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index))
                .copied()
                .unwrap_or_default(),
            _ => 0,
        }
    }

    fn list_set_obj(&mut self, handle: i64, index: i64, value: i64) {
        if let Some(existing) = usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get_mut(handle))
            .and_then(|object| match object {
                RuntimeObject::ListObject(values) => usize::try_from(index)
                    .ok()
                    .and_then(|index| values.get_mut(index)),
                _ => None,
            })
        {
            *existing = value;
        }
    }

    fn tuple_new(&mut self) -> i64 {
        self.store(RuntimeObject::TupleInt(Vec::new()))
    }

    fn tuple_str_new(&mut self) -> i64 {
        self.store(RuntimeObject::TupleString(Vec::new()))
    }

    fn tuple_push_i64(&mut self, handle: i64, value: i64) {
        if let Some(RuntimeObject::TupleInt(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.push(value);
        }
    }

    fn tuple_get_i64(&self, handle: i64, index: i64) -> i64 {
        match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::TupleInt(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index))
                .copied()
                .unwrap_or_default(),
            _ => 0,
        }
    }

    fn tuple_push_ref(&mut self, handle: i64, value: i64) {
        let value = self.string(value).unwrap_or_default().to_string();
        if let Some(RuntimeObject::TupleString(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.push(value);
        }
    }

    fn tuple_get_ref(&mut self, handle: i64, index: i64) -> i64 {
        let value = match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::TupleString(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.get(index))
                .cloned()
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.store(RuntimeObject::String(value))
    }

    fn dict_new(&mut self) -> i64 {
        self.store(RuntimeObject::DictStrInt(HashMap::new()))
    }

    fn dict_str_new(&mut self) -> i64 {
        self.store(RuntimeObject::DictStrString(HashMap::new()))
    }

    fn dict_set_str_i64(&mut self, handle: i64, key: i64, value: i64) {
        let key = self.string(key).unwrap_or_default().to_string();
        if let Some(RuntimeObject::DictStrInt(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.insert(key, value);
        }
    }

    fn dict_get_str_i64(&self, handle: i64, key: i64) -> i64 {
        let key = self.string(key).unwrap_or_default();
        match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::DictStrInt(values)) => values.get(key).copied().unwrap_or_default(),
            _ => 0,
        }
    }

    fn dict_set_str_ref(&mut self, handle: i64, key: i64, value: i64) {
        let key = self.string(key).unwrap_or_default().to_string();
        let value = self.string(value).unwrap_or_default().to_string();
        if let Some(RuntimeObject::DictStrString(values)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            values.insert(key, value);
        }
    }

    fn dict_get_str_ref(&mut self, handle: i64, key: i64) -> i64 {
        let key = self.string(key).unwrap_or_default();
        let value = match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::DictStrString(values)) => {
                values.get(key).cloned().unwrap_or_default()
            }
            _ => String::new(),
        };
        self.store(RuntimeObject::String(value))
    }

    fn dict_key_ref(&mut self, handle: i64, index: i64) -> i64 {
        let key = match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::DictStrInt(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.keys().nth(index))
                .cloned()
                .unwrap_or_default(),
            Some(RuntimeObject::DictStrString(values)) => usize::try_from(index)
                .ok()
                .and_then(|index| values.keys().nth(index))
                .cloned()
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.store(RuntimeObject::String(key))
    }

    fn object_new(&mut self) -> i64 {
        self.store(RuntimeObject::Object(HashMap::new()))
    }

    fn object_set_i64(&mut self, handle: i64, key: i64, value: i64) {
        let key = self.string(key).unwrap_or_default().to_string();
        if let Some(RuntimeObject::Object(fields)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            fields.insert(key, RuntimeFieldValue::I64(value));
        }
    }

    fn object_get_i64(&self, handle: i64, key: i64) -> i64 {
        let key = self.string(key).unwrap_or_default();
        match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::Object(fields)) => fields
                .get(key)
                .and_then(RuntimeFieldValue::as_i64)
                .unwrap_or_default(),
            _ => 0,
        }
    }

    fn object_set_f64(&mut self, handle: i64, key: i64, value: f64) {
        let key = self.string(key).unwrap_or_default().to_string();
        if let Some(RuntimeObject::Object(fields)) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get_mut(index))
        {
            fields.insert(key, RuntimeFieldValue::F64(value));
        }
    }

    fn object_get_f64(&self, handle: i64, key: i64) -> f64 {
        let key = self.string(key).unwrap_or_default();
        match usize::try_from(handle)
            .ok()
            .and_then(|handle| self.heap.get(handle))
        {
            Some(RuntimeObject::Object(fields)) => fields
                .get(key)
                .and_then(RuntimeFieldValue::as_f64)
                .unwrap_or_default(),
            _ => 0.0,
        }
    }

    fn ref_len(&self, handle: i64) -> i64 {
        let Some(object) = usize::try_from(handle)
            .ok()
            .and_then(|index| self.heap.get(index))
        else {
            return 0;
        };
        let len = match object {
            RuntimeObject::String(value) => value.chars().count(),
            RuntimeObject::ListInt(values) | RuntimeObject::TupleInt(values) => values.len(),
            RuntimeObject::ListString(values) | RuntimeObject::TupleString(values) => values.len(),
            RuntimeObject::ListObject(values) => values.len(),
            RuntimeObject::DictStrInt(values) => values.len(),
            RuntimeObject::DictStrString(values) => values.len(),
            RuntimeObject::Object(_) => 0,
        };
        i64::try_from(len).unwrap_or(i64::MAX)
    }

    fn read_text(&mut self, handle: i64) -> i64 {
        let path = self.string(handle).unwrap_or_default();
        let path = self.cwd.join(path);
        let value = fs::read_to_string(path).unwrap_or_default();
        self.store(RuntimeObject::String(value))
    }

    fn write_text(&mut self, path: i64, contents: i64) {
        let path = self.string(path).unwrap_or_default();
        let path = self.cwd.join(path);
        let contents = self.string(contents).unwrap_or_default();
        let _ = fs::write(path, contents);
    }

    fn store(&mut self, value: RuntimeObject) -> i64 {
        let handle = i64::try_from(self.heap.len()).expect("runtime heap handle fits in i64");
        self.heap.push(value);
        handle
    }
}

enum RuntimeObject {
    String(String),
    ListInt(Vec<i64>),
    ListString(Vec<String>),
    ListObject(Vec<i64>),
    TupleInt(Vec<i64>),
    TupleString(Vec<String>),
    DictStrInt(HashMap<String, i64>),
    DictStrString(HashMap<String, String>),
    Object(HashMap<String, RuntimeFieldValue>),
}

enum RuntimeFieldValue {
    I64(i64),
    F64(f64),
}

impl RuntimeFieldValue {
    fn as_i64(&self) -> Option<i64> {
        match self {
            RuntimeFieldValue::I64(value) => Some(*value),
            RuntimeFieldValue::F64(_) => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            RuntimeFieldValue::I64(_) => None,
            RuntimeFieldValue::F64(value) => Some(*value),
        }
    }

    fn display(&self) -> String {
        match self {
            RuntimeFieldValue::I64(value) => value.to_string(),
            RuntimeFieldValue::F64(value) => value.to_string(),
        }
    }
}

#[derive(Clone, Copy)]
struct Local {
    index: u32,
    ty: ValueType,
}

#[derive(Clone)]
struct FunctionParameter {
    name: String,
    ty: ValueType,
    keyword_allowed: bool,
    default: Option<DefaultValue>,
}

#[derive(Clone)]
enum DefaultValue {
    Int(i64),
    Bool(bool),
    Float(f64),
    String(String),
}

#[derive(Clone, Copy)]
enum CallArgument<'a> {
    Expr(&'a Expr),
    Default(&'a DefaultValue),
}

#[derive(Clone)]
struct FunctionSignature {
    index: u32,
    type_index: u32,
    parameters: Vec<FunctionParameter>,
    result: ValueType,
}

#[derive(Clone)]
struct ClassField {
    name: String,
    ty: ValueType,
    parameter_index: usize,
}

#[derive(Clone)]
struct ClassSignature {
    parameters: Vec<FunctionParameter>,
    fields: Vec<ClassField>,
}

struct MethodDefinition<'a, 'db> {
    key: String,
    function: &'a ast::StmtFunctionDef,
    model: &'a SemanticModel<'db>,
}

struct FunctionDefinition<'a, 'db> {
    key: String,
    function: &'a ast::StmtFunctionDef,
    model: &'a SemanticModel<'db>,
}

#[derive(Clone, Copy)]
enum ForLoopKind {
    Range,
    ListInt,
    ListString,
    ListObject,
    TupleInt,
    TupleString,
    DictStrInt,
    DictStrString,
}

impl ForLoopKind {
    const fn target_type(self) -> ValueType {
        match self {
            ForLoopKind::Range | ForLoopKind::ListInt | ForLoopKind::TupleInt => ValueType::Int,
            ForLoopKind::ListString
            | ForLoopKind::TupleString
            | ForLoopKind::DictStrInt
            | ForLoopKind::DictStrString => ValueType::String,
            ForLoopKind::ListObject => ValueType::Object,
        }
    }
}

struct Compiler;

impl Compiler {
    fn compile<'a, 'db>(
        body: &'a [Stmt],
        model: &'a SemanticModel<'db>,
        imported_definitions: Vec<ImportedDefinition<'a, 'db>>,
    ) -> anyhow::Result<CompiledProgram> {
        let (module_functions, classes, start_body) = split_module_body(body);
        let mut functions = module_functions
            .into_iter()
            .map(|function| FunctionDefinition {
                key: function.name.as_str().to_string(),
                function,
                model,
            })
            .collect::<Vec<_>>();
        let mut imported_constants = Vec::new();
        let mut imported_classes = Vec::new();
        for definition in imported_definitions {
            match definition {
                ImportedDefinition::Function(function) => functions.push(function),
                ImportedDefinition::Constant(constant) => imported_constants.push(constant),
                ImportedDefinition::Class(class) => imported_classes.push(class),
            }
        }
        let methods = collect_method_definitions(&classes, &imported_classes, model)?;
        let signatures = collect_function_signatures(&functions, &methods, model)?;
        let class_signatures = collect_class_signatures(&classes, &imported_classes, model)?;
        let strings = Rc::new(RefCell::new(Vec::new()));
        let uses_filesystem = Rc::new(Cell::new(false));

        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([ValType::I64], []);
        types.ty().function([ValType::F64], []);
        types.ty().function([ValType::I64], []);
        types.ty().function([ValType::I32], [ValType::I64]);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types.ty().function([ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types.ty().function([ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::F64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::F64]);
        types.ty().function([ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types.ty().function([ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([ValType::I64], [ValType::I64]);
        types.ty().function([], [ValType::I64]);
        types.ty().function([ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64], [ValType::I64]);
        types.ty().function([ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::I64], []);
        types
            .ty()
            .function([ValType::I64, ValType::I64, ValType::I64], [ValType::I32]);
        types.ty().function([ValType::F64], [ValType::I64]);
        for definition in &functions {
            let signature = function_signature(&signatures, &definition.key)?;
            types.ty().function(
                signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.wasm()),
                [signature.result.wasm()],
            );
        }
        for method in &methods {
            let signature = function_signature(&signatures, &method.key)?;
            types.ty().function(
                signature
                    .parameters
                    .iter()
                    .map(|parameter| parameter.ty.wasm()),
                [signature.result.wasm()],
            );
        }
        let start_type_index =
            u32::try_from(functions.len() + methods.len()).context("Too many wasm functions")? + 36;
        types.ty().function([], []);
        module.section(&types);

        let mut imports = ImportSection::new();
        imports.import("ty", "print_i64", EntityType::Function(0));
        imports.import("ty", "print_f64", EntityType::Function(1));
        imports.import("ty", "print_ref", EntityType::Function(2));
        imports.import("ty", "str_const", EntityType::Function(3));
        imports.import("ty", "str_concat", EntityType::Function(4));
        imports.import("ty", "list_new", EntityType::Function(5));
        imports.import("ty", "list_push_i64", EntityType::Function(6));
        imports.import("ty", "list_get_i64", EntityType::Function(7));
        imports.import("ty", "tuple_new", EntityType::Function(8));
        imports.import("ty", "tuple_push_i64", EntityType::Function(9));
        imports.import("ty", "tuple_get_i64", EntityType::Function(10));
        imports.import("ty", "dict_new", EntityType::Function(11));
        imports.import("ty", "dict_set_str_i64", EntityType::Function(12));
        imports.import("ty", "dict_get_str_i64", EntityType::Function(13));
        imports.import("ty", "dict_key_ref", EntityType::Function(14));
        imports.import("ty", "object_new", EntityType::Function(15));
        imports.import("ty", "object_set_i64", EntityType::Function(16));
        imports.import("ty", "object_get_i64", EntityType::Function(17));
        imports.import("ty", "object_set_f64", EntityType::Function(18));
        imports.import("ty", "object_get_f64", EntityType::Function(19));
        imports.import("ty", "ref_len", EntityType::Function(20));
        imports.import("ty", "list_str_new", EntityType::Function(21));
        imports.import("ty", "list_push_ref", EntityType::Function(22));
        imports.import("ty", "list_get_ref", EntityType::Function(23));
        imports.import("ty", "dict_str_new", EntityType::Function(24));
        imports.import("ty", "dict_set_str_ref", EntityType::Function(25));
        imports.import("ty", "dict_get_str_ref", EntityType::Function(26));
        imports.import("ty", "read_text", EntityType::Function(27));
        imports.import("ty", "tuple_str_new", EntityType::Function(28));
        imports.import("ty", "tuple_push_ref", EntityType::Function(29));
        imports.import("ty", "tuple_get_ref", EntityType::Function(30));
        imports.import("ty", "write_text", EntityType::Function(31));
        imports.import("ty", "list_set_i64", EntityType::Function(32));
        imports.import("ty", "list_set_ref", EntityType::Function(33));
        imports.import("ty", "str_compare", EntityType::Function(34));
        imports.import("ty", "list_obj_new", EntityType::Function(5));
        imports.import("ty", "list_push_obj", EntityType::Function(6));
        imports.import("ty", "list_get_obj", EntityType::Function(7));
        imports.import("ty", "list_set_obj", EntityType::Function(32));
        imports.import("ty", "str_from_i64", EntityType::Function(20));
        imports.import("ty", "str_from_f64", EntityType::Function(35));
        imports.import("ty", "str_from_bool", EntityType::Function(20));
        module.section(&imports);

        let mut function_section = FunctionSection::new();
        for definition in &functions {
            let signature = function_signature(&signatures, &definition.key)?;
            function_section.function(signature.type_index);
        }
        for method in &methods {
            let signature = function_signature(&signatures, &method.key)?;
            function_section.function(signature.type_index);
        }
        function_section.function(start_type_index);
        module.section(&function_section);

        let mut exports = ExportSection::new();
        let start_index =
            u32::try_from(functions.len() + methods.len()).context("Too many wasm functions")? + 42;
        exports.export("_start", ExportKind::Func, start_index);
        module.section(&exports);

        let mut code = CodeSection::new();
        for definition in &functions {
            let function = definition.function;
            let signature = function_signature(&signatures, &definition.key)?;
            let mut compiler = FunctionCompiler::for_function(
                &signatures,
                &class_signatures,
                signature,
                definition.model,
                Rc::clone(&strings),
                Rc::clone(&uses_filesystem),
            );
            compiler.collect_locals(&function.body)?;
            if !statements_guarantee_return(&function.body) {
                bail!(
                    "Function `{}` must return a value on every path in `ty run`",
                    function.name
                );
            }
            let mut wasm_function = Function::new(compiler.grouped_locals());
            compiler.compile_statements(&function.body, &mut wasm_function)?;
            wasm_function.instruction(&Instruction::Unreachable);
            wasm_function.instruction(&Instruction::End);
            code.function(&wasm_function);
        }
        for method in &methods {
            let signature = function_signature(&signatures, &method.key)?;
            let mut compiler = FunctionCompiler::for_function(
                &signatures,
                &class_signatures,
                signature,
                method.model,
                Rc::clone(&strings),
                Rc::clone(&uses_filesystem),
            );
            compiler.collect_locals(&method.function.body)?;
            if !statements_guarantee_return(&method.function.body) {
                bail!(
                    "Method `{}` must return a value on every path in `ty run`",
                    method.key
                );
            }
            let mut wasm_function = Function::new(compiler.grouped_locals());
            compiler.compile_statements(&method.function.body, &mut wasm_function)?;
            wasm_function.instruction(&Instruction::Unreachable);
            wasm_function.instruction(&Instruction::End);
            code.function(&wasm_function);
        }

        let mut start_compiler = FunctionCompiler::for_start(
            &signatures,
            &class_signatures,
            model,
            Rc::clone(&strings),
            Rc::clone(&uses_filesystem),
        );
        start_compiler.collect_imported_constants(&imported_constants)?;
        start_compiler.collect_locals(&start_body)?;
        let mut start = Function::new(start_compiler.grouped_locals());
        start_compiler.compile_imported_constants(&imported_constants, &mut start)?;
        start_compiler.compile_statements(&start_body, &mut start)?;
        start.instruction(&Instruction::End);
        code.function(&start);
        module.section(&code);

        Ok(CompiledProgram {
            wasm: module.finish(),
            strings: strings.borrow().clone(),
            uses_filesystem: uses_filesystem.get(),
        })
    }
}

fn split_module_body(
    body: &[Stmt],
) -> (
    Vec<&ast::StmtFunctionDef>,
    Vec<&ast::StmtClassDef>,
    Vec<&Stmt>,
) {
    let mut functions = Vec::new();
    let mut classes = Vec::new();
    let mut start_body = Vec::new();

    for statement in body {
        match statement {
            Stmt::FunctionDef(function) => functions.push(function),
            Stmt::ClassDef(class) => classes.push(class),
            statement => start_body.push(statement),
        }
    }

    (functions, classes, start_body)
}

fn collect_method_definitions<'a, 'db>(
    classes: &[&'a ast::StmtClassDef],
    imported_classes: &'a [ClassDefinition<'a, 'db>],
    model: &'a SemanticModel<'db>,
) -> anyhow::Result<Vec<MethodDefinition<'a, 'db>>> {
    let mut methods = Vec::new();

    for class in classes {
        for statement in &class.body {
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            if function.name.as_str() == "__init__" {
                continue;
            }
            methods.push(MethodDefinition {
                key: format!("{}.{}", class.name, function.name),
                function,
                model,
            });
        }
    }

    for class in imported_classes {
        for statement in &class.class.body {
            let Stmt::FunctionDef(function) = statement else {
                continue;
            };
            if function.name.as_str() == "__init__" {
                continue;
            }
            methods.push(MethodDefinition {
                key: format!("{}.{}", class.class.name, function.name),
                function,
                model: class.model,
            });
        }
    }

    Ok(methods)
}

fn collect_function_signatures(
    functions: &[FunctionDefinition<'_, '_>],
    methods: &[MethodDefinition<'_, '_>],
    _model: &SemanticModel<'_>,
) -> anyhow::Result<HashMap<String, FunctionSignature>> {
    let mut signatures = HashMap::new();

    for (offset, definition) in functions.iter().enumerate() {
        let function = definition.function;
        if function.is_async {
            bail!("Async functions are not supported in `ty run`");
        }
        if !function.decorator_list.is_empty() || function.type_params.is_some() {
            bail!("Decorators and generic type parameters are not supported on `ty run` functions");
        }
        if function.parameters.vararg.is_some()
            || function.parameters.kwarg.is_some()
            || !function.parameters.kwonlyargs.is_empty()
        {
            bail!(
                "`ty run` functions currently support only positional parameters without `*args`, keyword-only arguments, or `**kwargs`"
            );
        }

        let mut parameters = Vec::new();
        for parameter in function
            .parameters
            .posonlyargs
            .iter()
            .chain(&function.parameters.args)
        {
            let ty = lower_ty(
                parameter.inferred_type(definition.model).with_context(|| {
                    format!(
                        "ty did not infer a type for parameter `{}` in function `{}`",
                        parameter.name(),
                        function.name
                    )
                })?,
                definition.model,
                &format!(
                    "parameter `{}` in function `{}`",
                    parameter.name(),
                    function.name
                ),
            )?;
            parameters.push(FunctionParameter {
                name: parameter.name().as_str().to_string(),
                ty,
                keyword_allowed: !function
                    .parameters
                    .posonlyargs
                    .iter()
                    .any(|candidate| candidate.name().as_str() == parameter.name().as_str()),
                default: scalar_default_value(
                    parameter.default.as_deref(),
                    ty,
                    &format!(
                        "parameter `{}` in function `{}`",
                        parameter.name(),
                        function.name
                    ),
                )?,
            });
        }

        let result = function_return_type(
            function,
            definition.model,
            &format!("function `{}`", function.name),
        )?;
        let offset = u32::try_from(offset).context("Too many wasm functions")?;
        let name = definition.key.clone();
        let previous = signatures.insert(
            name.clone(),
            FunctionSignature {
                index: offset + 42,
                type_index: offset + 36,
                parameters,
                result,
            },
        );
        if previous.is_some() {
            bail!("Duplicate function definition `{name}` in `ty run`");
        }
    }

    for (method_offset, method) in methods.iter().enumerate() {
        let function = method.function;
        if function.is_async {
            bail!("Async methods are not supported in `ty run`");
        }
        if !function.decorator_list.is_empty() || function.type_params.is_some() {
            bail!("Decorators and generic type parameters are not supported on `ty run` methods");
        }
        if function.parameters.vararg.is_some()
            || function.parameters.kwarg.is_some()
            || !function.parameters.kwonlyargs.is_empty()
        {
            bail!(
                "`ty run` methods currently support only positional parameters without `*args`, keyword-only arguments, or `**kwargs`"
            );
        }

        let mut method_parameters = function
            .parameters
            .posonlyargs
            .iter()
            .chain(&function.parameters.args);
        let self_parameter = method_parameters.next().with_context(|| {
            format!(
                "Method `{}` must accept `self` as its first parameter",
                method.key
            )
        })?;
        if self_parameter.name().as_str() != "self" {
            bail!(
                "Method `{}` must use `self` as its first parameter",
                method.key
            );
        }
        let mut parameters = vec![FunctionParameter {
            name: "self".to_string(),
            ty: ValueType::Object,
            keyword_allowed: false,
            default: None,
        }];
        for parameter in method_parameters {
            let ty = lower_ty(
                parameter.inferred_type(method.model).with_context(|| {
                    format!(
                        "ty did not infer a type for parameter `{}` in method `{}`",
                        parameter.name(),
                        method.key
                    )
                })?,
                method.model,
                &format!(
                    "parameter `{}` in method `{}`",
                    parameter.name(),
                    method.key
                ),
            )?;
            parameters.push(FunctionParameter {
                name: parameter.name().as_str().to_string(),
                ty,
                keyword_allowed: !function
                    .parameters
                    .posonlyargs
                    .iter()
                    .any(|candidate| candidate.name().as_str() == parameter.name().as_str()),
                default: scalar_default_value(
                    parameter.default.as_deref(),
                    ty,
                    &format!(
                        "parameter `{}` in method `{}`",
                        parameter.name(),
                        method.key
                    ),
                )?,
            });
        }

        let result =
            function_return_type(function, method.model, &format!("method `{}`", method.key))?;
        let offset = functions
            .len()
            .checked_add(method_offset)
            .context("Too many wasm functions")?;
        let offset = u32::try_from(offset).context("Too many wasm functions")?;
        let previous = signatures.insert(
            method.key.clone(),
            FunctionSignature {
                index: offset + 42,
                type_index: offset + 36,
                parameters,
                result,
            },
        );
        if previous.is_some() {
            bail!("Duplicate method definition `{}` in `ty run`", method.key);
        }
    }

    Ok(signatures)
}

fn collect_class_signatures(
    classes: &[&ast::StmtClassDef],
    imported_classes: &[ClassDefinition<'_, '_>],
    model: &SemanticModel<'_>,
) -> anyhow::Result<HashMap<String, ClassSignature>> {
    let mut signatures = HashMap::new();

    for class in classes {
        add_class_signature(class, class.name.as_str(), model, &mut signatures)?;
    }
    for class in imported_classes {
        add_class_signature(
            class.class,
            &class.binding_name,
            class.model,
            &mut signatures,
        )?;
    }

    Ok(signatures)
}

fn add_class_signature(
    class: &ast::StmtClassDef,
    binding_name: &str,
    model: &SemanticModel<'_>,
    signatures: &mut HashMap<String, ClassSignature>,
) -> anyhow::Result<()> {
    if !class.decorator_list.is_empty() || class.type_params.is_some() {
        bail!("Decorators and generic type parameters are not supported on `ty run` classes");
    }
    if class.arguments.is_some() {
        bail!("Class bases and metaclass arguments are not supported in `ty run`");
    }

    let mut init = None;
    for statement in &class.body {
        match statement {
            Stmt::FunctionDef(function) if function.name.as_str() == "__init__" => {
                if init.replace(function).is_some() {
                    bail!("Class `{}` defines `__init__` more than once", class.name);
                }
            }
            Stmt::Pass(_) => {}
            Stmt::FunctionDef(_) => {}
            _ => {
                bail!(
                    "Class `{}` only supports an `__init__` method and optional `pass` statements in `ty run`",
                    class.name
                );
            }
        }
    }

    let init = init.with_context(|| {
        format!(
            "Class `{}` needs an `__init__` method in `ty run`",
            class.name
        )
    })?;
    if init.is_async
        || !init.decorator_list.is_empty()
        || init.type_params.is_some()
        || init.parameters.vararg.is_some()
        || init.parameters.kwarg.is_some()
        || !init.parameters.kwonlyargs.is_empty()
    {
        bail!(
            "Class `{}` has an unsupported `__init__` signature in `ty run`",
            class.name
        );
    }
    if init.parameters.posonlyargs.len() + init.parameters.args.len() < 1 {
        bail!("Class `{}` `__init__` must accept `self`", class.name);
    }

    let mut all_parameters = init
        .parameters
        .posonlyargs
        .iter()
        .chain(&init.parameters.args);
    let self_parameter = all_parameters
        .next()
        .context("Class constructor lost its `self` parameter")?;
    if self_parameter.name().as_str() != "self" {
        bail!(
            "Class `{}` `__init__` must use `self` as its first parameter",
            class.name
        );
    }

    let mut parameters = Vec::new();
    for parameter in all_parameters {
        let ty = lower_ty(
            parameter.inferred_type(model).with_context(|| {
                format!(
                    "ty did not infer a type for parameter `{}` in class `{}` `__init__`",
                    parameter.name(),
                    class.name
                )
            })?,
            model,
            &format!(
                "parameter `{}` in class `{}` `__init__`",
                parameter.name(),
                class.name
            ),
        )?;
        parameters.push(FunctionParameter {
            name: parameter.name().as_str().to_string(),
            ty,
            keyword_allowed: !init
                .parameters
                .posonlyargs
                .iter()
                .any(|candidate| candidate.name().as_str() == parameter.name().as_str()),
            default: scalar_default_value(
                parameter.default.as_deref(),
                ty,
                &format!(
                    "parameter `{}` in class `{}` `__init__`",
                    parameter.name(),
                    class.name
                ),
            )?,
        });
    }

    let mut fields = Vec::new();
    for statement in &init.body {
        let Stmt::Assign(assign) = statement else {
            bail!(
                "Class `{}` `__init__` currently supports only `self.field = argument` assignments",
                class.name
            );
        };
        let [target] = assign.targets.as_slice() else {
            bail!(
                "Class `{}` `__init__` only supports single-target field assignments",
                class.name
            );
        };
        let Expr::Attribute(attribute) = target else {
            bail!(
                "Class `{}` `__init__` currently supports only `self.field = argument` assignments",
                class.name
            );
        };
        let Expr::Name(owner) = attribute.value.as_ref() else {
            bail!(
                "Class `{}` `__init__` currently supports only `self.field = argument` assignments",
                class.name
            );
        };
        if owner.id.as_str() != "self" {
            bail!(
                "Class `{}` `__init__` currently supports only `self.field = argument` assignments",
                class.name
            );
        }
        let Expr::Name(value) = assign.value.as_ref() else {
            bail!(
                "Class `{}` `__init__` currently supports only `self.field = argument` assignments",
                class.name
            );
        };
        let (parameter_index, parameter) = parameters
            .iter()
            .enumerate()
            .find(|(_, parameter)| parameter.name == value.id.as_str())
            .with_context(|| {
                format!(
                    "Class `{}` `__init__` field `{}` must be assigned from a constructor argument",
                    class.name, attribute.attr
                )
            })?;
        fields.push(ClassField {
            name: attribute.attr.as_str().to_string(),
            ty: parameter.ty,
            parameter_index,
        });
    }

    let name = binding_name.to_string();
    let previous = signatures.insert(name.clone(), ClassSignature { parameters, fields });
    if previous.is_some() {
        bail!("Duplicate class definition `{name}` in `ty run`");
    }
    Ok(())
}

fn function_signature<'a>(
    signatures: &'a HashMap<String, FunctionSignature>,
    name: &str,
) -> anyhow::Result<&'a FunctionSignature> {
    signatures
        .get(name)
        .with_context(|| format!("Missing compiled signature for function `{name}`"))
}

fn statements_guarantee_return(body: &[Stmt]) -> bool {
    body.last().is_some_and(statement_guarantees_return)
}

fn function_return_type(
    function: &ast::StmtFunctionDef,
    model: &SemanticModel<'_>,
    context: &str,
) -> anyhow::Result<ValueType> {
    if let Some(return_annotation) = function.returns.as_deref() {
        return annotation_type(return_annotation);
    }

    let mut inferred = None;
    collect_return_types(&function.body, model, context, &mut inferred)?;
    inferred.with_context(|| {
        format!(
            "`ty run` could not infer a supported return type for {context}; add an explicit annotation"
        )
    })
}

fn collect_return_types(
    body: &[Stmt],
    model: &SemanticModel<'_>,
    context: &str,
    inferred: &mut Option<ValueType>,
) -> anyhow::Result<()> {
    for statement in body {
        match statement {
            Stmt::Return(statement) => {
                let value = statement.value.as_deref().with_context(|| {
                    format!("`ty run` {context} must return a value on every compiled path")
                })?;
                let ty = lower_ty(
                    value.inferred_type(model).with_context(|| {
                        format!("ty did not infer a type for a return expression in {context}")
                    })?,
                    model,
                    &format!("return expression in {context}"),
                )?;
                if let Some(previous) = inferred {
                    require_same_type(*previous, ty, "inferred return type")?;
                } else {
                    *inferred = Some(ty);
                }
            }
            Stmt::If(statement) => {
                collect_return_types(&statement.body, model, context, inferred)?;
                for clause in &statement.elif_else_clauses {
                    collect_return_types(&clause.body, model, context, inferred)?;
                }
            }
            Stmt::For(statement) => {
                collect_return_types(&statement.body, model, context, inferred)?;
                collect_return_types(&statement.orelse, model, context, inferred)?;
            }
            Stmt::While(statement) => {
                collect_return_types(&statement.body, model, context, inferred)?;
                collect_return_types(&statement.orelse, model, context, inferred)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn statement_guarantees_return(statement: &Stmt) -> bool {
    match statement {
        Stmt::Return(_) => true,
        Stmt::If(statement) => {
            statements_guarantee_return(&statement.body)
                && elif_else_clauses_guarantee_return(&statement.elif_else_clauses)
        }
        _ => false,
    }
}

fn elif_else_clauses_guarantee_return(clauses: &[ast::ElifElseClause]) -> bool {
    let Some((clause, rest)) = clauses.split_first() else {
        return false;
    };
    if clause.test.is_some() {
        statements_guarantee_return(&clause.body) && elif_else_clauses_guarantee_return(rest)
    } else {
        rest.is_empty() && statements_guarantee_return(&clause.body)
    }
}

fn range_stop_argument(iter: &Expr) -> Option<&Expr> {
    let Expr::Call(call) = iter else {
        return None;
    };
    let Expr::Name(function_name) = call.func.as_ref() else {
        return None;
    };
    if function_name.id.as_str() != "range"
        || !call.arguments.keywords.is_empty()
        || call.arguments.args.len() != 1
    {
        return None;
    }
    Some(&call.arguments.args[0])
}

fn ordered_call_arguments<'a>(
    call: &'a ast::ExprCall,
    parameters: &'a [FunctionParameter],
    context: &str,
) -> anyhow::Result<Vec<CallArgument<'a>>> {
    if call.arguments.args.iter().any(Expr::is_starred_expr) {
        bail!("Variadic positional arguments are not supported in {context}");
    }
    if call
        .arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
    {
        bail!("Variadic keyword arguments are not supported in {context}");
    }
    if call.arguments.args.len() > parameters.len() {
        bail!(
            "{context} expects at most {} positional arguments, found {}",
            parameters.len(),
            call.arguments.args.len()
        );
    }

    let mut ordered = vec![None; parameters.len()];
    for (index, argument) in call.arguments.args.iter().enumerate() {
        ordered[index] = Some(CallArgument::Expr(argument));
    }

    let mut seen_keywords = HashSet::new();
    for keyword in &*call.arguments.keywords {
        let name = keyword
            .arg
            .as_ref()
            .context("Checked keyword argument unexpectedly lost its name")?
            .as_str();
        if !seen_keywords.insert(name) {
            bail!("Keyword argument `{name}` is provided more than once in {context}");
        }
        let Some((index, parameter)) = parameters
            .iter()
            .enumerate()
            .find(|(_, parameter)| parameter.name == name)
        else {
            bail!("Unknown keyword argument `{name}` in {context}");
        };
        if !parameter.keyword_allowed {
            bail!("Parameter `{name}` is positional-only in {context}");
        }
        if ordered[index].is_some() {
            bail!("Argument `{name}` is provided more than once in {context}");
        }
        ordered[index] = Some(CallArgument::Expr(&keyword.value));
    }

    ordered
        .into_iter()
        .zip(parameters)
        .map(|(argument, parameter)| {
            argument
                .or_else(|| parameter.default.as_ref().map(CallArgument::Default))
                .with_context(|| {
                    format!(
                        "Missing required argument `{}` in {context}",
                        parameter.name
                    )
                })
        })
        .collect()
}

fn scalar_default_value(
    default: Option<&Expr>,
    expected_ty: ValueType,
    context: &str,
) -> anyhow::Result<Option<DefaultValue>> {
    let Some(default) = default else {
        return Ok(None);
    };
    let value = match (default, expected_ty) {
        (Expr::NumberLiteral(number), ValueType::Int) => {
            let Number::Int(value) = &number.value else {
                bail!("Default value for {context} must lower to `int`");
            };
            let Some(value) = value.as_i64() else {
                bail!(
                    "Integer default value for {context} is too large for the wasm `i64` backend"
                );
            };
            DefaultValue::Int(value)
        }
        (Expr::BooleanLiteral(boolean), ValueType::Bool) => DefaultValue::Bool(boolean.value),
        (Expr::NumberLiteral(number), ValueType::Float) => {
            let Number::Float(value) = number.value else {
                bail!("Default value for {context} must lower to `float`");
            };
            DefaultValue::Float(value)
        }
        (Expr::StringLiteral(string), ValueType::String) => {
            DefaultValue::String(string.value.to_str().to_string())
        }
        _ => bail!(
            "`ty run` currently supports only scalar literal defaults whose type matches {context}"
        ),
    };
    Ok(Some(value))
}

#[derive(Clone, Copy)]
struct ForLoopScratchLocals {
    counter: u32,
    stop: u32,
    iter: u32,
}

struct FunctionCompiler<'a> {
    signatures: &'a HashMap<String, FunctionSignature>,
    classes: &'a HashMap<String, ClassSignature>,
    model: &'a SemanticModel<'a>,
    strings: Rc<RefCell<Vec<String>>>,
    uses_filesystem: Rc<Cell<bool>>,
    locals: HashMap<String, Local>,
    local_types: Vec<ValType>,
    parameter_count: usize,
    scratch_ref_local: u32,
    scratch_collection_local: u32,
    for_loop_scratch_locals: Vec<ForLoopScratchLocals>,
    return_type: Option<ValueType>,
}

impl<'a> FunctionCompiler<'a> {
    fn for_start(
        signatures: &'a HashMap<String, FunctionSignature>,
        classes: &'a HashMap<String, ClassSignature>,
        model: &'a SemanticModel<'a>,
        strings: Rc<RefCell<Vec<String>>>,
        uses_filesystem: Rc<Cell<bool>>,
    ) -> Self {
        Self {
            signatures,
            classes,
            model,
            strings,
            uses_filesystem,
            locals: HashMap::new(),
            local_types: vec![
                ValType::I64,
                ValType::I64,
                ValType::I64,
                ValType::I64,
                ValType::I64,
            ],
            parameter_count: 0,
            scratch_ref_local: 0,
            scratch_collection_local: 1,
            for_loop_scratch_locals: vec![ForLoopScratchLocals {
                counter: 2,
                stop: 3,
                iter: 4,
            }],
            return_type: None,
        }
    }

    fn for_function(
        signatures: &'a HashMap<String, FunctionSignature>,
        classes: &'a HashMap<String, ClassSignature>,
        signature: &FunctionSignature,
        model: &'a SemanticModel<'a>,
        strings: Rc<RefCell<Vec<String>>>,
        uses_filesystem: Rc<Cell<bool>>,
    ) -> Self {
        let locals = signature
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                (
                    parameter.name.clone(),
                    Local {
                        index: u32::try_from(index)
                            .expect("parameter count already fit while building wasm"),
                        ty: parameter.ty,
                    },
                )
            })
            .collect();

        Self {
            signatures,
            classes,
            model,
            strings,
            uses_filesystem,
            locals,
            local_types: vec![
                ValType::I64,
                ValType::I64,
                ValType::I64,
                ValType::I64,
                ValType::I64,
            ],
            parameter_count: signature.parameters.len(),
            scratch_ref_local: u32::try_from(signature.parameters.len())
                .expect("parameter count already fit while building wasm"),
            scratch_collection_local: u32::try_from(signature.parameters.len() + 1)
                .expect("parameter count already fit while building wasm"),
            for_loop_scratch_locals: vec![ForLoopScratchLocals {
                counter: u32::try_from(signature.parameters.len() + 2)
                    .expect("parameter count already fit while building wasm"),
                stop: u32::try_from(signature.parameters.len() + 3)
                    .expect("parameter count already fit while building wasm"),
                iter: u32::try_from(signature.parameters.len() + 4)
                    .expect("parameter count already fit while building wasm"),
            }],
            return_type: Some(signature.result),
        }
    }

    fn grouped_locals(&self) -> Vec<(u32, ValType)> {
        if self.local_types.is_empty() {
            return Vec::new();
        }

        let mut grouped = Vec::new();
        let mut current = self.local_types[0];
        let mut count = 0;

        for ty in &self.local_types {
            if *ty == current {
                count += 1;
            } else {
                grouped.push((count, current));
                current = *ty;
                count = 1;
            }
        }

        grouped.push((count, current));
        grouped
    }

    fn collect_locals(&mut self, body: &[impl std::borrow::Borrow<Stmt>]) -> anyhow::Result<()> {
        self.collect_locals_with_for_depth(body, 0)
    }

    fn collect_imported_constants(
        &mut self,
        constants: &[ConstantDefinition<'_, '_>],
    ) -> anyhow::Result<()> {
        for constant in constants {
            let ty = lower_ty(
                constant
                    .value
                    .inferred_type(constant.model)
                    .with_context(|| {
                        format!(
                            "ty did not infer a type for imported constant `{}`",
                            constant.binding_name
                        )
                    })?,
                constant.model,
                &format!("imported constant `{}`", constant.binding_name),
            )?;
            match ty {
                ValueType::Int | ValueType::Bool | ValueType::Float | ValueType::String => {}
                _ => bail!(
                    "`ty run` currently imports only scalar constants; `{}` lowers to `{}`",
                    constant.binding_name,
                    ty.name()
                ),
            }
            self.collect_named_local(&constant.binding_name, ty)?;
        }
        Ok(())
    }

    fn collect_locals_with_for_depth(
        &mut self,
        body: &[impl std::borrow::Borrow<Stmt>],
        for_depth: usize,
    ) -> anyhow::Result<()> {
        for statement in body {
            let statement = statement.borrow();
            match statement {
                Stmt::AnnAssign(assign) => self.collect_annotated_local(assign)?,
                Stmt::Assign(assign) => self.collect_inferred_local(assign)?,
                Stmt::For(statement) => {
                    self.ensure_for_loop_scratch(for_depth)?;
                    self.collect_for_local(statement)?;
                    self.collect_locals_with_for_depth(&statement.body, for_depth + 1)?;
                    self.collect_locals_with_for_depth(&statement.orelse, for_depth)?;
                }
                Stmt::If(statement) => {
                    self.collect_locals_with_for_depth(&statement.body, for_depth)?;
                    for clause in &statement.elif_else_clauses {
                        self.collect_locals_with_for_depth(&clause.body, for_depth)?;
                    }
                }
                Stmt::While(statement) => {
                    self.collect_locals_with_for_depth(&statement.body, for_depth)?;
                    self.collect_locals_with_for_depth(&statement.orelse, for_depth)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn ensure_for_loop_scratch(&mut self, depth: usize) -> anyhow::Result<()> {
        while self.for_loop_scratch_locals.len() <= depth {
            let counter = self.allocate_i64_local()?;
            let stop = self.allocate_i64_local()?;
            let iter = self.allocate_i64_local()?;
            self.for_loop_scratch_locals.push(ForLoopScratchLocals {
                counter,
                stop,
                iter,
            });
        }
        Ok(())
    }

    fn allocate_i64_local(&mut self) -> anyhow::Result<u32> {
        let index = self
            .parameter_count
            .checked_add(self.local_types.len())
            .context("Too many wasm locals in the compiled program")?;
        let index = u32::try_from(index).context("Too many wasm locals in the compiled program")?;
        self.local_types.push(ValType::I64);
        Ok(index)
    }

    fn collect_annotated_local(&mut self, assign: &ast::StmtAnnAssign) -> anyhow::Result<()> {
        let Expr::Name(target) = assign.target.as_ref() else {
            bail!("Only annotated local names are supported on the left side of assignments");
        };
        let ty = annotation_type(assign.annotation.as_ref())?;
        let name = target.id.as_str().to_string();

        if let Some(existing) = self.locals.get(&name) {
            if existing.ty != ty {
                bail!(
                    "Local `{name}` is annotated as both `{}` and `{}`",
                    existing.ty.name(),
                    ty.name()
                );
            }
            return Ok(());
        }

        let index = self
            .parameter_count
            .checked_add(self.local_types.len())
            .context("Too many wasm locals in the compiled program")?;
        let index = u32::try_from(index).context("Too many wasm locals in the compiled program")?;
        self.locals.insert(name, Local { index, ty });
        self.local_types.push(ty.wasm());
        Ok(())
    }

    fn collect_for_local(&mut self, statement: &ast::StmtFor) -> anyhow::Result<()> {
        if statement.is_async {
            bail!("Async `for` loops are not supported in `ty run`");
        }
        if !statement.orelse.is_empty() {
            bail!("`for ... else` is not supported in `ty run`");
        }

        let Expr::Name(target) = statement.target.as_ref() else {
            bail!("`ty run` currently supports only name targets in `for` loops");
        };
        let target_ty = if range_stop_argument(statement.iter.as_ref()).is_some() {
            ValueType::Int
        } else {
            match self.iterable_type(statement.iter.as_ref())? {
                ValueType::ListInt | ValueType::TupleInt => ValueType::Int,
                ValueType::ListString
                | ValueType::TupleString
                | ValueType::DictStrInt
                | ValueType::DictStrString => ValueType::String,
                ValueType::ListObject => ValueType::Object,
                _ => bail!(
                    "`ty run` currently supports `for` loops over `range(stop)`, `list[int]`, `list[str]`, `list[object]`, integer/string tuples, and string-keyed dictionaries"
                ),
            }
        };
        let name = target.id.as_str().to_string();

        if let Some(existing) = self.locals.get(&name) {
            if existing.ty != target_ty {
                bail!(
                    "Loop target `{name}` must have type `{}`, found `{}`",
                    target_ty.name(),
                    existing.ty.name()
                );
            }
            return Ok(());
        }

        let index = self
            .parameter_count
            .checked_add(self.local_types.len())
            .context("Too many wasm locals in the compiled program")?;
        let index = u32::try_from(index).context("Too many wasm locals in the compiled program")?;
        self.locals.insert(
            name,
            Local {
                index,
                ty: target_ty,
            },
        );
        self.local_types.push(target_ty.wasm());
        Ok(())
    }

    fn collect_inferred_local(&mut self, assign: &ast::StmtAssign) -> anyhow::Result<()> {
        let [target] = assign.targets.as_slice() else {
            bail!("`ty run` only supports single-target assignments");
        };
        let Expr::Name(target) = target else {
            return Ok(());
        };
        let ty = self.assignment_initializer_type(assign.value.as_ref())?;
        let name = target.id.as_str().to_string();

        self.collect_named_local(&name, ty)
    }

    fn compile_statements(
        &self,
        body: &[impl std::borrow::Borrow<Stmt>],
        function: &mut Function,
    ) -> anyhow::Result<()> {
        self.compile_statements_with_for_depth(body, function, 0)
    }

    fn collect_named_local(&mut self, name: &str, ty: ValueType) -> anyhow::Result<()> {
        if let Some(existing) = self.locals.get(name) {
            if existing.ty != ty {
                bail!(
                    "Local `{name}` is assigned values of both `{}` and `{}`",
                    existing.ty.name(),
                    ty.name()
                );
            }
            return Ok(());
        }

        let index = self
            .parameter_count
            .checked_add(self.local_types.len())
            .context("Too many wasm locals in the compiled program")?;
        let index = u32::try_from(index).context("Too many wasm locals in the compiled program")?;
        self.locals.insert(name.to_string(), Local { index, ty });
        self.local_types.push(ty.wasm());
        Ok(())
    }

    fn compile_imported_constants(
        &self,
        constants: &[ConstantDefinition<'_, '_>],
        function: &mut Function,
    ) -> anyhow::Result<()> {
        for constant in constants {
            let local = self.local(&constant.binding_name)?;
            let value_ty = self.compile_external_expression(
                constant.value,
                constant.model,
                function,
                &format!("imported constant `{}`", constant.binding_name),
            )?;
            require_same_type(local.ty, value_ty, "imported constant initializer")?;
            function.instruction(&Instruction::LocalSet(local.index));
        }
        Ok(())
    }

    fn compile_external_expression(
        &self,
        expr: &Expr,
        model: &SemanticModel<'_>,
        function: &mut Function,
        context: &str,
    ) -> anyhow::Result<ValueType> {
        let inferred_ty = lower_ty(
            expr.inferred_type(model)
                .with_context(|| format!("ty did not infer a type for {context}"))?,
            model,
            context,
        )?;
        match expr {
            Expr::NumberLiteral(number) => match &number.value {
                Number::Int(value) => {
                    let Some(value) = value.as_i64() else {
                        bail!("Integer literal is too large for the wasm `i64` backend");
                    };
                    function.instruction(&Instruction::I64Const(value));
                    require_same_type(ValueType::Int, inferred_ty, context)?;
                    Ok(inferred_ty)
                }
                Number::Float(value) => {
                    function.instruction(&Instruction::F64Const((*value).into()));
                    require_same_type(ValueType::Float, inferred_ty, context)?;
                    Ok(inferred_ty)
                }
                Number::Complex { .. } => {
                    bail!("Complex literals are not supported in imported constants")
                }
            },
            Expr::BooleanLiteral(boolean) => {
                function.instruction(&Instruction::I64Const(i64::from(boolean.value)));
                require_same_type(ValueType::Bool, inferred_ty, context)?;
                Ok(inferred_ty)
            }
            Expr::StringLiteral(string) => {
                let index = self.intern_string(string.value.to_str());
                function.instruction(&Instruction::I32Const(index));
                function.instruction(&Instruction::Call(3));
                require_same_type(ValueType::String, inferred_ty, context)?;
                Ok(inferred_ty)
            }
            _ => bail!(
                "`ty run` currently imports only scalar literal constants; unsupported initializer `{expr:?}`"
            ),
        }
    }

    fn compile_statements_with_for_depth(
        &self,
        body: &[impl std::borrow::Borrow<Stmt>],
        function: &mut Function,
        for_depth: usize,
    ) -> anyhow::Result<()> {
        for statement in body {
            self.compile_statement(statement.borrow(), function, for_depth)?;
        }
        Ok(())
    }

    fn compile_statement(
        &self,
        statement: &Stmt,
        function: &mut Function,
        for_depth: usize,
    ) -> anyhow::Result<()> {
        match statement {
            Stmt::AnnAssign(assign) => self.compile_annotated_assignment(assign, function),
            Stmt::Assign(assign) => self.compile_assignment(assign, function),
            Stmt::AugAssign(assign) => self.compile_augmented_assignment(assign, function),
            Stmt::Expr(statement) => self.compile_expression_statement(statement, function),
            Stmt::If(statement) => self.compile_if(statement, function, for_depth),
            Stmt::For(statement) => self.compile_for(statement, function, for_depth),
            Stmt::While(statement) => self.compile_while(statement, function, for_depth),
            Stmt::Return(statement) => self.compile_return(statement, function),
            Stmt::Pass(_) => Ok(()),
            Stmt::ImportFrom(statement) => self.compile_import_from(statement),
            Stmt::FunctionDef(_) => bail!("Nested functions are not supported in `ty run`"),
            Stmt::ClassDef(_) => bail!("Nested classes are not supported in `ty run`"),
            _ => bail!("Unsupported statement in `ty run`: `{statement:?}`"),
        }
    }

    fn compile_import_from(&self, statement: &ast::StmtImportFrom) -> anyhow::Result<()> {
        let Some(module) = statement.module.as_ref() else {
            bail!("`ty run` relative imports must name a module");
        };
        if statement.level == 0 && module.id.as_str() == "ty_extensions" {
            if statement.names.is_empty()
                || statement.names.iter().any(|alias| {
                    !matches!(alias.name.id.as_str(), "read_text" | "write_text")
                        || alias.asname.is_some()
                })
            {
                bail!(
                    "Only `from ty_extensions import read_text, write_text` filesystem imports are supported in `ty run`"
                );
            }
            return Ok(());
        }
        if statement.names.is_empty() {
            bail!("`ty run` project-module imports must import at least one function");
        }
        for alias in &statement.names {
            if alias.name.id.as_str() == "*" {
                bail!(
                    "`ty run` project-module imports currently support only named functions, not `*` imports"
                );
            }
            let binding_name = alias
                .asname
                .as_ref()
                .map_or_else(|| alias.name.id.as_str(), |name| name.id.as_str());
            if !self.signatures.contains_key(binding_name)
                && !self.locals.contains_key(binding_name)
                && !self.classes.contains_key(binding_name)
            {
                bail!(
                    "`ty run` did not compile imported function, scalar constant, or class `{}` from `{}`",
                    binding_name,
                    module.id
                );
            }
        }
        Ok(())
    }

    fn compile_annotated_assignment(
        &self,
        assign: &ast::StmtAnnAssign,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let Expr::Name(target) = assign.target.as_ref() else {
            bail!("Only annotated local names are supported on the left side of assignments");
        };
        let Some(value) = assign.value.as_deref() else {
            bail!("Annotated locals in `ty run` must include an initializer");
        };
        let local = self.local(target.id.as_str())?;
        let value_ty = self.compile_expression(value, function)?;
        require_same_type(local.ty, value_ty, "annotated assignment")?;
        function.instruction(&Instruction::LocalSet(local.index));
        Ok(())
    }

    fn compile_assignment(
        &self,
        assign: &ast::StmtAssign,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let [target] = assign.targets.as_slice() else {
            bail!("`ty run` only supports single-target assignments");
        };
        match target {
            Expr::Name(target) => {
                let local = self.local(target.id.as_str())?;
                let value_ty = self.compile_expression(assign.value.as_ref(), function)?;
                require_same_type(local.ty, value_ty, "assignment")?;
                function.instruction(&Instruction::LocalSet(local.index));
                Ok(())
            }
            Expr::Subscript(subscript) => {
                self.compile_subscript_assignment(subscript, assign.value.as_ref(), function)
            }
            Expr::Attribute(attribute) => {
                self.compile_attribute_assignment(attribute, assign.value.as_ref(), function)
            }
            _ => bail!(
                "Only local-name, supported container-item, and typed attribute assignments are supported in `ty run`"
            ),
        }
    }

    fn compile_subscript_assignment(
        &self,
        subscript: &ast::ExprSubscript,
        value: &Expr,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let container_ty = self.compile_expression(subscript.value.as_ref(), function)?;
        match container_ty {
            ValueType::ListInt => {
                let index_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                require_same_type(ValueType::Int, index_ty, "list assignment index")?;
                let value_ty = self.compile_expression(value, function)?;
                require_same_type(ValueType::Int, value_ty, "list assignment value")?;
                function.instruction(&Instruction::Call(32));
                Ok(())
            }
            ValueType::ListString => {
                let index_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                require_same_type(ValueType::Int, index_ty, "list assignment index")?;
                let value_ty = self.compile_expression(value, function)?;
                require_same_type(ValueType::String, value_ty, "list assignment value")?;
                function.instruction(&Instruction::Call(33));
                Ok(())
            }
            ValueType::ListObject => {
                let index_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                require_same_type(ValueType::Int, index_ty, "list assignment index")?;
                let value_ty = self.compile_expression(value, function)?;
                require_same_type(ValueType::Object, value_ty, "list assignment value")?;
                function.instruction(&Instruction::Call(38));
                Ok(())
            }
            ValueType::DictStrInt => {
                let key_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                require_same_type(ValueType::String, key_ty, "dictionary assignment key")?;
                let value_ty = self.compile_expression(value, function)?;
                require_same_type(ValueType::Int, value_ty, "dictionary assignment value")?;
                function.instruction(&Instruction::Call(12));
                Ok(())
            }
            ValueType::DictStrString => {
                let key_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                require_same_type(ValueType::String, key_ty, "dictionary assignment key")?;
                let value_ty = self.compile_expression(value, function)?;
                require_same_type(ValueType::String, value_ty, "dictionary assignment value")?;
                function.instruction(&Instruction::Call(25));
                Ok(())
            }
            _ => bail!(
                "`ty run` currently supports item assignment only for `list[int]`, `list[str]`, `list[object]`, `dict[str, int]`, and `dict[str, str]`"
            ),
        }
    }

    fn compile_attribute_assignment(
        &self,
        attribute: &ast::ExprAttribute,
        value: &Expr,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let owner_ty = self.compile_expression(attribute.value.as_ref(), function)?;
        require_same_type(ValueType::Object, owner_ty, "attribute assignment owner")?;
        let key = self.intern_string(attribute.attr.as_str());
        function.instruction(&Instruction::I32Const(key));
        function.instruction(&Instruction::Call(3));

        let expected_ty = self.expression_type(
            &Expr::Attribute(attribute.clone()),
            "attribute assignment target",
        )?;
        let actual_ty = self.compile_expression(value, function)?;
        require_same_type(expected_ty, actual_ty, "attribute assignment value")?;
        function.instruction(&Instruction::Call(match expected_ty {
            ValueType::Float => 18,
            ValueType::Int
            | ValueType::Bool
            | ValueType::String
            | ValueType::ListInt
            | ValueType::ListString
            | ValueType::ListObject
            | ValueType::TupleInt
            | ValueType::TupleString
            | ValueType::DictStrInt
            | ValueType::DictStrString
            | ValueType::Object => 16,
        }));
        Ok(())
    }

    fn compile_augmented_assignment(
        &self,
        assign: &ast::StmtAugAssign,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let Expr::Name(target) = assign.target.as_ref() else {
            bail!("Only local-name augmented assignments are supported in `ty run`");
        };
        let local = self.local(target.id.as_str())?;
        if local.ty != ValueType::Int {
            bail!("Augmented assignment currently requires an `int` target");
        }
        if !matches!(assign.op, Operator::Add | Operator::Sub | Operator::Mult) {
            bail!("Unsupported augmented assignment operator `{}`", assign.op);
        }

        function.instruction(&Instruction::LocalGet(local.index));
        let value_ty = self.compile_expression(assign.value.as_ref(), function)?;
        require_same_type(ValueType::Int, value_ty, "augmented assignment")?;
        self.compile_int_operator(assign.op, function)?;
        function.instruction(&Instruction::LocalSet(local.index));
        Ok(())
    }

    fn compile_expression_statement(
        &self,
        statement: &ast::StmtExpr,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let Expr::Call(call) = statement.value.as_ref() else {
            bail!(
                "Only supported side-effecting calls are allowed as expression statements in `ty run`"
            );
        };
        match call.func.as_ref() {
            Expr::Name(function_name) if function_name.id.as_str() == "print" => {
                if !call.arguments.keywords.is_empty() || call.arguments.args.len() != 1 {
                    bail!(
                        "`ty run` currently supports exactly one positional `print(...)` argument"
                    );
                }

                let ty = self.compile_expression(&call.arguments.args[0], function)?;
                function.instruction(&Instruction::Call(match ty {
                    ValueType::Int | ValueType::Bool => 0,
                    ValueType::Float => 1,
                    ValueType::String
                    | ValueType::ListInt
                    | ValueType::ListString
                    | ValueType::ListObject
                    | ValueType::TupleInt
                    | ValueType::TupleString
                    | ValueType::DictStrInt
                    | ValueType::DictStrString
                    | ValueType::Object => 2,
                }));
                Ok(())
            }
            Expr::Attribute(attribute) if attribute.attr.as_str() == "append" => {
                if !call.arguments.keywords.is_empty() || call.arguments.args.len() != 1 {
                    bail!("`list.append(...)` expects exactly one positional argument in `ty run`");
                }
                let owner = self.compile_expression(attribute.value.as_ref(), function)?;
                let value = self.compile_expression(&call.arguments.args[0], function)?;
                match owner {
                    ValueType::ListInt => {
                        require_same_type(ValueType::Int, value, "`list.append(...)` argument")?;
                        function.instruction(&Instruction::Call(6));
                    }
                    ValueType::ListString => {
                        require_same_type(ValueType::String, value, "`list.append(...)` argument")?;
                        function.instruction(&Instruction::Call(22));
                    }
                    ValueType::ListObject => {
                        require_same_type(ValueType::Object, value, "`list.append(...)` argument")?;
                        function.instruction(&Instruction::Call(36));
                    }
                    _ => bail!(
                        "`list.append(...)` is only supported on `list[int]`, `list[str]`, and `list[object]` in `ty run`"
                    ),
                }
                Ok(())
            }
            Expr::Name(function_name) if function_name.id.as_str() == "write_text" => {
                if !call.arguments.keywords.is_empty() || call.arguments.args.len() != 2 {
                    bail!("`write_text(...)` expects exactly two positional arguments in `ty run`");
                }
                let path = self.compile_expression(&call.arguments.args[0], function)?;
                require_same_type(ValueType::String, path, "write_text path")?;
                let contents = self.compile_expression(&call.arguments.args[1], function)?;
                require_same_type(ValueType::String, contents, "write_text contents")?;
                self.uses_filesystem.set(true);
                function.instruction(&Instruction::Call(31));
                Ok(())
            }
            _ => bail!(
                "Only `print(...)`, `list.append(...)`, and `write_text(...)` expression statements are supported in `ty run`"
            ),
        }
    }

    fn compile_return(
        &self,
        statement: &ast::StmtReturn,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        let Some(expected) = self.return_type else {
            bail!("`return` is only supported inside compiled functions in `ty run`");
        };
        let Some(value) = statement.value.as_deref() else {
            bail!("Compiled `ty run` functions must return an annotated value");
        };
        let actual = self.compile_expression(value, function)?;
        require_same_type(expected, actual, "return")?;
        function.instruction(&Instruction::Return);
        Ok(())
    }

    fn compile_if(
        &self,
        statement: &ast::StmtIf,
        function: &mut Function,
        for_depth: usize,
    ) -> anyhow::Result<()> {
        self.compile_condition(statement.test.as_ref(), function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.compile_statements_with_for_depth(&statement.body, function, for_depth)?;
        self.compile_elif_else_clauses(&statement.elif_else_clauses, function, for_depth)?;
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_elif_else_clauses(
        &self,
        clauses: &[ast::ElifElseClause],
        function: &mut Function,
        for_depth: usize,
    ) -> anyhow::Result<()> {
        let Some((clause, rest)) = clauses.split_first() else {
            return Ok(());
        };

        function.instruction(&Instruction::Else);
        if let Some(test) = clause.test.as_ref() {
            self.compile_condition(test, function)?;
            function.instruction(&Instruction::If(BlockType::Empty));
            self.compile_statements_with_for_depth(&clause.body, function, for_depth)?;
            self.compile_elif_else_clauses(rest, function, for_depth)?;
            function.instruction(&Instruction::End);
        } else {
            if !rest.is_empty() {
                bail!("`else` must be the final conditional clause in `ty run`");
            }
            self.compile_statements_with_for_depth(&clause.body, function, for_depth)?;
        }
        Ok(())
    }

    fn compile_while(
        &self,
        statement: &ast::StmtWhile,
        function: &mut Function,
        for_depth: usize,
    ) -> anyhow::Result<()> {
        if !statement.orelse.is_empty() {
            bail!("`while ... else` is not supported in `ty run`");
        }

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        self.compile_condition(statement.test.as_ref(), function)?;
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        self.compile_statements_with_for_depth(&statement.body, function, for_depth)?;
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_for(
        &self,
        statement: &ast::StmtFor,
        function: &mut Function,
        for_depth: usize,
    ) -> anyhow::Result<()> {
        if statement.is_async {
            bail!("Async `for` loops are not supported in `ty run`");
        }
        if !statement.orelse.is_empty() {
            bail!("`for ... else` is not supported in `ty run`");
        }
        let scratch = self
            .for_loop_scratch_locals
            .get(for_depth)
            .copied()
            .with_context(|| {
                format!("Missing loop scratch locals for nesting depth {for_depth}")
            })?;

        let Expr::Name(target) = statement.target.as_ref() else {
            bail!("`ty run` currently supports only name targets in `for` loops");
        };
        let target = self.local(target.id.as_str())?;
        let loop_kind = if let Some(stop) = range_stop_argument(statement.iter.as_ref()) {
            let stop_ty = self.compile_expression(stop, function)?;
            require_same_type(ValueType::Int, stop_ty, "`range(...)` stop argument")?;
            function.instruction(&Instruction::LocalSet(scratch.stop));
            ForLoopKind::Range
        } else {
            let iter_ty = self.compile_expression(statement.iter.as_ref(), function)?;
            match iter_ty {
                ValueType::ListInt => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::ListInt
                }
                ValueType::ListString => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::ListString
                }
                ValueType::ListObject => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::ListObject
                }
                ValueType::TupleInt => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::TupleInt
                }
                ValueType::TupleString => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::TupleString
                }
                ValueType::DictStrInt => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::DictStrInt
                }
                ValueType::DictStrString => {
                    function.instruction(&Instruction::LocalSet(scratch.iter));
                    function.instruction(&Instruction::LocalGet(scratch.iter));
                    function.instruction(&Instruction::Call(20));
                    function.instruction(&Instruction::LocalSet(scratch.stop));
                    ForLoopKind::DictStrString
                }
                _ => bail!(
                    "`ty run` currently supports `for` loops over `range(stop)`, `list[int]`, `list[str]`, `list[object]`, integer/string tuples, and string-keyed dictionaries"
                ),
            }
        };
        require_same_type(loop_kind.target_type(), target.ty, "`for` loop target")?;
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::LocalSet(scratch.counter));

        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        function.instruction(&Instruction::LocalGet(scratch.counter));
        function.instruction(&Instruction::LocalGet(scratch.stop));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::BrIf(1));
        match loop_kind {
            ForLoopKind::Range => {
                function.instruction(&Instruction::LocalGet(scratch.counter));
            }
            ForLoopKind::ListInt => {
                function.instruction(&Instruction::LocalGet(scratch.iter));
                function.instruction(&Instruction::LocalGet(scratch.counter));
                function.instruction(&Instruction::Call(7));
            }
            ForLoopKind::ListString => {
                function.instruction(&Instruction::LocalGet(scratch.iter));
                function.instruction(&Instruction::LocalGet(scratch.counter));
                function.instruction(&Instruction::Call(23));
            }
            ForLoopKind::ListObject => {
                function.instruction(&Instruction::LocalGet(scratch.iter));
                function.instruction(&Instruction::LocalGet(scratch.counter));
                function.instruction(&Instruction::Call(37));
            }
            ForLoopKind::TupleInt => {
                function.instruction(&Instruction::LocalGet(scratch.iter));
                function.instruction(&Instruction::LocalGet(scratch.counter));
                function.instruction(&Instruction::Call(10));
            }
            ForLoopKind::TupleString => {
                function.instruction(&Instruction::LocalGet(scratch.iter));
                function.instruction(&Instruction::LocalGet(scratch.counter));
                function.instruction(&Instruction::Call(30));
            }
            ForLoopKind::DictStrInt | ForLoopKind::DictStrString => {
                function.instruction(&Instruction::LocalGet(scratch.iter));
                function.instruction(&Instruction::LocalGet(scratch.counter));
                function.instruction(&Instruction::Call(14));
            }
        }
        function.instruction(&Instruction::LocalSet(target.index));
        self.compile_statements_with_for_depth(&statement.body, function, for_depth + 1)?;
        function.instruction(&Instruction::LocalGet(scratch.counter));
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::LocalSet(scratch.counter));
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_condition(&self, expr: &Expr, function: &mut Function) -> anyhow::Result<()> {
        let ty = self.compile_expression(expr, function)?;
        if ty != ValueType::Bool {
            bail!("Conditions in `ty run` must have type `bool`");
        }
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        Ok(())
    }

    fn compile_expression(
        &self,
        expr: &Expr,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        let inferred_ty = match expr {
            Expr::Name(name) => self.local(name.id.as_str())?.ty,
            Expr::List(list) => self.list_literal_type(list)?,
            Expr::Dict(dict) => self.dict_literal_type(dict)?,
            Expr::Call(call) => {
                if let Some(result) = self.compiled_call_result_type(call)? {
                    result
                } else {
                    self.expression_type(expr, "expression")?
                }
            }
            _ => self.expression_type(expr, "expression")?,
        };
        match expr {
            Expr::NumberLiteral(number) => match &number.value {
                Number::Int(value) => {
                    let Some(value) = value.as_i64() else {
                        bail!("Integer literal is too large for the wasm `i64` backend");
                    };
                    function.instruction(&Instruction::I64Const(value));
                    require_same_type(ValueType::Int, inferred_ty, "integer literal")?;
                    Ok(inferred_ty)
                }
                Number::Float(value) => {
                    function.instruction(&Instruction::F64Const((*value).into()));
                    require_same_type(ValueType::Float, inferred_ty, "float literal")?;
                    Ok(inferred_ty)
                }
                Number::Complex { .. } => {
                    bail!("Complex literals are not supported in `ty run`")
                }
            },
            Expr::BooleanLiteral(boolean) => {
                function.instruction(&Instruction::I64Const(i64::from(boolean.value)));
                require_same_type(ValueType::Bool, inferred_ty, "boolean literal")?;
                Ok(inferred_ty)
            }
            Expr::StringLiteral(string) => {
                let index = self.intern_string(string.value.to_str());
                function.instruction(&Instruction::I32Const(index));
                function.instruction(&Instruction::Call(3));
                require_same_type(ValueType::String, inferred_ty, "string literal")?;
                Ok(inferred_ty)
            }
            Expr::FString(f_string) => {
                let actual = self.compile_f_string(f_string, function)?;
                require_same_type(inferred_ty, actual, "f-string")?;
                Ok(inferred_ty)
            }
            Expr::List(list) => {
                function.instruction(&Instruction::Call(match inferred_ty {
                    ValueType::ListInt => 5,
                    ValueType::ListString => 21,
                    ValueType::ListObject => 35,
                    _ => bail!("Unsupported list literal type in `ty run`"),
                }));
                function.instruction(&Instruction::LocalSet(self.scratch_collection_local));
                for element in list {
                    function.instruction(&Instruction::LocalGet(self.scratch_collection_local));
                    let element_ty = self.compile_expression(element, function)?;
                    match inferred_ty {
                        ValueType::ListInt => {
                            require_same_type(ValueType::Int, element_ty, "list element")?;
                            function.instruction(&Instruction::Call(6));
                        }
                        ValueType::ListString => {
                            require_same_type(ValueType::String, element_ty, "list element")?;
                            function.instruction(&Instruction::Call(22));
                        }
                        ValueType::ListObject => {
                            require_same_type(ValueType::Object, element_ty, "list element")?;
                            function.instruction(&Instruction::Call(36));
                        }
                        _ => bail!("Unsupported list literal type in `ty run`"),
                    }
                }
                function.instruction(&Instruction::LocalGet(self.scratch_collection_local));
                Ok(inferred_ty)
            }
            Expr::Tuple(tuple) => {
                function.instruction(&Instruction::Call(match inferred_ty {
                    ValueType::TupleInt => 8,
                    ValueType::TupleString => 28,
                    _ => bail!("Unsupported tuple literal type in `ty run`"),
                }));
                function.instruction(&Instruction::LocalSet(self.scratch_ref_local));
                for element in tuple {
                    function.instruction(&Instruction::LocalGet(self.scratch_ref_local));
                    let element_ty = self.compile_expression(element, function)?;
                    match inferred_ty {
                        ValueType::TupleInt => {
                            require_same_type(ValueType::Int, element_ty, "tuple element")?;
                            function.instruction(&Instruction::Call(9));
                        }
                        ValueType::TupleString => {
                            require_same_type(ValueType::String, element_ty, "tuple element")?;
                            function.instruction(&Instruction::Call(29));
                        }
                        _ => bail!("Unsupported tuple literal type in `ty run`"),
                    }
                }
                function.instruction(&Instruction::LocalGet(self.scratch_ref_local));
                Ok(inferred_ty)
            }
            Expr::Dict(dict) => {
                function.instruction(&Instruction::Call(match inferred_ty {
                    ValueType::DictStrInt => 11,
                    ValueType::DictStrString => 24,
                    _ => bail!("Unsupported dictionary literal type in `ty run`"),
                }));
                function.instruction(&Instruction::LocalSet(self.scratch_ref_local));
                for item in dict {
                    let Some(key) = item.key.as_ref() else {
                        bail!("Dictionary unpacking is not supported in `ty run`");
                    };
                    function.instruction(&Instruction::LocalGet(self.scratch_ref_local));
                    let key_ty = self.compile_expression(key, function)?;
                    require_same_type(ValueType::String, key_ty, "dictionary key")?;
                    let value_ty = self.compile_expression(&item.value, function)?;
                    match inferred_ty {
                        ValueType::DictStrInt => {
                            require_same_type(ValueType::Int, value_ty, "dictionary value")?;
                            function.instruction(&Instruction::Call(12));
                        }
                        ValueType::DictStrString => {
                            require_same_type(ValueType::String, value_ty, "dictionary value")?;
                            function.instruction(&Instruction::Call(25));
                        }
                        _ => bail!("Unsupported dictionary literal type in `ty run`"),
                    }
                }
                function.instruction(&Instruction::LocalGet(self.scratch_ref_local));
                Ok(inferred_ty)
            }
            Expr::Subscript(subscript) => {
                let collection_ty = self.compile_expression(subscript.value.as_ref(), function)?;
                match collection_ty {
                    ValueType::ListInt => {
                        let index_ty =
                            self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::Int, index_ty, "list index")?;
                        function.instruction(&Instruction::Call(7));
                        require_same_type(ValueType::Int, inferred_ty, "list subscript")?;
                        Ok(inferred_ty)
                    }
                    ValueType::ListString => {
                        let index_ty =
                            self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::Int, index_ty, "list index")?;
                        function.instruction(&Instruction::Call(23));
                        require_same_type(ValueType::String, inferred_ty, "list subscript")?;
                        Ok(inferred_ty)
                    }
                    ValueType::ListObject => {
                        let index_ty =
                            self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::Int, index_ty, "list index")?;
                        function.instruction(&Instruction::Call(37));
                        require_same_type(ValueType::Object, inferred_ty, "list subscript")?;
                        Ok(inferred_ty)
                    }
                    ValueType::TupleInt => {
                        let index_ty =
                            self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::Int, index_ty, "tuple index")?;
                        function.instruction(&Instruction::Call(10));
                        require_same_type(ValueType::Int, inferred_ty, "tuple subscript")?;
                        Ok(inferred_ty)
                    }
                    ValueType::TupleString => {
                        let index_ty =
                            self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::Int, index_ty, "tuple index")?;
                        function.instruction(&Instruction::Call(30));
                        require_same_type(ValueType::String, inferred_ty, "tuple subscript")?;
                        Ok(inferred_ty)
                    }
                    ValueType::DictStrInt => {
                        let key_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::String, key_ty, "dictionary key")?;
                        function.instruction(&Instruction::Call(13));
                        require_same_type(ValueType::Int, inferred_ty, "dictionary subscript")?;
                        Ok(inferred_ty)
                    }
                    ValueType::DictStrString => {
                        let key_ty = self.compile_expression(subscript.slice.as_ref(), function)?;
                        require_same_type(ValueType::String, key_ty, "dictionary key")?;
                        function.instruction(&Instruction::Call(26));
                        require_same_type(ValueType::String, inferred_ty, "dictionary subscript")?;
                        Ok(inferred_ty)
                    }
                    _ => bail!("Unsupported subscript target in `ty run`"),
                }
            }
            Expr::Name(name) => {
                let local = self.local(name.id.as_str())?;
                function.instruction(&Instruction::LocalGet(local.index));
                Ok(local.ty)
            }
            Expr::Attribute(attribute) => {
                let owner_ty = self.compile_expression(attribute.value.as_ref(), function)?;
                require_same_type(ValueType::Object, owner_ty, "attribute owner")?;
                let key = self.intern_string(attribute.attr.as_str());
                function.instruction(&Instruction::I32Const(key));
                function.instruction(&Instruction::Call(3));
                function.instruction(&Instruction::Call(match inferred_ty {
                    ValueType::Float => 19,
                    ValueType::Int
                    | ValueType::Bool
                    | ValueType::String
                    | ValueType::ListInt
                    | ValueType::ListString
                    | ValueType::ListObject
                    | ValueType::TupleInt
                    | ValueType::TupleString
                    | ValueType::DictStrInt
                    | ValueType::DictStrString
                    | ValueType::Object => 17,
                }));
                Ok(inferred_ty)
            }
            Expr::UnaryOp(unary) => {
                let actual = self.compile_unary(unary, function)?;
                require_same_type(inferred_ty, actual, "unary expression")?;
                Ok(inferred_ty)
            }
            Expr::BinOp(binary) => {
                let actual = self.compile_binary(binary, function)?;
                require_same_type(inferred_ty, actual, "binary expression")?;
                Ok(inferred_ty)
            }
            Expr::Compare(compare) => {
                let actual = self.compile_compare(compare, function)?;
                require_same_type(inferred_ty, actual, "comparison expression")?;
                Ok(inferred_ty)
            }
            Expr::BoolOp(bool_op) => {
                let actual = self.compile_bool_op(bool_op, function)?;
                require_same_type(inferred_ty, actual, "boolean expression")?;
                Ok(inferred_ty)
            }
            Expr::Call(call) => {
                let actual = self.compile_call(call, function)?;
                require_same_type(inferred_ty, actual, "call expression")?;
                Ok(inferred_ty)
            }
            _ => bail!("Unsupported expression in `ty run`: `{expr:?}`"),
        }
    }

    fn compile_f_string(
        &self,
        f_string: &ast::ExprFString,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        let mut has_segment = false;
        for part in f_string.value.iter() {
            match part {
                ast::FStringPart::Literal(literal) => self.compile_f_string_literal(
                    literal.value.as_ref(),
                    function,
                    &mut has_segment,
                ),
                ast::FStringPart::FString(f_string) => {
                    for element in &f_string.elements {
                        match element {
                            ast::InterpolatedStringElement::Literal(literal) => self
                                .compile_f_string_literal(
                                    literal.value.as_ref(),
                                    function,
                                    &mut has_segment,
                                ),
                            ast::InterpolatedStringElement::Interpolation(interpolation) => self
                                .compile_f_string_interpolation(
                                    interpolation,
                                    function,
                                    &mut has_segment,
                                ),
                        }?;
                    }
                    Ok(())
                }
            }?;
        }

        if !has_segment {
            self.compile_f_string_literal("", function, &mut has_segment)?;
        }

        Ok(ValueType::String)
    }

    fn compile_f_string_literal(
        &self,
        literal: &str,
        function: &mut Function,
        has_segment: &mut bool,
    ) -> anyhow::Result<()> {
        if literal.is_empty() && *has_segment {
            return Ok(());
        }
        let index = self.intern_string(literal);
        function.instruction(&Instruction::I32Const(index));
        function.instruction(&Instruction::Call(3));
        self.concat_f_string_segment(function, has_segment);
        Ok(())
    }

    fn compile_f_string_interpolation(
        &self,
        interpolation: &ast::InterpolatedElement,
        function: &mut Function,
        has_segment: &mut bool,
    ) -> anyhow::Result<()> {
        if interpolation.debug_text.is_some()
            || interpolation.format_spec.is_some()
            || interpolation.conversion != ast::ConversionFlag::None
        {
            bail!(
                "`ty run` currently supports only plain f-string interpolations like `f\"value: {{value}}\"`"
            );
        }

        match self.compile_expression(interpolation.expression.as_ref(), function)? {
            ValueType::String => {}
            ValueType::Int => {
                function.instruction(&Instruction::Call(39));
            }
            ValueType::Float => {
                function.instruction(&Instruction::Call(40));
            }
            ValueType::Bool => {
                function.instruction(&Instruction::Call(41));
            }
            ValueType::ListInt
            | ValueType::ListString
            | ValueType::ListObject
            | ValueType::TupleInt
            | ValueType::TupleString
            | ValueType::DictStrInt
            | ValueType::DictStrString
            | ValueType::Object => {
                bail!(
                    "`ty run` f-string interpolations currently support only `str`, `int`, `bool`, and `float` values"
                )
            }
        }

        self.concat_f_string_segment(function, has_segment);
        Ok(())
    }

    fn concat_f_string_segment(&self, function: &mut Function, has_segment: &mut bool) {
        if *has_segment {
            function.instruction(&Instruction::Call(4));
        } else {
            *has_segment = true;
        }
    }

    fn compile_unary(
        &self,
        unary: &ast::ExprUnaryOp,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        match unary.op {
            UnaryOp::USub => {
                let ty = self.compile_expression(unary.operand.as_ref(), function)?;
                match ty {
                    ValueType::Int => {
                        function.instruction(&Instruction::I64Const(-1));
                        function.instruction(&Instruction::I64Mul);
                        Ok(ValueType::Int)
                    }
                    ValueType::Float => {
                        function.instruction(&Instruction::F64Neg);
                        Ok(ValueType::Float)
                    }
                    ValueType::Bool => bail!("Unary `-` is not supported for `bool` in `ty run`"),
                    ValueType::String => {
                        bail!("Unary `-` is not supported for `str` in `ty run`")
                    }
                    ValueType::ListInt
                    | ValueType::ListString
                    | ValueType::ListObject
                    | ValueType::TupleInt
                    | ValueType::TupleString
                    | ValueType::DictStrInt
                    | ValueType::DictStrString
                    | ValueType::Object => {
                        bail!("Unary `-` is not supported for collections in `ty run`")
                    }
                }
            }
            UnaryOp::Not => {
                let ty = self.compile_expression(unary.operand.as_ref(), function)?;
                match ty {
                    ValueType::Bool => {}
                    ValueType::String
                    | ValueType::ListInt
                    | ValueType::ListString
                    | ValueType::ListObject
                    | ValueType::TupleInt
                    | ValueType::TupleString
                    | ValueType::DictStrInt
                    | ValueType::DictStrString => {
                        function.instruction(&Instruction::Call(20));
                    }
                    ValueType::Int | ValueType::Float | ValueType::Object => {
                        bail!(
                            "`not` is currently supported for bools, strings, and collections in `ty run`"
                        )
                    }
                }
                function.instruction(&Instruction::I64Eqz);
                function.instruction(&Instruction::I64ExtendI32U);
                Ok(ValueType::Bool)
            }
            _ => bail!("Unsupported unary operator `{}` in `ty run`", unary.op),
        }
    }

    fn compile_binary(
        &self,
        binary: &ast::ExprBinOp,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        if binary.op == Operator::Div {
            let left = self.compile_expression(binary.left.as_ref(), function)?;
            match left {
                ValueType::Int => {
                    function.instruction(&Instruction::F64ConvertI64S);
                }
                ValueType::Float => {}
                _ => bail!("`/` currently requires numeric operands in `ty run`"),
            }
            let right = self.compile_expression(binary.right.as_ref(), function)?;
            match right {
                ValueType::Int => {
                    function.instruction(&Instruction::F64ConvertI64S);
                }
                ValueType::Float => {}
                _ => bail!("`/` currently requires numeric operands in `ty run`"),
            }
            function.instruction(&Instruction::F64Div);
            return Ok(ValueType::Float);
        }

        let left = self.compile_expression(binary.left.as_ref(), function)?;
        let right = self.compile_expression(binary.right.as_ref(), function)?;
        require_same_type(left, right, "binary arithmetic")?;
        match left {
            ValueType::Int => {
                self.compile_int_operator(binary.op, function)?;
                Ok(ValueType::Int)
            }
            ValueType::Float => {
                self.compile_float_operator(binary.op, function)?;
                Ok(ValueType::Float)
            }
            ValueType::String => {
                if binary.op != Operator::Add {
                    bail!("Strings only support `+` in `ty run`");
                }
                function.instruction(&Instruction::Call(4));
                Ok(ValueType::String)
            }
            ValueType::ListInt
            | ValueType::ListString
            | ValueType::ListObject
            | ValueType::TupleInt
            | ValueType::TupleString
            | ValueType::DictStrInt
            | ValueType::DictStrString
            | ValueType::Object => {
                bail!("Collections do not support arithmetic in `ty run`")
            }
            ValueType::Bool => bail!("Boolean values do not support arithmetic in `ty run`"),
        }
    }

    fn compile_int_operator(
        &self,
        operator: Operator,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        match operator {
            Operator::Add => {
                function.instruction(&Instruction::I64Add);
            }
            Operator::Sub => {
                function.instruction(&Instruction::I64Sub);
            }
            Operator::Mult => {
                function.instruction(&Instruction::I64Mul);
            }
            _ => bail!("Unsupported integer operator `{operator}` in `ty run`"),
        }
        Ok(())
    }

    fn compile_float_operator(
        &self,
        operator: Operator,
        function: &mut Function,
    ) -> anyhow::Result<()> {
        match operator {
            Operator::Add => {
                function.instruction(&Instruction::F64Add);
            }
            Operator::Sub => {
                function.instruction(&Instruction::F64Sub);
            }
            Operator::Mult => {
                function.instruction(&Instruction::F64Mul);
            }
            Operator::Div => {
                function.instruction(&Instruction::F64Div);
            }
            _ => bail!("Unsupported float operator `{operator}` in `ty run`"),
        }
        Ok(())
    }

    fn compile_compare(
        &self,
        compare: &ast::ExprCompare,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        let [operator] = compare.ops.as_ref() else {
            bail!("`ty run` only supports single comparisons");
        };
        let [right] = compare.comparators.as_ref() else {
            bail!("`ty run` only supports single comparisons");
        };

        let left_ty = self.compile_expression(compare.left.as_ref(), function)?;
        let right_ty = self.compile_expression(right, function)?;
        require_same_type(left_ty, right_ty, "comparison")?;

        match left_ty {
            ValueType::Int | ValueType::Bool => match operator {
                CmpOp::Eq => {
                    function.instruction(&Instruction::I64Eq);
                }
                CmpOp::NotEq => {
                    function.instruction(&Instruction::I64Ne);
                }
                CmpOp::Lt => {
                    function.instruction(&Instruction::I64LtS);
                }
                CmpOp::LtE => {
                    function.instruction(&Instruction::I64LeS);
                }
                CmpOp::Gt => {
                    function.instruction(&Instruction::I64GtS);
                }
                CmpOp::GtE => {
                    function.instruction(&Instruction::I64GeS);
                }
                _ => bail!("Unsupported comparison operator `{operator}` in `ty run`"),
            },
            ValueType::Float => match operator {
                CmpOp::Eq => {
                    function.instruction(&Instruction::F64Eq);
                }
                CmpOp::NotEq => {
                    function.instruction(&Instruction::F64Ne);
                }
                CmpOp::Lt => {
                    function.instruction(&Instruction::F64Lt);
                }
                CmpOp::LtE => {
                    function.instruction(&Instruction::F64Le);
                }
                CmpOp::Gt => {
                    function.instruction(&Instruction::F64Gt);
                }
                CmpOp::GtE => {
                    function.instruction(&Instruction::F64Ge);
                }
                _ => bail!("Unsupported comparison operator `{operator}` in `ty run`"),
            },
            ValueType::String => {
                let operator = match operator {
                    CmpOp::Eq => 0,
                    CmpOp::NotEq => 1,
                    CmpOp::Lt => 2,
                    CmpOp::LtE => 3,
                    CmpOp::Gt => 4,
                    CmpOp::GtE => 5,
                    _ => bail!("Unsupported comparison operator `{operator}` in `ty run`"),
                };
                function.instruction(&Instruction::I64Const(operator));
                function.instruction(&Instruction::Call(34));
            }
            ValueType::ListInt
            | ValueType::ListString
            | ValueType::ListObject
            | ValueType::TupleInt
            | ValueType::TupleString
            | ValueType::DictStrInt
            | ValueType::DictStrString
            | ValueType::Object => {
                bail!("Collection comparisons are not supported in `ty run` yet")
            }
        }
        function.instruction(&Instruction::I64ExtendI32U);
        Ok(ValueType::Bool)
    }

    fn compile_bool_op(
        &self,
        bool_op: &ast::ExprBoolOp,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        if bool_op.values.len() != 2 {
            bail!("`ty run` currently supports two-term boolean expressions only");
        }
        let left = self.compile_expression(&bool_op.values[0], function)?;
        let right = self.compile_expression(&bool_op.values[1], function)?;
        require_same_type(ValueType::Bool, left, "boolean expression")?;
        require_same_type(ValueType::Bool, right, "boolean expression")?;
        match bool_op.op {
            BoolOp::And => {
                function.instruction(&Instruction::I64And);
            }
            BoolOp::Or => {
                function.instruction(&Instruction::I64Or);
            }
        }
        Ok(ValueType::Bool)
    }

    fn compile_call(
        &self,
        call: &ast::ExprCall,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        if let Expr::Attribute(attribute) = call.func.as_ref() {
            let class_name = self.receiver_class_name(attribute.value.as_ref())?;
            let key = format!("{class_name}.{}", attribute.attr);
            let signature = self
                .signatures
                .get(&key)
                .with_context(|| format!("Unknown compiled method `{key}` in `ty run`"))?;
            let Some((receiver, parameters)) = signature.parameters.split_first() else {
                bail!("Compiled method `{key}` is missing its receiver parameter");
            };
            let arguments =
                ordered_call_arguments(call, parameters, &format!("method `{key}` call"))?;

            let receiver_ty = self.compile_expression(attribute.value.as_ref(), function)?;
            require_same_type(receiver.ty, receiver_ty, "method receiver")?;
            for (argument, parameter) in arguments.into_iter().zip(parameters) {
                let actual = self.compile_call_argument(argument, function)?;
                require_same_type(parameter.ty, actual, "method argument")?;
            }
            function.instruction(&Instruction::Call(signature.index));
            return Ok(signature.result);
        }

        let Expr::Name(function_name) = call.func.as_ref() else {
            bail!("Only direct calls to compiled functions are supported in `ty run` expressions");
        };
        if function_name.id.as_str() == "print" {
            bail!("`print(...)` is only supported as a statement in `ty run`");
        }
        if function_name.id.as_str() == "len" {
            let [argument] = &*call.arguments.args else {
                bail!("`len(...)` expects exactly one positional argument in `ty run`");
            };
            let actual = self.compile_expression(argument, function)?;
            match actual {
                ValueType::String
                | ValueType::ListInt
                | ValueType::ListString
                | ValueType::ListObject
                | ValueType::TupleInt
                | ValueType::TupleString
                | ValueType::DictStrInt
                | ValueType::DictStrString => {
                    function.instruction(&Instruction::Call(20));
                    return Ok(ValueType::Int);
                }
                _ => bail!(
                    "`len(...)` is only supported for strings and supported collections in `ty run`"
                ),
            }
        }
        if function_name.id.as_str() == "read_text" {
            let [argument] = &*call.arguments.args else {
                bail!("`read_text(...)` expects exactly one positional argument in `ty run`");
            };
            let actual = self.compile_expression(argument, function)?;
            require_same_type(ValueType::String, actual, "read_text argument")?;
            self.uses_filesystem.set(true);
            function.instruction(&Instruction::Call(27));
            return Ok(ValueType::String);
        }

        if let Some(class) = self.classes.get(function_name.id.as_str()) {
            let arguments = ordered_call_arguments(
                call,
                &class.parameters,
                &format!("class `{}` constructor call", function_name.id),
            )?;

            function.instruction(&Instruction::Call(15));
            function.instruction(&Instruction::LocalSet(self.scratch_ref_local));
            for field in &class.fields {
                let argument = arguments
                    .get(field.parameter_index)
                    .context("Class field references a missing constructor argument")?;
                let parameter = class
                    .parameters
                    .get(field.parameter_index)
                    .context("Class field references a missing constructor parameter")?;
                function.instruction(&Instruction::LocalGet(self.scratch_ref_local));
                let key = self.intern_string(&field.name);
                function.instruction(&Instruction::I32Const(key));
                function.instruction(&Instruction::Call(3));
                let actual = self.compile_call_argument(*argument, function)?;
                require_same_type(parameter.ty, actual, "constructor argument")?;
                require_same_type(parameter.ty, field.ty, "constructor field")?;
                function.instruction(&Instruction::Call(match field.ty {
                    ValueType::Float => 18,
                    ValueType::Int
                    | ValueType::Bool
                    | ValueType::String
                    | ValueType::ListInt
                    | ValueType::ListString
                    | ValueType::ListObject
                    | ValueType::TupleInt
                    | ValueType::TupleString
                    | ValueType::DictStrInt
                    | ValueType::DictStrString
                    | ValueType::Object => 16,
                }));
            }
            function.instruction(&Instruction::LocalGet(self.scratch_ref_local));
            return Ok(ValueType::Object);
        }

        let signature = self
            .signatures
            .get(function_name.id.as_str())
            .with_context(|| {
                format!(
                    "Unknown compiled function `{}` in `ty run`",
                    function_name.id
                )
            })?;
        let arguments = ordered_call_arguments(
            call,
            &signature.parameters,
            &format!("function `{}` call", function_name.id),
        )?;
        for (argument, parameter) in arguments.into_iter().zip(&signature.parameters) {
            let actual = self.compile_call_argument(argument, function)?;
            require_same_type(parameter.ty, actual, "function argument")?;
        }
        function.instruction(&Instruction::Call(signature.index));
        Ok(signature.result)
    }

    fn local(&self, name: &str) -> anyhow::Result<Local> {
        self.locals.get(name).copied().with_context(|| {
            format!(
                "Local `{name}` must be initialized before use so `ty run` can infer a supported type"
            )
        })
    }

    fn compile_call_argument(
        &self,
        argument: CallArgument<'_>,
        function: &mut Function,
    ) -> anyhow::Result<ValueType> {
        match argument {
            CallArgument::Expr(expr) => self.compile_expression(expr, function),
            CallArgument::Default(default) => match default {
                DefaultValue::Int(value) => {
                    function.instruction(&Instruction::I64Const(*value));
                    Ok(ValueType::Int)
                }
                DefaultValue::Bool(value) => {
                    function.instruction(&Instruction::I64Const(i64::from(*value)));
                    Ok(ValueType::Bool)
                }
                DefaultValue::Float(value) => {
                    function.instruction(&Instruction::F64Const((*value).into()));
                    Ok(ValueType::Float)
                }
                DefaultValue::String(value) => {
                    let index = self.intern_string(value);
                    function.instruction(&Instruction::I32Const(index));
                    function.instruction(&Instruction::Call(3));
                    Ok(ValueType::String)
                }
            },
        }
    }

    fn expression_type(&self, expr: &Expr, context: &str) -> anyhow::Result<ValueType> {
        let ty = expr.inferred_type(self.model).with_context(|| {
            format!("ty did not infer a type for {context} `{expr:?}` in `ty run`")
        })?;
        lower_ty(ty, self.model, context)
    }

    fn assignment_initializer_type(&self, expr: &Expr) -> anyhow::Result<ValueType> {
        match expr {
            Expr::List(list) => self.list_literal_type(list),
            Expr::Dict(dict) => self.dict_literal_type(dict),
            _ => self.expression_type(expr, "assignment initializer"),
        }
    }

    fn iterable_type(&self, expr: &Expr) -> anyhow::Result<ValueType> {
        match expr {
            Expr::Name(name) => Ok(self.local(name.id.as_str())?.ty),
            Expr::List(list) => self.list_literal_type(list),
            Expr::Dict(dict) => self.dict_literal_type(dict),
            _ => self.expression_type(expr, "`for` iterable"),
        }
    }

    fn list_literal_type(&self, list: &ast::ExprList) -> anyhow::Result<ValueType> {
        let mut element_ty = None;
        for element in list {
            let actual = self.expression_type(element, "list element")?;
            match actual {
                ValueType::Int | ValueType::String | ValueType::Object => {}
                _ => bail!(
                    "`ty run` currently supports only `list[int]`, `list[str]`, and `list[object]` literals"
                ),
            }
            if let Some(expected) = element_ty {
                require_same_type(expected, actual, "list literal element")?;
            } else {
                element_ty = Some(actual);
            }
        }
        Ok(match element_ty.unwrap_or(ValueType::Int) {
            ValueType::Int => ValueType::ListInt,
            ValueType::String => ValueType::ListString,
            ValueType::Object => ValueType::ListObject,
            _ => bail!("Unsupported list literal element type in `ty run`"),
        })
    }

    fn dict_literal_type(&self, dict: &ast::ExprDict) -> anyhow::Result<ValueType> {
        let mut value_ty = None;
        for item in dict {
            let Some(key) = item.key.as_ref() else {
                bail!("Dictionary unpacking is not supported in `ty run`");
            };
            let key_ty = self.expression_type(key, "dictionary key")?;
            require_same_type(ValueType::String, key_ty, "dictionary key")?;
            let actual = self.expression_type(&item.value, "dictionary value")?;
            match actual {
                ValueType::Int | ValueType::String => {}
                _ => bail!(
                    "`ty run` currently supports only `dict[str, int]` and `dict[str, str]` literals"
                ),
            }
            if let Some(expected) = value_ty {
                require_same_type(expected, actual, "dictionary literal value")?;
            } else {
                value_ty = Some(actual);
            }
        }
        Ok(match value_ty.unwrap_or(ValueType::Int) {
            ValueType::Int => ValueType::DictStrInt,
            ValueType::String => ValueType::DictStrString,
            _ => bail!("Unsupported dictionary literal value type in `ty run`"),
        })
    }

    fn intern_string(&self, value: &str) -> i32 {
        let mut strings = self.strings.borrow_mut();
        let index = i32::try_from(strings.len()).expect("string constant index fits in i32");
        strings.push(value.to_string());
        index
    }

    fn receiver_class_name(&self, expr: &Expr) -> anyhow::Result<String> {
        let ty = expr.inferred_type(self.model).with_context(|| {
            format!("ty did not infer a receiver type for method call `{expr:?}`")
        })?;
        match ty {
            Type::NominalInstance(instance) => Ok(instance.class_name(self.model.db()).to_string()),
            _ => bail!("`ty run` only supports method calls on statically-known class instances"),
        }
    }

    fn compiled_call_result_type(&self, call: &ast::ExprCall) -> anyhow::Result<Option<ValueType>> {
        match call.func.as_ref() {
            Expr::Name(function_name) => Ok(self
                .signatures
                .get(function_name.id.as_str())
                .map(|signature| signature.result)),
            Expr::Attribute(attribute) => {
                let class_name = self.receiver_class_name(attribute.value.as_ref())?;
                let key = format!("{class_name}.{}", attribute.attr);
                Ok(self.signatures.get(&key).map(|signature| signature.result))
            }
            _ => Ok(None),
        }
    }
}

fn annotation_type(annotation: &Expr) -> anyhow::Result<ValueType> {
    match annotation {
        Expr::Name(name) => match name.id.as_str() {
            "int" => Ok(ValueType::Int),
            "bool" => Ok(ValueType::Bool),
            "float" => Ok(ValueType::Float),
            "str" => Ok(ValueType::String),
            _ => Ok(ValueType::Object),
        },
        Expr::Subscript(subscript) => {
            let Expr::Name(container) = subscript.value.as_ref() else {
                bail!("Unsupported collection annotation in `ty run`");
            };
            match container.id.as_str() {
                "list" if annotation_is_name(subscript.slice.as_ref(), "int") => {
                    Ok(ValueType::ListInt)
                }
                "list" if annotation_is_name(subscript.slice.as_ref(), "str") => {
                    Ok(ValueType::ListString)
                }
                "list" if annotation_type(subscript.slice.as_ref())? == ValueType::Object => {
                    Ok(ValueType::ListObject)
                }
                "tuple" if annotation_is_int_tuple(subscript.slice.as_ref()) => {
                    Ok(ValueType::TupleInt)
                }
                "tuple" if annotation_is_str_tuple(subscript.slice.as_ref()) => {
                    Ok(ValueType::TupleString)
                }
                "dict" if annotation_is_str_int_pair(subscript.slice.as_ref()) => {
                    Ok(ValueType::DictStrInt)
                }
                "dict" if annotation_is_str_str_pair(subscript.slice.as_ref()) => {
                    Ok(ValueType::DictStrString)
                }
                _ => bail!(
                    "`ty run` currently supports `list[int]`, `list[str]`, `list[object]`, integer/string `tuple[...]`, `dict[str, int]`, and `dict[str, str]` collection annotations"
                ),
            }
        }
        _ => bail!(
            "`ty run` only supports scalar, class-name, `list[int]`, `list[str]`, `list[object]`, integer/string `tuple[...]`, `dict[str, int]`, and `dict[str, str]` annotations"
        ),
    }
}

fn annotation_is_name(expr: &Expr, expected: &str) -> bool {
    matches!(expr, Expr::Name(name) if name.id.as_str() == expected)
}

fn annotation_is_int_tuple(expr: &Expr) -> bool {
    matches!(expr, Expr::Tuple(tuple) if !tuple.is_empty() && tuple.iter().all(|expr| annotation_is_name(expr, "int")))
}

fn annotation_is_str_tuple(expr: &Expr) -> bool {
    matches!(expr, Expr::Tuple(tuple) if !tuple.is_empty() && tuple.iter().all(|expr| annotation_is_name(expr, "str")))
}

fn annotation_is_str_int_pair(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Tuple(tuple)
            if tuple.len() == 2
                && annotation_is_name(&tuple.elts[0], "str")
                && annotation_is_name(&tuple.elts[1], "int")
    )
}

fn annotation_is_str_str_pair(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Tuple(tuple)
            if tuple.len() == 2
                && annotation_is_name(&tuple.elts[0], "str")
                && annotation_is_name(&tuple.elts[1], "str")
    )
}

fn lower_ty(ty: Type<'_>, model: &SemanticModel<'_>, context: &str) -> anyhow::Result<ValueType> {
    let db = model.db();
    if ty.has_dynamic(db) {
        bail!("`ty run` cannot compile dynamic type information for {context}");
    }
    if let Type::Union(union) = ty {
        let mut saw_int = false;
        let mut saw_bool = false;
        let mut saw_float = false;
        let mut saw_string = false;
        let mut saw_collection = false;
        let mut saw_object = false;

        for element in union.elements(db) {
            match lower_ty(*element, model, context)? {
                ValueType::Int => saw_int = true,
                ValueType::Bool => saw_bool = true,
                ValueType::Float => saw_float = true,
                ValueType::String => saw_string = true,
                ValueType::ListInt
                | ValueType::ListString
                | ValueType::ListObject
                | ValueType::TupleInt
                | ValueType::TupleString
                | ValueType::DictStrInt
                | ValueType::DictStrString => saw_collection = true,
                ValueType::Object => saw_object = true,
            }
        }

        return match (
            saw_int,
            saw_bool,
            saw_float,
            saw_string,
            saw_collection,
            saw_object,
        ) {
            (true, false, true, false, false, false) => Ok(ValueType::Float),
            (true, false, false, false, false, false) => Ok(ValueType::Int),
            (false, true, false, false, false, false) => Ok(ValueType::Bool),
            (false, false, true, false, false, false) => Ok(ValueType::Float),
            (false, false, false, true, false, false) => Ok(ValueType::String),
            (false, false, false, false, false, true) => Ok(ValueType::Object),
            _ => bail!("`ty run` cannot lower mixed inferred union type `{ty:?}` for {context}"),
        };
    }
    if ty.is_assignable_to(db, KnownClass::Bool.to_instance(db)) {
        return Ok(ValueType::Bool);
    }
    if ty.is_assignable_to(db, KnownClass::Int.to_instance(db)) {
        return Ok(ValueType::Int);
    }
    if ty.is_assignable_to(db, KnownClass::Float.to_instance(db)) {
        return Ok(ValueType::Float);
    }
    if ty.is_assignable_to(db, KnownClass::Str.to_instance(db)) {
        return Ok(ValueType::String);
    }
    if ty.is_assignable_to(db, KnownClass::List.to_instance(db)) {
        let element_ty = ty
            .iterable_homogeneous_element_type(db)
            .context("`ty run` could not determine the element type for a list")?;
        return match lower_ty(element_ty, model, "list element")? {
            ValueType::Int => Ok(ValueType::ListInt),
            ValueType::String => Ok(ValueType::ListString),
            ValueType::Object => Ok(ValueType::ListObject),
            _ => {
                bail!("`ty run` currently supports only integer, string, and object list elements")
            }
        };
    }
    if let Some(element_ty) = ty.tuple_instance_homogeneous_element_type(db) {
        return match lower_ty(element_ty, model, "tuple element")? {
            ValueType::Int => Ok(ValueType::TupleInt),
            ValueType::String => Ok(ValueType::TupleString),
            _ => bail!("`ty run` currently supports only integer and string tuple elements"),
        };
    }
    if ty.is_assignable_to(db, KnownClass::Dict.to_instance(db)) {
        return Ok(ValueType::DictStrInt);
    }
    if matches!(ty, Type::NominalInstance(_)) {
        return Ok(ValueType::Object);
    }
    bail!("`ty run` cannot lower ty's inferred type `{ty:?}` for {context}")
}

fn require_same_type(expected: ValueType, actual: ValueType, context: &str) -> anyhow::Result<()> {
    if expected == actual {
        return Ok(());
    }
    bail!(
        "Type mismatch in {context}: expected `{}`, found `{}`",
        expected.name(),
        actual.name()
    )
}
