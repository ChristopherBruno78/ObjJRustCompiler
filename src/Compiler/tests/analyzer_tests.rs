//! Integration tests for the method reachability analyzer.

use objj::analyzer::{AnalyzerOptions, CompiledEntry, MethodReachabilityAnalyzer};
use objj::compiler::{compile, CompileOptions};

/// Compile a snippet and wrap it as a single analyzer input entry.
fn compiled_entry(path: &str, source: &str) -> CompiledEntry {
    let code = compile(source, &CompileOptions::default())
        .expect("compile")
        .code;
    CompiledEntry {
        path: path.to_string(),
        source: source.to_string(),
        code,
    }
}

#[test]
fn always_keep_selectors_are_reachable() {
    let entries = vec![compiled_entry(
        "Foo.j",
        "@implementation Foo\n- (void)bar { }\n@end\n",
    )];
    let mut analyzer = MethodReachabilityAnalyzer::new(&entries, AnalyzerOptions::default());
    let result = analyzer.analyze("main");

    // Framework-essential selectors are always preserved.
    assert!(result.reachable_selectors.contains("init"));
    assert!(result.reachable_selectors.contains("alloc"));
    assert!(result.reachable_selectors.contains("description"));
}

#[test]
fn declared_methods_are_counted() {
    let entries = vec![compiled_entry(
        "Foo.j",
        "@implementation Foo\n- (void)bar { }\n- (void)baz { }\n@end\n",
    )];
    let mut analyzer = MethodReachabilityAnalyzer::new(&entries, AnalyzerOptions::default());
    let result = analyzer.analyze("main");

    assert_eq!(result.stats.total_methods, 2);
}

#[test]
fn whitelisted_selector_is_preserved() {
    let entries = vec![compiled_entry(
        "Foo.j",
        "@implementation Foo\n- (void)customThing { }\n@end\n",
    )];
    let opts = AnalyzerOptions {
        whitelist: vec!["customThing".to_string()],
        ..AnalyzerOptions::default()
    };
    let mut analyzer = MethodReachabilityAnalyzer::new(&entries, opts);
    let result = analyzer.analyze("main");

    assert!(result.reachable_selectors.contains("customThing"));
}

#[test]
fn empty_input_has_no_methods() {
    let entries: Vec<CompiledEntry> = Vec::new();
    let mut analyzer = MethodReachabilityAnalyzer::new(&entries, AnalyzerOptions::default());
    let result = analyzer.analyze("main");
    assert_eq!(result.stats.total_methods, 0);
}
