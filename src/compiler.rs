//! Objective-J Compiler
//!
//! A faithful Rust port of `src/ObjJCompiler.js` from CocotronWeb, which was
//! itself ported from the Cappuccino framework.
//!
//! Compiles Objective-J source to JavaScript, resolving `@import` dependencies
//! and running a C-style preprocessor (`#define`, `#if`, `#ifdef`, ...).

use regex::Regex;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Cached regexes (compiled once)
// ---------------------------------------------------------------------------

macro_rules! re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($pat).unwrap())
        }
    };
}

// TOKEN_WHITESPACE = /^(?:(?:\s+$)|(?:\/(?:\/|\*)))/
re!(re_whitespace, r"^(?:(?:\s+$)|(?:/(?:/|\*)))");
// TOKEN_NUMBER = /^[+-]?\d+(([.]\d+)*([eE][+-]?\d+))?$/
re!(re_number, r"^[+-]?\d+(([.]\d+)*([eE][+-]?\d+))?$");
// TOKEN_IDENTIFIER = /^[a-zA-Z_$](\w|$)*$/
re!(re_identifier, r"^[a-zA-Z_$](\w|$)*$");
// IS_WORD = /^\w+$/
re!(re_word, r"^\w+$");
// Lexer token pattern (no /g; used with find_iter)
re!(
    re_lexer,
    r#"(?://.*(?:\r|\n)?|/\*(?:.|\n|\r)*?\*/|\w+\b|[+-]?\d+(?:(?:[.]\d+)*(?:[eE][+-]?\d+))?|"[^"\\]*(?:\\[\s\S][^"\\]*)*"|'[^'\\]*(?:\\[\s\S][^'\\]*)*'|\s+|.)"#
);

fn is_whitespace_tok(t: &str) -> bool {
    re_whitespace().is_match(t)
}
fn is_identifier(t: &str) -> bool {
    re_identifier().is_match(t)
}
fn is_number(t: &str) -> bool {
    re_number().is_match(t)
}
fn is_word(t: &str) -> bool {
    re_word().is_match(t)
}

// ---------------------------------------------------------------------------
// Token constants
// ---------------------------------------------------------------------------

const T_ACCESSORS: &str = "accessors";
const T_CLASS: &str = "class";
const T_END: &str = "end";
const T_FUNCTION: &str = "function";
const T_IMPLEMENTATION: &str = "implementation";
const T_IMPORT: &str = "import";
const T_OUTLET: &str = "outlet";
const T_SELECTOR: &str = "selector";
const T_SUPER: &str = "super";
const T_PRAGMA: &str = "pragma";
const T_MARK: &str = "mark";
const T_DEFINE: &str = "define";
const T_UNDEF: &str = "undef";
const T_IF: &str = "if";
const T_IFDEF: &str = "ifdef";
const T_IFNDEF: &str = "ifndef";
const T_ELIF: &str = "elif";
const T_ELSE: &str = "else";
const T_ENDIF: &str = "endif";
const T_EQUAL: &str = "=";
const T_PLUS: &str = "+";
const T_MINUS: &str = "-";
const T_COLON: &str = ":";
const T_COMMA: &str = ",";
const T_PERIOD: &str = ".";
const T_SEMICOLON: &str = ";";
const T_LESS_THAN: &str = "<";
const T_OPEN_BRACE: &str = "{";
const T_CLOSE_BRACE: &str = "}";
const T_GREATER_THAN: &str = ">";
const T_OPEN_BRACKET: &str = "[";
const T_DOUBLE_QUOTE: char = '"';
const T_PREPROCESSOR: &str = "@";
const T_HASH: &str = "#";
const T_CLOSE_BRACKET: &str = "]";
const T_QUESTION_MARK: &str = "?";
const T_OPEN_PAREN: &str = "(";
const T_CLOSE_PAREN: &str = ")";

// Flags
pub const FLAG_INCLUDE_DEBUG_SYMBOLS: u32 = 1 << 0;
pub const FLAG_INCLUDE_TYPE_SIGNATURES: u32 = 1 << 1;

// ---------------------------------------------------------------------------
// String buffer (heterogeneous list of strings / nested buffers)
// ---------------------------------------------------------------------------

pub type Buf = Rc<RefCell<StringBuffer>>;

#[derive(Clone)]
pub enum Atom {
    S(String),
    B(Buf),
}

#[derive(Default)]
pub struct StringBuffer {
    pub atoms: Vec<Atom>,
}

impl StringBuffer {
    pub fn new_buf() -> Buf {
        Rc::new(RefCell::new(StringBuffer::default()))
    }
    pub fn render(&self, out: &mut String) {
        for atom in &self.atoms {
            match atom {
                Atom::S(s) => out.push_str(s),
                Atom::B(b) => b.borrow().render(out),
            }
        }
    }
    pub fn to_string(&self) -> String {
        let mut s = String::new();
        self.render(&mut s);
        s
    }
}

