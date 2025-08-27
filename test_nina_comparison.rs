use psf_guard::image_analysis::FitsImage;
use psf_guard::nina_star_detection::{detect_stars_with_original, StarDetectionParams, StarSensitivity, NoiseReduction};
use psf_guard::mtf_stretch::StretchParameters;
use std::path::Path;

fn test_image(path: &Path, description: &str, expected_hfr: Option<f64>, expected_stars: Option<usize>) {
    println!("\n{}", "=".repeat(80));
    println!("Testing: {}", description);
    println!("File: {}", path.display());
    println!("{}", "=".repeat(80));
    
    // Load FITS file
    let fits = match FitsImage::from_file(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to load FITS file: {:?}", e);
            return;
        }
    };
    
    println!("Image: {}x{} pixels", fits.width, fits.height);
    
    // Calculate statistics and stretch
    let stats = fits.calculate_basic_statistics();
    println!("\nStatistics:");
    println!("  Mean: {:.2}, Median: {:.2}, StdDev: {:.2}", stats.mean, stats.median, stats.std_dev);
    println!("  Min: {:.0}, Max: {:.0}", stats.min, stats.max);
    println!("  MAD: {:.2} (approx: {:.2})", stats.mad.unwrap_or(0.0), stats.std_dev * 0.6745);
    
    // Apply stretch
    let stretch_params = StretchParameters::default();
    let stretched_data = psf_guard::mtf_stretch::stretch_image(
        &fits.data, 
        &stats, 
        stretch_params.factor, 
        stretch_params.black_clipping
    );
    
    // Test different sensitivities
    let mut params = StarDetectionParams::default();
    params.noise_reduction = NoiseReduction::None;
    
    println!("\nStar Detection Results:");
    for sensitivity in [StarSensitivity::Normal, StarSensitivity::High] {
        params.sensitivity = sensitivity;
        let sensitivity_name = match sensitivity {
            StarSensitivity::Normal => "Normal",
            StarSensitivity::High => "High",
            _ => "Unknown",
        };
        
        // Detect with stretched data but measure HFR on original
        let result = detect_stars_with_original(
            &stretched_data,
            &fits.data,
            fits.width, 
            fits.height, 
            &params
        );
        
        println!("  {} sensitivity: {} stars, HFR: {:.3}", 
                 sensitivity_name, result.star_list.len(), result.average_hfr);
                 
        // If we're using High sensitivity and have expected values, compare
        if matches!(sensitivity, StarSensitivity::High) {
            if let Some(expected_hfr) = expected_hfr {
                println!("    Expected HFR: {:.3}, Difference: {:.3}", 
                         expected_hfr, (result.average_hfr - expected_hfr).abs());
            }
            if let Some(expected_stars) = expected_stars {
                println!("    Expected stars: {}, Difference: {}", 
                         expected_stars, (result.star_list.len() as i32 - expected_stars as i32).abs());
            }
            
            // Show HFR distribution
            if !result.star_list.is_empty() {
                let mut sorted_stars = result.star_list.clone();
                sorted_stars.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap());
                
                let hfr_3 = sorted_stars.iter().filter(|s| s.hfr <= 3.0).count();
                let hfr_4 = sorted_stars.iter().filter(|s| s.hfr <= 4.0).count();
                let hfr_5 = sorted_stars.iter().filter(|s| s.hfr <= 5.0).count();
                
                println!("\n    HFR Distribution:");
                println!("      <= 3.0: {} stars", hfr_3);
                println!("      <= 4.0: {} stars", hfr_4);
                println!("      <= 5.0: {} stars", hfr_5);
                println!("      > 5.0: {} stars", sorted_stars.len() - hfr_5);
                
                println!("\n    Top 5 stars (best HFR):");
                for (i, star) in sorted_stars.iter().take(5).enumerate() {
                    println!("      {}. HFR: {:.3}, Position: ({:.1}, {:.1})", 
                             i + 1, star.hfr, star.position.0, star.position.1);
                }
            }
        }
    }
}

fn main() {
    println!("N.I.N.A. Star Detection Comparison Test");
    println!("======================================\n");
    
    // Test OIII filter image (original test case)
    let oiii_path = Path::new("files2/Bubble Nebula/2025-08-17/LIGHT/2025-08-17_21-13-23_OIII_-10.00_300.00s_0005.fits");
    if oiii_path.exists() {
        test_image(oiii_path, "Bubble Nebula - OIII Filter", Some(2.920), Some(343));
    }
    
    // Test H-alpha filter images
    let ha_files = vec![
        "files/2025-08-20/North American/2025-08-20/LIGHT/2025-08-21_03-43-38_HA_-10.00_300.00s_0151.fits",
        "files/2025-08-20/North American/2025-08-20/LIGHT/2025-08-21_03-53-59_HA_-10.00_300.00s_0153.fits",
    ];
    
    for (i, ha_file) in ha_files.iter().enumerate() {
        let ha_path = Path::new(ha_file);
        if ha_path.exists() {
            test_image(ha_path, &format!("North American Nebula - H-alpha Filter #{}", i + 1), None, None);
        }
    }
    
    println!("\n{}", "=".repeat(80));
    println!("Test complete!");
}