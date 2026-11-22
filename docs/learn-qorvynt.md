# Learn QORVYNT: Beginner Guide

QORVYNT programs use the `.qv` extension. This guide introduces the syntax that is currently demonstrated by the repository's executable examples.

> **Implementation status:** QORVYNT is an early prototype. The current compiler and VM are Rust-hosted. The browser playground is a JavaScript demo runner and does not yet have full compiler/VM parity. AI teaching, generated voice, speech recognition, cartoon animation, GPU and USB APIs are roadmap work—not shipped features.

## Your first program

Create a file named `hello.qv`:

```qorvynt
define name := "QORVYNT"
show "Hello, " + name

define a := 10
define b := 20
show a + b
```

Expected output:

```text
Hello, QORVYNT
30
```

## Variables and reassignment

```qorvynt
define score := 10
score := score + 5
show score
```

Use `define` to create a binding. Use `:=` to update an existing variable.

## Conditions

```qorvynt
define age := 20

if age >= 18 {
    show "Adult"
} otherwise {
    show "Under 18"
}
```

## Loops

```qorvynt
define count := 1

repeat count <= 3 {
    show count
    count := count + 1
}
```

## Functions

```qorvynt
fn multiply(left, right) {
    return left * right
}

show multiply(6, 7)
```

Expected output: `42`.

## Arrays and maps

```qorvynt
define numbers := array(10, 20, 30)
show numbers[1]
show len(numbers)

define person := map("city", "Patna")
show get(person, "city")
```

## Run your code

The recommended source-controlled workflow is to download the appropriate prebuilt CLI from the [QORVYNT prototype releases](https://github.com/vikash7644/zenvora-lang/releases), then run:

```sh
qorvynt run hello.qv
qorvynt check hello.qv
```

The published v0.8.0 Prototype 1 binaries predate the newer `run` and `check` verbs. With that older release, use the compatible commands:

```sh
qorvynt hello.qv
qorvynt --check hello.qv
```

No Android Termux workflow is required. You can also inspect the source and examples on GitHub.

## Explore more examples

- [Language tour](../examples/qorvynt-language-tour.qv)
- [Loops](../examples/loops.qv)
- [Functions](../examples/functions.qv)
- [Collections](../examples/collections.qv)
- [Strings](../examples/strings.qv)
- [Getting started documentation](getting-started.md)
- [Project status and limitations](PROJECT_STATUS.md)

## What can QORVYNT build today?

The current prototype demonstrates general-purpose language basics: variables, expressions, conditions, loops, functions, strings, collections, modules and experimental contract syntax. The broader vision includes games, graphics, AI, web APIs, Web3 governance and permissioned hardware access. Those domain engines and libraries must be implemented and tested before they can be advertised as available.

## Learning roadmap

The planned QORVYNT Academy may add an animated AI tutor, voice conversation, spoken lessons, guided projects and adaptive exercises. These are planned capabilities; this guide is the current text-based learning resource.
