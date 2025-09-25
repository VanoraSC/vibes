use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let proto_root = PathBuf::from("protobuf_definitions");
    let generated_root = PathBuf::from("generated_rust");

    println!("cargo:rerun-if-changed={}", proto_root.display());

    let mut proto_files = Vec::new();
    collect_proto_files(&proto_root, &mut proto_files)?;
    proto_files.sort();

    fs::create_dir_all(&generated_root)?;

    clean_generated_directory(&generated_root)?;

    let mod_file_path = generated_root.join("generated_mod.rs");

    if proto_files.is_empty() {
        fs::write(
            &mod_file_path,
            "// This file is generated automatically and intentionally left blank because no .proto files were found.\n",
        )?;
        return Ok(());
    }

    // First generate types with prost into `generated_rust`, mirroring the
    // protobuf directory hierarchy. For example a proto at
    // `protobuf_definitions/echo/message.proto` will emit Rust into
    // `generated_rust/echo/message.rs`.
    let mut config = prost_build::Config::new();
    for proto in &proto_files {
        if let Ok(rel_path) = proto.strip_prefix(&proto_root) {
            // Determine target directory for this proto's generated code.
            let parent = rel_path.parent();
            if let Some(parent) = parent {
                let target_dir = generated_root.join(parent);
                fs::create_dir_all(&target_dir)?;
                config.out_dir(&target_dir);
                // Include both the crate-level proto root and the proto's
                // parent directory so relative imports like
                // `import "message.proto"` resolve when the files are in
                // subdirectories.
                let include_dirs = vec![proto_root.clone(), proto.parent().unwrap().to_path_buf()];
                config.compile_protos(&[proto.clone()], &include_dirs)?;
            } else {
                config.out_dir(&generated_root);
                config.compile_protos(&[proto.clone()], &[proto_root.clone()])?;
            }
        }
    }

    let known_types = collect_defined_types(&generated_root)?;

    // Run tonic per-proto and copy any newly emitted .rs files from OUT_DIR
    // into `generated_rust/<proto_stem>.rs`. This guarantees the generated
    // filenames match the proto filenames.
    // Run tonic per-proto and copy any .rs outputs from OUT_DIR into the
    // corresponding generated_rust subdirectory that mirrors the proto's
    // parent directory.
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    for proto in &proto_files {
        // Compile the individual proto with tonic to produce service code (if any)
        // Prepare include dirs for tonic so relative imports resolve.
        let tonic_include_dirs: Vec<PathBuf> = if let Some(parent) = proto.parent() {
            vec![proto_root.clone(), parent.to_path_buf()]
        } else {
            vec![proto_root.clone()]
        };

        tonic_build::configure()
            .build_client(true)
            .build_server(true)
            .compile(&[proto.clone()], &tonic_include_dirs)?;

        // Copy any .rs files produced into the generated_rust subtree for
        // this proto. Use the proto's parent directory as the destination
        // folder so the generated hierarchy mirrors the source hierarchy.
        let dest_dir = if let Ok(rel_path) = proto.strip_prefix(&proto_root) {
            if let Some(parent) = rel_path.parent() {
                generated_root.join(parent)
            } else {
                generated_root.clone()
            }
        } else {
            generated_root.clone()
        };

        fs::create_dir_all(&dest_dir)?;

        if out_dir.exists() {
            for entry in fs::read_dir(&out_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    // Copy to a file named after the proto stem inside the
                    // destination directory (e.g., `echo/message_service.rs`).
                    if let Some(stem) = proto.file_stem().and_then(|s| s.to_str()) {
                        let dst = dest_dir.join(format!("{}.rs", stem));
                        fs::copy(&path, &dst)?;

                        if looks_like_service_module(stem) {
                            let rel_dir = dest_dir
                                .strip_prefix(&generated_root)
                                .unwrap_or(Path::new(""));
                            let rel_key = rel_dir.to_string_lossy().to_string();
                            if let Some(types) = known_types.get(&rel_key) {
                                deduplicate_service_file(&dst, types)?;
                            }
                        }
                    }
                }
            }
        }
    }

    // Finally, regenerate the module index from the files actually present in
    // `generated_rust` so module names match filenames derived from proto
    // sources.
    let mut module_source = String::from("// @generated by build.rs - do not edit manually.\n");
    // Walk the generated_rust tree and emit modules that mirror the
    // directory layout. For example `generated_rust/echo/message_service.rs`
    // becomes `pub mod echo { pub mod message_service { include!(...) } }`.
    fn walk_dir(prefix: &Path, dir: &Path, out: &mut String) -> Result<(), Box<dyn Error>> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.path());

        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    let mod_name = sanitize_module_name(name);
                    out.push_str(&format!("pub mod {} {{\n", mod_name));
                    walk_dir(prefix, &path, out)?;
                    out.push_str("}\n");
                }
            } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rs") {
                // Decide whether this file should be included directly into
                // the current module (e.g. `message.rs`) or wrapped in a
                // submodule (e.g. `message_service.rs`). Files that look
                // like service/client/server helpers will be wrapped so
                // their references to `super::Message` resolve to the
                // containing directory module where `message.rs` will be
                // included directly.
                let rel_path = path.strip_prefix(prefix).unwrap();
                let rel_path_str = rel_path.to_str().unwrap();
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if looks_like_service_module(stem) {
                        // Wrap as submodule so its `super::` refers to the
                        // parent directory module.
                        let mod_name = sanitize_module_name(stem);
                        out.push_str(&format!(
                            "pub mod {mod_name} {{\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/generated_rust/{rel}\"));\n}}\n",
                            mod_name = mod_name,
                            rel = rel_path_str,
                        ));
                    } else {
                        // Include directly into the current module so
                        // sibling service modules can reference types via
                        // `super::Type`.
                        out.push_str(&format!(
                            "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/generated_rust/{rel}\"));\n",
                            rel = rel_path_str,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    walk_dir(&generated_root, &generated_root, &mut module_source)?;

    fs::write(&mod_file_path, module_source)?;

    Ok(())
}

fn collect_proto_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            println!("cargo:rerun-if-changed={}", path.display());
            collect_proto_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("proto") {
            println!("cargo:rerun-if-changed={}", path.display());
            files.push(path);
        }
    }

    Ok(())
}

