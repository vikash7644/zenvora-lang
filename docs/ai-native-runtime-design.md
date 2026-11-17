# QORVYNT AI-Native Runtime Design

Status: **Design proposal — not implemented yet**  
Scope: the QORVYNT language and runtime in this repository. This does not add blockchain or EVM behavior.

## Goal

Make QORVYNT a language in which developers can build capable AI applications and agent systems, and make it possible to implement increasingly capable assistants *using QORVYNT itself* over time.

A model call alone is not an agent runtime. The language needs explicit, testable primitives for structured data, asynchronous work, provider adapters, tool contracts, state, and policy enforcement.

## Non-goals

- Do not claim that QORVYNT contains or trains a frontier model.
- Do not hard-code one model vendor, API key, endpoint, or cloud service into the language core.
- Do not let generated code execute arbitrary shell commands or access the filesystem/network by default.
- Do not mix these AI runtime features with the separate Fleur Network / EVM work.
- Do not make proposed syntax appear executable before parser, compiler, VM, and tests support it.

## Proposed capability layers

### 1. Stable data and schema layer

Required foundations:
- JSON-compatible values and deterministic encode/decode.
- Explicit schema declarations with required fields, types, bounds, and validation errors.
- Structured result values that distinguish success, failure, cancellation, and timeout.
- Stable serialization rules for persisted agent state and tool arguments.

### 2. Async task runtime

Required foundations:
- \`async\` functions and \`await\` with deterministic task lifecycle semantics.
- Cancellation, deadlines, bounded concurrency, and resource accounting.
- No implicit unbounded background tasks.
- Tests for cancellation, timeout, ordering guarantees, and failure propagation.

### 3. Model-provider interface

A provider must implement a common host capability rather than being embedded in the language core. The interface should support:
- request messages and structured output schemas;
- model identifier and provider metadata;
- token / usage accounting when supplied by the provider;
- timeout, cancellation, retry policy, and typed errors;
- streaming as a later extension.

Credentials must be supplied by the host through named secrets/capabilities, never embedded in source files or bytecode. Provider adapters must be replaceable without changing application logic.

### 4. Tool contracts and permission system

Each tool must declare:
- a stable name and version;
- a validated input schema and output schema;
- required capabilities (for example, \`http.read\`, \`files.read\`, or \`calendar.write\`);
- whether it is read-only or has side effects;
- timeout and resource limits.

The runtime must deny undeclared capabilities by default. Side-effecting tools require explicit host policy and should support approval gates. Tool results are untrusted data, not instructions to the runtime.

### 5. Agent orchestration

An agent is a bounded state machine, not an unrestricted \`while true\` loop. It needs:
- explicit instructions and typed input/output;
- a bounded step budget and deadline;
- an allow-list of tools;
- structured state transitions;
- trace events for model calls, tool calls, errors, and decisions;
- deterministic replay where possible, with external responses recorded as fixtures.

### 6. Memory and retrieval

Memory should be explicit and scoped:
- short-lived run state;
- application-owned persistent records;
- optional retrieval/vector-store adapters behind capabilities.

Every memory operation needs a namespace, schema, retention policy, and access policy. The language must not silently persist prompts, credentials, or personal data.

### 7. Observability and evaluation

The runtime should expose structured traces, redacted logs, usage counters, and reproducible test fixtures. Agent tests should be able to replace model and tool providers with deterministic fakes. CI must test prompt-injection resistance at tool boundaries, schema rejection, capability denial, resource exhaustion, and state isolation.

## Illustrative future API (not valid QORVYNT syntax yet)

The following is a design sketch only. It must not be copied into a runnable example until the parser and runtime implement these constructs.

\`\`\`text
capability web_search: read
capability notes_store: read, write

tool search_web(query: SearchQuery) -> SearchResults
  requires web_search

agent research_assistant(input: ResearchRequest) -> ResearchReport
  tools [search_web, notes_store]
  max_steps 8
  timeout 30s
{
  let evidence = await search_web(input.question)
  let report = await model.generate(
    task: "summarize evidence",
    input: evidence,
    output: ResearchReport
  )
  return report
}
\`\`\`

The exact syntax is intentionally undecided. Semantics and safety contracts must be agreed before grammar changes.

## Implementation sequence and acceptance gates

1. **Data contracts:** JSON codec, schema validation, typed error/result values. Acceptance: round-trip tests, malformed-input tests, size/depth limits, deterministic output.
2. **Host capability ABI:** versioned host functions, capability declarations, deny-by-default behavior. Acceptance: unauthorized calls fail closed; ABI compatibility tests; no secrets in bytecode.
3. **Async runtime:** task, await, cancellation, deadlines, bounded concurrency. Acceptance: scheduler tests and resource-limit tests.
4. **Provider SDK:** provider-neutral request/response types plus a mock provider. Acceptance: deterministic tests run without network or credentials.
5. **Tool SDK:** validated tool registry, permissions, approvals, typed outputs. Acceptance: capability denial and hostile tool-output tests.
6. **Agent runtime:** bounded orchestration, trace events, replay fixtures. Acceptance: step-budget, timeout, loop, and replay tests.
7. **Memory adapters:** scoped persistence and optional retrieval. Acceptance: tenant/application isolation, retention, redaction, and deletion tests.
8. **QORVYNT-built assistant:** implement a reference assistant/agent in \`.qv\` using only the public language APIs. Acceptance: it runs in a clean environment without relying on Rust-only application logic for agent orchestration.

## Architecture constraint: self-hosting

The existing QORVYNT-written frontend is currently executed through a Rust reference host. That is useful compiler work, but it is not a Rust-independent bootstrap. AI features must not deepen this dependency. The public host ABI and runtime semantics should be specified so the QORVYNT implementation can eventually call the same stable interfaces.

## Immediate engineering prerequisites

Before adding AI syntax, audit and complete:
- JSON encode/decode and schema validation in the canonical runtime;
- module/package versioning and dependency integrity;
- a stable host-function ABI with explicit capabilities;
- structured errors and resource limits shared by source execution and Bytecode v2;
- test harnesses with deterministic mock providers and tools.

## Definition of done for this design

This document is a roadmap, not an implementation claim. A feature is complete only when its parser/compiler/VM implementation, security tests, docs, runnable \`.qv\` example, and CI checks all land together. A green documentation-only CI run does not prove that AI runtime capabilities exist.
