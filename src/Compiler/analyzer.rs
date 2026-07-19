//! Method Reachability Analyzer
//!
//! Rust port of `src/MethodReachabilityAnalyzer.js`. Performs static analysis
//! on compiled Objective-J (JavaScript) to determine which selectors are
//! reachable from an entry point, for method tree-shaking.

use regex::Regex;
use std::collections::{HashMap, HashSet, VecDeque};

/// A compiled file entry, as produced by the assembler.
pub struct CompiledEntry {
    pub path: String,
    pub source: String,
    pub code: String,
}

#[derive(Clone)]
struct MethodInfo {
    #[allow(dead_code)]
    class_name: String,
    #[allow(dead_code)]
    function_name: String,
    file_path: String,
    #[allow(dead_code)]
    is_meta_class: bool,
    selector: String,
}

pub struct AnalyzerOptions {
    pub mode: String,
    pub preserve_accessors: bool,
    pub preserve_kvc: bool,
    pub whitelist: Vec<String>,
    pub verbose: bool,
}
impl Default for AnalyzerOptions {
    fn default() -> Self {
        AnalyzerOptions {
            mode: "safe".to_string(),
            preserve_accessors: true,
            preserve_kvc: true,
            whitelist: Vec::new(),
            verbose: false,
        }
    }
}

pub struct AnalysisStats {
    pub total_methods: usize,
    pub reachable_methods: usize,
    pub dynamic_classes: usize,
    pub kvc_keys: usize,
}

pub struct AnalysisResult {
    pub reachable_selectors: HashSet<String>,
    pub dynamic_classes: HashSet<String>,
    pub stats: AnalysisStats,
}

pub struct MethodReachabilityAnalyzer<'a> {
    compiled: &'a [CompiledEntry],
    options: AnalyzerOptions,
    reachable_selectors: HashSet<String>,
    method_map: HashMap<String, Vec<MethodInfo>>,
    dynamic_classes: HashSet<String>,
    kvc_selectors: HashSet<String>,
    always_keep: HashSet<String>,
}

