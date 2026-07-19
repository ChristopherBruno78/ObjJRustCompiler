# ObjJRustCompiler

`objjc` is an Objective-J compiler, assembler, and method tree-shaker — a Rust
port of the [Cappuccino ObjectiveJ Compiler](https://github.com/cappuccino/objj-compiler). 
It compiles `.j` source into JavaScript and can bundle a
whole application into a single output, optionally shaking out unused methods.

## What's here

- `src/Compiler/` — the Rust compiler crate (`Cargo.toml`, `install.sh`, tests)
  - `compiler.rs` — Objective-J → JavaScript compiler
  - `assembler.rs` — bundling and tree-shaking of compiled output
  - `analyzer.rs` — method / dependency analysis
  - `main.rs` — the `objjc` command-line front end
- `src/Runtime/Runtime.js` — the Objective-J runtime (embedded by `exe`)

## Building

Requires a [Rust toolchain](https://rustup.rs) (edition 2021).

```sh
make build
# or, directly:
cargo build --release --manifest-path src/Compiler/Cargo.toml
```

The compiled binary is written to `src/Compiler/target/release/objjc`.

## Installing

Install to `/usr/local/bin` after building:

```sh
sudo make install
# or
./src/Compiler/install.sh
```

`install.sh` builds automatically if the release binary is missing and uses
`sudo` only when the target directory isn't writable. It also copies the bundled
`src/Frameworks` into `$PREFIX/share/objj/Frameworks` so the compiler can resolve
framework imports (`@import <Foo.j>`) without a project-local copy.

Framework imports are searched in this order:

1. any `--framework DIR` paths passed on the command line,
2. a `Frameworks/` folder in the project (next to the `--base` directory),
3. the shared `$PREFIX/share/objj/Frameworks` installed alongside the binary
   (override with the `OBJJ_FRAMEWORKS_PATH` environment variable).

Install to a different location with `PREFIX`:

```sh
make install PREFIX="$HOME/.local"      # installs to ~/.local/bin
PREFIX="$HOME/.local" ./src/Compiler/install.sh
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

# Build a standalone native executable (bundle + runtime, compiled with bun)
objjc exe <main.j> [bundle options] [-o OUT]
```

`exe` accepts the same options as `bundle`. It bundles the application,
prepends the embedded Objective-J runtime (`src/Runtime/Runtime.js`), appends a
call to the program's `main(args, argc)`, and invokes
[`bun build --compile`](https://bun.sh) to produce a native binary. It requires
[bun](https://bun.sh) on your `PATH`. When `-o` is omitted, the output name is
derived from the main file's stem (e.g. `main.j` → `main`).

Run `objjc --help` for the full list of commands and options.
