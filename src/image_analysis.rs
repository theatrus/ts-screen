use anyhow::{Context, Result};
use bumpalo::Bump;
use fitrs::{Fits, FitsData, FitsDataArray};
use std::path::Path;
use std::collections::VecDeque;

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
    pub mad: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StarDetection {
    pub x: f64,
    pub y: f64,
    pub brightness: f64,
    pub hfr: f64,
    pub fwhm: f64,
}

#[derive(Debug, Clone)]
struct Blob {
    pub bounds: BoundingBox,
    pub pixels: Vec<(usize, usize)>,
}

#[derive(Debug, Clone)]
struct BoundingBox {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
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
        // Note: We read these values but don't apply them to match NINA's behavior
        let _bzero = match hdu.value("BZERO") {
            Some(val) => match val {
                fitrs::HeaderValue::IntegerNumber(n) => *n as f64,
                fitrs::HeaderValue::RealFloatingNumber(f) => *f,
                _ => 0.0,
            },
            None => 0.0,
        };
        
        let _bscale = match hdu.value("BSCALE") {
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
        
        // Convert to u16 based on data type
        // IMPORTANT: N.I.N.A. uses raw values, NOT scaled values
        // So we ignore BZERO and BSCALE for star detection compatibility
        let data: Vec<u16> = match fits_data {
            FitsData::Characters(_) => {
                return Err(anyhow::anyhow!("FITS file contains character data, not image data"));
            },
            FitsData::IntegersI32(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    if let Some(raw_val) = x {
                        // For signed 16-bit data stored as i32, convert to unsigned
                        // This handles FITS files with BITPIX=16 and BZERO=32768
                        // The raw values range from -32768 to 32767
                        // We add 32768 to get unsigned 0 to 65535
                        ((raw_val + 32768).max(0).min(65535)) as u16
                    } else {
                        0u16
                    }
                }).collect()
            },
            FitsData::IntegersU32(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    if let Some(raw_val) = x {
                        // Use raw value directly, not scaled
                        // N.I.N.A. compatibility: ignore BZERO/BSCALE
                        raw_val.min(65535) as u16
                    } else {
                        0u16
                    }
                }).collect()
            },
            FitsData::FloatingPoint32(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    // For float data, still need to convert but don't apply BZERO/BSCALE
                    // to match N.I.N.A. behavior
                    x.max(0.0).min(65535.0) as u16
                }).collect()
            },
            FitsData::FloatingPoint64(FitsDataArray { data, .. }) => {
                data.into_iter().map(|x| {
                    // For float data, still need to convert but don't apply BZERO/BSCALE
                    // to match N.I.N.A. behavior
                    x.max(0.0).min(65535.0) as u16
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

    /// Calculate basic statistics without star detection  
    pub fn calculate_basic_statistics(&self) -> ImageStatistics {
        self.calculate_statistics_with_mad()
    }
    
    /// Calculate statistics including MAD
    pub fn calculate_statistics_with_mad(&self) -> ImageStatistics {
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
        
        // Calculate MAD (Median Absolute Deviation) using N.I.N.A.'s histogram approach
        let mad = self.calculate_mad_from_histogram(&sorted_data, median);
        
        eprintln!("Debug: Median={:.2}, Calculated MAD={:.2}, Approximation={:.2}", 
                  median, mad, std_dev * 0.6745);

        ImageStatistics {
            width: self.width,
            height: self.height,
            mean,
            median,
            std_dev,
            min,
            max,
            star_count: None,
            hfr: None,
            fwhm: None,
            mad: Some(mad),
        }
    }
    
    /// Calculate basic image statistics
    pub fn calculate_statistics(&self) -> ImageStatistics {
        let stats = self.calculate_basic_statistics();
        
        // Apply MTF stretch before star detection (N.I.N.A. behavior)
        let stretched_data = self.apply_nina_stretch(&stats);
        
        // Detect stars on stretched data
        let stars = self.detect_stars_on_data(&stretched_data);
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
            mean: stats.mean,
            median: stats.median,
            std_dev: stats.std_dev,
            min: stats.min,
            max: stats.max,
            star_count,
            hfr,
            fwhm,
            mad: stats.mad,
        }
    }
    
    /// Apply N.I.N.A.'s MTF stretch to image data
    fn apply_nina_stretch(&self, statistics: &ImageStatistics) -> Vec<u16> {
        use crate::mtf_stretch::{stretch_image, StretchParameters};
        
        // Use N.I.N.A. default stretch parameters
        let params = StretchParameters::default();
        stretch_image(&self.data, statistics, params.factor, params.black_clipping)
    }

    /// Convert 16-bit image to 8-bit for processing
    fn convert_to_8bit(&self) -> Vec<u8> {
        // N.I.N.A. likely uses a simpler bit shift conversion
        // For 16-bit to 8-bit: divide by 256 (shift right by 8)
        self.data.iter()
            .map(|&val| (val >> 8) as u8)
            .collect()
    }
    
    /// Resize image for detection (nearest neighbor interpolation)
    fn resize_image(data: &[u8], width: usize, height: usize, new_width: usize, new_height: usize) -> Vec<u8> {
        let mut resized = vec![0u8; new_width * new_height];
        let x_ratio = width as f64 / new_width as f64;
        let y_ratio = height as f64 / new_height as f64;
        
        for new_y in 0..new_height {
            for new_x in 0..new_width {
                let src_x = (new_x as f64 * x_ratio) as usize;
                let src_y = (new_y as f64 * y_ratio) as usize;
                resized[new_y * new_width + new_x] = data[src_y * width + src_x];
            }
        }
        
        resized
    }
    
    /// Apply Gaussian blur for noise reduction
    fn gaussian_blur_3x3(data: &[u8], width: usize, height: usize) -> Vec<u8> {
        let mut blurred = vec![0u8; data.len()];
        let kernel = [
            [1, 2, 1],
            [2, 4, 2],
            [1, 2, 1],
        ];
        let kernel_sum = 16;
        
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let mut sum = 0u32;
                for ky in 0..3 {
                    for kx in 0..3 {
                        let py = y + ky - 1;
                        let px = x + kx - 1;
                        sum += data[py * width + px] as u32 * kernel[ky][kx];
                    }
                }
                blurred[y * width + x] = (sum / kernel_sum) as u8;
            }
        }
        
        // Copy edges
        for x in 0..width {
            blurred[x] = data[x];
            blurred[(height - 1) * width + x] = data[(height - 1) * width + x];
        }
        for y in 0..height {
            blurred[y * width] = data[y * width];
            blurred[y * width + width - 1] = data[y * width + width - 1];
        }
        
        blurred
    }
    
    /// Apply Canny edge detection (simplified version)
    fn canny_edge_detection(data: &[u8], width: usize, height: usize, low_threshold: u8, high_threshold: u8) -> Vec<u8> {
        // Apply Gaussian blur first
        let blurred = Self::gaussian_blur_3x3(data, width, height);
        
        // Calculate gradients using Sobel operators
        let mut gradients = vec![0u16; data.len()];
        let mut edges = vec![0u8; data.len()];
        
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let idx = y * width + x;
                
                // Sobel X
                let gx = (blurred[(y - 1) * width + x + 1] as i16 + 2 * blurred[y * width + x + 1] as i16 + blurred[(y + 1) * width + x + 1] as i16)
                       - (blurred[(y - 1) * width + x - 1] as i16 + 2 * blurred[y * width + x - 1] as i16 + blurred[(y + 1) * width + x - 1] as i16);
                
                // Sobel Y
                let gy = (blurred[(y + 1) * width + x - 1] as i16 + 2 * blurred[(y + 1) * width + x] as i16 + blurred[(y + 1) * width + x + 1] as i16)
                       - (blurred[(y - 1) * width + x - 1] as i16 + 2 * blurred[(y - 1) * width + x] as i16 + blurred[(y - 1) * width + x + 1] as i16);
                
                gradients[idx] = ((gx * gx + gy * gy) as f64).sqrt() as u16;
            }
        }
        
        // Apply thresholding
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let idx = y * width + x;
                if gradients[idx] > high_threshold as u16 {
                    edges[idx] = 255;
                } else if gradients[idx] > low_threshold as u16 {
                    // Check if connected to strong edge
                    let mut has_strong_neighbor = false;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 { continue; }
                            let ny = (y as i32 + dy) as usize;
                            let nx = (x as i32 + dx) as usize;
                            if gradients[ny * width + nx] > high_threshold as u16 {
                                has_strong_neighbor = true;
                                break;
                            }
                        }
                        if has_strong_neighbor { break; }
                    }
                    if has_strong_neighbor {
                        edges[idx] = 255;
                    }
                }
            }
        }
        
        edges
    }
    
    /// Apply binary dilation to connect nearby edges
    fn binary_dilation_3x3(data: &[u8], width: usize, height: usize) -> Vec<u8> {
        let mut dilated = vec![0u8; data.len()];
        
        for y in 1..(height - 1) {
            for x in 1..(width - 1) {
                let mut has_edge = false;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let ny = (y as i32 + dy) as usize;
                        let nx = (x as i32 + dx) as usize;
                        if data[ny * width + nx] > 0 {
                            has_edge = true;
                            break;
                        }
                    }
                    if has_edge { break; }
                }
                if has_edge {
                    dilated[y * width + x] = 255;
                }
            }
        }
        
        dilated
    }
    
    /// Find connected components (blobs) using flood fill
    fn find_blobs(binary_image: &[u8], width: usize, height: usize) -> Vec<Blob> {
        let mut visited = vec![false; binary_image.len()];
        let mut blobs = Vec::new();
        
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if binary_image[idx] > 0 && !visited[idx] {
                    // Start flood fill
                    let mut blob_pixels = Vec::new();
                    let mut queue = VecDeque::new();
                    queue.push_back((x, y));
                    visited[idx] = true;
                    
                    let mut min_x = x;
                    let mut max_x = x;
                    let mut min_y = y;
                    let mut max_y = y;
                    
                    while let Some((cx, cy)) = queue.pop_front() {
                        blob_pixels.push((cx, cy));
                        min_x = min_x.min(cx);
                        max_x = max_x.max(cx);
                        min_y = min_y.min(cy);
                        max_y = max_y.max(cy);
                        
                        // Check 8-connected neighbors
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                if dx == 0 && dy == 0 { continue; }
                                let nx = (cx as i32 + dx) as usize;
                                let ny = (cy as i32 + dy) as usize;
                                
                                if nx < width && ny < height {
                                    let nidx = ny * width + nx;
                                    if binary_image[nidx] > 0 && !visited[nidx] {
                                        visited[nidx] = true;
                                        queue.push_back((nx, ny));
                                    }
                                }
                            }
                        }
                    }
                    
                    blobs.push(Blob {
                        bounds: BoundingBox {
                            x: min_x,
                            y: min_y,
                            // Match NINA's behavior: width/height don't include the last pixel
                            // This is technically incorrect but matches their implementation
                            width: if max_x > min_x { max_x - min_x } else { 0 },
                            height: if max_y > min_y { max_y - min_y } else { 0 },
                        },
                        pixels: blob_pixels,
                    });
                }
            }
        }
        
        blobs
    }
    
    /// Detect stars using N.I.N.A.'s exact algorithm  
    pub fn detect_stars(&self) -> Vec<StarDetection> {
        // First calculate statistics and apply stretch
        let stats = self.calculate_basic_statistics();
        let stretched_data = self.apply_nina_stretch(&stats);
        self.detect_stars_on_data(&stretched_data)
    }
    
    /// Calculate MAD using N.I.N.A.'s histogram approach
    fn calculate_mad_from_histogram(&self, _sorted_data: &[u16], median: f64) -> f64 {
        // Build histogram of pixel values
        let mut pixel_counts = vec![0u32; 65536];
        for &val in self.data.iter() {
            pixel_counts[val as usize] += 1;
        }
        
        // Find median values (handling even vs odd length arrays)
        let median1 = median.floor() as i32;
        let median2 = median.ceil() as i32;
        
        // Calculate MAD using N.I.N.A.'s algorithm
        // MAD = median(|x_i - median|)
        // Since we're looking for the median of absolute deviations,
        // we start from the median and step outward symmetrically
        let mut occurrences = 0u32;
        let medianlength = self.data.len() as f64 / 2.0;
        let mut idx_down = median1;
        let mut idx_up = median2;
        
        loop {
            // Count pixels at current deviation distance
            if idx_down >= 0 && idx_down != idx_up {
                occurrences += pixel_counts[idx_down as usize] + pixel_counts[idx_up as usize];
            } else if idx_up >= 0 && idx_up <= 65535 {
                occurrences += pixel_counts[idx_up as usize];
            }
            
            // Check if we've accumulated more than half the pixels
            if occurrences as f64 > medianlength {
                // The MAD is the distance from median to current index
                return (idx_up as f64 - median).abs();
            }
            
            // Step outward from median
            idx_up += 1;
            idx_down -= 1;
            
            // Bounds check
            if idx_up > 65535 {
                break;
            }
        }
        
        // This should rarely happen, but provide a fallback
        // Calculate MAD directly from the data
        let mut deviations: Vec<f64> = self.data.iter()
            .map(|&x| (x as f64 - median).abs())
            .collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        deviations[deviations.len() / 2]
    }
    
    /// Detect stars on provided data (can be stretched or raw)
    fn detect_stars_on_data(&self, data: &[u16]) -> Vec<StarDetection> {
        use crate::nina_star_detection::{detect_stars_with_original, StarDetectionParams, StarSensitivity};
        
        // Use N.I.N.A. star detection with default parameters
        let mut params = StarDetectionParams::default();
        params.sensitivity = StarSensitivity::High; // Try High sensitivity
        
        // Use stretched data for detection but original raw data for HFR measurement
        // This may be necessary since NINA's right-shift alone gives too few stars
        let result = detect_stars_with_original(
            data,           // MTF stretched data for detection 
            &self.data,     // Original raw data for HFR calculation
            self.width, 
            self.height, 
            &params
        );
        
        // Convert N.I.N.A. results to our StarDetection format
        result.star_list.into_iter().map(|star| {
            StarDetection {
                x: star.position.0,
                y: star.position.1,
                brightness: star.average_brightness,
                hfr: star.hfr,
                fwhm: star.hfr * 2.0 * 1.177, // Standard conversion
            }
        }).collect()
    }
    
    /// Analyze a star blob using N.I.N.A.'s exact algorithm on original 16-bit data
    fn analyze_star_nina_blob(
        &self,
        center_x: usize,
        center_y: usize,
        radius: usize,
        _blob: &Blob,
        inverse_resize_factor: f64,
    ) -> Option<StarDetection> {
        // The radius is already in original coordinates, so use it directly
        let rect_width = radius * 2;
        let rect_height = rect_width;
        
        // Build rectangles for analysis
        let rect_left = center_x.saturating_sub(rect_width / 2);
        let rect_top = center_y.saturating_sub(rect_height / 2);
        let rect_right = (rect_left + rect_width).min(self.width - 1);
        let rect_bottom = (rect_top + rect_height).min(self.height - 1);
        
        // Large rectangle for background estimation (3x the star size)
        let large_rect_left = rect_left.saturating_sub(rect_width);
        let large_rect_top = rect_top.saturating_sub(rect_height);
        let large_rect_right = (rect_right + rect_width).min(self.width - 1);
        let large_rect_bottom = (rect_bottom + rect_height).min(self.height - 1);
        
        // Calculate background statistics
        let mut large_rect_sum = 0.0;
        let mut large_rect_sum_squares = 0.0;
        let mut large_rect_pixel_count = 0;
        
        for y in large_rect_top..=large_rect_bottom {
            for x in large_rect_left..=large_rect_right {
                // Skip pixels inside the star rectangle
                if x >= rect_left && x <= rect_right && y >= rect_top && y <= rect_bottom {
                    continue;
                }
                let pixel_val = self.data[y * self.width + x] as f64;
                large_rect_sum += pixel_val;
                large_rect_sum_squares += pixel_val * pixel_val;
                large_rect_pixel_count += 1;
            }
        }
        
        if large_rect_pixel_count == 0 {
            return None;
        }
        
        let large_rect_mean = large_rect_sum / large_rect_pixel_count as f64;
        let large_rect_stdev = ((large_rect_sum_squares - large_rect_pixel_count as f64 * large_rect_mean * large_rect_mean) / large_rect_pixel_count as f64).sqrt();
        
        // Calculate star statistics
        let mut star_pixel_sum = 0.0;
        let mut star_pixel_count = 0;
        let mut inner_star_bright_pixels = 0;
        
        // Minimum bright pixels based on resized dimensions
        let resized_width = (self.width as f64 * (1.0 / inverse_resize_factor)) as usize;
        let resized_height = (self.height as f64 * (1.0 / inverse_resize_factor)) as usize;
        let minimum_bright_pixels = (resized_width.max(resized_height) as f64 / 1000.0).ceil() as usize;
        let bright_pixel_threshold = large_rect_mean + 1.5 * large_rect_stdev;
        
        // Check pixels within the star's circular region
        for y in rect_top..=rect_bottom {
            for x in rect_left..=rect_right {
                let dx = x as i32 - center_x as i32;
                let dy = y as i32 - center_y as i32;
                let dist_sq = (dx * dx + dy * dy) as f64;
                
                if dist_sq <= (radius * radius) as f64 {
                    let pixel_val = self.data[y * self.width + x] as f64;
                    star_pixel_sum += pixel_val;
                    star_pixel_count += 1;
                    
                    if pixel_val > bright_pixel_threshold {
                        inner_star_bright_pixels += 1;
                    }
                }
            }
        }
        
        if star_pixel_count == 0 {
            return None;
        }
        
        let star_mean_brightness = star_pixel_sum / star_pixel_count as f64;
        
        // N.I.N.A.'s exact detection criteria
        let brightness_threshold = large_rect_mean + (0.1 * large_rect_mean).min(large_rect_stdev);
        
        if star_mean_brightness < brightness_threshold {
            return None;
        }
        
        if inner_star_bright_pixels < minimum_bright_pixels {
            return None;
        }
        
        // Calculate HFR using N.I.N.A.'s exact method
        self.calculate_nina_hfr_exact(center_x, center_y, rect_left, rect_top, rect_right, rect_bottom, large_rect_mean, radius)
    }
    
    /// Analyze a potential star location using N.I.N.A.'s exact criteria
    fn analyze_potential_star_nina(
        &self,
        _arena: &Bump,
        center_x: usize,
        center_y: usize,
        min_size: usize,
        _max_size: usize,
        resize_factor: f64,
    ) -> Option<StarDetection> {
        // First check if we can fit a proper analysis rectangle
        let rect_radius = min_size.max(5); // At least 5 pixels for meaningful analysis
        
        // Star bounding box
        let star_left = center_x.saturating_sub(rect_radius);
        let star_top = center_y.saturating_sub(rect_radius);
        let star_right = (center_x + rect_radius).min(self.width - 1);
        let star_bottom = (center_y + rect_radius).min(self.height - 1);
        let star_width = star_right - star_left + 1;
        let star_height = star_bottom - star_top + 1;
        
        // N.I.N.A. builds a large rectangle (3x the star size) for background estimation
        let large_rect_left = star_left.saturating_sub(star_width);
        let large_rect_top = star_top.saturating_sub(star_height);
        let large_rect_right = (star_right + star_width).min(self.width - 1);
        let large_rect_bottom = (star_bottom + star_height).min(self.height - 1);
        
        // Calculate statistics for the large rect (excluding the star rect) - this is the background
        let mut large_rect_sum = 0.0;
        let mut large_rect_sum_squares = 0.0;
        let mut large_rect_pixel_count = 0;
        
        for y in large_rect_top..=large_rect_bottom {
            for x in large_rect_left..=large_rect_right {
                // Skip pixels inside the star rectangle
                if x >= star_left && x <= star_right && y >= star_top && y <= star_bottom {
                    continue;
                }
                let pixel_val = self.data[y * self.width + x] as f64;
                large_rect_sum += pixel_val;
                large_rect_sum_squares += pixel_val * pixel_val;
                large_rect_pixel_count += 1;
            }
        }
        
        if large_rect_pixel_count == 0 {
            return None;
        }
        
        let large_rect_mean = large_rect_sum / large_rect_pixel_count as f64;
        let large_rect_stdev = ((large_rect_sum_squares - large_rect_pixel_count as f64 * large_rect_mean * large_rect_mean) / large_rect_pixel_count as f64).sqrt();
        
        // Calculate star statistics
        let mut star_pixel_sum = 0.0;
        let mut star_pixel_count = 0;
        let mut max_pixel_value = 0.0_f64;
        let mut inner_star_bright_pixels = 0;
        
        // N.I.N.A.'s exact criteria - uses the conceptually resized dimensions
        let resized_width = (self.width as f64 * resize_factor) as usize;
        let resized_height = (self.height as f64 * resize_factor) as usize; 
        let minimum_bright_pixels = (resized_width.max(resized_height) as f64 / 1000.0).ceil() as usize;
        let bright_pixel_threshold = large_rect_mean + 1.5 * large_rect_stdev;
        
        for y in star_top..=star_bottom {
            for x in star_left..=star_right {
                // Check if inside circular star region  
                let dx = x as i32 - center_x as i32;
                let dy = y as i32 - center_y as i32;
                let dist_sq = (dx * dx + dy * dy) as f64;
                
                if dist_sq <= (rect_radius * rect_radius) as f64 {
                    let pixel_val = self.data[y * self.width + x] as f64;
                    star_pixel_sum += pixel_val;
                    star_pixel_count += 1;
                    max_pixel_value = max_pixel_value.max(pixel_val);
                    
                    if pixel_val > bright_pixel_threshold {
                        inner_star_bright_pixels += 1;
                    }
                }
            }
        }
        
        if star_pixel_count == 0 {
            return None;
        }
        
        let star_mean_brightness = star_pixel_sum / star_pixel_count as f64;
        
        // N.I.N.A.'s exact detection criteria
        let brightness_threshold = large_rect_mean + (0.1 * large_rect_mean).min(large_rect_stdev);
        
        if star_mean_brightness < brightness_threshold {
            return None;
        }
        
        if inner_star_bright_pixels < minimum_bright_pixels {
            return None;
        }
        
        // Star passed all tests, calculate HFR using N.I.N.A.'s exact method
        self.calculate_nina_hfr_exact(center_x, center_y, star_left, star_top, star_right, star_bottom, large_rect_mean, rect_radius)
    }
    
    /// Calculate HFR using N.I.N.A.'s exact algorithm
    fn calculate_nina_hfr_exact(
        &self,
        center_x: usize,
        center_y: usize,
        rect_left: usize,
        rect_top: usize, 
        rect_right: usize,
        rect_bottom: usize,
        surrounding_mean: f64,
        radius: usize,
    ) -> Option<StarDetection> {
        // N.I.N.A. uses outerRadius = radius * 1.2
        let outer_radius = radius as f64 * 1.2;
        let mut sum = 0.0;
        let mut sum_dist = 0.0;
        let mut sum_val_x = 0.0;
        let mut sum_val_y = 0.0;
        let mut all_sum = 0.0;
        let mut pixel_count = 0;
        
        // Process all pixels in the star rectangle
        for y in rect_top..=rect_bottom {
            for x in rect_left..=rect_right {
                let dx = x as f64 - center_x as f64;
                let dy = y as f64 - center_y as f64;
                let distance = (dx * dx + dy * dy).sqrt();
                
                let pixel_val = self.data[y * self.width + x] as f64;
                
                // N.I.N.A.'s exact background subtraction: Math.Round(value - SurroundingMean)
                let mut value = (pixel_val - surrounding_mean).round();
                if value < 0.0 {
                    value = 0.0;
                }
                
                all_sum += value;
                pixel_count += 1;
                
                // Only include pixels within outerRadius in HFR calculation
                if distance <= outer_radius {
                    sum += value;
                    sum_dist += value * distance;
                    sum_val_x += (x - rect_left) as f64 * value;
                    sum_val_y += (y - rect_top) as f64 * value;
                }
            }
        }
        
        // N.I.N.A.'s exact HFR calculation
        let hfr = if sum > 0.0 {
            sum_dist / sum
        } else {
            (2.0_f64).sqrt() * outer_radius
        };
        
        // Calculate average brightness
        let average = if pixel_count > 0 {
            all_sum / pixel_count as f64
        } else {
            0.0
        };
        
        // Update centroid if we have signal
        let (final_x, final_y) = if sum > 0.0 {
            let centroid_x = sum_val_x / sum + rect_left as f64;
            let centroid_y = sum_val_y / sum + rect_top as f64;
            (centroid_x, centroid_y)
        } else {
            (center_x as f64, center_y as f64)
        };
        
        let fwhm = hfr * 2.0 * 1.177; // Standard conversion
        
        // Check if centroid is within bounds (not touching edges)
        if final_x > (rect_left + 1) as f64 && final_y > (rect_top + 1) as f64 
            && final_x < (rect_right - 1) as f64 && final_y < (rect_bottom - 1) as f64 {
            Some(StarDetection {
                x: final_x,
                y: final_y,
                brightness: average, // N.I.N.A. uses average, not total flux
                hfr,
                fwhm,
            })
        } else {
            None // Reject stars whose centroids touch the edges
        }
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






