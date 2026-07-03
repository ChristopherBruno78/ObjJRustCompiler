//! Integration tests for the Objective-J compiler front end.

use objj::compiler::{compile, CompileOptions, MacroValue};

fn compile_ok(source: &str) -> objj::compiler::CompileResult {
    compile(source, &CompileOptions::default()).expect("compilation should succeed")
}

#[test]
fn plain_javascript_passes_through() {
    let result = compile_ok("var x = 1;\n");
    assert!(result.code.contains("var x = 1;"));
    assert!(result.dependencies.is_empty());
}

#[test]
fn import_directives_become_dependencies() {
    let result = compile_ok("@import \"Foo.j\"\n@import <Framework/Bar.j>\nvar x = 1;\n");
    assert_eq!(result.dependencies.len(), 2);

    let local = &result.dependencies[0];
    assert_eq!(local.url, "Foo.j");
    assert!(local.is_local, "quoted import should be local");

    let framework = &result.dependencies[1];
    assert_eq!(framework.url, "Framework/Bar.j");
    assert!(!framework.is_local, "angle-bracket import should not be local");
}

#[test]
fn imports_are_commented_out_in_output() {
    let result = compile_ok("@import \"Foo.j\"\nvar x = 1;\n");
    // The @import line is preserved as a comment, not emitted as code.
    assert!(result.code.contains("// @import \"Foo.j\""));
}

#[test]
fn preprocessor_define_is_expanded() {
    let result = compile_ok("#define TValue 42\nvar x = TValue;\n");
    assert!(result.code.contains("var x = 42;"));
    assert!(!result.code.contains("TValue"), "macro should be expanded away");
    assert!(matches!(
        result.defines.get("TValue"),
        Some(MacroValue::Str(_))
    ));
}

#[test]
fn conditional_compilation_selects_active_branch() {
    let source = "#define ENABLED 1\n#if ENABLED\nvar on = 1;\n#else\nvar off = 1;\n#endif\n";
    let result = compile_ok(source);
    assert!(result.code.contains("var on = 1;"));
    assert!(!result.code.contains("var off = 1;"));
}

#[test]
fn implementation_generates_class_pair() {
    let result = compile_ok("@implementation Foo\n- (void)bar { }\n@end\n");
    assert!(result.code.contains("objj_allocateClassPair(Nil, \"Foo\")"));
    assert!(result.code.contains("sel_getUid(\"bar\")"));
}

#[test]
fn class_with_ivars_is_registered() {
    // Class info is recorded when an ivar block is present.
    let result = compile_ok("@implementation Foo\n{\n    int _count;\n}\n@end\n");
    assert!(result.classes.contains_key("Foo"));
}

#[test]
fn subclass_records_super_class_name() {
    let result = compile_ok("@implementation Foo : Bar\n{\n    int _count;\n}\n@end\n");
    let info = result.classes.get("Foo").expect("Foo should be registered");
    assert_eq!(info.super_class_name.as_deref(), Some("Bar"));
}

#[test]
fn message_send_keeps_receiver_when_selector_matches_ivar() {
    // Regression: a selector whose name is also an ivar/accessor used to drop
    // the receiver, e.g. `[b action]` compiling to `objj_msgSend(, "action")`.
    let source = "@implementation Button {\n    SEL action @accessors;\n}\n\
                  - (void)doSomething {\n    let a = [b action];\n}\n@end\n";
    let result = compile_ok(source);
    assert!(
        result.code.contains("objj_msgSend(b, \"action\")"),
        "receiver `b` should be preserved, got:\n{}",
        result.code
    );
    assert!(!result.code.contains("objj_msgSend(, "));
}

#[test]
fn source_url_defaults_to_anonymous() {
    let opts = CompileOptions::default();
    assert_eq!(opts.source_url, "(anonymous)");
}
