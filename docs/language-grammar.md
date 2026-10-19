# Zenvora Lang — Current Grammar Draft

Status: **draft, not frozen for v1.0**. This document describes the current line-oriented parser and is intended to make implemented behavior explicit. It is not a promise that the syntax is stable; unresolved semantics are listed at the end.

## Source layout

- One statement or block header per physical line.
- Blank lines and lines whose first non-whitespace character is `#` are ignored. `#` outside a string literal ends the rest of that physical line, so trailing comments are accepted too.
- A trailing semicolon is stripped by the current parser. The exact semicolon policy is not frozen yet.
- Braces delimit `if`, `repeat`, and function blocks. Indentation is not used to determine block nesting.
- Identifiers are ASCII: a letter or underscore followed by letters, digits, or underscores.

## Statement grammar (EBNF)

```ebnf
program          = { line } ;
line             = blank-line | comment-line | function-header | statement | block-line ;
blank-line       = whitespace ;
comment-line     = { whitespace }, "#", { any-character } ;
function-header  = "fn", identifier, "(", [ parameters ], ")", "{" ;
parameters       = identifier, { ",", identifier } ;
identifier       = ( letter | "_" ), { letter | digit | "_" } ;
integer          = digit, { digit } ;
string           = quote, { string-character | escape }, quote ;
string-character = any-character except quote, backslash, CR, or LF ;
escape           = backslash, ( quote | backslash | "n" | "r" | "t" ) ;
backslash        = U+005C ;
quote            = U+0022 ;
boolean          = "true" | "false" ;
whitespace       = space | tab ;
space            = U+0020 ;
tab              = U+0009 ;
digit            = "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" ;
letter           = "A"…"Z" | "a"…"z" ;

statement        = declaration
                 | assignment
                 | indexed-assignment
                 | show-statement
                 | print-statement
                 | call-statement
                 | return-statement
                 | if-header
                 | repeat-header
                 | break-statement
                 | continue-statement ;

declaration      = "define", identifier, ":=", expression ;
assignment       = identifier, ":=", expression ;
indexed-assignment = identifier, "[", expression, "]", ":=", expression ;
show-statement   = "show", expression ;
print-statement  = "print(", expression, ")" ;
call-statement   = "call", identifier, "(", [ arguments ], ")" ;
return-statement = "return" | "return", expression ;
if-header        = "if", expression, "{" ;
repeat-header    = "repeat", expression, "{" ;
break-statement  = "break" ;
continue-statement = "continue" ;
block-line       = "}" | "} otherwise {" ;
arguments        = expression, { ",", expression } ;
```

## Expression grammar and precedence

Operators are parsed from lowest to highest precedence in this order: `or`, `and`, comparisons, `+`/`-`, `*`/`/`, then unary `-`/`not` and postfix indexing/calls.

```ebnf
expression       = logical-or ;
logical-or       = logical-and, { "or", logical-and } ;
logical-and      = comparison, { "and", comparison } ;
comparison       = additive, [ comparison-op, additive ] ;
comparison-op    = "==" | "!=" | "<" | "<=" | ">" | ">=" ;
additive         = term, { ( "+" | "-" ), term } ;
term             = factor, { ( "*" | "/" ), factor } ;
factor           = ( "-" | "not" ), factor | postfix ;
postfix          = primary, { "[", expression, "]" } ;
primary          = integer | string | boolean
                 | identifier, [ "(", [ arguments ], ")" ]
                 | "(", expression, ")" ;
```

The parser currently accepts at most one comparison operator in a comparison expression; chained comparisons are not part of the grammar.

## Literals and built-in collections

- Integers are parsed as non-negative base-10 integer tokens and represented as signed 64-bit values; unary minus is a separate operator.
- Boolean literals are `true` and `false`.
- Strings use double quotes and support the escapes `\"`, `\\`, `\n`, `\r`, and `\t`. Other escapes are rejected.
- Arrays and maps are constructed through built-ins such as `array(...)` and `map(...)`; array indexing uses integer indices and map indexing uses string keys.
- `push` and `put` return updated collections; they do not mutate the source variable implicitly. Indexed assignment (`items[0] := value`) updates the bound collection.
- Built-ins include collection helpers such as `len`, `range`, `get`, `push`, `put`, `keys`, `values`, `first`, `last`, `reverse`, `split`, and `join`. The implementation's built-in registry is authoritative; this list is not exhaustive.

## Experimental contract DSL

The current contract lowering accepts this additional outer syntax:

```ebnf
contract-declaration = "contract", identifier, "{", { state-declaration | action-declaration | view-declaration }, "}" ;
state-declaration    = "state", identifier, ":=", expression ;
action-declaration   = "action", identifier, "(", [ parameters ], ")", "{", { statement | block-line }, "}" ;
view-declaration     = "view", identifier, "(", [ parameters ], ")", "{", { statement | block-line }, "}" ;
```

This is a **local execution prototype**: state is initialized for a single VM run and is not persisted as a consensus-backed ledger. Durable state transitions, atomic rollback, caller authorization, deterministic gas, replay, and state-root commitments are not implemented production guarantees. See `contract-execution-v1.md` for the proposed host/runtime design.

## Diagnostics and resource limits

- Lexer/parser diagnostics use one-based line and column positions; columns count Unicode scalar values, not terminal display cells.
- Source input is limited to 16 MiB before contract lowering and compilation.
- Each expression is limited to 1,000,000 tokens; compiled output is limited to 1,000,000 instructions.
- Strings are limited to 1 MiB; estimated runtime value and operand-stack budgets are 16 MiB each; estimated live runtime state is limited to 64 MiB; nested values are limited to 256 levels.
- These are defensive estimates and limits, not a guarantee of constant-time execution or a complete formal cost model.

## Syntax and semantics not frozen yet

- Stable rules for integer overflow, division edge cases, equality across value types, and comparison errors need a dedicated reference section and exhaustive regression coverage.
- Function return-value rules and control-flow fallthrough rules need a single normative description.
- Repeated trailing semicolons, `otherwise if`, and same-line multiple statements need an explicit accept/reject decision.
- Module imports and the contract/state/action/view DSL have separate implementation paths. The contract execution proposal is not production behavior; see `contract-execution-v1.md`.
- Bytecode v2 remains experimental until its format is frozen and independently security-reviewed.

Do not use this draft alone to claim Zenvora Lang v1.0 is complete.