//! Objective-J Assembler
//!
//! Rust port of `src/ObjJAssembler.js`. Resolves `@import` dependencies from a
//! main file, topologically sorts them, recompiles with accumulated macros and
//! class info, and produces a single bundle. Supports method tree-shaking.

use crate::analyzer::{AnalysisResult, AnalyzerOptions, CompiledEntry, MethodReachabilityAnalyzer};
use crate::compiler::{compile, ClassInfo, CompileOptions, MacroValue};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

struct CacheEntry {
    path: String,
    source: String,
    code: String,
    defines: HashMap<String, MacroValue>,
    classes: HashMap<String, ClassInfo>,
}

struct TreeNode {
    path: String,
    cached: bool,
    circular: bool,
    dependencies: Vec<TreeNode>,
}

pub struct AssembleOptions {
    pub base_path: PathBuf,
    pub framework_paths: Vec<PathBuf>,
    pub defines: HashMap<String, MacroValue>,
    pub classes: HashMap<String, ClassInfo>,
    pub include_debug_symbols: bool,
    pub include_type_signatures: bool,
}
impl Default for AssembleOptions {
    fn default() -> Self {
        AssembleOptions {
            base_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            framework_paths: Vec::new(),
            defines: HashMap::new(),
            classes: HashMap::new(),
            include_debug_symbols: false,
            include_type_signatures: false,
        }
    }
}

#[derive(Default)]
pub struct BundleOptions {
    pub include_source_map: bool,
    pub minify: bool,
    pub tree_shake: bool,
    pub tree_shake_mode: Option<String>, // "safe" | "moderate" | "aggressive" | "off"
    pub tree_shake_whitelist: Vec<String>,
    pub tree_shake_preserve_accessors: bool,
    pub tree_shake_preserve_kvc: bool,
    pub tree_shake_verbose: bool,
    pub tree_shake_stats: bool,
    pub append_window_onload: bool,
}

pub struct BundleResult {
    pub code: String,
    pub files: Vec<String>,
    pub file_count: usize,
    pub total_size: usize,
    pub tree_shaking: Option<crate::analyzer::AnalysisStats>,
}

pub struct Assembler {
    opts: AssembleOptions,
    cache: HashMap<String, CacheEntry>,
    order: Vec<String>,
    processing: HashSet<String>,
}

impl Assembler {
    pub fn new(opts: AssembleOptions) -> Assembler {
        Assembler {
            opts,
            cache: HashMap::new(),
            order: Vec::new(),
            processing: HashSet::new(),
        }
    }

    fn base_compile_opts(&self, source_url: &str) -> CompileOptions {
        CompileOptions {
            include_debug_symbols: self.opts.include_debug_symbols,
            include_type_signatures: self.opts.include_type_signatures,
            source_url: source_url.to_string(),
            defines: HashMap::new(),
            classes: HashMap::new(),
        }
    }

    /// Assemble dependencies starting from a main file. Returns the ordered
    /// list of absolute file paths in compilation order.
    pub fn assemble(&mut self, main_file: &str) -> Result<Vec<String>, String> {
        self.cache.clear();
        self.processing.clear();

        let absolute = resolve(&self.opts.base_path, main_file);
        let abs_str = absolute.to_string_lossy().to_string();

        let tree = self.build_dependency_tree(&abs_str, None)?;
        let files = flatten_tree(&tree);

        // Second pass: recompile in order with accumulated defines/classes.
        let mut accumulated_defines = self.opts.defines.clone();
        let mut accumulated_classes = self.opts.classes.clone();

        for file_path in &files {
            if let Some(entry) = self.cache.get(file_path) {
                let source = entry.source.clone();
                let mut copts = self.base_compile_opts(file_path);
                copts.defines = accumulated_defines.clone();
                copts.classes = accumulated_classes.clone();

                let recompiled = compile(&source, &copts)
                    .map_err(|e| format!("Failed to compile \"{}\": {}", file_path, e))?;

                for (k, v) in &recompiled.defines {
                    accumulated_defines.insert(k.clone(), v.clone());
                }
                for (k, v) in &recompiled.classes {
                    accumulated_classes.insert(k.clone(), v.clone());
                }

                let entry = self.cache.get_mut(file_path).unwrap();
                entry.code = recompiled.code;
                entry.defines = recompiled.defines;
                entry.classes = recompiled.classes;
            }
        }

        self.order = files.clone();
        Ok(files)
    }