impl<'a> MethodReachabilityAnalyzer<'a> {
    pub fn new(compiled: &'a [CompiledEntry], options: AnalyzerOptions) -> Self {
        let mut always_keep: HashSet<String> = [
            "alloc",
            "init",
            "new",
            "dealloc",
            "finalize",
            "initialize",
            "description",
            "isEqual:",
            "hash",
            "copy",
            "copyWithZone:",
            "valueForKey:",
            "setValue:forKey:",
            "valueForKeyPath:",
            "setValue:forKeyPath:",
            "valueForUndefinedKey:",
            "setValue:forUndefinedKey:",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for w in &options.whitelist {
            always_keep.insert(w.clone());
        }

        MethodReachabilityAnalyzer {
            compiled,
            options,
            reachable_selectors: HashSet::new(),
            method_map: HashMap::new(),
            dynamic_classes: HashSet::new(),
            kvc_selectors: HashSet::new(),
            always_keep,
        }
    }

    pub fn analyze(&mut self, entry_point: &str) -> AnalysisResult {
        self.build_method_map();
        self.detect_dynamic_selectors();
        self.detect_kvc_usage();
        self.build_call_graph(entry_point);

        for s in self.always_keep.clone() {
            self.reachable_selectors.insert(s);
        }

        if self.options.preserve_kvc {
            self.add_kvc_accessors();
        }

        let stats = AnalysisStats {
            total_methods: self.count_total_methods(),
            reachable_methods: self.count_reachable_methods(),
            dynamic_classes: self.dynamic_classes.len(),
            kvc_keys: self.kvc_selectors.len(),
        };

        AnalysisResult {
            reachable_selectors: self.reachable_selectors.clone(),
            dynamic_classes: self.dynamic_classes.clone(),
            stats,
        }
    }

    fn build_method_map(&mut self) {
        let class_methods_re =
            Regex::new(r"class_addMethods\((the_class|meta_class),\s*\[([\s\S]*?)\]\);").unwrap();
        let class_name_re = Regex::new(r#"objj_allocateClassPair\([^,]+,\s*"([^"]+)""#).unwrap();
        let method_re =
            Regex::new(r#"new objj_method\(sel_getUid\("([^"]+)"\),\s*function\s*(\$\w+)?"#)
                .unwrap();

        for entry in self.compiled {
            let code = &entry.code;
            if code.is_empty() {
                continue;
            }

            let current_class_name = class_name_re
                .captures(code)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            for cm in class_methods_re.captures_iter(code) {
                let is_meta = &cm[1] == "meta_class";
                let methods_block = &cm[2];

                for mm in method_re.captures_iter(methods_block) {
                    let selector = mm[1].to_string();
                    let function_name = mm
                        .get(2)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_else(|| format!("anonymous_{}", selector));

                    let mut class_name = current_class_name.clone();
                    if let Some(stripped) = function_name.strip_prefix('$') {
                        let parts: Vec<&str> = stripped.split("__").collect();
                        if !parts.is_empty() {
                            class_name = parts[0].to_string();
                        }
                    }

                    self.method_map
                        .entry(selector.clone())
                        .or_default()
                        .push(MethodInfo {
                            class_name,
                            function_name,
                            file_path: entry.path.clone(),
                            is_meta_class: is_meta,
                            selector,
                        });
                }
            }
        }
    }

    fn detect_dynamic_selectors(&mut self) {
        let msg_send_re = Regex::new(r"objj_msgSend(Super)?\(").unwrap();
        let alloc_re =
            Regex::new(r#"var the_class = objj_allocateClassPair\([^,]+,\s*"([^"]+)""#).unwrap();
        let getclass_re = Regex::new(r#"var the_class = objj_getClass\("([^"]+)"\)"#).unwrap();
        let func_re = Regex::new(r"function\s+\$(\w+)__").unwrap();
        let selvar_re =
            Regex::new(r#"(?:var|let|const)\s+(\w+)\s*=\s*sel_getUid\("([^"]+)"\)"#).unwrap();
        let anyclass_re = Regex::new(r#"var the_class = objj_\w+Class\w*\("([^"]+)"\)"#).unwrap();

        for entry in self.compiled {
            let code = &entry.code;
            if code.is_empty() {
                continue;
            }
            let bytes = code.as_bytes();

            for m in msg_send_re.find_iter(code) {
                let selector_arg = extract_selector_arg(bytes, m.start());
                let arg = match selector_arg {
                    Some(a) => a,
                    None => continue,
                };
                if !arg.starts_with('"') && !arg.starts_with('\'') {
                    let before = &code[..m.start()];
                    let mut class_name = "Unknown".to_string();
                    for re in [&alloc_re, &getclass_re] {
                        if let Some(c) = re.captures_iter(before).last() {
                            class_name = c[1].to_string();
                            break;
                        }
                    }
                    if class_name == "Unknown" {
                        if let Some(c) = func_re.captures_iter(before).last() {
                            class_name = c[1].to_string();
                        }
                    }
                    self.dynamic_classes.insert(class_name);
                }
            }

            for c in selvar_re.captures_iter(code) {
                let var_name = &c[1];
                let usage_re = Regex::new(&format!(
                    r"objj_msgSend\([^,]+,\s*{}\b",
                    regex::escape(var_name)
                ))
                .unwrap();
                if usage_re.is_match(code) {
                    for cm in anyclass_re.captures_iter(code) {
                        self.dynamic_classes.insert(cm[1].to_string());
                    }
                }
            }
        }
    }

    fn detect_kvc_usage(&mut self) {
        let get_kvc_re = Regex::new(
            r#"objj_msgSend\([^,]+,\s*"(valueForKey:|valueForKeyPath:)"\s*,\s*"([^"]+)""#,
        )
        .unwrap();
        let set_kvc_re = Regex::new(
            r#"objj_msgSend\([^,]+,\s*"(setValue:forKey:|setValue:forKeyPath:)"\s*,\s*[^,]+,\s*"([^"]+)""#,
        )
        .unwrap();
        let ivar_re = Regex::new(r"(\w+)\s+(\w+)\s*@accessors").unwrap();

        let mut keys: Vec<String> = Vec::new();
        for entry in self.compiled {
            let code = &entry.code;
            if !code.is_empty() {
                for c in get_kvc_re.captures_iter(code) {
                    add_kvc_key(&mut keys, &c[2]);
                }
                for c in set_kvc_re.captures_iter(code) {
                    add_kvc_key(&mut keys, &c[2]);
                }
            }
            if !entry.source.is_empty() {
                for c in ivar_re.captures_iter(&entry.source) {
                    keys.push(c[2].to_string());
                }
            }
        }
        for k in keys {
            self.kvc_selectors.insert(k);
        }
    }

    fn build_call_graph(&mut self, entry_point: &str) {
        let mut worklist: VecDeque<String> = VecDeque::new();
        let mut visited: HashSet<String> = HashSet::new();
        let msg_send_re = Regex::new(r"objj_msgSend(?:Super)?\(").unwrap();

        let patterns = [
            Regex::new(&format!(
                r"(?m)var\s+{}\s*=\s*function\s*\([^)]*\)\s*\{{([\s\S]*?)\n\}}",
                regex::escape(entry_point)
            ))
            .unwrap(),
            Regex::new(&format!(
                r"(?m)function\s+{}\s*\([^)]*\)\s*\{{([\s\S]*?)\n\}}",
                regex::escape(entry_point)
            ))
            .unwrap(),
        ];

        // Step 1: seed from the entry point body.
        'outer: for entry in self.compiled {
            let code = &entry.code;
            if code.is_empty() {
                continue;
            }
            for re in &patterns {
                if let Some(cap) = re.captures(code) {
                    let body = cap.get(1).unwrap().as_str().to_string();
                    let body_bytes = body.as_bytes();
                    for m in msg_send_re.find_iter(&body) {
                        if let Some(sel) = extract_selector(body_bytes, m.start()) {
                            if !visited.contains(&sel) {
                                visited.insert(sel.clone());
                                worklist.push_back(sel.clone());
                                self.reachable_selectors.insert(sel);
                            }
                        }
                    }
                    break 'outer;
                }
            }
        }

        // Step 2: process worklist.
        while let Some(current_selector) = worklist.pop_front() {
            let methods = match self.method_map.get(&current_selector) {
                Some(m) => m.clone(),
                None => continue,
            };
            for method in &methods {
                let method_code = match self.find_method_body(method) {
                    Some(b) => b,
                    None => continue,
                };
                let mc_bytes = method_code.as_bytes();
                for m in msg_send_re.find_iter(&method_code) {
                    if let Some(sel) = extract_selector(mc_bytes, m.start()) {
                        if !visited.contains(&sel) {
                            visited.insert(sel.clone());
                            worklist.push_back(sel.clone());
                            self.reachable_selectors.insert(sel);
                        }
                    }
                }
            }
        }
    }

    fn find_method_body(&self, method: &MethodInfo) -> Option<String> {
        let entry = self.compiled.iter().find(|e| e.path == method.file_path)?;
        let code = &entry.code;
        let pattern = format!(
            r#"new objj_method\(sel_getUid\("{}"\),\s*function[^{{]*\{{"#,
            regex::escape(&method.selector)
        );
        let re = Regex::new(&pattern).unwrap();
        let m = re.find(code)?;
        let start_pos = m.end();
        Some(extract_balanced_braces(code.as_bytes(), start_pos))
    }

    fn add_kvc_accessors(&mut self) {
        for key in self.kvc_selectors.clone() {
            self.reachable_selectors.insert(key.clone());
            let mut chars = key.chars();
            let setter = match chars.next() {
                Some(first) => format!(
                    "set{}{}:",
                    first.to_uppercase(),
                    chars.as_str()
                ),
                None => continue,
            };
            self.reachable_selectors.insert(setter);
        }
    }

    fn count_total_methods(&self) -> usize {
        self.method_map.values().map(|v| v.len()).sum()
    }
    fn count_reachable_methods(&self) -> usize {
        self.reachable_selectors
            .iter()
            .filter_map(|s| self.method_map.get(s))
            .map(|v| v.len())
            .sum()
    }
}

fn add_kvc_key(keys: &mut Vec<String>, key: &str) {
    if key.contains('.') {
        for part in key.split('.') {
            if !part.starts_with('@') {
                keys.push(part.to_string());
            }
        }
    } else {
        keys.push(key.to_string());
    }
}

/// Extract the raw text of the 2nd argument of an `objj_msgSend(` call.
fn extract_selector_arg(code: &[u8], msg_send_pos: usize) -> Option<String> {
    let open = find_byte(code, b'(', msg_send_pos)?;
    let mut pos = open + 1;
    let mut depth = 1i32;
    let mut in_string = false;
    let mut string_char = 0u8;
    let mut arg_count = 0i32;
    let mut arg_start = pos;

    while pos < code.len() && depth > 0 {
        let ch = code[pos];
        let prev = if pos > 0 { code[pos - 1] } else { 0 };

        if (ch == b'"' || ch == b'\'') && prev != b'\\' {
            if !in_string {
                in_string = true;
                string_char = ch;
            } else if ch == string_char {
                in_string = false;
            }
        }

        if !in_string {
            if ch == b'(' {
                depth += 1;
            } else if ch == b')' {
                depth -= 1;
                if depth == 0 {
                    if arg_count == 1 {
                        return Some(byte_slice_trim(code, arg_start, pos));
                    }
                    break;
                }
            } else if ch == b',' && depth == 1 {
                if arg_count == 1 {
                    return Some(byte_slice_trim(code, arg_start, pos));
                }
                arg_count += 1;
                arg_start = pos + 1;
            }
        }
        pos += 1;
    }
    None
}

/// Extract the selector string literal (2nd argument) of an `objj_msgSend(` call.
fn extract_selector(code: &[u8], msg_send_pos: usize) -> Option<String> {
    let open = find_byte(code, b'(', msg_send_pos)?;
    let mut pos = open + 1;
    let mut depth = 1i32;
    let mut in_string = false;
    let mut string_char = 0u8;
    let mut arg_count = 0i32;
    let mut selector_start: i64 = -1;

    while pos < code.len() && depth > 0 {
        let ch = code[pos];
        let prev = if pos > 0 { code[pos - 1] } else { 0 };

        if (ch == b'"' || ch == b'\'') && prev != b'\\' {
            if !in_string {
                in_string = true;
                string_char = ch;
                if arg_count == 1 && selector_start == -1 {
                    selector_start = (pos + 1) as i64;
                }
            } else if ch == string_char {
                if arg_count == 1 && selector_start != -1 {
                    return Some(
                        String::from_utf8_lossy(&code[selector_start as usize..pos]).into_owned(),
                    );
                }
                in_string = false;
            }
        }

        if !in_string {
            if ch == b'(' {
                depth += 1;
            } else if ch == b')' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else if ch == b',' && depth == 1 {
                arg_count += 1;
            }
        }
        pos += 1;
    }
    None
}

/// Extract content between balanced braces, starting just inside the first `{`.
fn extract_balanced_braces(code: &[u8], start_pos: usize) -> String {
    let mut depth = 1i32;
    let mut pos = start_pos;
    let mut in_string = false;
    let mut string_char = 0u8;

    while pos < code.len() && depth > 0 {
        let ch = code[pos];
        let prev = if pos > 0 { code[pos - 1] } else { 0 };

        if (ch == b'"' || ch == b'\'') && prev != b'\\' {
            if !in_string {
                in_string = true;
                string_char = ch;
            } else if ch == string_char {
                in_string = false;
            }
        }

        if !in_string {
            if ch == b'{' {
                depth += 1;
            } else if ch == b'}' {
                depth -= 1;
            }
        }
        pos += 1;
    }
    String::from_utf8_lossy(&code[start_pos..pos.saturating_sub(1)]).into_owned()
}

fn find_byte(code: &[u8], target: u8, from: usize) -> Option<usize> {
    code[from..].iter().position(|&b| b == target).map(|i| from + i)
}

fn byte_slice_trim(code: &[u8], start: usize, end: usize) -> String {
    String::from_utf8_lossy(&code[start..end]).trim().to_string()
}
