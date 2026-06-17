//! Module loader and import resolver for the Forge driver.
//!
//! The first milestone supports a tiny module system:
//!
//!   - `import std.io` imports all public names from `core/io.dev` unqualified.
//!   - `import std.io as io` is accepted but the alias is not yet enforced;
//!     names are still imported unqualified.
//!   - `from std.io import puts, putchar` imports selected public names.
//!   - `from std.io import *` imports all public names.
//!
//! Only `std.*` modules are resolved in this milestone.  They are located by
//! walking up from the entry source directory looking for a `core/` directory
//! containing `<name>.dev`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::ast::{Import, Item, Module, Visibility};
use crate::parser::parse_module;

/// A loaded entry module plus all modules it transitively imports.
pub struct ModuleGraph {
    pub entry: Module,
    pub modules: HashMap<Vec<String>, Module>,
}

/// Parse `entry_source` (located at `entry_path`), then recursively load every
/// module imported by the entry module.
pub fn load_modules(entry_source: &str, entry_path: &Path) -> Result<ModuleGraph> {
    let entry = parse_module(entry_source)
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
        let module = parse_module(&src)
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
/// All public items from every loaded module are cloned into the merged module
/// so that the lowerer can resolve them in a single namespace.  Private items
/// are also merged so that imported wrappers can call their internal helpers.
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
            let name = item_name(item);
            if defined.contains(&name) {
                bail!(
                    "name conflict: `{}` from {} conflicts with an existing definition",
                    name,
                    path.join(".")
                );
            }
            defined.insert(name.clone());
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

    bail!(
        "only `std.*` modules are supported in the first milestone (got `{}`)",
        path.join(".")
    );
}

fn collect_names(module: &Module) -> HashSet<String> {
    module.items.iter().map(item_name).collect()
}

fn public_names(module: &Module) -> HashSet<String> {
    module
        .items
        .iter()
        .filter(|i| is_public(i))
        .map(|i| item_name(i))
        .collect()
}

fn is_public(item: &Item) -> bool {
    match item {
        Item::Function(f) => f.vis == Visibility::Public,
        Item::Struct(s) => s.vis == Visibility::Public,
        Item::Union(u) => u.vis == Visibility::Public,
        Item::Enum(e) => e.vis == Visibility::Public,
        Item::Const(c) => c.vis == Visibility::Public,
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
        Item::Use(u) => u
            .alias
            .clone()
            .unwrap_or_else(|| u.path.last().cloned().unwrap_or_default()),
        Item::Impl(_) => String::new(),
    }
}
