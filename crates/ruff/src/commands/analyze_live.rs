use crate::args::{AnalyzeGraphArgs, AnalyzeLiveArgs, ConfigArguments};
use crate::commands;
use crate::ExitStatus;
use anyhow::Result;
use log::warn;
use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::EventKind::{Create, Modify, Remove};
use notify::{Event, RecursiveMode, Result as WatcherResult, Watcher};
use ruff_db::system::SystemPathBuf;
use ruff_graph::{Direction, ImportMap};
use std::collections::{HashSet, VecDeque};
use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

pub(crate) fn analyze_live(
    args: AnalyzeLiveArgs,
    config_arguments: &ConfigArguments,
) -> Result<ExitStatus> {
    let cmd_name = OsStr::new(args.cmd.first().expect("Command must be provided"));
    let cmd_args = &args.cmd[1..];
    let cwd = env::current_dir()?;

    let (tx, rx) = mpsc::channel::<WatcherResult<Event>>();
    let mut watcher = notify::recommended_watcher(tx)?;

    // maintaining both a dependents graph and dependency graph since:
    // * the dependents graph directly powers the basic functionality
    // * the dependency graph allows us to monitor which edges were removed in a
    //   file change without traversing the entire graph
    let mut import_map_dependents = commands::analyze_graph::generate_import_map(
        AnalyzeGraphArgs {
            files: vec![PathBuf::from(".")],
            direction: Direction::Dependents,
        },
        &config_arguments,
    )?;

    let mut import_map_dependencies = commands::analyze_graph::generate_import_map(
        AnalyzeGraphArgs {
            files: vec![PathBuf::from(".")],
            direction: Direction::Dependencies,
        },
        &config_arguments,
    )?;

    watcher.watch(Path::new("."), RecursiveMode::Recursive)?;
    for res in rx {
        let _ = match res {
            Ok(event) => match event.kind {
                Modify(ModifyKind::Name(_))
                | Modify(ModifyKind::Data(_))
                | Create(CreateKind::File)
                | Remove(RemoveKind::File) => {
                    // we only want to rerun analyze on files that changed, specifically either
                    // files already tracked by the import map, or if they're python files, or
                    // if it's a project/ruff configuration
                    let changed_paths = event
                        .paths
                        .into_iter()
                        .filter(|p| {
                            let sp = p.to_str().unwrap();
                            // a non-python file might be a dependent explicitly declared
                            // `include-dependencies`; if so, we want to track its changes
                            import_map_dependents.contains_key(&SystemPathBuf::from(sp))
                                // there might be a new python file
                                || sp.ends_with(".py")
                                // or a change to the config itself
                                || sp.ends_with("ruff.toml")
                                || sp.ends_with(".ruff.toml")
                                || sp.ends_with("pyproject.toml")
                        })
                        .map(|p| p.strip_prefix(&cwd).map(PathBuf::from).unwrap())
                        .collect::<Vec<PathBuf>>();

                    if changed_paths.is_empty() {
                        continue;
                    }

                    warn!("changed paths: {:?}", changed_paths);

                    // if a file has been removed, first find the impacted files before changing the
                    // import map and losing that information; otherwise, we update the graph first -
                    // even if there are removed edges, we can still evaluate with the updated graph
                    // because for a file to be impacted by it, there must be some file in its path
                    // (possibly itself) that was modified, which will still trigger it
                    if event.kind != Remove(RemoveKind::Any) {
                        // TODO: if config file changed, reconstruct entire graph; this could be
                        // optimized by just adding new edges from include-dependencies, but
                        // in pathological cases, `src` and such might be modified as well
                        let import_map_dependencies_update =
                            match commands::analyze_graph::generate_import_map(
                                AnalyzeGraphArgs {
                                    files: changed_paths.clone(),
                                    // when a file is changed, only its dependencies might change
                                    // so this is sufficient to update our view of the graph
                                    direction: Direction::Dependencies,
                                },
                                &config_arguments,
                            ) {
                                Ok(new_import_map) => new_import_map,
                                Err(_) => continue,
                            };

                        for (path, new_dependencies) in import_map_dependencies_update.iter() {
                            let old_dependencies = import_map_dependencies
                                .insert(path.clone(), new_dependencies.clone());
                            // handle removed edges
                            if old_dependencies.is_some() {
                                for m in old_dependencies.unwrap().difference(new_dependencies) {
                                    if import_map_dependents.contains_key(m) {
                                        import_map_dependents
                                            .entry(m.clone())
                                            .and_modify(|curr| curr.remove(&path));
                                    }
                                }
                            }
                            // add new edges
                            for m in new_dependencies.iter() {
                                let values = import_map_dependents.entry(m.clone()).or_default();
                                values.insert(path.clone());
                            }
                        }
                    }

                    let affected_files = get_affected_files(&changed_paths, &import_map_dependents)
                        .into_iter()
                        .filter(|p| {
                            let sp = p.to_str();
                            sp.is_some()
                                && import_map_dependents
                                    .contains_key(&SystemPathBuf::from(sp.unwrap()))
                                && args.paths.iter().any(|args_path| p.starts_with(args_path))
                        })
                        .collect::<Vec<PathBuf>>();

                    if event.kind == Remove(RemoveKind::File) {
                        // remove node and all edges to it in both graphs
                        for p in changed_paths.into_iter() {
                            let spb = SystemPathBuf::from_path_buf(p).unwrap();
                            let _ = import_map_dependents.remove(&spb);
                            let old_dependencies = import_map_dependencies.remove(&spb);
                            if old_dependencies.is_some() {
                                for m in old_dependencies.unwrap().iter() {
                                    import_map_dependents
                                        .entry(m.clone())
                                        .and_modify(|curr| curr.remove(&spb));
                                }
                            }
                        }
                    }

                    if affected_files.is_empty() {
                        warn!("Nothing to do!");
                        continue;
                    }

                    warn!("transitively affected files: {:?}", affected_files);
                    Command::new(cmd_name)
                        .args(cmd_args)
                        .args(affected_files.into_iter().map(|p| p.into_os_string()))
                        .status()
                        .expect("failed to execute process");
                }
                _ => continue,
            },
            Err(_) => continue,
        };
    }

    return Ok(ExitStatus::Success);
}

fn get_affected_files(
    modified_files: &Vec<PathBuf>,
    import_map_dependents: &ImportMap,
) -> HashSet<PathBuf> {
    // run a plain BFS of the dependents graph; all visited nodes are affected files
    let mut visited: HashSet<PathBuf> = HashSet::new();
    let mut queue: VecDeque<PathBuf> = VecDeque::new();
    visited.extend(modified_files.clone());
    queue.extend(modified_files.clone());
    while let Some(file) = queue.pop_front() {
        let Ok(module_imports) =
            SystemPathBuf::from_path_buf(file).map(|p| import_map_dependents.get(&p))
        else {
            warn!("Failed to convert to system path");
            continue;
        };
        match module_imports {
            Some(mi) => {
                for dependent_file in mi.iter() {
                    if visited.insert(dependent_file.clone().into_std_path_buf()) {
                        queue.push_back(dependent_file.clone().into_std_path_buf());
                    }
                }
            }
            None => continue,
        }
    }

    visited
}
