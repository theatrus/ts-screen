use anyhow::Result;
#[cfg(feature = "opencv")]
use opencv::photo::{edge_preserving_filter, RECURS_FILTER};
#[cfg(feature = "opencv")]
use opencv::prelude::*;
#[cfg(feature = "opencv")]
use opencv::ximgproc::dt_filter;
#[cfg(feature = "opencv")]
use opencv::{core, imgproc};

/// Wavelet-based structure removal for astronomical images
pub struct WaveletStructureRemover {
    pub layers: usize,
    pub use_opencv_filters: bool,
}

impl Default for WaveletStructureRemover {
    fn default() -> Self {
        Self {
            layers: 6, // Default from HocusFocus
            use_opencv_filters: true,
        }
    }
}

impl WaveletStructureRemover {
    pub fn new(layers: usize) -> Self {
        Self {
            layers,
            use_opencv_filters: true,
        }
    }

    /// Remove large structures using wavelet decomposition
    /// Returns the residual (small structures + noise) after subtracting large structures
    pub fn remove_structures(&self, data: &[f64], width: usize, height: usize) -> Result<Vec<f64>> {
        #[cfg(feature = "opencv")]
        {
            if self.use_opencv_filters {
                return self.remove_structures_opencv(data, width, height);
            }
        }

        // Fallback to custom À trous implementation
        self.remove_structures_atrous(data, width, height)
    }

    #[cfg(feature = "opencv")]
    /// Enhanced structure removal using OpenCV filters
    fn remove_structures_opencv(
        &self,
        data: &[f64],
        width: usize,
        height: usize,
    ) -> Result<Vec<f64>> {
        // Convert to OpenCV format
        let mut mat = core::Mat::zeros(height as i32, width as i32, core::CV_32F)?.to_mat()?;

        // Copy data into the Mat
        for (i, &val) in data.iter().enumerate() {
            let row = (i / width) as i32;
            let col = (i % width) as i32;
            *mat.at_2d_mut::<f32>(row, col)? = val as f32;
        }

        let mut residual = mat.clone();

        for layer in 0..self.layers {
            let scale = 1 << layer; // 2^layer
            let kernel_size = 2 * scale + 1;

            // Use OpenCV's domain transform filter for better edge preservation
            let mut filtered = core::Mat::default();

            // For first few layers, use Gaussian blur with increasing sigma
            if layer < 3 {
                let sigma = scale as f64 * 0.8; // Adaptive sigma
                imgproc::gaussian_blur(
                    &residual,
                    &mut filtered,
                    core::Size::new(kernel_size, kernel_size),
                    sigma,
                    sigma,
                    core::BORDER_REFLECT,
                    core::AlgorithmHint::ALGO_HINT_ACCURATE,
                )?;
            } else {
                // For larger scales, use domain transform for better structure preservation
                dt_filter(
                    &residual,
                    &residual, // Use residual as guide image
                    &mut filtered,
                    10.0 * scale as f64,      // sigma_s (spatial sigma)
                    0.1,                      // sigma_r (range sigma)
                    opencv::ximgproc::DTF_NC, // mode
                    1,                        // num_iters
                )?;
            }

            // Subtract this layer from residual
            let mut temp_residual = core::Mat::default();
            core::subtract(
                &residual,
                &filtered,
                &mut temp_residual,
                &core::no_array(),
                -1,
            )?;
            residual = temp_residual;
        }

        // Convert back to Vec<f64>
        let mut result = Vec::with_capacity(width * height);
        for row in 0..height as i32 {
            for col in 0..width as i32 {
                let val: f32 = *residual.at_2d(row, col)?;
                result.push(val as f64);
            }
        }

        Ok(result)
    }

    /// Custom À trous B3 spline implementation (matches HocusFocus exactly)
    fn remove_structures_atrous(
        &self,
        data: &[f64],
        width: usize,
        height: usize,
    ) -> Result<Vec<f64>> {
        let mut residual = data.to_vec();

        for layer in 0..self.layers {
            let scale = 1 << layer; // 2^layer - determines spacing
            let mut temp = vec![0.0; width * height];

            // B3 spline coefficients
            let coeffs = [0.0625, 0.25, 0.375, 0.25, 0.0625];
            let offsets = [-2, -1, 0, 1, 2];

            // Horizontal pass
            for y in 0..height {
                for x in 0..width {
                    let mut sum = 0.0;
                    let mut weight = 0.0;

                    for i in 0..5 {
                        let sx = x as i32 + offsets[i] * scale;
                        if sx >= 0 && sx < width as i32 {
                            sum += residual[y * width + sx as usize] * coeffs[i];
                            weight += coeffs[i];
                        }
                    }
                    temp[y * width + x] = if weight > 0.0 { sum / weight } else { 0.0 };
                }
            }

            // Vertical pass
            let mut smoothed = vec![0.0; width * height];
            for y in 0..height {
                for x in 0..width {
                    let mut sum = 0.0;
                    let mut weight = 0.0;

                    for i in 0..5 {
                        let sy = y as i32 + offsets[i] * scale;
                        if sy >= 0 && sy < height as i32 {
                            sum += temp[sy as usize * width + x] * coeffs[i];
                            weight += coeffs[i];
                        }
                    }
                    smoothed[y * width + x] = if weight > 0.0 { sum / weight } else { 0.0 };
                }
            }

            // Subtract the smoothed version from residual
            for i in 0..residual.len() {
                residual[i] -= smoothed[i];
            }
        }

        Ok(residual)
    }

