# ObjJRustCompiler

`objjc` is an Objective-J compiler, assembler, and method tree-shaker — a Rust
port of the [Cappuccino ObjectiveJ Compiler](https://github.com/cappuccino/objj-compiler). 
It compiles `.j` source into JavaScript and can bundle a
whole application into a single output, optionally shaking out unused methods.

## What's here

- `src/compiler.rs` — Objective-J → JavaScript compiler
- `src/assembler.rs` — bundling and tree-shaking of compiled output
- `src/analyzer.rs` — method / dependency analysis
- `src/main.rs` — the `objjc` command-line front end

## Building

Requires a [Rust toolchain](https://rustup.rs) (edition 2021).

```sh
cargo build --release
```

The compiled binary is written to `target/release/objjc`.

## Installing

Install to `/usr/local/bin` after building:

```sh
sudo make install
# or
./install.sh
```

`install.sh` builds automatically if the release binary is missing and uses
`sudo` only when the target directory isn't writable.

Install to a different location with `PREFIX`:

```sh
make install PREFIX="$HOME/.local"      # installs to ~/.local/bin
PREFIX="$HOME/.local" ./install.sh
```

Remove it again with:

```sh
sudo make uninstall
```

## Usage

```sh
# Compile a single file to JavaScript (stdout, or -o FILE)
objjc compile <file.j> [--debug] [--types] [-o OUT]

# Bundle an application starting from a main file
objjc bundle <main.j> [--base DIR] [--framework DIR]... [--tree-shake] \
             [--mode safe|moderate|aggressive|off] [--minify] [--onload] \
             [--verbose] [-o OUT]
```

Run `objjc --help` for the full list of commands and options.