    fn build_dependency_tree(
        &mut self,
        file_path: &str,
        importer_path: Option<&str>,
    ) -> Result<TreeNode, String> {
        if self.cache.contains_key(file_path) {
            return Ok(TreeNode {
                path: file_path.to_string(),
                cached: true,
                circular: false,
                dependencies: Vec::new(),
            });
        }
        if self.processing.contains(file_path) {
            return Ok(TreeNode {
                path: file_path.to_string(),
                cached: false,
                circular: true,
                dependencies: Vec::new(),
            });
        }

        self.processing.insert(file_path.to_string());

        let source = fs::read_to_string(file_path).map_err(|e| {
            format!(
                "Failed to read file \"{}\"{}: {}",
                file_path,
                importer_path
                    .map(|p| format!(" (imported from \"{}\")", p))
                    .unwrap_or_default(),
                e
            )
        })?;

        let copts = self.base_compile_opts(file_path);
        let compiled = compile(&source, &copts)
            .map_err(|e| format!("Failed to compile \"{}\": {}", file_path, e))?;

        self.cache.insert(
            file_path.to_string(),
            CacheEntry {
                path: file_path.to_string(),
                source,
                code: compiled.code,
                defines: compiled.defines,
                classes: compiled.classes,
            },
        );

        let mut dependencies = Vec::new();
        for dep in &compiled.dependencies {
            if let Some(dep_path) = self.resolve_dependency(&dep.url, dep.is_local, file_path)? {
                let dep_tree = self.build_dependency_tree(&dep_path, Some(file_path))?;
                dependencies.push(dep_tree);
            }
        }

        self.processing.remove(file_path);

        Ok(TreeNode {
            path: file_path.to_string(),
            cached: false,
            circular: false,
            dependencies,
        })
    }

    fn resolve_dependency(
        &self,
        url: &str,
        is_local: bool,
        importer_path: &str,
    ) -> Result<Option<String>, String> {
        if is_local {
            let importer_dir = Path::new(importer_path)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let resolved = resolve(importer_dir, url);
            if resolved.exists() {
                return Ok(Some(resolved.to_string_lossy().to_string()));
            }
            if !url.ends_with(".j") {
                let with_ext = PathBuf::from(format!("{}.j", resolved.to_string_lossy()));
                if with_ext.exists() {
                    return Ok(Some(with_ext.to_string_lossy().to_string()));
                }
            }
            Err(format!(
                "Local import \"{}\" not found (imported from \"{}\")",
                url, importer_path
            ))
        } else {
            for fw in self.framework_search_dirs() {
                let resolved = resolve(&fw, url);
                if resolved.exists() {
                    return Ok(Some(resolved.to_string_lossy().to_string()));
                }
                if !url.ends_with(".j") {
                    let with_ext = PathBuf::from(format!("{}.j", resolved.to_string_lossy()));
                    if with_ext.exists() {
                        return Ok(Some(with_ext.to_string_lossy().to_string()));
                    }
                }
            }
            eprintln!(
                "Framework import \"{}\" not found in framework paths (imported from \"{}\")",
                url, importer_path
            );
            Ok(None)
        }
    }