fn clean_generated_directory(dir: &Path) -> Result<(), Box<dyn Error>> {
    // Remove the entire directory tree for a fully clean state, then
    // recreate the directory. This ensures older files from previous
    // generations (including nested subdirectories) are removed.
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }

    fs::create_dir_all(dir)?;
    Ok(())
}

fn sanitize_module_name(stem: &str) -> String {
    let mut sanitized = String::new();

    for ch in stem.chars() {
        if ch == '_' || ch.is_ascii_alphanumeric() {
            sanitized.push(ch.to_ascii_lowercase());
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        "generated_file".to_string()
    } else {
        sanitized
    }
}

/// Determines if the generated Rust file name corresponds to a service module.
///
/// This helper is shared between the module tree generation logic and the
/// post-processing step that removes duplicated message types from service
/// modules. The heuristic mirrors the expectations from tonic-build where
/// generated service helpers contain `service`, `server`, or `client` in the
/// stem.
fn looks_like_service_module(stem: &str) -> bool {
    let lower = stem.to_ascii_lowercase();
    lower.contains("service") || lower.contains("server") || lower.contains("client")
}

/// Collects the set of message type names generated by `prost_build` for each
/// module directory.
///
/// The returned map is keyed by the directory path relative to the
/// `generated_rust` root so that service modules in the same directory can look
/// up which types already exist and should therefore be imported instead of
/// redefined.
fn collect_defined_types(
    root: &Path,
) -> Result<BTreeMap<String, BTreeSet<String>>, Box<dyn Error>> {
    let mut map = BTreeMap::new();

    if !root.exists() {
        return Ok(map);
    }

    fn walk(
        root: &Path,
        dir: &Path,
        map: &mut BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), Box<dyn Error>> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                walk(root, &path, map)?;
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "rs")
                .unwrap_or(false)
            {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if looks_like_service_module(stem) {
                        continue;
                    }
                }

                let rel_dir = path
                    .parent()
                    .and_then(|p| p.strip_prefix(root).ok())
                    .unwrap_or(Path::new(""));
                let key = rel_dir.to_string_lossy().to_string();
                let entry = map.entry(key).or_insert_with(BTreeSet::new);
                for ty in extract_public_type_names(&path)? {
                    entry.insert(ty);
                }
            }
        }

        Ok(())
    }

    walk(root, root, &mut map)?;

    Ok(map)
}

/// Extracts the names of public structs and enums present in a generated Rust file.
fn extract_public_type_names(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let contents = fs::read_to_string(path)?;
    let mut names = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("pub struct ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        } else if let Some(rest) = trimmed.strip_prefix("pub enum ") {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }
    }

    Ok(names)
}

