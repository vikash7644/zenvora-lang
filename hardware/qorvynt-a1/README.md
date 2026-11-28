# QORVYNT A1 — AI accelerator hardware foundation

QORVYNT A1 is a proposed, software-first AI accelerator platform. This directory is the beginning of its hardware design work; it is **not** a completed chip, tape-out, FPGA image, or manufactured product.

## First engineering slice

The first RTL block is a parameterized INT8 dot-product / multiply-accumulate (MAC) unit. It provides a small, testable compute primitive for later tensor tiles. The initial verification target is simulation with Icarus Verilog.

## Memory configurations (product targets, not implemented capacity)

The planned product family has 8 GiB, 16 GiB, 32 GiB and 64 GiB memory configurations. These are system-level capacity targets. They would be implemented using external memory devices (for example, GDDR or HBM, selected after bandwidth, package, power and cost analysis), a memory controller/PHY, and a suitable package. The current RTL does not contain a memory controller, PHY, HBM/GDDR interface, PCIe endpoint, or any claim of those capacities.

## Design principles

- Start with a reproducible, simulation-tested compute block.
- Keep the instruction/runtime contract vendor-neutral where practical.
- Separate compute tiles, memory subsystem, host interface, and software runtime.
- Treat performance, bandwidth, power, thermal limits, safety margins and manufacturability as measured engineering constraints.
- Do not claim compatibility with CUDA, Vulkan, ROCm, FPGA boards, or any foundry process until a real backend or implementation is built and tested.

## Current status

| Area | Status |
|---|---|
| INT8 MAC RTL | Initial parameterized block |
| Simulation testbench | Initial directed test |
| CI simulation | GitHub Actions workflow added |
| Tensor tile / scheduler | Not implemented |
| SRAM/cache hierarchy | Not implemented |
| GDDR/HBM controller + PHY | Not implemented |
| PCIe host interface | Not implemented |
| FPGA prototype | Not implemented |
| Physical design / tape-out | Not implemented |

## Run the simulation

Install Icarus Verilog, then:

```sh
iverilog -g2012 -s tb_qorvynt_a1_int8_mac -o /tmp/qorvynt_a1_sim \
  hardware/qorvynt-a1/rtl/qorvynt_a1_int8_mac.sv \
  hardware/qorvynt-a1/tb/tb_qorvynt_a1_int8_mac.sv
vvp /tmp/qorvynt_a1_sim
```

Expected terminal result: `PASS: QORVYNT A1 INT8 MAC directed test`.

## Naming

The initial RTL module name is `qorvynt_a1_int8_mac`. The project name is QORVYNT A1. The language/compiler project remains separate in purpose; this directory is a hardware-design starting point inside the current repository and can be split into its own repository when repository-creation access is available.