    /// The ordered list of directories searched for framework (`<...>`) imports:
    ///   1. explicit `--framework` paths,
    ///   2. a project-level `Frameworks` folder next to the base path,
    ///   3. the shared Frameworks folder installed alongside the binary.
    fn framework_search_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = self.opts.framework_paths.clone();
        dirs.push(self.opts.base_path.join("Frameworks"));
        if let Some(shared) = shared_frameworks_dir() {
            dirs.push(shared);
        }
        dirs
    }

    pub fn bundle(
        &mut self,
        main_file: &str,
        options: &BundleOptions,
    ) -> Result<BundleResult, String> {
        let files = self.assemble(main_file)?;

        // Tree-shaking analysis.
        let tree_shake_enabled =
            options.tree_shake && options.tree_shake_mode.as_deref() != Some("off");

        let mut analysis: Option<AnalysisResult> = None;
        if tree_shake_enabled {
            let compiled_entries: Vec<CompiledEntry> = files
                .iter()
                .filter_map(|f| self.cache.get(f))
                .map(|e| CompiledEntry {
                    path: e.path.clone(),
                    source: e.source.clone(),
                    code: e.code.clone(),
                })
                .collect();

            let aopts = AnalyzerOptions {
                mode: options.tree_shake_mode.clone().unwrap_or_else(|| "safe".to_string()),
                preserve_accessors: options.tree_shake_preserve_accessors,
                preserve_kvc: options.tree_shake_preserve_kvc,
                whitelist: options.tree_shake_whitelist.clone(),
                verbose: options.tree_shake_verbose,
            };

            let mut analyzer = MethodReachabilityAnalyzer::new(&compiled_entries, aopts);
            let result = analyzer.analyze("main");

            if options.tree_shake_verbose || options.tree_shake_stats {
                let s = &result.stats;
                let reduction = if s.total_methods > 0 {
                    (1.0 - s.reachable_methods as f64 / s.total_methods as f64) * 100.0
                } else {
                    0.0
                };
                eprintln!("\n=== Tree-Shaking Statistics ===");
                eprintln!("Mode: {}", options.tree_shake_mode.as_deref().unwrap_or("safe"));
                eprintln!("Total methods: {}", s.total_methods);
                eprintln!("Reachable methods: {}", s.reachable_methods);
                eprintln!(
                    "Methods to prune: {}",
                    s.total_methods.saturating_sub(s.reachable_methods)
                );
                eprintln!("Reduction: {:.1}%", reduction);
                if s.dynamic_classes > 0 {
                    eprintln!("Classes with dynamic selectors: {}", s.dynamic_classes);
                }
                eprintln!("================================\n");
            }

            analysis = Some(result);
        }

        let mut parts: Vec<String> = Vec::new();
        let include_source_map = true; // matches JS (forced on)

        for file_path in &files {
            if let Some(entry) = self.cache.get(file_path) {
                if include_source_map {
                    parts.push(format!("// #region {}", file_path));
                }
                parts.push(format!("// File: {}", file_path));

                let mut code = entry.code.clone();
                if tree_shake_enabled {
                    if let Some(ref a) = analysis {
                        code = prune_unreachable_methods(&code, a);
                    }
                }
                parts.push(code);
                if include_source_map {
                    parts.push("// #endregion".to_string());
                }
                parts.push(String::new());
            }
        }

        let mut code = parts.join("\n");
        if options.append_window_onload {
            code.push_str("window.onload = main;\n");
        }

        if options.minify {
            code = minify(&code);
        }

        let total_size = code.len();
        let tree_shaking = analysis.map(|a| a.stats);

        Ok(BundleResult {
            code,
            file_count: files.len(),
            files,
            total_size,
            tree_shaking,
        })
    }
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Locate the shared Frameworks directory installed alongside the binary.
///
/// `OBJJ_FRAMEWORKS_PATH` overrides everything. Otherwise, given a binary at
/// `<prefix>/bin/objjc`, the frameworks live at `<prefix>/share/objj/Frameworks`
/// (matching `install.sh`). Returns `None` if no candidate directory exists.
fn shared_frameworks_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("OBJJ_FRAMEWORKS_PATH") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Some(path);
        }
    }

    let exe = std::env::current_exe().ok()?;
    // <prefix>/bin/objjc -> <prefix>
    let prefix = exe.parent()?.parent()?;
    let shared = prefix.join("share").join("objj").join("Frameworks");
    if shared.is_dir() {
        Some(shared)
    } else {
        None
    }
}