fn push_s(buf: &Buf, s: impl Into<String>) {
    buf.borrow_mut().atoms.push(Atom::S(s.into()));
}
fn push_b(buf: &Buf, b: Buf) {
    buf.borrow_mut().atoms.push(Atom::B(b));
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

pub struct Lexer {
    tokens: Vec<String>,
    index: i64,
    context: Vec<i64>,
}

impl Lexer {
    pub fn new(input: &str) -> Lexer {
        let source = format!("{}\n", input);
        let tokens = re_lexer()
            .find_iter(&source)
            .map(|m| m.as_str().to_string())
            .collect();
        Lexer {
            tokens,
            index: -1,
            context: Vec::new(),
        }
    }

    fn get(&self, i: i64) -> Option<String> {
        if i < 0 {
            return None;
        }
        self.tokens.get(i as usize).cloned()
    }

    fn push(&mut self) {
        self.context.push(self.index);
    }
    fn pop(&mut self) {
        if let Some(i) = self.context.pop() {
            self.index = i;
        }
    }
    fn next(&mut self) -> Option<String> {
        self.index += 1;
        self.get(self.index)
    }
    fn previous(&mut self) -> Option<String> {
        self.index -= 1;
        self.get(self.index)
    }

    /// Skip whitespace/comment tokens moving forward (default) or backward.
    fn skip_whitespace(&mut self, backwards: bool) -> Option<String> {
        loop {
            let token = if backwards { self.previous() } else { self.next() };
            match token {
                Some(t) if is_whitespace_tok(&t) => continue,
                other => return other,
            }
        }
    }
    fn skip_ws(&mut self) -> Option<String> {
        self.skip_whitespace(false)
    }
}

// ---------------------------------------------------------------------------
// Macro / class metadata
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum MacroValue {
    Flag(bool),
    Str(String),
    Func { params: Vec<String>, body: String },
}

#[derive(Clone, Debug, Default)]
pub struct ClassInfo {
    pub super_class_name: Option<String>,
    pub ivars: HashMap<String, i32>,
}

#[derive(Clone, Debug)]
pub struct Dependency {
    pub url: String,
    pub is_local: bool,
}

#[derive(Clone, Debug)]
enum AttrVal {
    Flag,
    Str(String),
}
impl AttrVal {
    fn as_str(&self) -> Option<&str> {
        match self {
            AttrVal::Str(s) => Some(s),
            AttrVal::Flag => None,
        }
    }
}

struct Condition {
    active: bool,
    has_been_true: bool,
    in_else: bool,
}

/// Result of `preprocess`, mirroring JS's polymorphic return value.
enum PreResult {
    Buf(Buf),
    Bool(bool),
    Unit,
}
impl PreResult {
    fn truthy(&self) -> bool {
        match self {
            PreResult::Buf(_) => true,
            PreResult::Bool(b) => *b,
            PreResult::Unit => false,
        }
    }
}

/// Working state for a message-send / bracket tuple.
struct Tuple {
    buffer: Buf,
    label: Option<String>,
    closures: [i64; 3],
}
impl Tuple {
    fn new() -> Tuple {
        Tuple {
            buffer: StringBuffer::new_buf(),
            label: None,
            closures: [0, 0, 0],
        }
    }
}

pub struct CompileResult {
    pub code: String,
    pub dependencies: Vec<Dependency>,
    pub defines: HashMap<String, MacroValue>,
    pub classes: HashMap<String, ClassInfo>,
}

pub struct CompileOptions {
    pub include_debug_symbols: bool,
    pub include_type_signatures: bool,
    pub source_url: String,
    pub defines: HashMap<String, MacroValue>,
    pub classes: HashMap<String, ClassInfo>,
}
impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions {
            include_debug_symbols: false,
            include_type_signatures: false,
            source_url: "(anonymous)".to_string(),
            defines: HashMap::new(),
            classes: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Preprocessor
// ---------------------------------------------------------------------------

struct Preprocessor {
    url: String,
    current_selector: String,
    current_class: String,
    current_super_class: String,
    current_super_meta_class: String,
    buffer: Buf,
    dependencies: Vec<Dependency>,
    flags: u32,
    class_method: bool,
    class_lookup_table: HashMap<String, ClassInfo>,
    class_vars: HashMap<String, i32>,
    defines: HashMap<String, MacroValue>,
    condition_stack: Vec<Condition>,
    current_ivar_names: Option<HashMap<String, i32>>,
}

type R<T> = Result<T, String>;

impl Preprocessor {
    fn run(source: &str, opts: &CompileOptions) -> R<CompileResult> {
        // Remove shebang (but not other # directives)
        let source = if source.starts_with("#!") {
            format!("//{}", &source[2..])
        } else {
            source.to_string()
        };

        let mut p = Preprocessor {
            url: opts.source_url.clone(),
            current_selector: String::new(),
            current_class: String::new(),
            current_super_class: String::new(),
            current_super_meta_class: String::new(),
            buffer: StringBuffer::new_buf(),
            dependencies: Vec::new(),
            flags: {
                let mut f = 0;
                if opts.include_debug_symbols {
                    f |= FLAG_INCLUDE_DEBUG_SYMBOLS;
                }
                if opts.include_type_signatures {
                    f |= FLAG_INCLUDE_TYPE_SIGNATURES;
                }
                f
            },
            class_method: false,
            class_lookup_table: HashMap::new(),
            class_vars: HashMap::new(),
            defines: opts.defines.clone(),
            condition_stack: Vec::new(),
            current_ivar_names: None,
        };

        // Pre-populate class info from external definitions.
        for (name, info) in &opts.classes {
            p.set_class_info(name, info.super_class_name.clone(), info.ivars.clone());
        }

        // Standard class object properties.
        for name in [
            "isa",
            "super_class",
            "name",
            "version",
            "info",
            "ivar_list",
            "method_list",
            "cache",
            "subclass_list",
            "instance_size",
        ] {
            p.class_vars.insert(name.to_string(), 1);
        }

        let mut lexer = Lexer::new(&source);
        let buf = p.buffer.clone();
        p.preprocess(&mut lexer, Some(buf), None, None, None)?;

        let code = p.buffer.borrow().to_string();
        Ok(CompileResult {
            code,
            dependencies: p.dependencies.clone(),
            defines: p.defines.clone(),
            classes: p.class_lookup_table.clone(),
        })
    }

    fn error_message(&self, msg: &str) -> String {
        format!(
            "{} <File: {}{}{}>",
            msg,
            self.url,
            if !self.current_class.is_empty() {
                format!(" Class: {}", self.current_class)
            } else {
                String::new()
            },
            if !self.current_selector.is_empty() {
                format!(" Method: {}", self.current_selector)
            } else {
                String::new()
            }
        )
    }

    fn set_class_info(
        &mut self,
        class_name: &str,
        super_class_name: Option<String>,
        ivars: HashMap<String, i32>,
    ) {
        self.class_lookup_table.insert(
            class_name.to_string(),
            ClassInfo {
                super_class_name,
                ivars,
            },
        );
    }

    fn all_ivar_names_for_class(&self, class_name: &str) -> HashMap<String, i32> {
        let mut names = HashMap::new();
        let mut current = self.class_lookup_table.get(class_name);
        while let Some(info) = current {
            for k in info.ivars.keys() {
                names.insert(k.clone(), 1);
            }
            current = match &info.super_class_name {
                Some(s) => self.class_lookup_table.get(s),
                None => None,
            };
        }
        names
    }

    // --- macro helpers ---

    fn is_defined(&self, name: &str) -> bool {
        self.defines.contains_key(name)
    }

    fn is_output_active(&self) -> bool {
        self.condition_stack.iter().all(|c| c.active)
    }

    fn define_macro(&mut self, name: &str, value: MacroValue) {
        self.defines.insert(name.to_string(), value);
    }
    fn undefine_macro(&mut self, name: &str) {
        self.defines.remove(name);
    }

    fn expand_function_macro(&self, name: &str, args: &[String]) -> Option<String> {
        let macro_val = self.defines.get(name)?;
        if let MacroValue::Func { params, body } = macro_val {
            let mut body = body.clone();
            for (i, param) in params.iter().enumerate() {
                let arg = args.get(i).map(|a| a.trim().to_string()).unwrap_or_default();
                let re = Regex::new(&format!(r"\b{}\b", regex::escape(param))).unwrap();
                body = re.replace_all(&body, arg.as_str()).into_owned();
            }
            Some(body)
        } else {
            None
        }
    }

    fn should_prefix_with_self(&self, token: &str, prev_token: &str) -> bool {
        let ivars = match &self.current_ivar_names {
            Some(m) => m,
            None => return false,
        };
        if !is_identifier(token) {
            return false;
        }
        if !ivars.contains_key(token) {
            return false;
        }
        if prev_token == T_PERIOD {
            return false;
        }
        if matches!(
            prev_token,
            "var" | "let" | "const" | "function" | "catch" | "class" | "new"
        ) {
            return false;
        }
        true
    }

    /// Try to expand a macro (simple or function-like) from the token stream.
    fn try_expand_macro(&mut self, token: &str, tokens: &mut Lexer) -> R<Option<String>> {
        if !is_identifier(token) || !self.is_defined(token) {
            return Ok(None);
        }
        let value = self.defines.get(token).cloned().unwrap();

        match value {
            MacroValue::Func { .. } => {
                // Peek ahead for opening parenthesis.
                tokens.push();
                let mut next = tokens.next();

                // Skip whitespace that does not contain a newline.
                while let Some(ref n) = next {
                    if Regex::new(r"^\s+$").unwrap().is_match(n) && !n.contains('\n') {
                        next = tokens.next();
                    } else {
                        break;
                    }
                }

                if next.as_deref() == Some(T_OPEN_PAREN) {
                    let mut args: Vec<String> = Vec::new();
                    let mut current_arg = String::new();
                    let mut depth = 1;

                    while depth > 0 {
                        let n = tokens.next();
                        let n = match n {
                            Some(n) => n,
                            None => {
                                return Err(self
                                    .error_message("*** Unterminated macro arguments"))
                            }
                        };
                        if n == T_OPEN_PAREN {
                            depth += 1;
                            current_arg.push_str(&n);
                        } else if n == T_CLOSE_PAREN {
                            depth -= 1;
                            if depth > 0 {
                                current_arg.push_str(&n);
                            }
                        } else if n == T_COMMA && depth == 1 {
                            args.push(current_arg.clone());
                            current_arg.clear();
                        } else {
                            current_arg.push_str(&n);
                        }
                    }

                    if !current_arg.is_empty() || !args.is_empty() {
                        args.push(current_arg);
                    }

                    tokens.context.pop(); // Discard saved position.
                    Ok(self.expand_function_macro(token, &args))
                } else {
                    tokens.pop();
                    Ok(None)
                }
            }
            MacroValue::Str(s) => Ok(Some(s)),
            MacroValue::Flag(_) => Ok(None),
        }
    }

    // --- condition evaluation ---

    fn evaluate_condition(&self, tokens: &mut Lexer) -> bool {
        let mut parts = String::new();
        while let Some(token) = tokens.next() {
            if token.contains('\n') {
                break;
            }
            if is_whitespace_tok(&token) && !token.contains('\n') {
                parts.push(' ');
            } else {
                parts.push_str(&token);
            }
        }
        self.eval_condition_expr(parts.trim())
    }

    fn eval_condition_expr(&self, expr: &str) -> bool {
        // Handle defined(NAME) and defined NAME.
        let re_def_paren = Regex::new(r"defined\s*\(\s*(\w+)\s*\)").unwrap();
        let s = re_def_paren.replace_all(expr, |c: &regex::Captures| {
            if self.is_defined(&c[1]) {
                "1"
            } else {
                "0"
            }
            .to_string()
        });
        let re_def = Regex::new(r"defined\s+(\w+)").unwrap();
        let s = re_def.replace_all(&s, |c: &regex::Captures| {
            if self.is_defined(&c[1]) {
                "1"
            } else {
                "0"
            }
            .to_string()
        });

        // Replace macro names with their values.
        let re_ident = Regex::new(r"\b([A-Za-z_]\w*)\b").unwrap();
        let s = re_ident.replace_all(&s, |c: &regex::Captures| {
            let name = &c[1];
            if name == "true" {
                return "1".to_string();
            }
            if name == "false" {
                return "0".to_string();
            }
            if let Some(val) = self.defines.get(name) {
                return match val {
                    MacroValue::Flag(true) => "1".to_string(),
                    MacroValue::Flag(false) => "0".to_string(),
                    MacroValue::Str(v) => v.clone(),
                    MacroValue::Func { .. } => "0".to_string(),
                };
            }
            "0".to_string()
        });

        let expr = s.into_owned();
        // Only allow numbers, operators and parentheses.
        if !Regex::new(r"^[\d\s()+\-*/%<>=!&|^~.]+$")
            .unwrap()
            .is_match(&expr)
        {
            eprintln!("Warning: Could not evaluate condition \"{}\"", expr);
            return false;
        }
        match eval_numeric(&expr) {
            Some(v) => v != 0.0,
            None => {
                eprintln!("Warning: Could not evaluate condition \"{}\"", expr);
                false
            }
        }
    }

    /// Skip tokens until the matching #endif / #elif / #else.
    fn skip_to_next_directive(&self, tokens: &mut Lexer) -> R<String> {
        let mut depth = 1;
        while let Some(token) = tokens.next() {
            if token == T_HASH {
                let directive = tokens.next().unwrap_or_default();
                if directive == T_IF || directive == T_IFDEF || directive == T_IFNDEF {
                    depth += 1;
                    while let Some(t) = tokens.next() {
                        if t.contains('\n') {
                            break;
                        }
                    }
                } else if directive == T_ENDIF {
                    depth -= 1;
                    if depth == 0 {
                        tokens.previous(); // put endif back
                        tokens.previous(); // put # back
                        return Ok("endif".to_string());
                    }
                    while let Some(t) = tokens.next() {
                        if t.contains('\n') {
                            break;
                        }
                    }
                } else if depth == 1 && (directive == T_ELIF || directive == T_ELSE) {
                    tokens.previous();
                    tokens.previous();
                    return Ok(directive);
                } else {
                    while let Some(t) = tokens.next() {
                        if t.contains('\n') {
                            break;
                        }
                    }
                }
            }
        }
        Err(self.error_message("*** Unterminated conditional: missing #endif"))
    }

    // --- @accessors ---

    fn accessors(&self, tokens: &mut Lexer) -> R<HashMap<String, AttrVal>> {
        let mut attributes: HashMap<String, AttrVal> = HashMap::new();
        let mut token = tokens.skip_ws();

        if token.as_deref() != Some(T_OPEN_PAREN) {
            tokens.previous();
            return Ok(attributes);
        }

        loop {
            token = tokens.skip_ws();
            if token.as_deref() == Some(T_CLOSE_PAREN) {
                break;
            }
            let name = token.clone().unwrap_or_default();
            let mut value = AttrVal::Flag;

            if !is_word(&name) {
                return Err(self.error_message("*** @accessors attribute name not valid."));
            }

            token = tokens.skip_ws();
            if token.as_deref() == Some(T_EQUAL) {
                let mut v = tokens.skip_ws().unwrap_or_default();
                if !is_word(&v) {
                    return Err(self.error_message("*** @accessors attribute value not valid."));
                }
                if name == "setter" {
                    if tokens.next().as_deref() != Some(T_COLON) {
                        return Err(self.error_message(
                            "*** @accessors setter attribute requires \":\" at end.",
                        ));
                    }
                    v.push(':');
                }
                value = AttrVal::Str(v);
                token = tokens.skip_ws();
            }

            attributes.insert(name, value);

            if token.as_deref() == Some(T_CLOSE_PAREN) {
                break;
            }
            if token.as_deref() != Some(T_COMMA) {
                return Err(self.error_message("*** Expected ',' or ')' in @accessors."));
            }
        }

        Ok(attributes)
    }

    // --- message sends / arrays ---

    fn brackets(&mut self, tokens: &mut Lexer, a_string_buffer: &Buf) -> R<()> {
        let mut tuples: Vec<Tuple> = Vec::new();
        loop {
            let mut t = Tuple::new();
            let cont = self
                .preprocess(tokens, None, None, None, Some(&mut t))?
                .truthy();
            tuples.push(t);
            if !cont {
                break;
            }
        }

        // tuples[0][0] is the receiver buffer; [1] is the label.
        let first_len = tuples.len();
        let first_atoms_len = tuples[0].buffer.borrow().atoms.len();

        if first_len == 1 && tuples[0].label.is_none() {
            // Plain array: [ expr ]
            push_s(a_string_buffer, "[");
            push_b(a_string_buffer, tuples[0].buffer.clone());
            push_s(a_string_buffer, "]");
        } else {
            let _ = first_atoms_len;
            let selector = StringBuffer::new_buf();

            // Determine if receiver is `super`.
            let receiver_is_super = {
                let recv = tuples[0].buffer.borrow();
                matches!(recv.atoms.first(), Some(Atom::S(s)) if s == T_SUPER)
            };

            if receiver_is_super {
                push_s(a_string_buffer, "objj_msgSendSuper(");
                push_s(
                    a_string_buffer,
                    format!(
                        "{{ receiver:self, super_class:{} }}",
                        if self.class_method {
                            &self.current_super_meta_class
                        } else {
                            &self.current_super_class
                        }
                    ),
                );
            } else {
                push_s(a_string_buffer, "objj_msgSend(");
                push_b(a_string_buffer, tuples[0].buffer.clone());
            }

            if let Some(l) = &tuples[0].label {
                push_s(&selector, l.clone());
            }

            let marg_list = StringBuffer::new_buf();
            for index in 1..tuples.len() {
                if let Some(l) = &tuples[index].label {
                    push_s(&selector, l.clone());
                }
                push_s(&marg_list, ", ");
                push_b(&marg_list, tuples[index].buffer.clone());
            }

            push_s(a_string_buffer, ", \"");
            push_b(a_string_buffer, selector);
            push_s(a_string_buffer, "\"");
            push_b(a_string_buffer, marg_list);
            push_s(a_string_buffer, ")");
        }
        Ok(())
    }

    // --- @ directives ---

    fn directive(&mut self, tokens: &mut Lexer, buffer: &Buf) -> R<()> {
        let token = tokens.next().unwrap_or_default();

        if token.chars().next() == Some(T_DOUBLE_QUOTE) {
            push_s(buffer, token);
        } else if token == T_CLASS {
            tokens.skip_ws();
        } else if token == T_IMPLEMENTATION {
            self.implementation(tokens, buffer)?;
        } else if token == T_IMPORT {
            self.import(tokens)?;
        } else if token == T_SELECTOR {
            self.selector(tokens, buffer)?;
        }
        Ok(())
    }

    // --- # preprocessor directives ---

    fn hash(&mut self, tokens: &mut Lexer, buffer: &Buf) -> R<()> {
        let token = tokens.next().unwrap_or_default();

        if token == T_PRAGMA {
            let t = tokens.skip_ws().unwrap_or_default();
            if t == T_MARK {
                while let Some(t) = tokens.next() {
                    if t.contains('\n') {
                        break;
                    }
                }
            }
        } else if token == T_DEFINE {
            let name = tokens.skip_ws().unwrap_or_default();
            if !is_identifier(&name) {
                return Err(self.error_message(&format!("*** Invalid macro name: {}", name)));
            }

            let mut next_token = tokens.next();
            let mut is_function_macro = false;
            let mut params: Vec<String> = Vec::new();

            if next_token.as_deref() == Some(T_OPEN_PAREN) {
                is_function_macro = true;
                loop {
                    let param_token = tokens.skip_ws();
                    match param_token.as_deref() {
                        Some(T_CLOSE_PAREN) => break,
                        Some(pt) if is_identifier(pt) => params.push(pt.to_string()),
                        Some(T_COMMA) => {}
                        None => {
                            return Err(self
                                .error_message("*** Unterminated macro parameter list"))
                        }
                        Some(pt) => {
                            return Err(self.error_message(&format!(
                                "*** Invalid macro parameter: {}",
                                pt
                            )))
                        }
                    }
                }
                next_token = tokens.next();
            }

            // Collect body/value until end of line (with backslash continuation).
            let mut value_parts: Vec<String> = Vec::new();
            let mut continue_reading = true;
            while continue_reading {
                loop {
                    match &next_token {
                        Some(t) if !t.contains('\n') => {
                            if !is_whitespace_tok(t) || !value_parts.is_empty() {
                                value_parts.push(t.clone());
                            }
                            next_token = tokens.next();
                        }
                        _ => break,
                    }
                }

                if value_parts.last().map(|s| s.as_str()) == Some("\\") {
                    value_parts.pop();
                    value_parts.push(" ".to_string());
                    next_token = tokens.next();
                } else {
                    continue_reading = false;
                }
            }

            let value = value_parts.join("").trim().to_string();

            if self.is_output_active() {
                if is_function_macro {
                    self.define_macro(&name, MacroValue::Func { params, body: value });
                } else if value.is_empty() {
                    self.define_macro(&name, MacroValue::Flag(true));
                } else {
                    self.define_macro(&name, MacroValue::Str(value));
                }
            }
        } else if token == T_UNDEF {
            let name = tokens.skip_ws().unwrap_or_default();
            if !is_identifier(&name) {
                return Err(self.error_message(&format!("*** Invalid macro name: {}", name)));
            }
            while let Some(t) = tokens.next() {
                if t.contains('\n') {
                    break;
                }
            }
            if self.is_output_active() {
                self.undefine_macro(&name);
            }
        } else if token == T_IF {
            let condition = self.evaluate_condition(tokens);
            let parent_active = self.is_output_active();
            self.condition_stack.push(Condition {
                active: parent_active && condition,
                has_been_true: condition,
                in_else: false,
            });
            if !self.is_output_active() {
                let found = self.skip_to_next_directive(tokens)?;
                if found != "endif" {
                    tokens.next();
                    self.hash(tokens, buffer)?;
                }
            }
        } else if token == T_IFDEF || token == T_IFNDEF {
            let name = tokens.skip_ws().unwrap_or_default();
            while let Some(t) = tokens.next() {
                if t.contains('\n') {
                    break;
                }
            }
            let defined = self.is_defined(&name);
            let condition = if token == T_IFDEF { defined } else { !defined };
            let parent_active = self.is_output_active();
            self.condition_stack.push(Condition {
                active: parent_active && condition,
                has_been_true: condition,
                in_else: false,
            });
            if !self.is_output_active() {
                let found = self.skip_to_next_directive(tokens)?;
                if found != "endif" {
                    tokens.next();
                    self.hash(tokens, buffer)?;
                }
            }
        } else if token == T_ELIF {
            if self.condition_stack.is_empty() {
                return Err(self.error_message("*** #elif without matching #if"));
            }
            if self.condition_stack.last().unwrap().in_else {
                return Err(self.error_message("*** #elif after #else"));
            }
            let condition = self.evaluate_condition(tokens);
            let n = self.condition_stack.len();
            let parent_active = self.condition_stack[..n - 1].iter().all(|c| c.active);
            let current = self.condition_stack.last_mut().unwrap();
            current.active = parent_active && condition && !current.has_been_true;
            if current.active {
                current.has_been_true = true;
            }
            if !self.is_output_active() {
                let found = self.skip_to_next_directive(tokens)?;
                if found != "endif" {
                    tokens.next();
                    self.hash(tokens, buffer)?;
                }
            }
        } else if token == T_ELSE {
            while let Some(t) = tokens.next() {
                if t.contains('\n') {
                    break;
                }
            }
            if self.condition_stack.is_empty() {
                return Err(self.error_message("*** #else without matching #if"));
            }
            if self.condition_stack.last().unwrap().in_else {
                return Err(self.error_message("*** Duplicate #else"));
            }
            let n = self.condition_stack.len();
            let parent_active = self.condition_stack[..n - 1].iter().all(|c| c.active);
            let current = self.condition_stack.last_mut().unwrap();
            current.in_else = true;
            current.active = parent_active && !current.has_been_true;
            if current.active {
                current.has_been_true = true;
            }
            if !self.is_output_active() {
                let found = self.skip_to_next_directive(tokens)?;
                if found != "endif" {
                    tokens.next();
                    self.hash(tokens, buffer)?;
                }
            }
        } else if token == T_ENDIF {
            while let Some(t) = tokens.next() {
                if t.contains('\n') {
                    break;
                }
            }
            if self.condition_stack.is_empty() {
                return Err(self.error_message("*** #endif without matching #if"));
            }
            self.condition_stack.pop();
        } else {
            return Err(self.error_message(&format!(
                "*** Unknown preprocessor directive: #{}",
                token
            )));
        }
        Ok(())
    }

    // --- @implementation ---

    fn implementation(&mut self, tokens: &mut Lexer, buffer: &Buf) -> R<()> {
        let class_name = tokens.skip_ws().unwrap_or_default();
        let mut superclass_name = "Nil".to_string();

        let instance_methods = StringBuffer::new_buf();
        let class_methods = StringBuffer::new_buf();

        if !Regex::new(r"^\w").unwrap().is_match(&class_name) {
            return Err(self.error_message(&format!(
                "*** Expected class name, found \"{}\".",
                class_name
            )));
        }

        self.current_super_class = format!("objj_getClass(\"{}\").super_class", class_name);
        self.current_super_meta_class =
            format!("objj_getMetaClass(\"{}\").super_class", class_name);
        self.current_class = class_name.clone();
        self.current_selector = String::new();

        let mut token = tokens.skip_ws();

        if token.as_deref() == Some(T_OPEN_PAREN) {
            // Category.
            token = tokens.skip_ws();
            if token.as_deref() == Some(T_CLOSE_PAREN) {
                return Err(self.error_message("*** Can't have empty category name."));
            }
            if tokens.skip_ws().as_deref() != Some(T_CLOSE_PAREN) {
                return Err(self.error_message("*** Improper category definition."));
            }
            push_s(
                buffer,
                format!(
                    "{{\nvar the_class = objj_getClass(\"{}\")\n",
                    class_name
                ),
            );
            push_s(
                buffer,
                format!(
                    "if(!the_class) throw new SyntaxError(\"*** Could not find class \\\"{}\\\"\");\n",
                    class_name
                ),
            );
            push_s(buffer, "var meta_class = the_class.isa;");
        } else {
            if token.as_deref() == Some(T_COLON) {
                token = tokens.skip_ws();
                let t = token.clone().unwrap_or_default();
                if !is_identifier(&t) {
                    return Err(self
                        .error_message(&format!("*** Expected class name, found \"{}\".", t)));
                }
                superclass_name = t;
                token = tokens.skip_ws();
            }

            push_s(
                buffer,
                format!(
                    "{{var the_class = objj_allocateClassPair({}, \"{}\"),\nmeta_class = the_class.isa;",
                    superclass_name, class_name
                ),
            );

            if token.as_deref() == Some(T_OPEN_BRACE) {
                let mut ivar_names: HashMap<String, i32> = HashMap::new();
                let mut ivar_count = 0;
                let mut declaration: Vec<String> = Vec::new();
                let mut attributes: Option<HashMap<String, AttrVal>> = None;
                let mut accessors: Vec<(String, HashMap<String, AttrVal>)> = Vec::new();
                let mut types: Vec<String> = Vec::new();

                loop {
                    token = tokens.skip_ws();
                    match token.as_deref() {
                        None => break,
                        Some(T_CLOSE_BRACE) => break,
                        Some(T_PREPROCESSOR) => {
                            let t = tokens.next().unwrap_or_default();
                            if t == T_ACCESSORS {
                                attributes = Some(self.accessors(tokens)?);
                            } else if t != T_OUTLET {
                                return Err(self.error_message(&format!(
                                    "*** Unexpected '@{}' in ivar declaration.",
                                    t
                                )));
                            } else {
                                types.push(format!("@{}", t));
                            }
                        }
                        Some(T_SEMICOLON) => {
                            if ivar_count == 0 {
                                push_s(buffer, "class_addIvars(the_class, [");
                            } else {
                                push_s(buffer, ", ");
                            }
                            ivar_count += 1;

                            let name = declaration.last().cloned().unwrap_or_default();

                            if self.flags & FLAG_INCLUDE_TYPE_SIGNATURES != 0 {
                                let type_sig = if types.len() > 1 {
                                    types[..types.len() - 1].join(" ")
                                } else {
                                    String::new()
                                };
                                push_s(
                                    buffer,
                                    format!("new objj_ivar(\"{}\", \"{}\")", name, type_sig),
                                );
                            } else {
                                push_s(buffer, format!("new objj_ivar(\"{}\")", name));
                            }

                            ivar_names.insert(name.clone(), 1);
                            declaration.clear();
                            types.clear();

                            if let Some(attrs) = attributes.take() {
                                accessors.push((name, attrs));
                            }
                        }
                        Some(t) => {
                            declaration.push(t.to_string());
                            types.push(t.to_string());
                        }
                    }
                }

                if !declaration.is_empty() {
                    return Err(self.error_message("*** Expected ';' in ivar declaration."));
                }
                if ivar_count > 0 {
                    push_s(buffer, "]);\n");
                }
                if token.is_none() {
                    return Err(self.error_message("*** Expected '}'"));
                }

                self.set_class_info(
                    &class_name,
                    if superclass_name == "Nil" {
                        None
                    } else {
                        Some(superclass_name.clone())
                    },
                    ivar_names.clone(),
                );

                let all_ivar_names = self.all_ivar_names_for_class(&class_name);

                for (ivar_name, accessor) in &accessors {
                    let property = accessor
                        .get("property")
                        .and_then(|a| a.as_str())
                        .unwrap_or(ivar_name)
                        .to_string();

                    // Getter.
                    let getter_name = accessor
                        .get("getter")
                        .and_then(|a| a.as_str())
                        .unwrap_or(&property)
                        .to_string();
                    let getter_code =
                        format!("(id){}\n{{\nreturn {};\n}}", getter_name, ivar_name);

                    if !instance_methods.borrow().atoms.is_empty() {
                        push_s(&instance_methods, ",\n");
                    }
                    let mut lx = Lexer::new(&getter_code);
                    let m = self.method(&mut lx, &all_ivar_names)?;
                    push_b(&instance_methods, m);

                    // Setter.
                    if accessor.contains_key("readonly") {
                        continue;
                    }

                    let setter_name = match accessor.get("setter").and_then(|a| a.as_str()) {
                        Some(s) => s.to_string(),
                        None => {
                            let start = if property.starts_with('_') { 1 } else { 0 };
                            let prefix = if start == 1 { "_" } else { "" };
                            let first = property[start..start + 1].to_uppercase();
                            let rest = &property[start + 1..];
                            format!("{}set{}{}:", prefix, first, rest)
                        }
                    };

                    let mut setter_code =
                        format!("(void){}(id)newValue\n{{\n", setter_name);
                    if accessor.contains_key("copy") {
                        setter_code.push_str(&format!(
                            "if ({} !== newValue)\n{} = [newValue copy];\n}}",
                            ivar_name, ivar_name
                        ));
                    } else {
                        setter_code.push_str(&format!("{} = newValue;\n}}", ivar_name));
                    }

                    if !instance_methods.borrow().atoms.is_empty() {
                        push_s(&instance_methods, ",\n");
                    }
                    let mut lx = Lexer::new(&setter_code);
                    let m = self.method(&mut lx, &all_ivar_names)?;
                    push_b(&instance_methods, m);
                }
            } else {
                tokens.previous();
            }

            push_s(buffer, "objj_registerClassPair(the_class);\n");
        }

        let ivar_names = self.all_ivar_names_for_class(&class_name);

        loop {
            token = tokens.skip_ws();
            match token.as_deref() {
                None => break,
                Some(T_PLUS) => {
                    self.class_method = true;
                    if !class_methods.borrow().atoms.is_empty() {
                        push_s(&class_methods, ", ");
                    }
                    let class_vars = self.class_vars.clone();
                    let m = self.method(tokens, &class_vars)?;
                    push_b(&class_methods, m);
                }
                Some(T_MINUS) => {
                    self.class_method = false;
                    if !instance_methods.borrow().atoms.is_empty() {
                        push_s(&instance_methods, ", ");
                    }
                    let m = self.method(tokens, &ivar_names)?;
                    push_b(&instance_methods, m);
                }
                Some(T_HASH) => {
                    self.hash(tokens, buffer)?;
                }
                Some(T_PREPROCESSOR) => {
                    let t = tokens.next().unwrap_or_default();
                    if t == T_END {
                        break;
                    } else {
                        return Err(self.error_message(&format!(
                            "*** Expected \"@end\", found \"@{}\".",
                            t
                        )));
                    }
                }
                Some(_) => {}
            }
        }

        if !instance_methods.borrow().atoms.is_empty() {
            push_s(buffer, "class_addMethods(the_class, [");
            push_b(buffer, instance_methods);
            push_s(buffer, "]);\n");
        }
        if !class_methods.borrow().atoms.is_empty() {
            push_s(buffer, "class_addMethods(meta_class, [");
            push_b(buffer, class_methods);
            push_s(buffer, "]);\n");
        }

        push_s(buffer, "}");
        self.current_class = String::new();
        Ok(())
    }

    fn import(&mut self, tokens: &mut Lexer) -> R<()> {
        let mut url_string = String::new();
        let token = tokens.skip_ws().unwrap_or_default();
        let is_quoted = token != T_LESS_THAN;

        if token == T_LESS_THAN {
            loop {
                match tokens.next() {
                    Some(t) if t == T_GREATER_THAN => break,
                    Some(t) => url_string.push_str(&t),
                    None => {
                        return Err(self.error_message("*** Unterminated import statement."))
                    }
                }
            }
        } else if token.chars().next() == Some(T_DOUBLE_QUOTE) {
            // Strip surrounding quotes.
            url_string = token[1..token.len() - 1].to_string();
        } else {
            return Err(self.error_message(&format!(
                "*** Expecting '<' or '\"', found \"{}\".",
                token
            )));
        }

        if is_quoted {
            push_s(&self.buffer, format!("// @import \"{}\"", url_string));
        } else {
            push_s(&self.buffer, format!("// @import <{}>", url_string));
        }

        self.dependencies.push(Dependency {
            url: url_string,
            is_local: is_quoted,
        });
        Ok(())
    }

    // --- method ---

    fn method(&mut self, tokens: &mut Lexer, ivar_names: &HashMap<String, i32>) -> R<Buf> {
        let buffer = StringBuffer::new_buf();
        let mut selector = String::new();
        let mut parameters: Vec<String> = Vec::new();
        let mut types: Vec<Option<String>> = vec![None];

        let mut token = tokens.skip_ws();
        loop {
            match token.as_deref() {
                None => break,
                Some(T_OPEN_BRACE) | Some(T_SEMICOLON) => break,
                Some(T_COLON) => {
                    let mut ty = String::new();
                    selector.push(':');
                    token = tokens.skip_ws();

                    if token.as_deref() == Some(T_OPEN_PAREN) {
                        loop {
                            token = tokens.skip_ws();
                            match token.as_deref() {
                                Some(T_CLOSE_PAREN) | None => break,
                                Some(t) => ty.push_str(t),
                            }
                        }
                        token = tokens.skip_ws();
                    }

                    types.push(if ty.is_empty() { None } else { Some(ty) });
                    let param = token.clone().unwrap_or_default();
                    parameters.push(param.clone());

                    if ivar_names.contains_key(&param) {
                        eprintln!(
                            "{}",
                            self.error_message(&format!(
                                "*** Warning: Parameter name shadows ivar: {}",
                                param
                            ))
                        );
                    }
                }
                Some(T_OPEN_PAREN) => {
                    let mut ty = String::new();
                    loop {
                        token = tokens.skip_ws();
                        match token.as_deref() {
                            Some(T_CLOSE_PAREN) | None => break,
                            Some(t) => ty.push_str(t),
                        }
                    }
                    types[0] = if ty.is_empty() { None } else { Some(ty) };
                }
                Some(T_COMMA) => {
                    let a = tokens.skip_ws();
                    if a.as_deref() != Some(T_PERIOD)
                        || tokens.next().as_deref() != Some(T_PERIOD)
                        || tokens.next().as_deref() != Some(T_PERIOD)
                    {
                        return Err(self.error_message("*** Argument list expected after ','."));
                    }
                }
                Some(t) => {
                    selector.push_str(t);
                }
            }
            token = tokens.skip_ws();
        }

        if token.as_deref() == Some(T_SEMICOLON) {
            token = tokens.skip_ws();
            if token.as_deref() != Some(T_OPEN_BRACE) {
                return Err(self.error_message("Invalid semi-colon in method declaration."));
            }
        }

        push_s(&buffer, "new objj_method(sel_getUid(\"");
        push_s(&buffer, selector.clone());
        push_s(&buffer, "\"), function");

        self.current_selector = selector.clone();

        if self.flags & FLAG_INCLUDE_DEBUG_SYMBOLS != 0 {
            push_s(
                &buffer,
                format!(
                    " ${}__{}",
                    self.current_class,
                    selector.replace(':', "_")
                ),
            );
        }

        push_s(&buffer, "(self, _cmd");
        for p in &parameters {
            push_s(&buffer, ", ");
            push_s(&buffer, p.clone());
        }
        push_s(&buffer, ")\n{");

        // Set current ivars for self. prefixing, excluding parameters.
        let mut method_ivars = ivar_names.clone();
        for p in &parameters {
            method_ivars.remove(p);
        }
        let saved_ivars = self.current_ivar_names.take();
        self.current_ivar_names = Some(method_ivars);

        let body = self.preprocess(
            tokens,
            None,
            Some(T_CLOSE_BRACE),
            Some(T_OPEN_BRACE),
            None,
        )?;
        if let PreResult::Buf(b) = body {
            push_b(&buffer, b);
        }

        self.current_ivar_names = saved_ivars;

        push_s(&buffer, "\n}");

        if self.flags & FLAG_INCLUDE_DEBUG_SYMBOLS != 0 {
            push_s(&buffer, format!(",{}", json_types(&types)));
        }

        push_s(&buffer, ")");
        self.current_selector = String::new();
        Ok(buffer)
    }

    // --- @selector(...) ---

    fn selector(&mut self, tokens: &mut Lexer, buffer: &Buf) -> R<()> {
        push_s(buffer, "sel_getUid(\"");

        if tokens.skip_ws().as_deref() != Some(T_OPEN_PAREN) {
            return Err(self.error_message("*** Expected '('"));
        }

        let selector = tokens.skip_ws().unwrap_or_default();
        if selector == T_CLOSE_PAREN {
            return Err(self.error_message("*** Unexpected ')', can't have empty @selector()"));
        }
        push_s(buffer, selector.clone());

        let re_digits = Regex::new(r"^\d+$").unwrap();
        let re_startok = Regex::new(r"^(\w|$|:)").unwrap();
        let re_nonspace = Regex::new(r"\S").unwrap();

        let mut starting = true;
        loop {
            let token = match tokens.next() {
                Some(t) if t != T_CLOSE_PAREN => t,
                _ => break,
            };

            if (starting && re_digits.is_match(&token)) || !re_startok.is_match(&token) {
                if !re_nonspace.is_match(&token) {
                    if tokens.skip_ws().as_deref() == Some(T_CLOSE_PAREN) {
                        break;
                    } else {
                        return Err(self
                            .error_message("*** Unexpected whitespace in @selector()."));
                    }
                } else {
                    return Err(self.error_message(&format!(
                        "*** Illegal character '{}' in @selector().",
                        token
                    )));
                }
            }

            push_s(buffer, token.clone());
            starting = token == T_COLON;
        }

        push_s(buffer, "\")");
        Ok(())
    }

    // --- core preprocess loop ---

    fn preprocess(
        &mut self,
        tokens: &mut Lexer,
        a_string_buffer: Option<Buf>,
        terminator: Option<&str>,
        instigator: Option<&str>,
        mut tuple: Option<&mut Tuple>,
    ) -> R<PreResult> {
        let has_own_buffer = a_string_buffer.is_none();
        let buffer: Buf = match (&a_string_buffer, &tuple) {
            (Some(b), _) => b.clone(),
            (None, _) => StringBuffer::new_buf(),
        };
        if let Some(t) = tuple.as_deref_mut() {
            t.buffer = buffer.clone();
        }

        let mut count: i64 = 0;
        let mut prev_token = String::new();

        loop {
            let token = match tokens.next() {
                Some(t) => t,
                None => break,
            };
            // (token !== terminator) || count
            if Some(token.as_str()) == terminator && count == 0 {
                break;
            }

            if let Some(t) = tuple.as_deref_mut() {
                let mut bracket = false;
                if token == T_QUESTION_MARK {
                    t.closures[2] += 1;
                } else if token == T_OPEN_BRACE {
                    t.closures[0] += 1;
                } else if token == T_CLOSE_BRACE {
                    t.closures[0] -= 1;
                } else if token == T_OPEN_PAREN {
                    t.closures[1] += 1;
                } else if token == T_CLOSE_PAREN {
                    t.closures[1] -= 1;
                } else {
                    // (token === ':' && closures[2]-- === 0 || (bracket = (token === ']')))
                    //   && closures[0]===0 && closures[1]===0
                    let left = if token == T_COLON {
                        let was_zero = t.closures[2] == 0;
                        t.closures[2] -= 1;
                        was_zero
                    } else {
                        bracket = token == T_CLOSE_BRACKET;
                        bracket
                    };

                    if left && t.closures[0] == 0 && t.closures[1] == 0 {
                        tokens.push();

                        let label = if bracket {
                            tokens.skip_whitespace(true)
                        } else {
                            tokens.previous()
                        };
                        let label_str = label.clone().unwrap_or_default();
                        let is_empty_label = is_whitespace_tok(&label_str);

                        let prev_ws = {
                            let p = tokens.previous();
                            p.map(|s| is_whitespace_tok(&s)).unwrap_or(false)
                        };

                        if is_empty_label || (is_identifier(&label_str) && prev_ws) {
                            tokens.push();

                            let mut last = tokens.skip_whitespace(true);
                            let mut operator_check = true;
                            let mut is_double_operator = false;

                            if last.as_deref() == Some("+") || last.as_deref() == Some("-") {
                                let cur = last.clone();
                                if tokens.previous() != cur {
                                    operator_check = false;
                                } else {
                                    last = tokens.skip_whitespace(true);
                                    is_double_operator = true;
                                }
                            }

                            tokens.pop();
                            tokens.pop();

                            let last_ok = {
                                let l = last.clone().unwrap_or_default();
                                let last_char = l.chars().last();
                                (!is_double_operator && l == T_CLOSE_BRACE)
                                    || l == T_CLOSE_PAREN
                                    || l == T_CLOSE_BRACKET
                                    || l == T_PERIOD
                                    || is_number(&l)
                                    || last_char == Some('"')
                                    || last_char == Some('\'')
                                    || (is_identifier(&l)
                                        && !matches!(
                                            l.as_str(),
                                            "new" | "return" | "case" | "var"
                                        ))
                            };

                            if operator_check && last_ok {
                                if is_empty_label {
                                    t.label = Some(":".to_string());
                                } else {
                                    let mut lbl = label_str.clone();
                                    if !bracket {
                                        lbl.push(':');
                                    }
                                    t.label = Some(lbl);

                                    // Remove the label atom (and the atom before it)
                                    // from the buffer, matching the JS truncation.
                                    truncate_at_label(&buffer, &label_str);
                                }
                                return Ok(PreResult::Bool(!bracket));
                            }

                            if bracket {
                                return Ok(PreResult::Bool(false));
                            }
                        }

                        tokens.pop();

                        if bracket {
                            return Ok(PreResult::Bool(false));
                        }
                    }
                }

                if t.closures[2] < 0 {
                    t.closures[2] = 0;
                }
            }

            if let Some(inst) = instigator {
                if token == inst {
                    count += 1;
                } else if Some(token.as_str()) == terminator {
                    count -= 1;
                }
            }

            if token == T_FUNCTION {
                let mut accumulator = String::new();
                let mut t2 = tokens.next();
                loop {
                    match &t2 {
                        Some(tt)
                            if tt != T_OPEN_PAREN
                                && !Regex::new(r"^\w").unwrap().is_match(tt) =>
                        {
                            accumulator.push_str(tt);
                            t2 = tokens.next();
                        }
                        _ => break,
                    }
                }

                if t2.as_deref() == Some(T_OPEN_PAREN) {
                    if instigator == Some(T_OPEN_PAREN) {
                        count += 1;
                    }
                    push_s(&buffer, format!("function{}(", accumulator));
                    if let Some(t) = tuple.as_deref_mut() {
                        t.closures[1] += 1;
                    }
                } else {
                    push_s(
                        &buffer,
                        format!("var {} = function", t2.unwrap_or_default()),
                    );
                }
            } else if token == T_PREPROCESSOR {
                self.directive(tokens, &buffer)?;
            } else if token == T_HASH {
                self.hash(tokens, &buffer)?;
            } else if token == T_OPEN_BRACKET {
                self.brackets(tokens, &buffer)?;
            } else {
                let expanded = self.try_expand_macro(&token, tokens)?;
                if let Some(exp) = expanded {
                    push_s(&buffer, exp);
                } else if self.should_prefix_with_self(&token, &prev_token) {
                    push_s(&buffer, format!("self.{}", token));
                } else {
                    push_s(&buffer, token.clone());
                }
            }

            if !is_whitespace_tok(&token) {
                prev_token = token.clone();
            }
        }

        if tuple.is_some() {
            return Err(
                self.error_message("*** Expected ']' - Unterminated message send or array.")
            );
        }

        if has_own_buffer {
            Ok(PreResult::Buf(buffer))
        } else {
            Ok(PreResult::Unit)
        }
    }
}

/// Mirror `buffer.atoms.length = cnt` truncation logic in JS `preprocess`.
fn truncate_at_label(buffer: &Buf, label: &str) {
    let mut buf = buffer.borrow_mut();
    let mut cnt: i64 = buf.atoms.len() as i64;
    loop {
        let idx = cnt;
        cnt -= 1;
        let is_label = if idx >= 0 && (idx as usize) < buf.atoms.len() {
            matches!(&buf.atoms[idx as usize], Atom::S(s) if s == label)
        } else {
            false
        };
        if is_label {
            break;
        }
        if cnt < -1 {
            break;
        }
    }
    let new_len = cnt.max(0) as usize;
    if new_len <= buf.atoms.len() {
        buf.atoms.truncate(new_len);
    }
}

/// JSON.stringify of the types array (used for debug symbols).
fn json_types(types: &[Option<String>]) -> String {
    let mut s = String::from("[");
    for (i, t) in types.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        match t {
            Some(v) => {
                s.push('"');
                for c in v.chars() {
                    match c {
                        '"' => s.push_str("\\\""),
                        '\\' => s.push_str("\\\\"),
                        _ => s.push(c),
                    }
                }
                s.push('"');
            }
            None => s.push_str("null"),
        }
    }
    s.push(']');
    s
}

// ---------------------------------------------------------------------------
// Numeric expression evaluator (replaces JS `new Function(...)`)
// ---------------------------------------------------------------------------

fn eval_numeric(expr: &str) -> Option<f64> {
    let toks = tokenize_expr(expr)?;
    let mut parser = ExprParser { toks, pos: 0 };
    let v = parser.parse_expr(0)?;
    if parser.pos != parser.toks.len() {
        return None;
    }
    Some(v)
}

#[derive(Clone, PartialEq, Debug)]
enum ETok {
    Num(f64),
    Op(String),
    LParen,
    RParen,
}

fn tokenize_expr(expr: &str) -> Option<Vec<ETok>> {
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            out.push(ETok::Num(s.parse::<f64>().ok()?));
            continue;
        }
        if c == '(' {
            out.push(ETok::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            out.push(ETok::RParen);
            i += 1;
            continue;
        }
        // Two-char operators.
        let two: String = if i + 1 < chars.len() {
            chars[i..i + 2].iter().collect()
        } else {
            String::new()
        };
        if matches!(two.as_str(), "&&" | "||" | "==" | "!=" | "<=" | ">=") {
            out.push(ETok::Op(two));
            i += 2;
            continue;
        }
        if "+-*/%<>!&|^~".contains(c) {
            out.push(ETok::Op(c.to_string()));
            i += 1;
            continue;
        }
        return None;
    }
    Some(out)
}

