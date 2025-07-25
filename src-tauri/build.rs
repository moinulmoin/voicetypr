fn main() {
    // Set the deployment target to match our minimum system version
    println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=13.0");
    
    // Note: SENTRY_DSN is read from environment during build time
    // The release script (scripts/release-separate.sh) loads .env before building
    // This makes the DSN available to option_env!("SENTRY_DSN") in the code

    tauri_build::build()
}
