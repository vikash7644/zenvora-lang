//! Standalone QORVYNT virtual-machine runtime crate.
//! This crate contains VM instructions, values, safety limits, built-ins,
//! gas accounting interfaces, and the execution engine. Compiler parsing,
//! source loading, and bytecode verification remain in the host compiler.

pub mod qvm {
    pub mod bytecode;
    pub mod builtins;
    pub mod gas;
    pub mod limits;
    pub mod runtime;
    pub mod runtime_support;
    pub mod value;

    pub use bytecode::Op;
    pub use value::Value;
}
