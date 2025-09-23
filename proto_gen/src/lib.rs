//! Utilities for generating Rust code from Protocol Buffer definitions.
//!
//! The current implementation only contains scaffolding while the actual
//! code-generation pipeline is being built. The placeholder helpers make it
//! possible to exercise the testing infrastructure and keep the crate ready
//! for future expansion.

/// Auto-generated modules derived from the Protocol Buffer definitions.
///
/// The contents of this module are generated at build time by the crate's
/// `build.rs` script, which compiles any `.proto` files located in the
/// `protobuf_definitions/` directory.
#[allow(clippy::all)]
pub mod generated {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/generated_rust/generated_mod.rs"
    ));
}
