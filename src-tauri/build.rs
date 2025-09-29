use std::process::Command;

fn main() {
    // Set the deployment target to match our minimum system version
    println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=13.0");

    // Build Swift Parakeet sidecar on macOS
    #[cfg(target_os = "macos")]
    {
        println!("cargo:warning=Building Swift Parakeet sidecar...");
        
        let sidecar_dir = std::path::Path::new("../sidecar/parakeet-swift");
        let build_script = sidecar_dir.join("build.sh");
        
        if build_script.exists() {
            let output = Command::new("bash")
                .arg(build_script)
                .arg("release")
                .current_dir(sidecar_dir)
                .output();
            
            match output {
                Ok(output) => {
                    if !output.status.success() {
                        println!("cargo:warning=Swift sidecar build failed: {}", 
                                 String::from_utf8_lossy(&output.stderr));
                        println!("cargo:warning=Continuing build without Parakeet sidecar...");
                    } else {
                        println!("cargo:warning=Swift sidecar built successfully");
                    }
                }
                Err(e) => {
                    println!("cargo:warning=Failed to run Swift build script: {}", e);
                    println!("cargo:warning=Continuing build without Parakeet sidecar...");
                }
            }
        } else {
            println!("cargo:warning=Swift build script not found, skipping sidecar build");
        }
        
        // Tell Cargo to re-run if Swift sources change
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/Sources");
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/Package.swift");
    }

    tauri_build::build()
}
