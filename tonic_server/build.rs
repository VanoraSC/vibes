use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let proto_dir = crate_dir.join("proto");
    let external_proto_dir = crate_dir.join("../proto_gen/protobuf_definitions");

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(
            &[proto_dir.join("message_service.proto")],
            &[proto_dir, external_proto_dir],
        )?;

    Ok(())
}
