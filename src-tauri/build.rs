#[cfg(target_os = "macos")]
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
        let dist_dir = sidecar_dir.join("dist");

        if build_script.exists() {
            // Ensure dist directory exists
            std::fs::create_dir_all(&dist_dir).ok();

            let output = Command::new("bash")
                .arg("build.sh")
                .arg("release")
                .current_dir(&sidecar_dir)
                .output();

            match output {
                Ok(output) => {
                    if !output.status.success() {
                        println!(
                            "cargo:warning=Swift sidecar build failed: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        println!("cargo:warning=Continuing build without Parakeet sidecar...");
                    } else {
                        println!("cargo:warning=Swift sidecar built successfully");

                        // Verify the binary exists
                        let target_triple = std::env::var("TARGET")
                            .unwrap_or_else(|_| "aarch64-apple-darwin".to_string());
                        let binary_name = format!("parakeet-sidecar-{}", target_triple);
                        let binary_path = dist_dir.join(&binary_name);

                        if binary_path.exists() {
                            println!(
                                "cargo:warning=Parakeet sidecar binary verified at: {}",
                                binary_path.display()
                            );
                        } else {
                            println!(
                                "cargo:warning=Warning: Expected binary not found at {}",
                                binary_path.display()
                            );
                        }
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
        println!("cargo:rerun-if-changed=../sidecar/parakeet-swift/build.sh");

        // Verify ffmpeg/ffprobe sidecars exist for macOS (aarch64)
        let ffmpeg_dir = std::path::Path::new("../sidecar/ffmpeg/dist");
        let ffmpeg = ffmpeg_dir.join("ffmpeg");
        let ffprobe = ffmpeg_dir.join("ffprobe");
        if !ffmpeg.exists() {
            panic!(
                "FFmpeg sidecar missing: {}. Place the macOS aarch64 binary at this path.",
                ffmpeg.display()
            );
        }
        if !ffprobe.exists() {
            panic!(
                "FFprobe sidecar missing: {}. Place the macOS aarch64 binary at this path.",
                ffprobe.display()
            );
        }
    }

    // On Windows, verify ffmpeg sidecars exist
    #[cfg(target_os = "windows")]
    {
        let ffmpeg_dir = std::path::Path::new("../sidecar/ffmpeg/dist");
        let ffmpeg = ffmpeg_dir.join("ffmpeg.exe");
        let ffprobe = ffmpeg_dir.join("ffprobe.exe");
        if !ffmpeg.exists() {
            panic!(
                "FFmpeg sidecar missing: {}. Place the Windows x64 binary at this path.",
                ffmpeg.display()
            );
        }
        if !ffprobe.exists() {
            panic!(
                "FFprobe sidecar missing: {}. Place the Windows x64 binary at this path.",
                ffprobe.display()
            );
        }
    }

    tauri_build::build()
}