struct ExprParser {
    toks: Vec<ETok>,
    pos: usize,
}

impl ExprParser {
    fn peek(&self) -> Option<&ETok> {
        self.toks.get(self.pos)
    }

    // Binary precedence for an operator; higher binds tighter.
    fn bin_prec(op: &str) -> Option<u8> {
        Some(match op {
            "||" => 1,
            "&&" => 2,
            "|" => 3,
            "^" => 4,
            "&" => 5,
            "==" | "!=" => 6,
            "<" | ">" | "<=" | ">=" => 7,
            "+" | "-" => 8,
            "*" | "/" | "%" => 9,
            _ => return None,
        })
    }

    fn parse_expr(&mut self, min_prec: u8) -> Option<f64> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Some(ETok::Op(o)) => o.clone(),
                _ => break,
            };
            let prec = match Self::bin_prec(&op) {
                Some(p) if p >= min_prec => p,
                _ => break,
            };
            self.pos += 1;
            let rhs = self.parse_expr(prec + 1)?;
            lhs = apply_bin(&op, lhs, rhs)?;
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<f64> {
        match self.peek() {
            Some(ETok::Op(o)) if matches!(o.as_str(), "!" | "-" | "+" | "~") => {
                let o = o.clone();
                self.pos += 1;
                let v = self.parse_unary()?;
                Some(match o.as_str() {
                    "!" => {
                        if v == 0.0 {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    "-" => -v,
                    "+" => v,
                    "~" => !(v as i64) as f64,
                    _ => return None,
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Option<f64> {
        match self.peek().cloned() {
            Some(ETok::Num(n)) => {
                self.pos += 1;
                Some(n)
            }
            Some(ETok::LParen) => {
                self.pos += 1;
                let v = self.parse_expr(0)?;
                if self.peek() == Some(&ETok::RParen) {
                    self.pos += 1;
                    Some(v)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn apply_bin(op: &str, a: f64, b: f64) -> Option<f64> {
    let bl = |x: bool| if x { 1.0 } else { 0.0 };
    Some(match op {
        "||" => bl(a != 0.0 || b != 0.0),
        "&&" => bl(a != 0.0 && b != 0.0),
        "|" => ((a as i64) | (b as i64)) as f64,
        "^" => ((a as i64) ^ (b as i64)) as f64,
        "&" => ((a as i64) & (b as i64)) as f64,
        "==" => bl(a == b),
        "!=" => bl(a != b),
        "<" => bl(a < b),
        ">" => bl(a > b),
        "<=" => bl(a <= b),
        ">=" => bl(a >= b),
        "+" => a + b,
        "-" => a - b,
        "*" => a * b,
        "/" => a / b,
        "%" => a % b,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compile Objective-J source code to JavaScript.
pub fn compile(source: &str, opts: &CompileOptions) -> Result<CompileResult, String> {
    Preprocessor::run(source, opts)
}
