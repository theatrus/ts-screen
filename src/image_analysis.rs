use anyhow::{Context, Result};
use bumpalo::Bump;
use fitrs::{Fits, FitsData, FitsDataArray};
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
    pub bit_depth: i32,
    pub data: Vec<u16>, // Keep as 16-bit unsigned integers
}

impl FitsImage {
    /// Load FITS image data from file using fitrs
    pub fn from_file(path: &Path) -> Result<Self> {
        let fits = Fits::open(path)
            .with_context(|| format!("Failed to open FITS file: {}", path.display()))?;
        
        // Get the primary HDU (index 0)
        let hdu = fits.get(0)
            .ok_or_else(|| anyhow::anyhow!("No primary HDU found in FITS file"))?;
        
        // Get image dimensions and bit depth from header
        let width = match hdu.value("NAXIS1") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as usize,
                _ => return Err(anyhow::anyhow!("NAXIS1 is not an integer")),
            },
            None => return Err(anyhow::anyhow!("Missing NAXIS1 header")),
        };
        
        let height = match hdu.value("NAXIS2") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as usize,
                _ => return Err(anyhow::anyhow!("NAXIS2 is not an integer")),
            },
            None => return Err(anyhow::anyhow!("Missing NAXIS2 header")),
        };
        
        let bit_depth = match hdu.value("BITPIX") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as i32,
                _ => return Err(anyhow::anyhow!("BITPIX is not an integer")),
            },
            None => return Err(anyhow::anyhow!("Missing BITPIX header")),
        };
        
        // Check that we have a 2D image
        let naxis = match hdu.value("NAXIS") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as u32,
                _ => return Err(anyhow::anyhow!("NAXIS is not an integer")),
            },
            None => return Err(anyhow::anyhow!("Missing NAXIS header")),
        };
        
        // Get BZERO and BSCALE for data scaling (FITS standard)
        let bzero = match hdu.value("BZERO") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as f64,
                fitrs::HeaderValue::RealFloatingNumber(f) => *f,
                _ => 0.0,
            },
            None => 0.0,
        };
        
        let bscale = match hdu.value("BSCALE") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as f64,
                fitrs::HeaderValue::RealFloatingNumber(f) => *f,
                _ => 1.0,
            },
            None => 1.0,
        };
        
        if naxis < 2 {
            return Err(anyhow::anyhow!(
                "FITS file does not contain 2D image data (NAXIS={})", naxis
            ));
        }
        
        // Read image data
        let fits_data = hdu.read_data();
        
        // Convert to u16 based on data type, applying FITS scaling (Actual = BSCALE * Raw + BZERO)
        let data: Vec<u16> = match fits_data {
            FitsData::Characters(_) => {
                return Err(anyhow::anyhow!("FITS file contains character data, not image data"));
            },
            FitsData::IntegersI32(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    if let Some(raw_val) = x {
                        let scaled_val = bscale * (raw_val as f64) + bzero;
                        scaled_val.max(0.0).min(65535.0) as u16
                    } else {
                        0u16
                    }
                }).collect()
            },
            FitsData::IntegersU32(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    if let Some(raw_val) = x {
                        let scaled_val = bscale * (raw_val as f64) + bzero;
                        scaled_val.max(0.0).min(65535.0) as u16
                    } else {
                        0u16
                    }
                }).collect()
            },
            FitsData::FloatingPoint32(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    let scaled_val = bscale * (x as f64) + bzero;
                    scaled_val.max(0.0).min(65535.0) as u16
                }).collect()
            },
            FitsData::FloatingPoint64(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    let scaled_val = bscale * x + bzero;
                    scaled_val.max(0.0).min(65535.0) as u16
                }).collect()
            },
        };
        
        
        if data.len() != width * height {
            return Err(anyhow::anyhow!(
                "Data size mismatch: expected {} pixels, got {}", 
                width * height, 
                data.len()
            ));
        }
        
        Ok(FitsImage {
            width,
            height,
            bit_depth,
            data,
        })
    }

    /// Calculate basic image statistics
    pub fn calculate_statistics(&self) -> ImageStatistics {
        // Use arena for temporary allocation
        let arena = Bump::new();
        let mut sorted_data = bumpalo::vec![in &arena];
        sorted_data.extend_from_slice(&self.data);
        sorted_data.sort();

        let sum: u64 = self.data.iter().map(|&x| x as u64).sum();
        let mean = sum as f64 / self.data.len() as f64;

        let median = if sorted_data.len() % 2 == 0 {
            let mid = sorted_data.len() / 2;
            (sorted_data[mid - 1] as f64 + sorted_data[mid] as f64) / 2.0
        } else {
            sorted_data[sorted_data.len() / 2] as f64
        };

        let variance: f64 = self.data.iter()
            .map(|&x| (x as f64 - mean).powi(2))
            .sum::<f64>() / (self.data.len() - 1) as f64;
        let std_dev = variance.sqrt();

        let min = sorted_data[0] as f64;
        let max = sorted_data[sorted_data.len() - 1] as f64;

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

    /// Detect stars using N.I.N.A.'s exact algorithm (optimized)
    pub fn detect_stars(&self) -> Vec<StarDetection> {
        // Create arena for temporary allocations
        let arena = Bump::new();
        let mut stars = Vec::new();

        // N.I.N.A. uses minimum and maximum star sizes based on image resolution
        let resize_factor = 1.0_f64; // For full resolution images
        let min_star_size = (5.0_f64 * resize_factor).floor() as usize;
        let max_star_size = (150.0_f64 * resize_factor).ceil() as usize;

        // First pass: find bright pixels that could be star centers (optimization)
        let global_background = self.estimate_background_arena(&arena);
        let global_noise = self.estimate_noise_arena(&arena);
        let candidate_threshold = (global_background + 2.0 * global_noise) as u16;
        
        let mut candidates = bumpalo::vec![in &arena];
        
        // Find local maxima above threshold (efficient first pass)
        for y in (min_star_size..(self.height - min_star_size)).step_by(4) {
            for x in (min_star_size..(self.width - min_star_size)).step_by(4) {
                let center_value = self.data[y * self.width + x];
                
                if center_value > candidate_threshold {
                    // Quick local maximum check
                    let mut is_local_max = true;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 { continue; }
                            let check_y = (y as i32 + dy) as usize;
                            let check_x = (x as i32 + dx) as usize;
                            if check_y < self.height && check_x < self.width {
                                if self.data[check_y * self.width + check_x] >= center_value {
                                    is_local_max = false;
                                    break;
                                }
                            }
                        }
                        if !is_local_max { break; }
                    }
                    
                    if is_local_max {
                        candidates.push((x, y));
                    }
                }
            }
        }
        
        // Second pass: apply N.I.N.A.'s exact criteria to candidates
        for (x, y) in candidates.iter() {
            if let Some(star) = self.analyze_potential_star_nina(&arena, *x, *y, min_star_size, max_star_size) {
                // Check if this star is too close to existing stars
                let too_close = stars.iter().any(|existing_star: &StarDetection| {
                    let dx = existing_star.x - star.x;
                    let dy = existing_star.y - star.y;
                    (dx * dx + dy * dy).sqrt() < min_star_size as f64
                });

                if !too_close {
                    stars.push(star);
                }
            }
        }

        // Sort by brightness (descending) like N.I.N.A.
        stars.sort_by(|a, b| {
            b.brightness
                .partial_cmp(&a.brightness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        stars
    }
    
    /// Analyze a potential star location using N.I.N.A.'s exact criteria
    fn analyze_potential_star_nina(
        &self,
        arena: &Bump,
        center_x: usize,
        center_y: usize,
        min_size: usize,
        max_size: usize,
    ) -> Option<StarDetection> {
        // N.I.N.A. examines a large rectangle around the potential star center
        let large_rect_size = max_size;
        let half_rect = large_rect_size / 2;
        
        if center_x < half_rect || center_y < half_rect || 
           center_x + half_rect >= self.width || center_y + half_rect >= self.height {
            return None;
        }
        
        // Calculate mean and standard deviation of large rectangle area
        let mut large_rect_pixels = bumpalo::vec![in arena];
        for dy in 0..large_rect_size {
            for dx in 0..large_rect_size {
                let px = center_x - half_rect + dx;
                let py = center_y - half_rect + dy;
                large_rect_pixels.push(self.data[py * self.width + px] as f64);
            }
        }
        
        let large_rect_sum: f64 = large_rect_pixels.iter().sum();
        let large_rect_mean = large_rect_sum / large_rect_pixels.len() as f64;
        
        let large_rect_variance: f64 = large_rect_pixels
            .iter()
            .map(|&val| (val - large_rect_mean).powi(2))
            .sum::<f64>() / large_rect_pixels.len() as f64;
        let large_rect_stdev = large_rect_variance.sqrt();
        
        // N.I.N.A.'s star detection criteria: star must be brighter than background
        let brightness_threshold = large_rect_mean + (0.1 * large_rect_mean).min(large_rect_stdev);
        
        // Examine inner star area
        let inner_radius = min_size;
        let mut inner_star_pixels = bumpalo::vec![in arena];
        let mut star_brightness_sum = 0.0;
        let mut pixel_count = 0;
        
        for dy in -(inner_radius as i32)..=(inner_radius as i32) {
            for dx in -(inner_radius as i32)..=(inner_radius as i32) {
                let px = center_x as i32 + dx;
                let py = center_y as i32 + dy;
                
                if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
                    continue;
                }
                
                let distance_sq = (dx * dx + dy * dy) as f64;
                if distance_sq <= (inner_radius * inner_radius) as f64 {
                    let pixel_val = self.data[py as usize * self.width + px as usize] as f64;
                    inner_star_pixels.push(pixel_val);
                    star_brightness_sum += pixel_val;
                    pixel_count += 1;
                }
            }
        }
        
        if pixel_count == 0 {
            return None;
        }
        
        let star_mean_brightness = star_brightness_sum / pixel_count as f64;
        
        // N.I.N.A.'s brightness check
        if star_mean_brightness < brightness_threshold {
            return None;
        }
        
        // N.I.N.A.'s minimum bright pixels check
        let bright_pixel_threshold = large_rect_mean + 1.5 * large_rect_stdev;
        let minimum_bright_pixels = 3; // N.I.N.A. uses a small minimum
        let bright_pixel_count = inner_star_pixels
            .iter()
            .filter(|&&val| val > bright_pixel_threshold)
            .count();
            
        if bright_pixel_count < minimum_bright_pixels {
            return None;
        }
        
        // Calculate HFR using N.I.N.A.'s exact method
        self.calculate_nina_hfr(arena, center_x, center_y, large_rect_mean, inner_radius)
    }
    
    /// Calculate HFR using N.I.N.A.'s exact algorithm
    fn calculate_nina_hfr(
        &self,
        arena: &Bump,
        center_x: usize,
        center_y: usize, 
        surrounding_mean: f64,
        base_radius: usize,
    ) -> Option<StarDetection> {
        let outer_radius = (base_radius as f64 * 1.2) as usize;
        let mut sum = 0.0;
        let mut sum_dist = 0.0;
        let mut total_flux = 0.0;
        let mut weighted_x = 0.0;
        let mut weighted_y = 0.0;
        
        for dy in -(outer_radius as i32)..=(outer_radius as i32) {
            for dx in -(outer_radius as i32)..=(outer_radius as i32) {
                let px = center_x as i32 + dx;
                let py = center_y as i32 + dy;
                
                if px < 0 || py < 0 || px >= self.width as i32 || py >= self.height as i32 {
                    continue;
                }
                
                let distance = ((dx * dx + dy * dy) as f64).sqrt();
                if distance <= outer_radius as f64 {
                    let pixel_val = self.data[py as usize * self.width + px as usize] as f64;
                    // N.I.N.A. subtracts surrounding mean and rounds
                    let background_subtracted = (pixel_val - surrounding_mean).round().max(0.0);
                    
                    if background_subtracted > 0.0 {
                        sum += background_subtracted;
                        sum_dist += background_subtracted * distance;
                        total_flux += background_subtracted;
                        weighted_x += px as f64 * background_subtracted;
                        weighted_y += py as f64 * background_subtracted;
                    }
                }
            }
        }
        
        if sum <= 0.0 {
            return None;
        }
        
        // N.I.N.A.'s exact HFR calculation
        let hfr = if sum > 0.0 {
            sum_dist / sum
        } else {
            // N.I.N.A.'s fallback value
            (2.0_f64).sqrt() * outer_radius as f64
        };
        
        let centroid_x = weighted_x / total_flux;
        let centroid_y = weighted_y / total_flux;
        let fwhm = hfr * 2.0 * 1.177; // Standard conversion
        
        Some(StarDetection {
            x: centroid_x,
            y: centroid_y,
            brightness: total_flux,
            hfr,
            fwhm,
        })
    }

    /// Estimate background level using median of border pixels (heap-allocated version)
    fn estimate_background(&self) -> f64 {
        let arena = Bump::new();
        self.estimate_background_arena(&arena)
    }
    
    /// Estimate background level using median of border pixels with arena allocation
    fn estimate_background_arena(&self, arena: &Bump) -> f64 {
        let border_width = self.width.min(self.height) / 10; // Use 10% of smallest dimension
        let mut border_pixels = bumpalo::vec![in arena];

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

        border_pixels.sort();

        // Return median as f64 for calculations
        if border_pixels.len() % 2 == 0 {
            let mid = border_pixels.len() / 2;
            (border_pixels[mid - 1] as f64 + border_pixels[mid] as f64) / 2.0
        } else {
            border_pixels[border_pixels.len() / 2] as f64
        }
    }

    /// Estimate noise level using MAD (Median Absolute Deviation) of background (heap version)
    fn estimate_noise(&self) -> f64 {
        let arena = Bump::new();
        self.estimate_noise_arena(&arena)
    }
    
    /// Estimate noise level using MAD (Median Absolute Deviation) of background with arena
    fn estimate_noise_arena(&self, arena: &Bump) -> f64 {
        let background = self.estimate_background_arena(arena);
        let border_width = self.width.min(self.height) / 10;
        let mut deviations = bumpalo::vec![in arena];

        // Sample background regions
        for y in 0..border_width {
            for x in 0..self.width {
                deviations.push((self.data[y * self.width + x] as f64 - background).abs());
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

    
    /// Measure star properties using N.I.N.A.-compatible algorithm with arena allocation
    fn measure_star_nina_style_arena(
        &self,
        arena: &Bump,
        center_x: usize,
        center_y: usize,
        radius: usize,
    ) -> Option<StarDetection> {
        // Estimate local background using annulus method
        let local_background = self.estimate_local_background_arena(arena, center_x, center_y, radius);
        
        let mut total_flux = 0.0;
        let mut weighted_x = 0.0;
        let mut weighted_y = 0.0;
        let mut weighted_distance_sum = 0.0;
        
        // Calculate centroid and flux using local background subtraction
        for dy in -(radius as i32)..=(radius as i32) {
            for dx in -(radius as i32)..=(radius as i32) {
                let x = center_x as i32 + dx;
                let y = center_y as i32 + dy;
                
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                
                let distance = ((dx * dx + dy * dy) as f64).sqrt();
                if distance > radius as f64 {
                    continue;
                }
                
                let idx = (y as usize) * self.width + (x as usize);
                let background_subtracted = (self.data[idx] as f64 - local_background).max(0.0);
                
                if background_subtracted > 0.0 {
                    total_flux += background_subtracted;
                    weighted_x += x as f64 * background_subtracted;
                    weighted_y += y as f64 * background_subtracted;
                    weighted_distance_sum += distance * background_subtracted;
                }
            }
        }
        
        if total_flux <= 0.0 {
            return None; // Not enough signal
        }
        
        let centroid_x = weighted_x / total_flux;
        let centroid_y = weighted_y / total_flux;
        
        // Calculate N.I.N.A.-style HFR: Σ(Vi × di) / Σ(Vi)
        // N.I.N.A.'s exact HFR formula: sumDist / sum
        let hfr = weighted_distance_sum / total_flux;
        
        // Convert HFR to FWHM using standard approximation
        let fwhm = hfr * 2.0 * 1.177;
        
        // Apply brightness threshold to filter out faint detections
        let brightness_threshold = local_background + 10.0 * self.estimate_noise();
        if total_flux < brightness_threshold {
            return None;
        }
        
        Some(StarDetection {
            x: centroid_x,
            y: centroid_y,
            brightness: total_flux,
            hfr,
            fwhm,
        })
    }
    
    /// Estimate local background using annulus method (heap version)
    fn estimate_local_background(&self, center_x: usize, center_y: usize, inner_radius: usize) -> f64 {
        let arena = Bump::new();
        self.estimate_local_background_arena(&arena, center_x, center_y, inner_radius)
    }
    
    /// Estimate local background using annulus method with arena allocation
    fn estimate_local_background_arena(&self, arena: &Bump, center_x: usize, center_y: usize, inner_radius: usize) -> f64 {
        let outer_radius = inner_radius + 5; // Smaller annulus width for performance  
        let inner_radius_sq = (inner_radius as f64 + 1.0).powi(2); // Smaller gap from star
        let outer_radius_sq = (outer_radius as f64).powi(2);
        
        let mut background_pixels = bumpalo::vec![in arena];
        
        // Sample every 2nd pixel in the annulus for performance
        for dy in (-(outer_radius as i32)..=(outer_radius as i32)).step_by(2) {
            for dx in (-(outer_radius as i32)..=(outer_radius as i32)).step_by(2) {
                let x = center_x as i32 + dx;
                let y = center_y as i32 + dy;
                
                if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
                    continue;
                }
                
                let distance_sq = (dx * dx + dy * dy) as f64;
                
                // Only include pixels in the annulus (between inner and outer radius)
                if distance_sq > inner_radius_sq && distance_sq <= outer_radius_sq {
                    let idx = (y as usize) * self.width + (x as usize);
                    background_pixels.push(self.data[idx]);
                    
                    // Limit sample size for performance
                    if background_pixels.len() >= 50 {
                        break;
                    }
                }
            }
            if background_pixels.len() >= 50 {
                break;
            }
        }
        
        if background_pixels.is_empty() {
            return self.estimate_background(); // Fallback to global background
        }
        
        // Use median for robust background estimation
        background_pixels.sort();
        
        if background_pixels.len() % 2 == 0 {
            let mid = background_pixels.len() / 2;
            (background_pixels[mid - 1] as f64 + background_pixels[mid] as f64) / 2.0
        } else {
            background_pixels[background_pixels.len() / 2] as f64
        }
    }

    /// Measure properties of a star at given position (original method)
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
                let flux = (self.data[idx] as f64 - background).max(0.0);

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

        let v00 = self.data[y0 * self.width + x0] as f64;
        let v10 = self.data[y0 * self.width + x1] as f64;
        let v01 = self.data[y1 * self.width + x0] as f64;
        let v11 = self.data[y1 * self.width + x1] as f64;

        let interpolated = v00 * (1.0 - dx) * (1.0 - dy)
            + v10 * dx * (1.0 - dy)
            + v01 * (1.0 - dx) * dy
            + v11 * dx * dy;

        Some(interpolated)
    }
}






