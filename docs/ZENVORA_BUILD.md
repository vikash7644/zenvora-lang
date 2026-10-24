# Zenvora Build (early CLI)

`zvbuild` is the first standalone project-build tool for Zenvora Lang. It is intentionally a small, strict MVP: it reads a `Zenvora.toml` manifest, compile-checks the entry source, and asks the installed `zv` compiler to emit a versioned bytecode artifact. It does not call Cargo to build user projects.

## Build the tool

The current compiler is still bootstrapped in Rust, so Cargo is required to build the compiler and `zvbuild` themselves:

```sh
cargo build --locked --bins
cargo test --locked --all-targets
```

For installed binaries, put both `zv` and `zvbuild` on `PATH`. If the compiler is installed elsewhere, set `ZENVORA_COMPILER=/path/to/zv`.

## Start and build a project

```sh
zvbuild init hello-app
zvbuild build hello-app
zv --run-bytecode hello-app/build/main.zvb
zvbuild clean hello-app
```

A generated project contains:

```text
hello-app/
  Zenvora.toml
  main.zv
  build/main.zvb   # generated artifact
```

Manifest format:

```toml
name = "hello-app"
entry = "main.zv"
output = "build/main.zvb"
```

Only `name`, `entry`, and `output` are accepted. Entry/output paths must be relative and may not traverse outside the project. The tool refuses to overwrite a manifest or starter source during `init` and only removes the configured artifact during `clean`.

## Current limitations

- This is an MVP, not a Cargo-equivalent package manager yet: no dependency graph, lockfile, registry, build scripts, cross-compilation, or plugin system.
- `zvbuild` invokes the existing `zv` compiler for checking and bytecode emission; the self-hosted compiler is still in progress.
- `Zenvora.toml` uses a deliberately tiny key/value subset rather than a general TOML parser.
- Rust/Cargo remain necessary to build the compiler distribution until the Zenvora compiler can reproducibly compile itself and a native bootstrap/install path exists.
