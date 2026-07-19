//! CLI for the Objective-J compiler / assembler.
//!
//! Usage:
//!   objjc compile <file.j> [--debug] [--types]
//!   objjc bundle  <main.j> [--base DIR] [--framework DIR]... [--tree-shake]
//!                         [--mode safe|moderate|aggressive|off] [--minify]
//!                         [--onload] [--verbose] [-o OUT]
//!   objjc exe     <main.j> [bundle options] [-o OUT]

use objj::assembler::{Assembler, AssembleOptions, BundleOptions};
use objj::compiler::{compile, CompileOptions};
use std::path::PathBuf;
use std::process::{exit, Command};

/// The Objective-J runtime, embedded so `exe` works no matter where objjc
/// is installed.
const RUNTIME_JS: &str = include_str!("../Runtime/Runtime.js");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        exit(1);
    }

    let result = match args[0].as_str() {
        "compile" => cmd_compile(&args[1..]),
        "bundle" => cmd_bundle(&args[1..]),
        "exe" => cmd_exe(&args[1..]),
        "-h" | "--help" | "help" => {
            usage();
            return;
        }
        other => Err(format!("Unknown command: {}", other)),
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        exit(1);
    }
}

fn usage() {
    eprintln!(
        "objjc - Objective-J compiler & assembler\n\n\
         Commands:\n  \
           compile <file.j> [--debug] [--types] [-o OUT]\n  \
           bundle  <main.j> [--base DIR] [--framework DIR]... [--tree-shake]\n          \
                   [--mode safe|moderate|aggressive|off] [--minify] [--onload]\n          \
                   [--verbose] [-o OUT]\n  \
           exe     <main.j> [bundle options] [-o OUT]\n          \
                   (bundles with the runtime and compiles a native binary via bun)"
    );
}

fn cmd_compile(args: &[String]) -> Result<(), String> {
    let mut file: Option<String> = None;
    let mut out: Option<String> = None;
    let mut opts = CompileOptions::default();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--debug" => opts.include_debug_symbols = true,
            "--types" => opts.include_type_signatures = true,
            "-o" => {
                i += 1;
                out = Some(args.get(i).ok_or("-o requires a value")?.clone());
            }
            f if !f.starts_with('-') => file = Some(f.to_string()),
            other => return Err(format!("Unknown option: {}", other)),
        }
        i += 1;
    }

    let file = file.ok_or("compile requires a file argument")?;
    let source = std::fs::read_to_string(&file).map_err(|e| format!("read {}: {}", file, e))?;
    opts.source_url = file.clone();

    let result = compile(&source, &opts)?;

    match out {
        Some(path) => std::fs::write(&path, &result.code)
            .map_err(|e| format!("write {}: {}", path, e))?,
        None => print!("{}", result.code),
    }
    eprintln!("{} dependencies", result.dependencies.len());
    Ok(())
}

/// Result of parsing the shared bundle-style options used by `bundle` and
/// `exe`.
struct ParsedBundle {
    out: Option<String>,
    assembler: Assembler,
    bopts: BundleOptions,
    main_file: String,
}

fn parse_bundle_args(args: &[String]) -> Result<ParsedBundle, String> {
    let mut main_file: Option<String> = None;
    let mut out: Option<String> = None;
    let mut aopts = AssembleOptions::default();
    let mut bopts = BundleOptions {
        tree_shake_preserve_accessors: true,
        tree_shake_preserve_kvc: true,
        tree_shake_stats: true,
        ..Default::default()
    };

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => {
                i += 1;
                aopts.base_path = PathBuf::from(args.get(i).ok_or("--base requires a value")?);
            }
            "--framework" => {
                i += 1;
                aopts
                    .framework_paths
                    .push(PathBuf::from(args.get(i).ok_or("--framework requires a value")?));
            }
            "--tree-shake" => bopts.tree_shake = true,
            "--mode" => {
                i += 1;
                bopts.tree_shake_mode =
                    Some(args.get(i).ok_or("--mode requires a value")?.clone());
            }
            "--minify" => bopts.minify = true,
            "--onload" => bopts.append_window_onload = true,
            "--verbose" => bopts.tree_shake_verbose = true,
            "--debug" => aopts.include_debug_symbols = true,
            "--types" => aopts.include_type_signatures = true,
            "-o" => {
                i += 1;
                out = Some(args.get(i).ok_or("-o requires a value")?.clone());
            }
            f if !f.starts_with('-') => main_file = Some(f.to_string()),
            other => return Err(format!("Unknown option: {}", other)),
        }
        i += 1;
    }

    let main_file = main_file.ok_or("a main file argument is required")?;

    Ok(ParsedBundle {
        out,
        assembler: Assembler::new(aopts),
        bopts,
        main_file,
    })
}

fn cmd_bundle(args: &[String]) -> Result<(), String> {
    let mut parsed = parse_bundle_args(args)?;
    let result = parsed.assembler.bundle(&parsed.main_file, &parsed.bopts)?;

    match parsed.out {
        Some(path) => std::fs::write(&path, &result.code)
            .map_err(|e| format!("write {}: {}", path, e))?,
        None => print!("{}", result.code),
    }

    eprintln!(
        "bundled {} files, {} bytes",
        result.file_count, result.total_size
    );
    if let Some(stats) = result.tree_shaking {
        eprintln!(
            "tree-shaking: {}/{} methods reachable",
            stats.reachable_methods, stats.total_methods
        );
    }
    Ok(())
}

fn cmd_exe(args: &[String]) -> Result<(), String> {
    let mut parsed = parse_bundle_args(args)?;

    // Default output name: the main file's stem (e.g. main.j -> main).
    let out = parsed.out.clone().unwrap_or_else(|| {
        PathBuf::from(&parsed.main_file)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "a.out".to_string())
    });

    let result = parsed.assembler.bundle(&parsed.main_file, &parsed.bopts)?;
    eprintln!(
        "bundled {} files, {} bytes",
        result.file_count, result.total_size
    );

    // Assemble the runnable program: runtime + bundled code + entry point.
    let mut program = String::with_capacity(RUNTIME_JS.len() + result.code.len() + 256);
    program.push_str(RUNTIME_JS);
    program.push('\n');
    program.push_str(&result.code);
    program.push_str(
        "\n// objjc exe entry point\n\
         if (typeof main === \"function\") { main(Bun.argv.slice(2), Bun.argv.length - 2); }\n",
    );

    // Stage the program in a temp file for bun to compile.
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("objjc-exe-{}.js", std::process::id()));
    std::fs::write(&tmp, &program)
        .map_err(|e| format!("write {}: {}", tmp.display(), e))?;

    let status = Command::new("bun")
        .arg("build")
        .arg("--compile")
        .arg(&tmp)
        .arg("--outfile")
        .arg(&out)
        .status()
        .map_err(|e| format!("failed to run bun (is it installed and on PATH?): {}", e));

    let _ = std::fs::remove_file(&tmp);
    let status = status?;

    if !status.success() {
        return Err(format!("bun build failed with status {}", status));
    }

    eprintln!("built executable: {}", out);
    Ok(())
}