    /// Enhanced structure removal with multiple methods
    pub fn remove_structures_multi_method(
        &self,
        data: &[f64],
        width: usize,
        height: usize,
    ) -> Result<Vec<f64>> {
        // Try different methods and combine results
        let atrous_result = self.remove_structures_atrous(data, width, height)?;

        #[cfg(feature = "opencv")]
        {
            if let Ok(opencv_result) = self.remove_structures_opencv(data, width, height) {
                // Blend results for robustness
                let mut combined = Vec::with_capacity(atrous_result.len());
                for i in 0..atrous_result.len() {
                    // Weight favor the à trous result (proven to work) but enhance with OpenCV
                    let blended = 0.7 * atrous_result[i] + 0.3 * opencv_result[i];
                    combined.push(blended);
                }
                return Ok(combined);
            }
        }

        Ok(atrous_result)
    }

    /// Apply edge-preserving smoothing before wavelet decomposition
    #[cfg(feature = "opencv")]
    pub fn preprocess_with_edge_preserving(
        &self,
        data: &[f64],
        width: usize,
        height: usize,
    ) -> Result<Vec<f64>> {
        let mut mat = core::Mat::zeros(height as i32, width as i32, core::CV_32F)?.to_mat()?;

        // Copy data into the Mat
        for (i, &val) in data.iter().enumerate() {
            let row = (i / width) as i32;
            let col = (i % width) as i32;
            *mat.at_2d_mut::<f32>(row, col)? = val as f32;
        }

        let mut smoothed = core::Mat::default();

        // Edge-preserving filter to reduce noise while preserving star edges
        edge_preserving_filter(
            &mat,
            &mut smoothed,
            RECURS_FILTER, // or NORMCONV_FILTER
            50.0,          // sigma_s (spatial sigma)
            0.4,           // sigma_r (range sigma - lower preserves more edges)
        )?;

        // Convert back
        let mut result = Vec::with_capacity(width * height);
        for row in 0..height as i32 {
            for col in 0..width as i32 {
                let val: f32 = *smoothed.at_2d(row, col)?;
                result.push(val as f64);
            }
        }

        Ok(result)
    }

    #[cfg(not(feature = "opencv"))]
    pub fn preprocess_with_edge_preserving(
        &self,
        data: &[f64],
        _width: usize,
        _height: usize,
    ) -> Result<Vec<f64>> {
        // Fallback: return data unchanged
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wavelet_structure_removal() {
        let width = 10;
        let height = 10;
        let data: Vec<f64> = (0..100).map(|i| i as f64).collect();

        let remover = WaveletStructureRemover::new(3);
        let result = remover.remove_structures(&data, width, height);

        assert!(result.is_ok());
        let residual = result.unwrap();
        assert_eq!(residual.len(), data.len());

        // Residual should be different from original (structure removed)
        assert_ne!(residual, data);
    }

    #[test]
    fn test_atrous_fallback() {
        let width = 5;
        let height = 5;
        let data: Vec<f64> = vec![1.0; 25]; // Uniform data

        let remover = WaveletStructureRemover {
            layers: 2,
            use_opencv_filters: false, // Force à trous
        };

        let result = remover.remove_structures_atrous(&data, width, height);
        assert!(result.is_ok());

        let residual = result.unwrap();
        assert_eq!(residual.len(), 25);

        // For uniform data, residual should be mostly zeros
        let sum: f64 = residual.iter().map(|x| x.abs()).sum();
        assert!(sum < 1.0); // Very small residual for uniform input
    }

    #[test]
    fn test_multi_method_structure_removal() {
        let width = 8;
        let height = 8;
        // Create data with large-scale gradient (structure) and small variations (stars)
        let data: Vec<f64> = (0..64)
            .map(|i| {
                let x = i % width;
                let y = i / width;
                // Large-scale gradient + small local peaks
                let structure = (x + y) as f64 * 10.0;
                let detail = if (x == 3 && y == 3) || (x == 6 && y == 5) {
                    50.0
                } else {
                    0.0
                };
                structure + detail
            })
            .collect();

        let remover = WaveletStructureRemover::new(4);
        let result = remover.remove_structures_multi_method(&data, width, height);

        assert!(result.is_ok());
        let residual = result.unwrap();
        assert_eq!(residual.len(), 64);

        // Residual should preserve the small peaks while removing the gradient
        // The peaks at (3,3) and (6,5) should still be prominent in residual
        let peak1_idx = 3 * width + 3;
        let peak2_idx = 5 * width + 6;

        assert!(residual[peak1_idx].abs() > 10.0); // Peak preserved
        assert!(residual[peak2_idx].abs() > 10.0); // Peak preserved
    }
}
