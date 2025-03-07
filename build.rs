fn main() {
    capnpc::CompilerCommand::new()
        .output_path("src/")
        .src_prefix("schemas/")
        .file("schemas/event_schema.capnp")
        .run()
        .unwrap();
}
