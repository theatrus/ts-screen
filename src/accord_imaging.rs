/// Rust implementation of Accord.NET imaging functions used by N.I.N.A.
/// Based on the exact algorithms from the Accord.NET framework

// Remove unused imports

/// Utility functions matching N.I.N.A.'s ImageUtility class
pub struct ImageUtility;

impl ImageUtility {
    /// Convert 16-bit grayscale to 8-bit grayscale
    /// Matches Accord.Imaging.Image.Convert16bppTo8bpp: *d = (byte)(*s >> 8)
    pub fn convert_16bpp_to_8bpp(data_16bit: &[u16]) -> Vec<u8> {
        data_16bit.iter().map(|&val| (val >> 8) as u8).collect()
    }
}

/// Detection utility functions
pub struct DetectionUtility;

impl DetectionUtility {
    /// Resize image for detection using bicubic interpolation
    pub fn resize_for_detection(image: &[u8], width: usize, height: usize, max_width: usize, resize_factor: f64) -> (Vec<u8>, usize, usize) {
        if width <= max_width {
            // No resizing needed
            return (image.to_vec(), width, height);
        }
        
        let new_width = (width as f64 * resize_factor).floor() as usize;
        let new_height = (height as f64 * resize_factor).floor() as usize;
        
        // Use ResizeBicubic algorithm
        let resizer = ResizeBicubic::new(new_width, new_height);
        let resized = resizer.apply(image, width, height);
        
        (resized, new_width, new_height)
    }
}

/// Resize using bicubic interpolation
pub struct ResizeBicubic {
    new_width: usize,
    new_height: usize,
}

impl ResizeBicubic {
    pub fn new(new_width: usize, new_height: usize) -> Self {
        Self { new_width, new_height }
    }
    
    pub fn apply(&self, image: &[u8], width: usize, height: usize) -> Vec<u8> {
        let mut result = vec![0u8; self.new_width * self.new_height];
        
        let x_ratio = width as f64 / self.new_width as f64;
        let y_ratio = height as f64 / self.new_height as f64;
        
        for new_y in 0..self.new_height {
            for new_x in 0..self.new_width {
                let src_x = new_x as f64 * x_ratio;
                let src_y = new_y as f64 * y_ratio;
                
                // Bicubic interpolation
                let pixel_value = self.bicubic_interpolate(image, width, height, src_x, src_y);
                result[new_y * self.new_width + new_x] = pixel_value;
            }
        }
        
        result
    }
    
    fn bicubic_interpolate(&self, image: &[u8], width: usize, height: usize, x: f64, y: f64) -> u8 {
        // Mitchell-Netravali cubic filter with a = -0.5 (as per Wikipedia bicubic)
        let x0 = x.floor() as i32;
        let y0 = y.floor() as i32;
        let fx = x - x0 as f64;
        let fy = y - y0 as f64;
        
        let mut sum = 0.0;
        
        for j in -1..=2 {
            for i in -1..=2 {
                let px = (x0 + i).clamp(0, width as i32 - 1) as usize;
                let py = (y0 + j).clamp(0, height as i32 - 1) as usize;
                
                let wx = self.cubic_weight((i as f64 - fx).abs());
                let wy = self.cubic_weight((j as f64 - fy).abs());
                
                sum += image[py * width + px] as f64 * wx * wy;
            }
        }
        
        sum.round().clamp(0.0, 255.0) as u8
    }
    
    fn cubic_weight(&self, x: f64) -> f64 {
        // Mitchell-Netravali cubic filter with a = -0.5
        let a = -0.5;
        let x_abs = x.abs();
        
        if x_abs <= 1.0 {
            (a + 2.0) * x_abs.powi(3) - (a + 3.0) * x_abs.powi(2) + 1.0
        } else if x_abs < 2.0 {
            a * x_abs.powi(3) - 5.0 * a * x_abs.powi(2) + 8.0 * a * x_abs - 4.0 * a
        } else {
            0.0
        }
    }
}

/// Canny edge detector implementation
pub struct CannyEdgeDetector {
    low_threshold: u8,
    high_threshold: u8,
    gaussian_size: usize,
    gaussian_sigma: f64,
    apply_blur: bool,
}

impl CannyEdgeDetector {
    pub fn new(low_threshold: u8, high_threshold: u8) -> Self {
        Self {
            low_threshold,
            high_threshold,
            gaussian_size: 5,
            gaussian_sigma: 1.4,
            apply_blur: true,
        }
    }
    
