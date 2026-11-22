# QORVYNT-generated SVG icon

This example is a small self-hosting milestone: a QORVYNT `.qv` program emits an SVG asset using QORVYNT variables, string concatenation, and `show`.

## Generate the SVG

From the repository root, with the QORVYNT CLI built:

```sh
cargo run -- examples/qorvynt-icon.qv > qorvynt-icon.svg
```

Or with an installed CLI:

```sh
qorvynt examples/qorvynt-icon.qv > qorvynt-icon.svg
```

Open `qorvynt-icon.svg` in a browser or vector editor.

## Current scope

The QORVYNT program generates SVG markup on standard output. Shell redirection saves that output as a file; the language does not yet have a native filesystem-writing API, PNG rasterizer, or ICO encoder. Those are separate compiler/runtime milestones, not features claimed by this example.
