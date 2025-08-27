use anyhow::{Context, Result};
use byteorder::{BigEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImageStatistics {
    pub width: usize,
    pub height: usize,
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub star_count: Option<usize>,
    pub hfr: Option<f64>,
    pub fwhm: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StarDetection {
    pub x: f64,
    pub y: f64,
    pub brightness: f64,
    pub hfr: f64,
    pub fwhm: f64,
}

/// FITS image data structure
pub struct FitsImage {
    pub width: usize,
    pub height: usize,
    #[allow(dead_code)]
    pub bit_depth: i32,
    pub data: Vec<f64>,
}

impl FitsImage {
    /// Load FITS image data from file
    pub fn from_file(path: &Path) -> Result<Self> {
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open FITS file: {}", path.display()))?;

        // Parse header to get image dimensions
        let (width, height, bit_depth, data_offset) = parse_fits_header(&mut file)?;

        // Seek to data section
        file.seek(SeekFrom::Start(data_offset))?;

        // Read image data based on bit depth
        let data = match bit_depth {
            8 => read_8bit_data(&mut file, width * height)?,
            16 => read_16bit_data(&mut file, width * height)?,
            32 => read_32bit_data(&mut file, width * height)?,
            -32 => read_float32_data(&mut file, width * height)?,
            -64 => read_float64_data(&mut file, width * height)?,
            _ => return Err(anyhow::anyhow!("Unsupported bit depth: {}", bit_depth)),
        };

        Ok(FitsImage {
            width,
            height,
            bit_depth,
            data,
        })
    }

    /// Calculate basic image statistics
    pub fn calculate_statistics(&self) -> ImageStatistics {
        let mut sorted_data = self.data.clone();
        sorted_data.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let sum: f64 = self.data.iter().sum();
        let mean = sum / self.data.len() as f64;

        let median = if sorted_data.len() % 2 == 0 {
            let mid = sorted_data.len() / 2;
            (sorted_data[mid - 1] + sorted_data[mid]) / 2.0
        } else {
            sorted_data[sorted_data.len() / 2]
        };

        let variance: f64 = self.data.iter().map(|&x| (x - mean).powi(2)).sum::<f64>()
            / (self.data.len() - 1) as f64;
        let std_dev = variance.sqrt();

        let min = sorted_data[0];
        let max = sorted_data[sorted_data.len() - 1];

        // Detect stars and calculate HFR
        let stars = self.detect_stars();
        let star_count = Some(stars.len());
        let hfr = if !stars.is_empty() {
            Some(stars.iter().map(|s| s.hfr).sum::<f64>() / stars.len() as f64)
        } else {
            None
        };
        let fwhm = if !stars.is_empty() {
            Some(stars.iter().map(|s| s.fwhm).sum::<f64>() / stars.len() as f64)
        } else {
            None
        };

        ImageStatistics {
            width: self.width,
            height: self.height,
            mean,
            median,
            std_dev,
            min,
            max,
            star_count,
            hfr,
            fwhm,
        }
    }

    /// Detect stars in the image using a simple threshold-based algorithm
    pub fn detect_stars(&self) -> Vec<StarDetection> {
        let mut stars = Vec::new();

        // Calculate background statistics for thresholding
        let background_level = self.estimate_background();
        let threshold = background_level + 5.0 * self.estimate_noise();

        // Find local maxima above threshold
        let min_separation = 5; // Minimum pixels between star centers
        let aperture_radius = 10; // Radius for star measurement

        for y in aperture_radius..(self.height - aperture_radius) {
            for x in aperture_radius..(self.width - aperture_radius) {
                let center_idx = y * self.width + x;
                let center_value = self.data[center_idx];

                // Check if this pixel is above threshold
                if center_value < threshold {
                    continue;
                }

                // Check if this is a local maximum
                let mut is_maximum = true;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let check_idx =
                            ((y as i32 + dy) as usize) * self.width + ((x as i32 + dx) as usize);
                        if self.data[check_idx] >= center_value {
                            is_maximum = false;
                            break;
                        }
                    }
                    if !is_maximum {
                        break;
                    }
                }

                if !is_maximum {
                    continue;
                }

                // Check minimum separation from existing stars
                let too_close = stars.iter().any(|star: &StarDetection| {
                    let dx = star.x - x as f64;
                    let dy = star.y - y as f64;
                    (dx * dx + dy * dy).sqrt() < min_separation as f64
                });

                if too_close {
                    continue;
                }

                // Refine centroid and measure star properties
                if let Some(star) = self.measure_star(x, y, aperture_radius, background_level) {
                    stars.push(star);
                }
            }
        }

        // Sort by brightness (descending)
        stars.sort_by(|a, b| {
            b.brightness
                .partial_cmp(&a.brightness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top 1000 stars to avoid memory issues
        stars.truncate(1000);
        stars
    }

    /// Estimate background level using median of border pixels
    fn estimate_background(&self) -> f64 {
        let border_width = self.width.min(self.height) / 10; // Use 10% of smallest dimension
        let mut border_pixels = Vec::new();

        // Top and bottom borders
        for y in 0..border_width {
            for x in 0..self.width {
                border_pixels.push(self.data[y * self.width + x]);
                border_pixels.push(self.data[(self.height - 1 - y) * self.width + x]);
            }
        }

        // Left and right borders (excluding corners already counted)
        for y in border_width..(self.height - border_width) {
            for x in 0..border_width {
                border_pixels.push(self.data[y * self.width + x]);
                border_pixels.push(self.data[y * self.width + (self.width - 1 - x)]);
            }
        }

        border_pixels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Return median
        if border_pixels.len() % 2 == 0 {
            let mid = border_pixels.len() / 2;
            (border_pixels[mid - 1] + border_pixels[mid]) / 2.0
        } else {
            border_pixels[border_pixels.len() / 2]
        }
    }

    /// Estimate noise level using MAD (Median Absolute Deviation) of background
    fn estimate_noise(&self) -> f64 {
        let background = self.estimate_background();
        let border_width = self.width.min(self.height) / 10;
        let mut deviations = Vec::new();

        // Sample background regions
        for y in 0..border_width {
            for x in 0..self.width {
                deviations.push((self.data[y * self.width + x] - background).abs());
            }
        }

        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mad = if deviations.len() % 2 == 0 {
            let mid = deviations.len() / 2;
            (deviations[mid - 1] + deviations[mid]) / 2.0
        } else {
            deviations[deviations.len() / 2]
        };

        mad * 1.4826 // Convert MAD to equivalent standard deviation
    }

    /// Measure properties of a star at given position
    fn measure_star(
        &self,
        center_x: usize,
        center_y: usize,
        radius: usize,
        background: f64,
    ) -> Option<StarDetection> {
        let mut total_flux = 0.0;
        let mut weighted_x = 0.0;
        let mut weighted_y = 0.0;
        let mut pixel_count = 0;

        // Calculate centroid using flux-weighted position
        for dy in -(radius as i32)..=(radius as i32) {
            for dx in -(radius as i32)..=(radius as i32) {
                let x = center_x as i32 + dx;
                let y = center_y as i32 + dy;

                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }

                let distance_sq = (dx * dx + dy * dy) as f64;
                if distance_sq > (radius * radius) as f64 {
                    continue;
                }

                let idx = (y as usize) * self.width + (x as usize);
                let flux = (self.data[idx] - background).max(0.0);

                total_flux += flux;
                weighted_x += x as f64 * flux;
                weighted_y += y as f64 * flux;
                pixel_count += 1;
            }
        }

        if total_flux <= 0.0 || pixel_count < 9 {
            return None; // Not enough signal
        }

        let centroid_x = weighted_x / total_flux;
        let centroid_y = weighted_y / total_flux;

        // Calculate HFR (Half Flux Radius)
        let hfr = self.calculate_hfr(centroid_x, centroid_y, radius, background, total_flux);

        // Calculate FWHM (Full Width Half Maximum)
        let fwhm = hfr * 2.0 * 1.177; // Convert HFR to FWHM for Gaussian PSF

        Some(StarDetection {
            x: centroid_x,
            y: centroid_y,
            brightness: total_flux,
            hfr,
            fwhm,
        })
    }

    /// Calculate Half Flux Radius for a star
    fn calculate_hfr(
        &self,
        center_x: f64,
        center_y: f64,
        max_radius: usize,
        background: f64,
        total_flux: f64,
    ) -> f64 {
        let half_flux = total_flux / 2.0;
        let mut cumulative_flux = 0.0;

        // Sample radii from 0.5 to max_radius
        for r_tenth in 5..=(max_radius * 10) {
            let radius = r_tenth as f64 / 10.0;
            let mut ring_flux = 0.0;
            let mut ring_pixels = 0;

            // Sample points around the circle at this radius
            let num_samples = (radius * 8.0).max(12.0) as usize;
            for i in 0..num_samples {
                let angle = 2.0 * std::f64::consts::PI * i as f64 / num_samples as f64;
                let sample_x = center_x + radius * angle.cos();
                let sample_y = center_y + radius * angle.sin();

                // Bilinear interpolation
                if let Some(value) = self.interpolate_pixel(sample_x, sample_y) {
                    ring_flux += (value - background).max(0.0);
                    ring_pixels += 1;
                }
            }

            if ring_pixels > 0 {
                cumulative_flux += ring_flux / ring_pixels as f64;
            }

            if cumulative_flux >= half_flux {
                return radius;
            }
        }

        max_radius as f64 // Fallback if we never reach half flux
    }

    /// Bilinear interpolation for sub-pixel sampling
    fn interpolate_pixel(&self, x: f64, y: f64) -> Option<f64> {
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = x0 + 1;
        let y1 = y0 + 1;

        if x1 >= self.width || y1 >= self.height {
            return None;
        }

        let dx = x - x0 as f64;
        let dy = y - y0 as f64;

        let v00 = self.data[y0 * self.width + x0];
        let v10 = self.data[y0 * self.width + x1];
        let v01 = self.data[y1 * self.width + x0];
        let v11 = self.data[y1 * self.width + x1];

        let interpolated = v00 * (1.0 - dx) * (1.0 - dy)
            + v10 * dx * (1.0 - dy)
            + v01 * (1.0 - dx) * dy
            + v11 * dx * dy;

        Some(interpolated)
    }
}

/// Parse FITS header to extract image dimensions and data offset
fn parse_fits_header(file: &mut File) -> Result<(usize, usize, i32, u64)> {
    let mut width = 0;
    let mut height = 0;
    let mut bit_depth = 0;
    let mut data_offset = 0u64;

    // Read FITS header in 2880-byte blocks
    loop {
        let mut block = vec![0u8; 2880];
        file.read_exact(&mut block)?;
        data_offset += 2880;

        // Parse header cards (80 characters each)
        for card_data in block.chunks(80) {
            let card = String::from_utf8_lossy(card_data);
            let card = card.trim();

            if card.starts_with("END") {
                if width == 0 || height == 0 {
                    return Err(anyhow::anyhow!(
                        "Could not find image dimensions in FITS header"
                    ));
                }
                return Ok((width, height, bit_depth, data_offset));
            }

            if let Some(eq_pos) = card.find('=') {
                let keyword = card[..eq_pos].trim();
                let value_part = &card[eq_pos + 1..];
                let value = if let Some(comment_pos) = value_part.find('/') {
                    value_part[..comment_pos].trim()
                } else {
                    value_part.trim()
                };

                match keyword {
                    "NAXIS1" => width = value.parse().unwrap_or(0),
                    "NAXIS2" => height = value.parse().unwrap_or(0),
                    "BITPIX" => bit_depth = value.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }
    }
}

/// Read 8-bit unsigned integer data
fn read_8bit_data(file: &mut File, num_pixels: usize) -> Result<Vec<f64>> {
    let mut data = Vec::with_capacity(num_pixels);
    let mut buffer = vec![0u8; num_pixels];
    file.read_exact(&mut buffer)?;

    for &byte in &buffer {
        data.push(byte as f64);
    }

    Ok(data)
}

/// Read 16-bit signed integer data (big-endian)
fn read_16bit_data(file: &mut File, num_pixels: usize) -> Result<Vec<f64>> {
    let mut data = Vec::with_capacity(num_pixels);

    for _ in 0..num_pixels {
        let value = file.read_i16::<BigEndian>()?;
        data.push(value as f64);
    }

    Ok(data)
}

/// Read 32-bit signed integer data (big-endian)
fn read_32bit_data(file: &mut File, num_pixels: usize) -> Result<Vec<f64>> {
    let mut data = Vec::with_capacity(num_pixels);

    for _ in 0..num_pixels {
        let value = file.read_i32::<BigEndian>()?;
        data.push(value as f64);
    }

    Ok(data)
}

/// Read 32-bit floating point data (big-endian)
fn read_float32_data(file: &mut File, num_pixels: usize) -> Result<Vec<f64>> {
    let mut data = Vec::with_capacity(num_pixels);

    for _ in 0..num_pixels {
        let value = file.read_f32::<BigEndian>()?;
        data.push(value as f64);
    }

    Ok(data)
}

/// Read 64-bit floating point data (big-endian)
fn read_float64_data(file: &mut File, num_pixels: usize) -> Result<Vec<f64>> {
    let mut data = Vec::with_capacity(num_pixels);

    for _ in 0..num_pixels {
        let value = file.read_f64::<BigEndian>()?;
        data.push(value);
    }

    Ok(data)
}