    pub fn new_no_blur(low_threshold: u8, high_threshold: u8) -> Self {
        Self {
            low_threshold,
            high_threshold,
            gaussian_size: 5,
            gaussian_sigma: 1.4,
            apply_blur: false,
        }
    }

    pub fn apply_in_place(&self, image: &mut [u8], width: usize, height: usize) {
        // Apply Gaussian blur if enabled
        let blurred = if self.apply_blur {
            gaussian_blur(image, width, height, self.gaussian_size, self.gaussian_sigma)
        } else {
            image.to_vec()
        };
        
        // Calculate gradients
        let (gradients, orientations) = calculate_gradients(&blurred, width, height);
        
        // Non-maximum suppression
        let suppressed = non_maximum_suppression(&gradients, &orientations, width, height);
        
        // Double thresholding and edge tracking
        let edges = hysteresis_thresholding(&suppressed, width, height, self.low_threshold, self.high_threshold);
        
        // Copy result back to original image
        image.copy_from_slice(&edges);
    }
}

/// Canny edge detector without Gaussian blur
pub struct NoBlurCannyEdgeDetector {
    low_threshold: u8,
    high_threshold: u8,
}

impl NoBlurCannyEdgeDetector {
    pub fn new(low_threshold: u8, high_threshold: u8) -> Self {
        Self {
            low_threshold,
            high_threshold,
        }
    }

    pub fn apply_in_place(&self, image: &mut [u8], width: usize, height: usize) {
        // Calculate gradients without blur
        let (gradients, orientations) = calculate_gradients(image, width, height);
        
        // Non-maximum suppression
        let suppressed = non_maximum_suppression(&gradients, &orientations, width, height);
        
        // Double thresholding and edge tracking
        let edges = hysteresis_thresholding(&suppressed, width, height, self.low_threshold, self.high_threshold);
        
        // Copy result back to original image
        image.copy_from_slice(&edges);
    }
}

/// SIS (Simple Image Statistics) threshold
pub struct SISThreshold;

impl SISThreshold {
    pub fn apply_in_place(&self, image: &mut [u8], width: usize, height: usize) {
        // Calculate threshold using SIS algorithm
        let threshold = calculate_sis_threshold(image, width, height);
        
        // Apply threshold
        for pixel in image.iter_mut() {
            *pixel = if *pixel > threshold { 255 } else { 0 };
        }
    }
}

/// Binary dilation with 3x3 structuring element
pub struct BinaryDilation3x3;

impl BinaryDilation3x3 {
    pub fn apply_in_place(&self, image: &mut [u8], width: usize, height: usize) {
        let original = image.to_vec();
        
        for y in 0..height {
            for x in 0..width {
                let mut has_neighbor = false;
                
                // Check 3x3 neighborhood
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ny = y as i32 + dy;
                        let nx = x as i32 + dx;
                        
                        if ny >= 0 && ny < height as i32 && nx >= 0 && nx < width as i32 {
                            let idx = (ny as usize) * width + (nx as usize);
                            if original[idx] > 0 {
                                has_neighbor = true;
                                break;
                            }
                        }
                    }
                    if has_neighbor {
                        break;
                    }
                }
                
                image[y * width + x] = if has_neighbor { 255 } else { 0 };
            }
        }
    }
}

/// Blob representation
#[derive(Debug, Clone)]
pub struct Blob {
    pub rectangle: Rectangle,
    pub id: u32,
    pub area: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Blob counter for connected component labeling
pub struct BlobCounter {
    blobs: Vec<Blob>,
}

impl BlobCounter {
    pub fn new() -> Self {
        Self { blobs: Vec::new() }
    }

    pub fn process_image(&mut self, image: &[u8], width: usize, height: usize) {
        self.blobs.clear();
        
        // Create label image
        let mut labels = vec![0u32; width * height];
        let mut next_label = 1u32;
        let mut equivalences = Vec::new();
        
        // First pass - assign temporary labels
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                
                if image[idx] > 0 {
                    let mut neighbors = Vec::new();
                    
                    // Check left and top neighbors
                    if x > 0 && labels[idx - 1] > 0 {
                        neighbors.push(labels[idx - 1]);
                    }
                    if y > 0 && labels[idx - width] > 0 {
                        neighbors.push(labels[idx - width]);
                    }
                    
                    if neighbors.is_empty() {
                        labels[idx] = next_label;
                        next_label += 1;
                    } else {
                        let min_label = *neighbors.iter().min().unwrap();
                        labels[idx] = min_label;
                        
                        // Record equivalences
                        for &neighbor in &neighbors {
                            if neighbor != min_label {
                                equivalences.push((min_label, neighbor));
                            }
                        }
                    }
                }
            }
        }
        