/// Node-style path resolution (`path.resolve(base, url)`).
fn resolve(base: &Path, url: &str) -> PathBuf {
    let p = Path::new(url);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    normalize(&joined)
}

/// Normalize `.` and `..` components without touching the filesystem.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        use std::path::Component::*;
        match comp {
            ParentDir => {
                out.pop();
            }
            CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn flatten_tree(tree: &TreeNode) -> Vec<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut result: Vec<String> = Vec::new();
    visit(tree, &mut visited, &mut result);
    result
}

fn visit(node: &TreeNode, visited: &mut HashSet<String>, result: &mut Vec<String>) {
    if visited.contains(&node.path) || node.circular || node.cached {
        return;
    }
    visited.insert(node.path.clone());
    for dep in &node.dependencies {
        visit(dep, visited, result);
    }
    result.push(node.path.clone());
}

/// Prune unreachable methods from a single file's compiled code.
fn prune_unreachable_methods(code: &str, analysis: &AnalysisResult) -> String {
    let class_re =
        Regex::new(r#"var the_class = objj_allocateClassPair\([^,]+,\s*"([^"]+)"\)"#).unwrap();
    let current_class = class_re
        .captures(code)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    if analysis.dynamic_classes.contains(&current_class) {
        return code.to_string();
    }

    let add_methods_re =
        Regex::new(r"class_addMethods\((the_class|meta_class),\s*\[([\s\S]*?)\]\);").unwrap();
    let selector_re = Regex::new(r#"new objj_method\(sel_getUid\("([^"]+)"\)"#).unwrap();

    add_methods_re
        .replace_all(code, |caps: &regex::Captures| {
            let class_var = &caps[1];
            let methods_block = &caps[2];
            let methods = split_methods(methods_block);
            let mut kept: Vec<String> = Vec::new();

            for method in methods {
                if let Some(sc) = selector_re.captures(&method) {
                    let selector = &sc[1];
                    if analysis.reachable_selectors.contains(selector) {
                        kept.push(method);
                    }
                } else {
                    kept.push(method);
                }
            }

            if kept.is_empty() {
                format!("// class_addMethods({}, []); - all methods pruned", class_var)
            } else {
                format!("class_addMethods({}, [{}]);", class_var, kept.join(", "))
            }
        })
        .into_owned()
}

/// Split method definitions by top-level commas.
fn split_methods(methods_block: &str) -> Vec<String> {
    let chars: Vec<char> = methods_block.chars().collect();
    let mut methods = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut string_char = '\0';

    for i in 0..chars.len() {
        let ch = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { '\0' };

        if (ch == '"' || ch == '\'') && prev != '\\' {
            if !in_string {
                in_string = true;
                string_char = ch;
            } else if ch == string_char {
                in_string = false;
            }
        }

        if !in_string {
            if ch == '(' || ch == '{' || ch == '[' {
                depth += 1;
            } else if ch == ')' || ch == '}' || ch == ']' {
                depth -= 1;
            }
            if ch == ',' && depth == 0 {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    methods.push(trimmed.to_string());
                }
                current.clear();
                continue;
            }
        }
        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        methods.push(trimmed.to_string());
    }
    methods
}

/// Basic minification: strip comments and collapse blank lines.
fn minify(code: &str) -> String {
    let s = Regex::new(r"(?m)^[ \t]*//.*$").unwrap().replace_all(code, "");
    let s = Regex::new(r"/\*[\s\S]*?\*/").unwrap().replace_all(&s, "");
    let s = Regex::new(r"(?m)^\s*\r?\n").unwrap().replace_all(&s, "");
    let s = Regex::new(r"\n{2,}").unwrap().replace_all(&s, "\n");
    s.into_owned()
}
