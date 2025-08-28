use anyhow::{Context, Result};
use image::{ImageBuffer, Luma, Rgb};
use std::path::{Path, PathBuf};

use crate::image_analysis::FitsImage;
use crate::mtf_stretch::StretchParameters;

pub fn stretch_to_png(
    fits_path: &str,
    output: Option<String>,
    midtone_factor: f64,
    shadow_clipping: f64,
    logarithmic: bool,
    invert: bool,
) -> Result<()> {
    // Load FITS file
    let fits_path = Path::new(fits_path);
    println!("Loading FITS file: {}", fits_path.display());
    
    let image = FitsImage::from_file(fits_path)
        .with_context(|| format!("Failed to load FITS file: {}", fits_path.display()))?;

    println!("Image dimensions: {}x{}", image.width, image.height);

    // Calculate statistics
    let stats = image.calculate_basic_statistics();
    println!("Statistics:");
    println!("  Mean: {:.3}", stats.mean);
    println!("  Median: {:.3}", stats.median);
    println!("  MAD: {:.3}", stats.mad.unwrap_or(0.0));
    println!("  Min: {:.0}", stats.min);
    println!("  Max: {:.0}", stats.max);

    // Determine output path
    let output_path = match output {
        Some(path) => PathBuf::from(path),
        None => {
            let mut path = fits_path.to_path_buf();
            path.set_extension("png");
            path
        }
    };

    println!("Processing image...");
    
    // Apply stretch or logarithmic scaling
    let processed_data = if logarithmic {
        apply_logarithmic_stretch(&image, invert)
    } else {
        apply_mtf_stretch(&image, &stats, midtone_factor, shadow_clipping, invert)?
    };

    // Create PNG image
    let img_buffer = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(
        image.width as u32,
        image.height as u32,
        processed_data,
    )
    .context("Failed to create image buffer")?;

    // Save PNG
    img_buffer
        .save(&output_path)
        .with_context(|| format!("Failed to save PNG to: {}", output_path.display()))?;

    println!("Saved stretched image to: {}", output_path.display());
    Ok(())
}

fn apply_mtf_stretch(
    image: &FitsImage,
    stats: &crate::image_analysis::ImageStatistics,
    midtone_factor: f64,
    shadow_clipping: f64,
    invert: bool,
) -> Result<Vec<u8>> {
    use crate::mtf_stretch::stretch_image;

    // Create stretch parameters
    let stretch_params = StretchParameters {
        factor: midtone_factor,
        black_clipping: shadow_clipping,
    };

    println!("Applying MTF stretch (factor: {:.2}, shadow clipping: {:.2})", 
             midtone_factor, shadow_clipping);

    // Apply MTF stretch to get 16-bit data
    let stretched_16bit = stretch_image(&image.data, stats, stretch_params.factor, stretch_params.black_clipping);
    
    // Convert to 8-bit
    let mut result = Vec::with_capacity(stretched_16bit.len());
    for &pixel in &stretched_16bit {
        let eight_bit = (pixel >> 8) as u8;
        let final_pixel = if invert { 255 - eight_bit } else { eight_bit };
        result.push(final_pixel);
    }

    Ok(result)
}

fn apply_logarithmic_stretch(image: &FitsImage, invert: bool) -> Vec<u8> {
    println!("Applying logarithmic stretch");
    
    // Find min/max for scaling
    let min_val = *image.data.iter().min().unwrap() as f64;
    let max_val = *image.data.iter().max().unwrap() as f64;
    
    println!("Value range: {:.0} - {:.0}", min_val, max_val);
    
    let mut result = Vec::with_capacity(image.data.len());
    
    // Apply logarithmic scaling: log(1 + x)
    let log_max = (1.0 + max_val - min_val).ln();
    
    for &pixel in &image.data {
        let normalized = (pixel as f64 - min_val).max(0.0);
        let log_val = (1.0 + normalized).ln();
        let scaled = (log_val / log_max * 255.0) as u8;
        let final_pixel = if invert { 255 - scaled } else { scaled };
        result.push(final_pixel);
    }
    
    result
}

pub fn create_color_stretch_to_png(
    fits_path: &str,
    output: Option<String>,
    midtone_factor: f64,
    shadow_clipping: f64,
) -> Result<()> {
    // Load FITS file
    let fits_path = Path::new(fits_path);
    println!("Loading FITS file for color visualization: {}", fits_path.display());
    
    let image = FitsImage::from_file(fits_path)
        .with_context(|| format!("Failed to load FITS file: {}", fits_path.display()))?;

    // Calculate statistics
    let stats = image.calculate_basic_statistics();
    
    // Determine output path
    let output_path = match output {
        Some(path) => PathBuf::from(path),
        None => {
            let mut path = fits_path.to_path_buf();
            let stem = path.file_stem().unwrap().to_string_lossy();
            path.set_file_name(format!("{}_color.png", stem));
            path
        }
    };

    println!("Creating false-color visualization...");
    
    // Apply MTF stretch
    let stretched_16bit = crate::mtf_stretch::stretch_image(
        &image.data, 
        &stats, 
        midtone_factor, 
        shadow_clipping
    );
    
    // Create false-color image (heat map style)
    let mut rgb_data = Vec::with_capacity(image.data.len() * 3);
    
    for &pixel in &stretched_16bit {
        let intensity = (pixel >> 8) as u8;
        let (r, g, b) = intensity_to_color(intensity);
        rgb_data.push(r);
        rgb_data.push(g);
        rgb_data.push(b);
    }

    // Create RGB image
    let img_buffer = ImageBuffer::<Rgb<u8>, Vec<u8>>::from_raw(
        image.width as u32,
        image.height as u32,
        rgb_data,
    )
    .context("Failed to create RGB image buffer")?;

    // Save PNG
    img_buffer
        .save(&output_path)
        .with_context(|| format!("Failed to save color PNG to: {}", output_path.display()))?;

    println!("Saved color visualization to: {}", output_path.display());
    Ok(())
}

// Convert intensity to heat map colors
fn intensity_to_color(intensity: u8) -> (u8, u8, u8) {
    let i = intensity as f32 / 255.0;
    
    if i < 0.25 {
        // Black to blue
        let t = i * 4.0;
        (0, 0, (t * 255.0) as u8)
    } else if i < 0.5 {
        // Blue to cyan
        let t = (i - 0.25) * 4.0;
        (0, (t * 255.0) as u8, 255)
    } else if i < 0.75 {
        // Cyan to yellow
        let t = (i - 0.5) * 4.0;
        ((t * 255.0) as u8, 255, (255.0 * (1.0 - t)) as u8)
    } else {
        // Yellow to white
        let t = (i - 0.75) * 4.0;
        (255, 255, (255.0 * t) as u8)
    }
}