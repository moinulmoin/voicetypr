fn main() {
    // Set the deployment target to match our minimum system version
    println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=13.0");

    tauri_build::build()
}
