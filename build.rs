fn main() {
    // Manifeste "requireAdministrator" : indispensable pour SeProfileSingleProcessPrivilege
    // (purge de la Standby List). Ignoré hors Windows.
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        use embed_manifest::{embed_manifest, manifest::ExecutionLevel, new_manifest};
        embed_manifest(
            new_manifest("MemGuard").requested_execution_level(ExecutionLevel::RequireAdministrator),
        )
        .expect("impossible d'embarquer le manifeste");
    }
    println!("cargo:rerun-if-changed=build.rs");
}
