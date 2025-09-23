# proto_gen

The `proto_gen` crate is intended to produce Rust source code from Protocol Buffer (`.proto`) definitions. It will eventually host the tooling required to compile those definitions into Rust modules that can be consumed by other crates in this workspace.

## Protobuf definitions

The `protobuf_definitions/` directory is intended to hold the `.proto` files that will be transformed into Rust code by the crate's code-generation pipeline.
