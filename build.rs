fn main() {
    let sidecar_proto = &"src/bin/dagyo-sidecar/dagyo-sidecar.proto";
    tonic_build::compile_protos(sidecar_proto)
        .unwrap_or_else(|e| panic!("Failed to compile protos {:?}", e));
    println!("cargo:rerun-if-changed={sidecar_proto}");
}