        // Resolve equivalences
        let mut label_map = vec![0u32; next_label as usize];
        for i in 0..next_label {
            label_map[i as usize] = i;
        }
        
        for &(label1, label2) in &equivalences {
            let root1 = find_root(&mut label_map, label1);
            let root2 = find_root(&mut label_map, label2);
            if root1 != root2 {
                label_map[root2 as usize] = root1;
            }
        }
        
        // Second pass - relabel and collect blob info
        let mut blob_info: std::collections::HashMap<u32, (i32, i32, i32, i32, usize)> = std::collections::HashMap::new();
        
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                if labels[idx] > 0 {
                    let final_label = find_root(&mut label_map, labels[idx]);
                    labels[idx] = final_label;
                    
                    let entry = blob_info.entry(final_label).or_insert((x as i32, y as i32, x as i32, y as i32, 0));
                    entry.0 = entry.0.min(x as i32); // min x
                    entry.1 = entry.1.min(y as i32); // min y
                    entry.2 = entry.2.max(x as i32); // max x
                    entry.3 = entry.3.max(y as i32); // max y
                    entry.4 += 1; // area
                }
            }
        }
        
        // Create blob objects
        for (id, (min_x, min_y, max_x, max_y, area)) in blob_info {
            self.blobs.push(Blob {
                rectangle: Rectangle {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x + 1,
                    height: max_y - min_y + 1,
                },
                id,
                area,
            });
        }
    }

    pub fn get_objects_information(&self) -> Vec<Blob> {
        self.blobs.clone()
    }

    pub fn get_blobs_edge_points(&self, _blob: &Blob) -> Vec<(i32, i32)> {
        // Simplified - return corners for now
        vec![]
    }
}

/// Simple shape checker for circle detection
pub struct SimpleShapeChecker;

impl SimpleShapeChecker {
    pub fn is_circle(&self, points: &[(i32, i32)], center_x: &mut f32, center_y: &mut f32, radius: &mut f32) -> bool {
        if points.len() < 3 {
            return false;
        }
        
        // Calculate center as mean of all points
        let sum_x: i32 = points.iter().map(|p| p.0).sum();
        let sum_y: i32 = points.iter().map(|p| p.1).sum();
        let cx = sum_x as f32 / points.len() as f32;
        let cy = sum_y as f32 / points.len() as f32;
        
        // Calculate mean radius
        let mut sum_radius = 0.0;
        for &(x, y) in points {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            sum_radius += (dx * dx + dy * dy).sqrt();
        }
        let mean_radius = sum_radius / points.len() as f32;
        
        // Check how well points fit the circle
        let mut max_deviation = 0.0f32;
        for &(x, y) in points {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = (dx * dx + dy * dy).sqrt();
            let deviation = (r - mean_radius).abs();
            max_deviation = max_deviation.max(deviation);
        }
        
        // Consider it a circle if max deviation is less than 20% of radius
        let is_circle = max_deviation < mean_radius * 0.2;
        
        if is_circle {
            *center_x = cx;
            *center_y = cy;
            *radius = mean_radius;
        }
        
        is_circle
    }
}

/// Fast Gaussian blur implementation
pub struct FastGaussianBlur {
    radius: i32,
}

impl FastGaussianBlur {
    pub fn new() -> Self {
        Self { radius: 1 }
    }

    pub fn process(&self, image: &[u8], width: usize, height: usize, radius: i32) -> Vec<u8> {
        let size = (radius * 2 + 1) as usize;
        let sigma = radius as f64 / 3.0;
        gaussian_blur(image, width, height, size, sigma)
    }
}

/// Median filter implementation
pub struct Median;

impl Median {
    pub fn apply(&self, image: &[u8], width: usize, height: usize) -> Vec<u8> {
        let mut result = vec![0u8; width * height];
        
        for y in 0..height {
            for x in 0..width {
                let mut values = Vec::new();
                
                // Collect 3x3 neighborhood
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let ny = y as i32 + dy;
                        let nx = x as i32 + dx;
                        
                        if ny >= 0 && ny < height as i32 && nx >= 0 && nx < width as i32 {
                            values.push(image[(ny as usize) * width + (nx as usize)]);
                        }
                    }
                }
                
                values.sort();
                result[y * width + x] = values[values.len() / 2];
            }
        }
        
        result
    }
}

// Helper functions

