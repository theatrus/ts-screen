use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;
use std::fs::File;
use std::io::BufWriter;
use image::codecs::png::{PngEncoder, CompressionType, FilterType};
use image::{ColorType, ImageEncoder};

use crate::hocus_focus_star_detection::{detect_stars_hocus_focus, HocusFocusParams};
use crate::image_analysis::FitsImage;
use crate::psf_fitting::{PSFType, PSFFitter};

/// Create a heatmap color from value (0.0 to 1.0)
fn heatmap_color(value: f64, mode: &str) -> Rgb<u8> {
    let clamped = value.max(0.0).min(1.0);
    
    match mode {
        "residual" => {
            // Red-white-blue for residuals (negative to positive)
            if value < 0.5 {
                // Blue to white (negative residuals)
                let t = value * 2.0;
                let r = (255.0 * t) as u8;
                let g = (255.0 * t) as u8;
                let b = 255;
                Rgb([r, g, b])
            } else {
                // White to red (positive residuals)
                let t = (value - 0.5) * 2.0;
                let r = 255;
                let g = (255.0 * (1.0 - t)) as u8;
                let b = (255.0 * (1.0 - t)) as u8;
                Rgb([r, g, b])
            }
        }
        _ => {
            // Grayscale for observed/fitted
            let gray = (255.0 * clamped) as u8;
            Rgb([gray, gray, gray])
        }
    }
}

