# QORVYNT-generated SVG icon

This example is a small self-hosting milestone: a QORVYNT `.qv` program emits an SVG asset using QORVYNT variables, string concatenation, and `show`.

## Generate the SVG

From the repository root, with the QORVYNT CLI built:

```sh
cargo run -- --render examples/qorvynt-icon.qv qorvynt-icon.svg
```

Or with an installed CLI:

```sh
qorvynt --render examples/qorvynt-icon.qv qorvynt-icon.svg
```

Open `qorvynt-icon.svg` in a browser or vector editor.

## Current scope

The QORVYNT program generates SVG markup on standard output. The `--render` CLI command runs the QORVYNT source program and writes its printed output to the requested text asset. File writing is deliberately a host CLI capability, not a contract/consensus VM builtin. Native PNG rasterization and ICO encoding are still future milestones.
