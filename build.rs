use std::env;
use std::process::Command;

fn main() {
    // Only run OpenCV version detection if the opencv feature is enabled
    if env::var("CARGO_FEATURE_OPENCV").is_ok() {
        detect_opencv_version();
    }
}

fn detect_opencv_version() {
    // Try to get OpenCV version using pkg-config
    let output = Command::new("pkg-config")
        .args(&["--modversion", "opencv4"])
        .output();

    // If opencv4 isn't found, try opencv
    let version = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            // Try plain opencv
            let output = Command::new("pkg-config")
                .args(&["--modversion", "opencv"])
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
            eprintln!("cargo:warning=OpenCV {} detected, enabling algorithm_hint feature", version);
        }
    }
}