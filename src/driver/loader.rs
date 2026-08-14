//! Module loader and import resolver for the Forge driver.
//!
//! The module system supports two kinds of imports:
//!
//!   - **Standard library modules**: `import std.io` or `from std.io import puts, putchar`.
//!     These resolve to `core/<name>.dev` by walking up from the entry source
//!     directory looking for a `core/` directory containing `<name>.dev`.
//!     Both `pub` and private items are merged so that wrapper functions in
//!     stdlib modules can call internal helpers (e.g. `puts` calls `_dev_puts`).
//!
//!   - **User-defined modules**: `import mymod` or `from mymod import helper`.
//!     These resolve to `<name>.dev` (or `<name>/<sub>.dev` for dotted paths)
//!     by walking up from the entry source directory.  All items — including
//!     non-`pub` ones — are merged into the entry module's namespace, and name
//!     conflicts across modules are reported as errors.
//!
//! The `import std.io as io` and `from std.io import *` forms are also accepted.
//! All resolved modules are recursively loaded and merged into a single module
//! before semantic analysis and lowering.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::ast::{Import, Item, Module, Visibility};
use crate::parser::parse_module_in_dir;

/// A loaded entry module plus all modules it transitively imports.
pub struct ModuleGraph {
    pub entry: Module,
    pub modules: HashMap<Vec<String>, Module>,
}

/// Parse `entry_source` (located at `entry_path`), then recursively load every
/// module imported by the entry module.
pub fn load_modules(entry_source: &str, entry_path: &Path) -> Result<ModuleGraph> {
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let entry = parse_module_in_dir(entry_source, &entry_dir)
        .map_err(|e| anyhow::anyhow!("parse error in {}: {}", entry_path.display(), e))?;

    let mut graph = ModuleGraph {
        entry,
        modules: HashMap::new(),
    };

    let mut pending: Vec<Vec<String>> = Vec::new();
    let mut visited: HashSet<Vec<String>> = HashSet::new();

    for imp in &graph.entry.imports {
        let path = import_path(imp);
        if visited.insert(path.clone()) {
            pending.push(path);
        }
    }

    while let Some(path) = pending.pop() {
        let file = resolve_module_path(&path, entry_path)?;
        let src = std::fs::read_to_string(&file)
            .with_context(|| format!("reading module {} at {}", path.join("."), file.display()))?;
        let dir = file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let module = parse_module_in_dir(&src, &dir)
            .map_err(|e| anyhow::anyhow!("parse error in {}: {}", file.display(), e))?;

        for imp in &module.imports {
            let dep_path = import_path(imp);
            if visited.insert(dep_path.clone()) {
                pending.push(dep_path);
            }
        }

        graph.modules.insert(path, module);
    }

    Ok(graph)
}

/// Merge the entry module and all transitively imported modules into a single
/// module.
///
/// All items from every loaded module are cloned into the merged module so that
/// the lowerer and codegen can resolve them in a single namespace.  This
/// includes private items (e.g. the stdlib's `_dev_*` externs that are called by
/// public wrappers like `puts`).  Name conflicts across modules are detected
/// and reported as errors — users should give each module's non-`pub` items
/// unique names to avoid conflicts.
pub fn merge_modules(graph: ModuleGraph) -> Result<Module> {
    let mut merged = Module {
        package: graph.entry.package.clone(),
        imports: Vec::new(),
        items: graph.entry.items.clone(),
    };

    // Track names already defined in the merged module to report conflicts.
    let mut defined: HashSet<String> = collect_names(&graph.entry);

    for (path, dep) in &graph.modules {
        for item in &dep.items {
            let names = defined_names(item);
            for name in &names {
                if defined.contains(name) {
                    bail!(
                        "name conflict: `{}` from {} conflicts with an existing definition",
                        name,
                        path.join(".")
                    );
                }
                defined.insert(name.clone());
            }
            merged.items.push(item.clone());
        }
    }

    Ok(merged)
}

fn import_path(imp: &Import) -> Vec<String> {
    match imp {
        Import::Path { path, .. } => path.clone(),
        Import::From { path, .. } => path.clone(),
    }
}

fn resolve_module_path(path: &[String], entry_path: &Path) -> Result<PathBuf> {
    if path.is_empty() {
        bail!("empty import path");
    }

    if path[0] == "std" {
        let name = path[1..].join("/");
        let mut dir = entry_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        loop {
            let candidate = dir.join("core").join(&name).with_extension("dev");
            if candidate.is_file() {
                return Ok(candidate);
            }
            if !dir.pop() {
                break;
            }
        }
        bail!(
            "cannot resolve std module `{}`: core/{}.dev not found near {}",
            path.join("."),
            name,
            entry_path.display()
        );
    }

    resolve_local_module(path, entry_path)
}

/// Resolve a user-defined (non-`std`) module path to a `.dev` file by walking
/// up the directory tree from the entry file, mirroring the stdlib search.
fn resolve_local_module(path: &[String], entry_path: &Path) -> Result<PathBuf> {
    let name = path.join("/");
    let mut dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    loop {
        let candidate = dir.join(&name).with_extension("dev");
        if candidate.is_file() {
            return Ok(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    bail!(
        "cannot resolve module `{}`: {}.dev not found near {}",
        name,
        name,
        entry_path.display()
    );
}

fn collect_names(module: &Module) -> HashSet<String> {
    module.items.iter().flat_map(defined_names).collect()
}

#[allow(dead_code)]
fn public_names(module: &Module) -> HashSet<String> {
    module
        .items
        .iter()
        .filter(|i| is_public(i))
        .map(item_name)
        .collect()
}

#[allow(dead_code)]
fn is_public(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.vis == Visibility::Public,
        Item::Struct(s) => s.vis == Visibility::Public,
        Item::Union(u) => u.vis == Visibility::Public,
        Item::Enum(e) => e.vis == Visibility::Public,
        Item::Const(c) => c.vis == Visibility::Public,
        Item::Embed(e) => e.vis == Visibility::Public,
        // Extern functions and use declarations are considered public for
        // import purposes; visibility is not modeled for them yet.
        Item::ExternFn(_) | Item::Use(_) => true,
        Item::Impl(_) => true,
    }
}

fn item_name(item: &Item) -> String {
    match item {
        Item::Function(f) => f.name.clone(),
        Item::Struct(s) => s.name.clone(),
        Item::Union(u) => u.name.clone(),
        Item::Enum(e) => e.name.clone(),
        Item::ExternFn(e) => e.name.clone(),
        Item::Const(c) => c.name.clone(),
        Item::Embed(e) => e.name.clone(),
        Item::Use(u) => u
            .alias
            .clone()
            .unwrap_or_else(|| u.path.last().cloned().unwrap_or_default()),
        Item::Impl(_) => String::new(),
    }
}

/// Names an item brings into the merged namespace. An `embed` binds both the
/// data symbol `NAME` and its implicit length constant `NAME_LEN`.
fn defined_names(item: &Item) -> Vec<String> {
    match item {
        Item::Embed(e) => vec![e.name.clone(), format!("{}_LEN", e.name)],
        other => vec![item_name(other)],
    }
}
