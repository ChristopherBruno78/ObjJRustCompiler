//! CLI for the Objective-J compiler / assembler.
//!
//! Usage:
//!   objjc compile <file.j> [--debug] [--types]
//!   objjc bundle  <main.j> [--base DIR] [--framework DIR]... [--tree-shake]
//!                         [--mode safe|moderate|aggressive|off] [--minify]
//!                         [--onload] [--verbose] [-o OUT]

use objj::assembler::{Assembler, AssembleOptions, BundleOptions};
use objj::compiler::{compile, CompileOptions};
use std::path::PathBuf;
use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        exit(1);
    }

    let result = match args[0].as_str() {
        "compile" => cmd_compile(&args[1..]),
        "bundle" => cmd_bundle(&args[1..]),
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
                   [--verbose] [-o OUT]"
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

fn cmd_bundle(args: &[String]) -> Result<(), String> {
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

    let main_file = main_file.ok_or("bundle requires a main file argument")?;

    let mut assembler = Assembler::new(aopts);
    let result = assembler.bundle(&main_file, &bopts)?;

    match out {
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
