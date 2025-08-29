use crate::hocus_focus_star_detection::{detect_stars_hocus_focus, HocusFocusParams};
use crate::image_analysis::FitsImage;
use crate::psf_fitting::PSFType;
use anyhow::Result;
use std::path::Path;
use std::time::Instant;

pub fn benchmark_psf(fits_path: &str, n_runs: usize, verbose: bool) -> Result<()> {
    if verbose {
        println!("Loading FITS file: {}", fits_path);
    }

    // Load FITS file
    let fits = FitsImage::from_file(Path::new(fits_path))?;
    let width = fits.width;
    let height = fits.height;
    println!("Image dimensions: {}x{}", width, height);

    // Warm up (first run is often slower due to cold cache)
    if verbose {
        println!("\nWarming up...");
    }
    let mut params = HocusFocusParams::default();
    params.psf_type = PSFType::None;
    let _ = detect_stars_hocus_focus(&fits.data, width, height, &params);

    // Benchmark configurations
    let configs = vec![
        ("HFR Only (No PSF)", PSFType::None),
        ("Gaussian PSF", PSFType::Gaussian),
        ("Moffat4 PSF", PSFType::Moffat4),
    ];

    println!("\nRunning benchmarks ({} runs each)...\n", n_runs);
    println!(
        "{:<20} | {:>10} | {:>10} | {:>8} | {:>10} | {:>10}",
        "Method", "Stars", "Avg Time", "Per Star", "HFR Mean", "HFR StdDev"
    );
    println!(
        "{:-<20}-+-{:-<10}-+-{:-<10}-+-{:-<8}-+-{:-<10}-+-{:-<10}",
        "", "", "", "", "", ""
    );

    for (name, psf_type) in &configs {
        let mut total_time = 0.0;
        let mut star_count = 0;
        let mut avg_hfr = 0.0;
        let mut hfr_std = 0.0;
        let mut psf_success_count = 0;

        // Run multiple times for average
        for _ in 0..n_runs {
            let mut params = HocusFocusParams::default();
            params.psf_type = *psf_type;

            let start = Instant::now();
            let result = detect_stars_hocus_focus(&fits.data, width, height, &params);
            let elapsed = start.elapsed().as_secs_f64();

            total_time += elapsed;
            star_count = result.stars.len();

            if !result.stars.is_empty() {
                let hfr_values: Vec<f64> = result.stars.iter().map(|s| s.hfr).collect();
                avg_hfr = hfr_values.iter().sum::<f64>() / hfr_values.len() as f64;
                let variance = hfr_values
                    .iter()
                    .map(|&hfr| (hfr - avg_hfr).powi(2))
                    .sum::<f64>()
                    / hfr_values.len() as f64;
                hfr_std = variance.sqrt();

                // Count successful PSF fits
                if *psf_type != PSFType::None {
                    psf_success_count = result
                        .stars
                        .iter()
                        .filter(|s| s.psf_model.is_some())
                        .count();
                }
            }
        }

        let avg_time = total_time / n_runs as f64;
        let time_per_star = if star_count > 0 {
            avg_time / star_count as f64 * 1000.0 // Convert to milliseconds
        } else {
            0.0
        };

        println!(
            "{:<20} | {:>10} | {:>10.3}s | {:>7.2}ms | {:>10.3} | {:>10.3}",
            name, star_count, avg_time, time_per_star, avg_hfr, hfr_std
        );

        if *psf_type != PSFType::None && star_count > 0 {
            println!(
                "{:<20} | PSF fits succeeded: {}/{} ({:.1}%)",
                "",
                psf_success_count,
                star_count,
                psf_success_count as f64 / star_count as f64 * 100.0
            );
        }
    }

    // Additional detailed analysis for PSF fitting
    if verbose {
        println!("\n=== Detailed PSF Analysis ===\n");

        for psf_type in &[PSFType::Gaussian, PSFType::Moffat4] {
            let mut params = HocusFocusParams::default();
            params.psf_type = *psf_type;

            let result = detect_stars_hocus_focus(&fits.data, width, height, &params);
            let stars_with_psf: Vec<_> = result
                .stars
                .into_iter()
                .filter(|s| s.psf_model.is_some())
                .collect();

            if !stars_with_psf.is_empty() {
                println!("{:?} PSF Analysis:", psf_type);

                // Collect R² values
                let r_squared_values: Vec<f64> = stars_with_psf
                    .iter()
                    .map(|s| s.psf_model.as_ref().unwrap().r_squared)
                    .collect();

                let avg_r2 = r_squared_values.iter().sum::<f64>() / r_squared_values.len() as f64;
                let min_r2 = r_squared_values
                    .iter()
                    .fold(f64::INFINITY, |a, &b| a.min(b));
                let max_r2 = r_squared_values
                    .iter()
                    .fold(f64::NEG_INFINITY, |a, &b| a.max(b));

                // Collect FWHM values
                let fwhm_values: Vec<f64> = stars_with_psf
                    .iter()
                    .map(|s| s.psf_model.as_ref().unwrap().fwhm)
                    .collect();

                let avg_fwhm = fwhm_values.iter().sum::<f64>() / fwhm_values.len() as f64;
                let fwhm_std = {
                    let variance = fwhm_values
                        .iter()
                        .map(|&f| (f - avg_fwhm).powi(2))
                        .sum::<f64>()
                        / fwhm_values.len() as f64;
                    variance.sqrt()
                };

                println!(
                    "  R² Statistics: avg={:.3}, min={:.3}, max={:.3}",
                    avg_r2, min_r2, max_r2
                );
                println!("  FWHM: {:.3} ± {:.3} pixels", avg_fwhm, fwhm_std);

                // Show top 5 stars by R²
                let mut sorted_by_r2 = stars_with_psf;
                sorted_by_r2.sort_by(|a, b| {
                    let r2_a = a.psf_model.as_ref().unwrap().r_squared;
                    let r2_b = b.psf_model.as_ref().unwrap().r_squared;
                    r2_b.partial_cmp(&r2_a).unwrap()
                });

                println!("  Top 5 stars by R²:");
                for (i, star) in sorted_by_r2.iter().take(5).enumerate() {
                    let psf = star.psf_model.as_ref().unwrap();
                    println!(
                        "    {}. Position ({:.1}, {:.1}): R²={:.3}, FWHM={:.2}, Ecc={:.3}",
                        i + 1,
                        star.position.0,
                        star.position.1,
                        psf.r_squared,
                        psf.fwhm,
                        psf.eccentricity
                    );
                }
                println!();
            }
        }
    }

    Ok(())
}
