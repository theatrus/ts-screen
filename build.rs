use std::env;
use std::process::Command;

fn main() {
    // Register our custom cfg flag with cargo
    println!("cargo::rustc-check-cfg=cfg(opencv_has_algorithm_hint)");

    // Only run OpenCV version detection if the opencv feature is enabled
    if env::var("CARGO_FEATURE_OPENCV").is_ok() {
        // Check for macOS libclang issues
        check_macos_libclang();
        detect_opencv_version();
    }
}

#[cfg(target_os = "macos")]
fn check_macos_libclang() {
    // Check if DYLD_FALLBACK_LIBRARY_PATH is set
    if env::var("DYLD_FALLBACK_LIBRARY_PATH").is_err() {
        eprintln!("cargo:warning=================================================================");
        eprintln!("cargo:warning=macOS libclang setup required for OpenCV!");
        eprintln!("cargo:warning=");
        eprintln!("cargo:warning=Please run one of these commands before building:");
        eprintln!("cargo:warning=");
        eprintln!("cargo:warning=For Xcode:");
        eprintln!("cargo:warning=  export DYLD_FALLBACK_LIBRARY_PATH=\"$(xcode-select --print-path)/Toolchains/XcodeDefault.xctoolchain/usr/lib/\"");
        eprintln!("cargo:warning=");
        eprintln!("cargo:warning=For Command Line Tools only:");
        eprintln!("cargo:warning=  export DYLD_FALLBACK_LIBRARY_PATH=\"$(xcode-select --print-path)/usr/lib/\"");
        eprintln!("cargo:warning=");
        eprintln!("cargo:warning=Or add to your shell profile (~/.zshrc or ~/.bash_profile)");
        eprintln!("cargo:warning=================================================================");
    }
}

#[cfg(not(target_os = "macos"))]
fn check_macos_libclang() {
    // No-op on non-macOS platforms
}

fn detect_opencv_version() {
    // Try to get OpenCV version using pkg-config
    let output = Command::new("pkg-config")
        .args(["--modversion", "opencv4"])
        .output();

    // If opencv4 isn't found, try opencv
    let version = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // Try plain opencv
            let output = Command::new("pkg-config")
                .args(["--modversion", "opencv"])
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    String::from_utf8_lossy(&output.stdout).trim().to_string()
                }
                _ => {
                    // If we can't detect version, assume older version
                    eprintln!("cargo:warning=Could not detect OpenCV version, assuming < 4.9");
                    return;
                }
            }
        }
    };

    eprintln!("cargo:warning=Detected OpenCV version: {}", version);

    // Parse the version
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let major = parts[0].parse::<u32>().unwrap_or(0);
        let minor = parts[1].parse::<u32>().unwrap_or(0);

        // OpenCV 4.9+ added the AlgorithmHint parameter to gaussian_blur
        if major > 4 || (major == 4 && minor >= 9) {
            println!("cargo:rustc-cfg=opencv_has_algorithm_hint");
            eprintln!(
                "cargo:warning=OpenCV {} detected, enabling algorithm_hint feature",
                version
            );
        }
    }
}
