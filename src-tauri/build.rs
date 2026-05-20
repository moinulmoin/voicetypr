use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

fn modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

fn newest_modified_time(path: &Path) -> Option<SystemTime> {
    if path.is_file() {
        return modified_time(path);
    }

    let entries = std::fs::read_dir(path).ok()?;
    entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            newest_modified_time(&entry.path())
        })
        .max()
}

fn formatting_sidecar_inputs_newer_than(binary_path: &Path) -> bool {
    let Some(binary_mtime) = modified_time(binary_path) else {
        return true;
    };

    [
        Path::new("../package.json"),
        Path::new("../pnpm-lock.yaml"),
        Path::new("../pnpm-workspace.yaml"),
        Path::new("../sidecar/formatting-engine/package.json"),
        Path::new("../sidecar/formatting-engine/tsconfig.json"),
        Path::new("../sidecar/formatting-engine/scripts"),
        Path::new("../sidecar/formatting-engine/src"),
    ]
    .iter()
    .filter_map(|path| newest_modified_time(path))
    .any(|mtime| mtime > binary_mtime)
}

fn build_formatting_sidecar(target_triple: &str, binary_path: &Path) {
    let pnpm = if cfg!(target_os = "windows") {
        "pnpm.cmd"
    } else {
        "pnpm"
    };
    let output = Command::new(pnpm)
        .arg("run")
        .arg("sidecar:build-formatting")
        .current_dir("..")
        .env("TARGET", target_triple)
        .output();

    match output {
        Ok(output) if output.status.success() && binary_path.exists() => {
            println!("cargo:warning=Formatting sidecar built successfully");
        }
        Ok(output) => {
            println!(
                "cargo:warning=Formatting sidecar build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            panic!(
                "Formatting sidecar missing: {}. Run `pnpm run sidecar:build-formatting`.",
                binary_path.display()
            );
        }
        Err(err) => {
            panic!(
                "Failed to run pnpm for formatting sidecar: {}. Run `pnpm run sidecar:build-formatting`.",
                err
            );
        }
    }
}

fn ensure_formatting_sidecar() {
    let target_triple = std::env::var("TARGET").unwrap_or_default();
    let extension = if target_triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let binary_name = format!(
        "../sidecar/dist/formatting-sidecar-{}{}",
        target_triple, extension
    );
    let binary_path = Path::new(&binary_name);

    if !formatting_sidecar_inputs_newer_than(binary_path) {
        println!(
            "cargo:warning=Formatting sidecar binary verified at: {}",
            binary_path.display()
        );
        return;
    }

    if binary_path.exists() {
        println!("cargo:warning=Formatting sidecar stale; rebuilding Node SEA sidecar...");
    } else {
        println!("cargo:warning=Formatting sidecar missing; building Node SEA sidecar...");
    }
    build_formatting_sidecar(&target_triple, binary_path);
}

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
                .current_dir(sidecar_dir)
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

    ensure_formatting_sidecar();
    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../pnpm-lock.yaml");
    println!("cargo:rerun-if-changed=../pnpm-workspace.yaml");
    println!("cargo:rerun-if-changed=../sidecar/formatting-engine/src");
    println!("cargo:rerun-if-changed=../sidecar/formatting-engine/scripts");
    println!("cargo:rerun-if-changed=../sidecar/formatting-engine/package.json");
    println!("cargo:rerun-if-changed=../sidecar/formatting-engine/tsconfig.json");

    tauri_build::build()
}
