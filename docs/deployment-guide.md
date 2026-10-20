# Deploy a Zenvora Lang project

This guide covers shipping a Zenvora Lang **CLI/script project** with the current `zv` runtime. It does not claim support for native web servers, mobile apps, browser execution by the real compiler, or production smart contracts.

## 1. Install the runtime

Choose the archive for your operating system from the [latest Zenvora Lang release](https://github.com/vikash7644/zenvora-lang/releases). The currently published v0.8.0 artifacts are prototypes; check the release notes and CI status before using them for anything important.

Extract the archive and ensure the `zv` executable is on your PATH. Verify it:

```sh
zv --version
zv --help
```

If there is no suitable prebuilt binary for your platform, install Rust/Cargo and build from the repository:

```sh
cargo build --locked --release
./target/release/zv --version
```

## 2. Create and test a project

```sh
zv --new my-project
zv --check my-project/main.zv
zv my-project/main.zv
```

For a repository checkout, replace `zv` with `cargo run --`, for example:

```sh
cargo run --locked -- --new my-project
cargo run --locked -- --check my-project/main.zv
cargo run --locked -- my-project/main.zv
```

Keep source files, imported modules, and test fixtures in version control. Relative module imports are resolved from the source file; test from the same directory layout you intend to deploy.

## 3. Add a deployment smoke test to GitHub Actions

Create `.github/workflows/zenvora-project.yml` in your own project:

```yaml
name: Zenvora project
on:
  push:
  pull_request:
jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Build Zenvora runtime
        run: |
          git clone --depth 1 https://github.com/vikash7644/zenvora-lang.git /tmp/zenvora-lang
          cargo build --locked --release --manifest-path /tmp/zenvora-lang/Cargo.toml
      - name: Check and run project
        run: |
          /tmp/zenvora-lang/target/release/zv --check main.zv
          /tmp/zenvora-lang/target/release/zv main.zv
```

Pin the runtime checkout to a reviewed commit or release tag before using this workflow for a production deployment. Add project-specific assertions/tests so CI checks expected behavior, not only that the program exits successfully.

## 4. Run on a Linux server

After validating the exact runtime and source revision, a simple scheduled or manual CLI program can be run from a deployment directory:

```sh
/path/to/zv --check /srv/my-project/main.zv
/path/to/zv /srv/my-project/main.zv
```

Use a dedicated unprivileged OS account, least-privilege file permissions, explicit working directories, logs, monitoring, and an external process supervisor for long-running tasks. The language runtime is not currently a web-server framework or process supervisor.

## Current deployment boundaries

- The compiler and VM are implemented in Rust. End users can run a prebuilt `zv` binary without installing Rust; rebuilding the runtime from source requires Rust/Cargo.
- The browser playground is a JavaScript demo runner, not the actual Rust compiler/VM. Do not treat it as full-language execution.
- The compiler HTTP API binds to loopback for local development. It is not a hosted public API and must not be exposed directly to the internet.
- Contract execution, ABI artifacts, and the Bytecode v2 format remain experimental. Do not deploy contracts holding real assets.
- There is not yet a stable package manager, dependency registry, lockfile for Zenvora modules, debugger, language server, or production security certification.

For release status and remaining gates, see [PROJECT_STATUS.md](PROJECT_STATUS.md). A workflow being green proves only the checks in that workflow; it does not remove these limitations.