fn gaussian_blur(image: &[u8], width: usize, height: usize, kernel_size: usize, sigma: f64) -> Vec<u8> {
    // Create Gaussian kernel
    let kernel = create_gaussian_kernel(kernel_size, sigma);
    
    // Apply separable convolution (horizontal then vertical)
    let temp = convolve_horizontal(image, width, height, &kernel);
    convolve_vertical(&temp, width, height, &kernel)
}

fn create_gaussian_kernel(size: usize, sigma: f64) -> Vec<f64> {
    let mut kernel = vec![0.0; size];
    let center = size as f64 / 2.0 - 0.5;
    let mut sum = 0.0;
    
    for i in 0..size {
        let x = i as f64 - center;
        kernel[i] = (-x * x / (2.0 * sigma * sigma)).exp();
        sum += kernel[i];
    }
    
    // Normalize
    for val in &mut kernel {
        *val /= sum;
    }
    
    kernel
}

fn convolve_horizontal(image: &[u8], width: usize, height: usize, kernel: &[f64]) -> Vec<u8> {
    let mut result = vec![0u8; width * height];
    let half_size = kernel.len() / 2;
    
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            
            for (i, &k) in kernel.iter().enumerate() {
                let sx = x as i32 + i as i32 - half_size as i32;
                if sx >= 0 && sx < width as i32 {
                    sum += image[y * width + sx as usize] as f64 * k;
                }
            }
            
            result[y * width + x] = sum.round().clamp(0.0, 255.0) as u8;
        }
    }
    
    result
}

fn convolve_vertical(image: &[u8], width: usize, height: usize, kernel: &[f64]) -> Vec<u8> {
    let mut result = vec![0u8; width * height];
    let half_size = kernel.len() / 2;
    
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0.0;
            
            for (i, &k) in kernel.iter().enumerate() {
                let sy = y as i32 + i as i32 - half_size as i32;
                if sy >= 0 && sy < height as i32 {
                    sum += image[sy as usize * width + x] as f64 * k;
                }
            }
            
            result[y * width + x] = sum.round().clamp(0.0, 255.0) as u8;
        }
    }
    
    result
}

fn calculate_gradients(image: &[u8], width: usize, height: usize) -> (Vec<f64>, Vec<f64>) {
    let mut magnitudes = vec![0.0; width * height];
    let mut orientations = vec![0.0; width * height];
    
    // Sobel operators
    let sobel_x = [-1, 0, 1, -2, 0, 2, -1, 0, 1];
    let sobel_y = [-1, -2, -1, 0, 0, 0, 1, 2, 1];
    
    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let mut gx = 0.0;
            let mut gy = 0.0;
            
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let idx = ((dy + 1) * 3 + (dx + 1)) as usize;
                    let pixel_idx = ((y as i32 + dy) as usize) * width + (x as i32 + dx) as usize;
                    let pixel = image[pixel_idx] as f64;
                    
                    gx += pixel * sobel_x[idx] as f64;
                    gy += pixel * sobel_y[idx] as f64;
                }
            }
            
            let magnitude = (gx * gx + gy * gy).sqrt();
            let orientation = gy.atan2(gx);
            
            magnitudes[y * width + x] = magnitude;
            orientations[y * width + x] = orientation;
        }
    }
    
    (magnitudes, orientations)
}

fn non_maximum_suppression(magnitudes: &[f64], orientations: &[f64], width: usize, height: usize) -> Vec<u8> {
    let mut result = vec![0u8; width * height];
    
    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let idx = y * width + x;
            let mag = magnitudes[idx];
            let angle = orientations[idx];
            
            // Convert angle to 0-180 range
            let angle_deg = angle.to_degrees().abs();
            
            // Determine direction
            let (dx1, dy1, dx2, dy2) = if angle_deg < 22.5 || angle_deg >= 157.5 {
                // Horizontal edge
                (-1, 0, 1, 0)
            } else if angle_deg < 67.5 {
                // Diagonal /
                (-1, -1, 1, 1)
            } else if angle_deg < 112.5 {
                // Vertical edge
                (0, -1, 0, 1)
            } else {
                // Diagonal \
                (-1, 1, 1, -1)
            };
            
            let mag1 = magnitudes[((y as i32 + dy1) as usize) * width + (x as i32 + dx1) as usize];
            let mag2 = magnitudes[((y as i32 + dy2) as usize) * width + (x as i32 + dx2) as usize];
            
            if mag >= mag1 && mag >= mag2 {
                result[idx] = (mag.clamp(0.0, 255.0)) as u8;
            }
        }
    }
    
    result
}

