use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb, Rgba};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect;
use std::fs::File;
use std::io::BufWriter;
use image::codecs::png::{PngEncoder, CompressionType, FilterType};
use image::{ColorType, ImageEncoder};

use crate::hocus_focus_star_detection::{detect_stars_hocus_focus, HocusFocusParams};
use crate::image_analysis::FitsImage;
use crate::psf_fitting::{PSFType, PSFFitter};
use crate::mtf_stretch::{stretch_image, StretchParameters};

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

/// Enhanced PSF visualization showing multiple stars
pub fn visualize_psf_multi(
    fits_path: &str,
    output: Option<String>,
    num_stars: usize,
    psf_type: &str,
    sort_by: &str,
    grid_cols: usize,
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

    // Filter stars with PSF fits
    let mut stars_with_psf: Vec<_> = result.stars.into_iter()
        .filter(|s| s.psf_model.is_some())
        .collect();
    
    if stars_with_psf.is_empty() {
        anyhow::bail!("No stars with successful PSF fits found");
    }
    
    // Sort stars based on criteria
    match sort_by {
        "hfr" => stars_with_psf.sort_by(|a, b| a.hfr.partial_cmp(&b.hfr).unwrap()),
        "r2" => stars_with_psf.sort_by(|a, b| {
            let r2_a = a.psf_model.as_ref().unwrap().r_squared;
            let r2_b = b.psf_model.as_ref().unwrap().r_squared;
            r2_b.partial_cmp(&r2_a).unwrap() // Higher R² first
        }),
        "brightness" => stars_with_psf.sort_by(|a, b| b.brightness.partial_cmp(&a.brightness).unwrap()),
        _ => {} // Default order
    }
    
    let stars_to_show: Vec<_> = stars_with_psf.into_iter().take(num_stars).collect();
    
    if verbose {
        eprintln!("Showing {} stars sorted by {}", stars_to_show.len(), sort_by);
    }

    // Calculate layout
    let panel_size = 256; // Size for each star's panels
    let panel_spacing = 20;
    let num_rows = (stars_to_show.len() + grid_cols - 1) / grid_cols;
    
    // Each star gets 3 panels (observed, fitted, residual)
    let star_panel_width = panel_size * 3 + panel_spacing * 2;
    let star_panel_height = panel_size + 80; // Extra space for text
    
    // Total image size
    let total_width = grid_cols * star_panel_width + (grid_cols - 1) * panel_spacing + 40;
    let total_height = num_rows * star_panel_height + (num_rows - 1) * panel_spacing + 40;
    
    // Add space for location map
    let map_size = 300;
    let final_width = total_width.max(map_size + 80);
    let final_height = total_height + map_size + 60;
    
    let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(final_width as u32, final_height as u32);
    
    // Fill background
    for pixel in img.pixels_mut() {
        *pixel = Rgba([30, 30, 30, 255]); // Dark gray background
    }

    // Generate residual maps for each star
    let fitter = PSFFitter::new(psf_type_enum);
    
    for (star_idx, star) in stars_to_show.iter().enumerate() {
        let row = star_idx / grid_cols;
        let col = star_idx % grid_cols;
        
        let x_offset = 20 + col * (star_panel_width + panel_spacing);
        let y_offset = 20 + row * (star_panel_height + panel_spacing);
        
        let psf_model = star.psf_model.as_ref().unwrap();
        
        // Generate residual maps
        if let Some((observed, fitted, residuals)) = fitter.generate_residuals(
            &fits.data,
            width,
            height,
            star.position.0,
            star.position.1,
            psf_model,
        ) {
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
            let scale_factor = panel_size / observed.len();
            let panels = [
                (&observed, "Observed", obs_min, obs_range, "grayscale"),
                (&fitted, "Fitted", fit_min, fit_range, "grayscale"),
                (&residuals, "Residual", -res_absmax, 2.0 * res_absmax, "residual"),
            ];
            
            for (panel_idx, (data, _title, min_val, range, color_mode)) in panels.iter().enumerate() {
                let panel_x = x_offset + panel_idx * (panel_size + panel_spacing);
                let panel_y = y_offset + 40;
                
                // Draw panel data
                for (i, row) in data.iter().enumerate() {
                    for (j, &value) in row.iter().enumerate() {
                        let normalized = if *range > 0.0 {
                            (value - min_val) / range
                        } else {
                            0.5
                        };
                        
                        let color = heatmap_color(normalized, color_mode);
                        
                        // Draw scaled pixel
                        for dy in 0..scale_factor {
                            for dx in 0..scale_factor {
                                let px = panel_x + j * scale_factor + dx;
                                let py = panel_y + i * scale_factor + dy;
                                if px < final_width && py < final_height {
                                    img.put_pixel(px as u32, py as u32, Rgba([color.0[0], color.0[1], color.0[2], 255]));
                                }
                            }
                        }
                    }
                }
                
                // Draw border
                draw_hollow_rect_mut(
                    &mut img,
                    Rect::at(panel_x as i32 - 1, panel_y as i32 - 1)
                        .of_size((panel_size + 2) as u32, (panel_size + 2) as u32),
                    Rgba([200, 200, 200, 255])
                );
                
                // Add simple title marker (O=Observed, F=Fitted, R=Residual)
                let marker = match panel_idx {
                    0 => "O",
                    1 => "F",
                    2 => "R",
                    _ => "",
                };
                // Draw marker as simple pixels (very basic)
                if !marker.is_empty() {
                    // This is a placeholder - in a real implementation, you'd want proper text rendering
                }
            }
            
            // Add star info box with color coding based on R²
            let info_y = y_offset + panel_size + 50;
            let r2_color = if psf_model.r_squared > 0.9 {
                Rgba([0, 255, 0, 255]) // Green for excellent fit
            } else if psf_model.r_squared > 0.7 {
                Rgba([255, 255, 0, 255]) // Yellow for good fit
            } else {
                Rgba([255, 0, 0, 255]) // Red for poor fit
            };
            
            // Draw R² indicator box
            for dy in 0..10 {
                for dx in 0..20 {
                    img.put_pixel(
                        (x_offset + dx) as u32,
                        (info_y + dy) as u32,
                        r2_color
                    );
                }
            }
        }
    }
    
    // Create location map
    let map_y = total_height + 20;
    let map_x = (final_width - map_size) / 2;
    
    // Apply stretch to create mini preview
    let stats = fits.calculate_basic_statistics();
    let stretch_params = StretchParameters::default();
    let stretched = stretch_image(&fits.data, &stats, stretch_params.factor, stretch_params.black_clipping);
    
    // Create mini map
    let scale = (width.max(height) as f64 / map_size as f64).ceil() as usize;
    let map_width = width / scale;
    let map_height = height / scale;
    
    for y in 0..map_height {
        for x in 0..map_width {
            let src_x = x * scale;
            let src_y = y * scale;
            let idx = src_y * width + src_x;
            let value = (stretched[idx] >> 8) as u8;
            
            img.put_pixel(
                (map_x + x) as u32,
                (map_y + y) as u32,
                Rgba([value, value, value, 255])
            );
        }
    }
    
    // Draw border around map
    draw_hollow_rect_mut(
        &mut img,
        Rect::at(map_x as i32 - 1, map_y as i32 - 1)
            .of_size((map_width + 2) as u32, (map_height + 2) as u32),
        Rgba([200, 200, 200, 255])
    );
    
    // Mark star locations on map
    for star in &stars_to_show {
        let map_star_x = (star.position.0 / scale as f64) as i32;
        let map_star_y = (star.position.1 / scale as f64) as i32;
        
        // Draw circle around star
        for angle in 0..360 {
            let rad = (angle as f64).to_radians();
            let cx = map_x as i32 + map_star_x + (5.0 * rad.cos()) as i32;
            let cy = map_y as i32 + map_star_y + (5.0 * rad.sin()) as i32;
            
            if cx >= 0 && cx < final_width as i32 && cy >= 0 && cy < final_height as i32 {
                img.put_pixel(cx as u32, cy as u32, Rgba([255, 0, 0, 255]));
            }
        }
    }
    
    // Add map title indicator (simple colored box)
    for dy in 0..3 {
        for dx in 0..50 {
            img.put_pixel(
                (map_x + dx) as u32,
                (map_y - 20 + dy) as u32,
                Rgba([100, 100, 200, 255]) // Blue indicator for map
            );
        }
    }

    // Generate output filename
    let output_path = output.unwrap_or_else(|| {
        let base = fits_path.trim_end_matches(".fits").trim_end_matches(".fit");
        format!("{}_psf_multi.png", base)
    });

    // Save the image
    let file = File::create(&output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path))?;
    let writer = BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(writer, CompressionType::Best, FilterType::Adaptive);
    
    encoder
        .write_image(
            &img,
            final_width as u32,
            final_height as u32,
            ColorType::Rgba8.into(),
        )
        .with_context(|| format!("Failed to write PNG image to {}", output_path))?;

    println!("Created multi-star PSF visualization: {}", output_path);
    println!("Visualized {} stars sorted by {}", stars_to_show.len(), sort_by);
    
    Ok(())
}