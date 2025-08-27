use psf_guard::image_analysis::FitsImage;
use psf_guard::nina_star_detection::{detect_stars_with_original, StarDetectionParams, StarSensitivity, NoiseReduction};
use psf_guard::mtf_stretch::{stretch_image, StretchParameters};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    println!("Testing N.I.N.A. star detection with separated detection/measurement data");
    println!("=========================================================================");
    
    // Load test FITS file
    let test_file = "files/AT_Cnc/2025-01-10/LIGHT/AT_Cnc_2025-01-10_05-39-21_SHO_-15.00_300.00s_0001.fits";
    let path = Path::new(test_file);
    
    if !path.exists() {
        eprintln!("Test file not found: {}", test_file);
        return Ok(());
    }
    
    println!("Loading FITS file: {}", test_file);
    let fits = FitsImage::from_file(path)?;
    println!("Image dimensions: {}x{}, {} pixels", fits.width, fits.height, fits.data.len());
    
    // Calculate basic statistics
    let stats = fits.calculate_basic_statistics();
    println!("\nImage statistics:");
    println!("  Mean: {:.2}", stats.mean);
    println!("  Median: {:.2}", stats.median);
    println!("  Std Dev: {:.2}", stats.std_dev);
    println!("  Min: {:.0}, Max: {:.0}", stats.min, stats.max);
    
    // Apply MTF stretch using N.I.N.A. defaults
    let stretch_params = StretchParameters::default();
    println!("\nApplying MTF stretch:");
    println!("  Factor: {}", stretch_params.factor);
    println!("  Black Clipping: {}", stretch_params.black_clipping);
    
    let stretched_data = stretch_image(
        &fits.data, 
        &stats, 
        stretch_params.factor, 
        stretch_params.black_clipping
    );
    
    // Detect stars using stretched data for detection but original data for HFR
    let mut params = StarDetectionParams::default();
    params.sensitivity = StarSensitivity::High; // N.I.N.A. default
    params.noise_reduction = NoiseReduction::None; // Start with no noise reduction
    
    println!("\nDetecting stars with N.I.N.A. algorithm:");
    println!("  Sensitivity: High");
    println!("  Noise Reduction: None");
    println!("  Using stretched data for detection");
    println!("  Using original raw data for HFR measurement");
    
    let result = detect_stars_with_original(
        &stretched_data,  // Detection data (stretched)
        &fits.data,       // Original raw data for HFR calculation
        fits.width, 
        fits.height, 
        &params
    );
    
    println!("\nResults:");
    println!("  Detected stars: {}", result.star_list.len());
    println!("  Average HFR: {:.3}", result.average_hfr);
    println!("  HFR Std Dev: {:.3}", result.hfr_std_dev);
    
    if !result.star_list.is_empty() {
        println!("\nTop 10 stars by HFR:");
        let mut sorted_stars = result.star_list.clone();
        sorted_stars.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap());
        
        for (i, star) in sorted_stars.iter().take(10).enumerate() {
            println!("  {}. Position: ({:.1}, {:.1}), HFR: {:.3}, Brightness: {:.1}", 
                i + 1, star.position.0, star.position.1, star.hfr, star.average_brightness);
        }
    }
    
    Ok(())
}