fn hysteresis_thresholding(suppressed: &[u8], width: usize, height: usize, low: u8, high: u8) -> Vec<u8> {
    let mut result = vec![0u8; width * height];
    let mut stack = Vec::new();
    
    // Mark strong edges
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if suppressed[idx] >= high {
                result[idx] = 255;
                stack.push((x, y));
            }
        }
    }
    
    // Trace connected weak edges
    while let Some((x, y)) = stack.pop() {
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                
                if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                    let nidx = (ny as usize) * width + (nx as usize);
                    if suppressed[nidx] >= low && result[nidx] == 0 {
                        result[nidx] = 255;
                        stack.push((nx as usize, ny as usize));
                    }
                }
            }
        }
    }
    
    result
}

fn calculate_sis_threshold(image: &[u8], width: usize, height: usize) -> u8 {
    // SIS (Simple Image Statistics) threshold calculation
    let mut weight_total = 0.0;
    let mut total = 0.0;
    
    // Process inner pixels (skip border)
    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            let idx = y * width + x;
            
            // Calculate gradients
            // ex = |I(x+1,y) - I(x-1,y)|
            // ey = |I(x,y+1) - I(x,y-1)|
            let ex = (image[idx + 1] as f64 - image[idx - 1] as f64).abs();
            let ey = (image[idx + width] as f64 - image[idx - width] as f64).abs();
            
            // weight = max(ex, ey)
            let weight = ex.max(ey);
            
            weight_total += weight;
            total += weight * image[idx] as f64;
        }
    }
    
    // The result threshold is sum of weighted pixel values divided by sum of weights
    if weight_total > 0.0 {
        (total / weight_total).round() as u8
    } else {
        0 // N.I.N.A. returns 0 when no gradients
    }
}

fn find_root(label_map: &mut [u32], label: u32) -> u32 {
    let mut current = label;
    while label_map[current as usize] != current {
        current = label_map[current as usize];
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaussian_kernel() {
        let kernel = create_gaussian_kernel(5, 1.0);
        assert_eq!(kernel.len(), 5);
        
        // Kernel should sum to 1
        let sum: f64 = kernel.iter().sum();
        assert!((sum - 1.0).abs() < 0.001);
        
        // Center should be highest
        assert!(kernel[2] > kernel[1]);
        assert!(kernel[2] > kernel[3]);
    }

    #[test]
    fn test_binary_dilation() {
        let mut image = vec![
            0, 0, 0, 0, 0,
            0, 0, 255, 0, 0,
            0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
            0, 0, 0, 0, 0,
        ];
        
        let dilation = BinaryDilation3x3;
        dilation.apply_in_place(&mut image, 5, 5);
        
        // Check that dilation expanded the single pixel
        assert_eq!(image[1 * 5 + 1], 255); // top-left
        assert_eq!(image[1 * 5 + 2], 255); // top
        assert_eq!(image[1 * 5 + 3], 255); // top-right
        assert_eq!(image[2 * 5 + 1], 255); // left
        assert_eq!(image[2 * 5 + 2], 255); // center
        assert_eq!(image[2 * 5 + 3], 255); // right
        assert_eq!(image[3 * 5 + 1], 255); // bottom-left
        assert_eq!(image[3 * 5 + 2], 255); // bottom
        assert_eq!(image[3 * 5 + 3], 255); // bottom-right
    }

    #[test]
    fn test_median_filter() {
        let image = vec![
            0, 0, 0, 0, 0,
            0, 10, 20, 30, 0,
            0, 40, 255, 50, 0, // 255 is noise
            0, 60, 70, 80, 0,
            0, 0, 0, 0, 0,
        ];
        
        let median = Median;
        let result = median.apply(&image, 5, 5);
        
        // The noisy pixel (255) should be replaced by median of neighborhood
        assert!(result[2 * 5 + 2] < 100); // Much less than 255
    }

    #[test]
    fn test_blob_counter() {
        let image = vec![
            0, 0, 0, 0, 0, 0, 0,
            0, 255, 255, 0, 0, 0, 0,
            0, 255, 255, 0, 0, 0, 0,
            0, 0, 0, 0, 255, 255, 0,
            0, 0, 0, 0, 255, 255, 0,
            0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ];
        
        let mut counter = BlobCounter::new();
        counter.process_image(&image, 7, 7);
        
        let blobs = counter.get_objects_information();
        assert_eq!(blobs.len(), 2); // Should find 2 blobs
        
        // Check blob dimensions
        for blob in blobs {
            assert_eq!(blob.rectangle.width, 2);
            assert_eq!(blob.rectangle.height, 2);
            assert_eq!(blob.area, 4);
        }
    }
}