pub fn visualize_psf_residuals(
    fits_path: &str,
    output: Option<String>,
    star_index: Option<usize>,
    psf_type: &str,
    max_stars: usize,
    verbose: bool,
) -> Result<()> {
    if verbose {
        eprintln!("Loading FITS file: {}", fits_path);
    }

    // Load the FITS file
    let fits = FitsImage::from_file(std::path::Path::new(fits_path))?;
    let width = fits.width;
    let height = fits.height;

    // Parse PSF type
    let psf_type_enum: PSFType = psf_type.parse().unwrap_or(PSFType::Moffat4);
    
    if psf_type_enum == PSFType::None {
        anyhow::bail!("PSF type cannot be 'none' for residual visualization");
    }

    // Detect stars using HocusFocus
    let mut params = HocusFocusParams::default();
    params.psf_type = psf_type_enum;
    
    if verbose {
        eprintln!("Detecting stars with PSF fitting enabled ({:?})...", psf_type_enum);
    }

    let result = detect_stars_hocus_focus(&fits.data, width, height, &params);
    
    if result.stars.is_empty() {
        anyhow::bail!("No stars detected in image");
    }

    // Sort stars by HFR and take top N
    let mut stars_with_psf: Vec<_> = result.stars.into_iter()
        .filter(|s| s.psf_model.is_some())
        .collect();
    
    if stars_with_psf.is_empty() {
        anyhow::bail!("No stars with successful PSF fits found");
    }
    
    stars_with_psf.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap());
    let stars_to_show: Vec<_> = stars_with_psf.into_iter().take(max_stars).collect();
    
    if verbose {
        eprintln!("Found {} stars with PSF fits, showing top {}", stars_to_show.len(), max_stars);
    }

    // Determine which star to visualize
    let star_idx = star_index.unwrap_or(0);
    if star_idx >= stars_to_show.len() {
        anyhow::bail!("Star index {} out of range (0-{})", star_idx, stars_to_show.len() - 1);
    }
    
    let star = &stars_to_show[star_idx];
    let psf_model = star.psf_model.as_ref().unwrap();
    
    if verbose {
        eprintln!("Visualizing star at ({:.1}, {:.1})", star.position.0, star.position.1);
        eprintln!("PSF Model: R² = {:.3}, FWHM = {:.2}", psf_model.r_squared, psf_model.fwhm);
    }

    // Generate residual maps
    let fitter = PSFFitter::new(psf_type_enum);
    let (observed, fitted, residuals) = fitter.generate_residuals(
        &fits.data,
        width,
        height,
        star.position.0,
        star.position.1,
        psf_model,
    ).context("Failed to generate residual maps")?;

    // Create composite image (3 panels side by side)
    let panel_size = observed.len() * 8; // Scale up for visibility
    let total_width = panel_size * 3 + 40; // Extra space for borders
    let total_height = panel_size + 60; // Extra space for labels
    
    let mut img = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(total_width as u32, total_height as u32);
    
    // Fill background
    for pixel in img.pixels_mut() {
        *pixel = Rgb([20, 20, 20]); // Dark gray background
    }

    // Normalize data for visualization
    let obs_min = observed.iter().flat_map(|row| row.iter()).fold(f64::INFINITY, |a, &b| a.min(b));
    let obs_max = observed.iter().flat_map(|row| row.iter()).fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let obs_range = obs_max - obs_min;

    let fit_min = fitted.iter().flat_map(|row| row.iter()).fold(f64::INFINITY, |a, &b| a.min(b));
    let fit_max = fitted.iter().flat_map(|row| row.iter()).fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let fit_range = fit_max - fit_min;

    let res_min = residuals.iter().flat_map(|row| row.iter()).fold(f64::INFINITY, |a, &b| a.min(b));
    let res_max = residuals.iter().flat_map(|row| row.iter()).fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let res_absmax = res_min.abs().max(res_max.abs());

    // Draw panels
    let scale_factor = 8;
    let panel_offsets = [10, panel_size + 20, panel_size * 2 + 30];
    let panel_titles = ["Observed", "Fitted", "Residuals"];
    
    for (panel_idx, (data, _title)) in [
        (&observed, panel_titles[0]),
        (&fitted, panel_titles[1]),
        (&residuals, panel_titles[2])
    ].iter().enumerate() {
        let x_offset = panel_offsets[panel_idx];
        let y_offset = 40;
        
        // Draw panel data
        for (i, row) in data.iter().enumerate() {
            for (j, &value) in row.iter().enumerate() {
                let normalized = match panel_idx {
                    0 => (value - obs_min) / obs_range,
                    1 => (value - fit_min) / fit_range,
                    2 => (value + res_absmax) / (2.0 * res_absmax), // Center residuals at 0.5
                    _ => 0.0,
                };
                
                let color = if panel_idx == 2 {
                    heatmap_color(normalized, "residual")
                } else {
                    heatmap_color(normalized, "grayscale")
                };
                
                // Draw scaled pixel
                for dy in 0..scale_factor {
                    for dx in 0..scale_factor {
                        let px = x_offset + j * scale_factor + dx;
                        let py = y_offset + i * scale_factor + dy;
                        if px < total_width && py < total_height {
                            img.put_pixel(px as u32, py as u32, color);
                        }
                    }
                }
            }
        }
        
        // Draw border
        draw_hollow_rect_mut(
            &mut img,
            Rect::at(x_offset as i32 - 1, y_offset as i32 - 1)
                .of_size((panel_size + 2) as u32, (panel_size + 2) as u32),
            Rgb([200, 200, 200])
        );
    }

    // Add text labels (if we have font support)
    // For now, we'll skip text rendering as it requires additional font setup

    // Generate output filename
    let output_path = output.unwrap_or_else(|| {
        let base = fits_path.trim_end_matches(".fits").trim_end_matches(".fit");
        format!("{}_psf_residuals.png", base)
    });

    // Save the image
    let file = File::create(&output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path))?;
    let writer = BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(writer, CompressionType::Best, FilterType::Adaptive);
    
    encoder
        .write_image(
            &img,
            total_width as u32,
            total_height as u32,
            ColorType::Rgb8.into(),
        )
        .with_context(|| format!("Failed to write PNG image to {}", output_path))?;

    println!("Created PSF residual visualization: {}", output_path);
    println!("Star {}/{}: Position ({:.1}, {:.1}), HFR {:.3}, PSF R² {:.3}",
        star_idx + 1, stars_to_show.len(),
        star.position.0, star.position.1,
        star.hfr, psf_model.r_squared
    );
    
    if verbose {
        println!("\nPSF Model Details:");
        println!("  Type: {:?}", psf_model.psf_type);
        println!("  FWHM: {:.3} pixels", psf_model.fwhm);
        println!("  Eccentricity: {:.3}", psf_model.eccentricity);
        println!("  Orientation: {:.1}°", psf_model.theta.to_degrees());
        println!("  Amplitude: {:.0}", psf_model.amplitude);
        println!("  Background: {:.1}", psf_model.background);
        println!("  RMSE: {:.1}", psf_model.rmse);
    }

    Ok(())
}