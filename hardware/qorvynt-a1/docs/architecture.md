# QORVYNT A1 — initial architecture note

**Revision:** 0.1-draft  
**Status:** architecture proposal; not a silicon specification

## 1. Product intent

Build a programmable accelerator for AI inference first, with a path toward training workloads. QORVYNT A1 is intended to become one hardware target for the wider QORVYNT software stack. It must not depend on a single vendor's proprietary programming ecosystem.

## 2. First workload

The first supported arithmetic primitive is signed INT8 dot product with a signed 32-bit accumulator:

`acc_next = acc + Σ(a[i] × b[i])`

The initial RTL implements a configurable number of parallel lanes. It is a verification seed, not yet a complete tensor processor.

## 3. Proposed logical blocks

1. **Host interface** — future PCIe endpoint and command queue.
2. **Command processor** — future descriptor fetch, validation and dispatch.
3. **Compute tiles** — INT8 MAC lanes first; later BF16/FP16 and other formats only after numerical requirements and verification are defined.
4. **Local storage** — future register files, scratchpad SRAM and cache hierarchy.
5. **Memory subsystem** — future memory controller, PHY and external memory devices.
6. **Telemetry and safety** — future temperature, voltage, power and fault reporting.
7. **Software stack** — QORVYNT compiler lowering, runtime, driver and profiling tools.

## 4. Memory product family

The requested 8/16/32/64 GiB options are capacity SKUs, not a statement that memory is integrated into the compute die. A realistic design must select memory technology and package after modelling bandwidth, pin count, signal integrity, power, thermal envelope, board complexity and bill of materials.

Candidate directions to evaluate:
- GDDR: potentially simpler board-level integration, with high-speed routing and power trade-offs.
- HBM: very high bandwidth and advanced packaging requirements; likely a later/high-cost option.

No memory technology is selected in this draft. No capacity is implemented by the current RTL.

## 5. Portability strategy

The hardware/software boundary should be documented independently of the first RTL implementation. The initial software path should have a CPU reference backend. Hardware-specific backends should be capability-discovered and optional. A portable API is not the same as automatic compatibility with every GPU or operating system.

## 6. Verification gates

- Directed arithmetic tests for positive, negative and boundary operands.
- Randomized comparison against a software reference model.
- Reset, valid/idle and accumulator-clear behavior.
- Lint and width/sign review.
- Synthesis estimates for area and timing.
- Formal checks for control logic when the block grows.
- FPGA validation before any ASIC commitment.

## 7. Explicit non-goals of revision 0.1

This revision does not claim a finished GPU, AI training engine, memory controller, PCIe device, driver, compiler backend, FPGA bitstream, physical layout, foundry-qualified design, or manufactured semiconductor.

## 8. Immediate next blocks

After the MAC block passes CI: add a software reference model and randomized tests; then design a small tiled MAC datapath with explicit operand buffering and backpressure. Do not scale lane count or claim TOPS until clock, area, power and memory bandwidth have been measured.