/// Removes duplicate message definitions from tonic-generated service modules and
/// inserts imports referencing the canonical message types.
fn deduplicate_service_file(
    file_path: &Path,
    available_types: &BTreeSet<String>,
) -> Result<(), Box<dyn Error>> {
    if available_types.is_empty() {
        return Ok(());
    }

    let contents = fs::read_to_string(file_path)?;
    let mut lines: Vec<String> = contents.lines().map(|line| line.to_string()).collect();
    let mut removed_names = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if let Some(name) = match_public_type_name(trimmed, available_types) {
            let start = find_declaration_start(&lines, index);
            let end = find_declaration_end(&lines, index);
            lines.drain(start..=end);
            removed_names.push(name);
            index = start;
        } else {
            index += 1;
        }
    }

    if removed_names.is_empty() {
        return Ok(());
    }

    let mut unique: BTreeSet<String> = removed_names.into_iter().collect();
    insert_super_import(&mut lines, &mut unique);

    let mut new_contents = lines.join("\n");
    if !new_contents.ends_with('\n') {
        new_contents.push('\n');
    }
    fs::write(file_path, new_contents)?;

    Ok(())
}

/// Matches a public struct or enum declaration against a known set of type names.
fn match_public_type_name<'a>(line: &'a str, available_types: &BTreeSet<String>) -> Option<String> {
    if let Some(rest) = line.strip_prefix("pub struct ") {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if available_types.contains(&name) {
            return Some(name);
        }
    } else if let Some(rest) = line.strip_prefix("pub enum ") {
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if available_types.contains(&name) {
            return Some(name);
        }
    }

    None
}

/// Finds the starting line index for a type declaration, including any
/// attributes, doc comments, or blank lines that precede it.
fn find_declaration_start(lines: &[String], mut index: usize) -> usize {
    while index > 0 {
        let prev = lines[index - 1].trim_start();
        if prev.starts_with("#")
            || prev.starts_with("///")
            || prev.starts_with("//!")
            || prev.is_empty()
        {
            index -= 1;
        } else {
            break;
        }
    }

    index
}

/// Finds the ending line index (inclusive) for a type declaration by tracking
/// curly brace depth.
fn find_declaration_end(lines: &[String], mut index: usize) -> usize {
    let mut depth = brace_delta(&lines[index]);

    while depth > 0 && index + 1 < lines.len() {
        index += 1;
        depth += brace_delta(&lines[index]);
    }

    index
}

/// Computes the net change in brace depth for a single line of Rust source.
fn brace_delta(line: &str) -> i32 {
    let mut delta = 0;
    for ch in line.chars() {
        match ch {
            '{' => delta += 1,
            '}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

/// Inserts or updates a `pub use super::...;` statement with any deduplicated type
/// names.
fn insert_super_import(lines: &mut Vec<String>, names: &mut BTreeSet<String>) {
    if names.is_empty() {
        return;
    }

    for line in lines.iter_mut() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed
            .strip_prefix("use super::")
            .or_else(|| trimmed.strip_prefix("pub use super::"))
        {
            let existing = rest.trim_end_matches(';').trim();
            if existing.starts_with('{') && existing.ends_with('}') {
                let inner = &existing[1..existing.len() - 1];
                for name in inner.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                    names.insert(name.to_string());
                }
            } else if !existing.is_empty() {
                names.insert(existing.to_string());
            }

            *line = format_pub_use_super(names);
            return;
        }
    }

    let use_line = format_pub_use_super(names);
    let mut insert_index = 0;
    while insert_index < lines.len() {
        let trimmed = lines[insert_index].trim_start();
        if trimmed.starts_with("//") || trimmed.is_empty() {
            insert_index += 1;
        } else {
            break;
        }
    }

    if insert_index > 0 && !lines[insert_index - 1].trim().is_empty() {
        lines.insert(insert_index, String::new());
        insert_index += 1;
    }

    lines.insert(insert_index, use_line);
    if insert_index + 1 >= lines.len() || !lines[insert_index + 1].trim().is_empty() {
        lines.insert(insert_index + 1, String::new());
    }
}

/// Formats a `pub use super` statement for the provided type names.
fn format_pub_use_super(names: &BTreeSet<String>) -> String {
    if names.len() == 1 {
        format!("pub use super::{};", names.iter().next().unwrap())
    } else {
        let joined = names.iter().cloned().collect::<Vec<_>>().join(", ");
        format!("pub use super::{{{}}};", joined)
    }
}
