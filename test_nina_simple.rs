use psf_guard::image_analysis::FitsImage;
use psf_guard::nina_star_detection::{detect_stars_with_original, StarDetectionParams, StarSensitivity, NoiseReduction};
use psf_guard::mtf_stretch::{stretch_image, StretchParameters};
use std::path::Path;

fn main() {
    println!("Testing N.I.N.A. star detection with separated detection/measurement data");
    println!("=========================================================================");
    
    // Load test FITS file - this is the exact file N.I.N.A. reported HFR 2.920 on
    let test_file = "files2/Bubble Nebula/2025-08-17/LIGHT/2025-08-17_21-13-23_OIII_-10.00_300.00s_0005.fits";
    let path = Path::new(test_file);
    
    if !path.exists() {
        eprintln!("Test file not found: {}", test_file);
        return;
    }
    
    println!("Loading FITS file: {}", test_file);
    let fits = match FitsImage::from_file(path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to load FITS file: {:?}", e);
            return;
        }
    };
    
    println!("Image dimensions: {}x{}, {} pixels", fits.width, fits.height, fits.data.len());
    
    // Calculate a checksum of the image data
    let checksum: u64 = fits.data.iter().map(|&x| x as u64).sum();
    println!("Image data checksum: {}", checksum);
    
    // Calculate basic statistics
    let stats = fits.calculate_basic_statistics();
    println!("\nImage statistics:");
    println!("  Mean: {:.2}", stats.mean);
    println!("  Median: {:.2}", stats.median);
    println!("  Std Dev: {:.2}", stats.std_dev);
    println!("  Min: {:.0}, Max: {:.0}", stats.min, stats.max);
    println!("  MAD approximation: {:.2}", stats.std_dev * 0.6745);
    println!("  MAD calculated: {:.2}", stats.mad.unwrap_or(0.0));
    
    // Check if our image is truly 16-bit or if it's actually using less bits
    let actual_bit_depth = if stats.max <= 255.0 {
        8
    } else if stats.max <= 4095.0 {
        12
    } else if stats.max <= 16383.0 {
        14
    } else {
        16
    };
    println!("  Actual bit depth (based on max value): {}", actual_bit_depth);
    
    // Apply MTF stretch using N.I.N.A. defaults
    let stretch_params = StretchParameters::default();
    println!("\nApplying MTF stretch:");
    println!("  Factor: {}", stretch_params.factor);
    println!("  Black Clipping: {}", stretch_params.black_clipping);
    
    // Use the actual bit depth for stretching
    let stretched_data = psf_guard::mtf_stretch::stretch_image_with_bit_depth(
        &fits.data, 
        &stats, 
        stretch_params.factor, 
        stretch_params.black_clipping,
        actual_bit_depth as u8
    );
    
    // Debug: Check stretched image statistics
    let stretched_min = *stretched_data.iter().min().unwrap();
    let stretched_max = *stretched_data.iter().max().unwrap();
    let stretched_mean = stretched_data.iter().map(|&x| x as u64).sum::<u64>() as f64 / stretched_data.len() as f64;
    println!("\nStretched image stats:");
    println!("  Min: {}, Max: {}, Mean: {:.2}", stretched_min, stretched_max, stretched_mean);
    
    // Check how many pixels were stretched to the minimum value
    let min_count = stretched_data.iter().filter(|&&x| x == stretched_min).count();
    println!("  Pixels at minimum value: {} ({:.2}%)", min_count, 
             min_count as f64 / stretched_data.len() as f64 * 100.0);
    
    // Check distribution of 8-bit values
    let data_8bit: Vec<u8> = stretched_data.iter().map(|&val| (val >> 8) as u8).collect();
    let mut histogram_8bit = vec![0u32; 256];
    for &val in data_8bit.iter() {
        histogram_8bit[val as usize] += 1;
    }
    println!("\n  8-bit histogram (showing non-zero bins):");
    for (val, count) in histogram_8bit.iter().enumerate() {
        if *count > 0 {
            println!("    [{}]: {} pixels", val, count);
            if val >= 10 { break; } // Just show first few
        }
    }
             
    // Find what original values map to the minimum
    let original_min = stats.min as u16;
    let original_median = stats.median as u16;
    println!("  Original min {} -> stretched {}", original_min, 
             psf_guard::mtf_stretch::stretch_image(&vec![original_min], &stats, stretch_params.factor, stretch_params.black_clipping)[0]);
    println!("  Original median {} -> stretched {}", original_median, 
             psf_guard::mtf_stretch::stretch_image(&vec![original_median], &stats, stretch_params.factor, stretch_params.black_clipping)[0]);
    
    // Sample some stretched values to understand the distribution
    println!("\n  Sample stretched values:");
    for val in [204u16, 300, 350, 398, 450, 500, 600, 800, 1000, 2000] {
        if val <= stats.max as u16 {
            let stretched = psf_guard::mtf_stretch::stretch_image(&vec![val], &stats, stretch_params.factor, stretch_params.black_clipping)[0];
            println!("    {} -> {}", val, stretched);
        }
    }
    
    // Detect stars using stretched data for detection but original data for HFR
    let mut params = StarDetectionParams::default();
    params.sensitivity = StarSensitivity::High; // N.I.N.A. default
    params.noise_reduction = NoiseReduction::None; // Start with no noise reduction
    
    println!("\nDetecting stars with N.I.N.A. algorithm:");
    println!("  Sensitivity: High");
    println!("  Noise Reduction: None");
    println!("  Using stretched data for detection");
    println!("  Using original raw data for HFR measurement");
    
    // Let's also test with different sensitivities
    println!("\nTesting different sensitivity settings:");
    
    for sensitivity in [psf_guard::nina_star_detection::StarSensitivity::Normal, 
                       psf_guard::nina_star_detection::StarSensitivity::High] {
        params.sensitivity = sensitivity;
        let sensitivity_name = match sensitivity {
            psf_guard::nina_star_detection::StarSensitivity::Normal => "Normal",
            psf_guard::nina_star_detection::StarSensitivity::High => "High",
            _ => "Unknown",
        };
        
        let test_result = detect_stars_with_original(
            &stretched_data,
            &fits.data,
            fits.width, 
            fits.height, 
            &params
        );
        
        println!("  {} sensitivity: {} stars detected, HFR: {:.3}", 
                 sensitivity_name, test_result.star_list.len(), test_result.average_hfr);
    }
    
    // Test without stretching to see if detection works
    println!("\n\nTesting WITHOUT stretching:");
    let result_no_stretch = detect_stars_with_original(
        &fits.data,       // Use original data for detection
        &fits.data,       // Original raw data for HFR calculation
        fits.width, 
        fits.height, 
        &params
    );
    println!("  Without stretch: {} stars detected, HFR: {:.3}", 
             result_no_stretch.star_list.len(), result_no_stretch.average_hfr);
    
    // Use High sensitivity for final result
    params.sensitivity = StarSensitivity::High;
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
    
    println!("\n===========================================");
    println!("N.I.N.A. reported: Average HFR = 2.920, Stars = 343");
    println!("Our result:       Average HFR = {:.3}, Stars = {}", result.average_hfr, result.star_list.len());
    println!("Difference:       {:.3} HFR", (result.average_hfr - 2.920).abs());
    println!("===========================================");
    
    if !result.star_list.is_empty() {
        println!("\nTop 10 stars by HFR:");
        let mut sorted_stars = result.star_list.clone();
        sorted_stars.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap());
        
        for (i, star) in sorted_stars.iter().take(10).enumerate() {
            println!("  {}. Position: ({:.1}, {:.1}), HFR: {:.3}, Brightness: {:.1}", 
                i + 1, star.position.0, star.position.1, star.hfr, star.average_brightness);
        }
        
        // Analyze HFR distribution
        println!("\nHFR Distribution:");
        let hfr_3_count = sorted_stars.iter().filter(|s| s.hfr <= 3.0).count();
        let hfr_4_count = sorted_stars.iter().filter(|s| s.hfr <= 4.0).count();
        let hfr_5_count = sorted_stars.iter().filter(|s| s.hfr <= 5.0).count();
        println!("  Stars with HFR <= 3.0: {}", hfr_3_count);
        println!("  Stars with HFR <= 4.0: {}", hfr_4_count); 
        println!("  Stars with HFR <= 5.0: {}", hfr_5_count);
        println!("  Stars with HFR > 5.0: {}", sorted_stars.len() - hfr_5_count);
        
        // Calculate average HFR for best stars
        let top_n = std::cmp::min(sorted_stars.len(), 343); // N.I.N.A.'s count
        let top_stars_hfr: f64 = sorted_stars.iter()
            .take(top_n)
            .map(|s| s.hfr)
            .sum::<f64>() / top_n as f64;
        println!("\n  Average HFR of best {} stars: {:.3}", top_n, top_stars_hfr);
        
        // Sort by brightness and show relationship to HFR
        let mut brightness_sorted = sorted_stars.clone();
        brightness_sorted.sort_by(|a, b| b.average_brightness.partial_cmp(&a.average_brightness).unwrap());
        println!("\nBrightness vs HFR (top 10 brightest):");
        for (i, star) in brightness_sorted.iter().take(10).enumerate() {
            println!("  {}. Brightness: {:.1}, HFR: {:.3}, Max pixel: {:.0}", 
                     i + 1, star.average_brightness, star.hfr, star.max_brightness);
        }
        
        // Check if any stars are near saturation
        let saturated_count = sorted_stars.iter()
            .filter(|s| s.max_brightness >= 65000.0)
            .count();
        println!("\nStars with max pixel >= 65000: {}", saturated_count);
    }
}