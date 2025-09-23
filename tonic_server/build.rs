use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    // All Protocol Buffer sources are maintained in the `proto_gen` crate so
    // they can be shared across the workspace.
    let proto_root = crate_dir.join("../proto_gen/protobuf_definitions");
    let message_service_proto = proto_root.join("message_service.proto");
    let message_proto = proto_root.join("message.proto");

    println!(
        "cargo:rerun-if-changed={}",
        message_service_proto.display()
    );
    println!("cargo:rerun-if-changed={}", message_proto.display());

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(
            &[message_service_proto.clone()],
            &[proto_root.clone()],
        )?;

    Ok(())
}
