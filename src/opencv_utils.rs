use anyhow::{Context, Result};
use opencv::prelude::*;
use opencv::{core, Result as OpenCVResult};

/// Convert u16 slice to OpenCV Mat for image processing
pub fn create_mat_from_u16(data: &[u16], width: usize, height: usize) -> OpenCVResult<core::Mat> {
    // Create a new Mat and copy data
    let mut mat = core::Mat::zeros(height as i32, width as i32, core::CV_16UC1)?.to_mat()?;

    // Copy data into the Mat
    for (i, &val) in data.iter().enumerate() {
        let row = (i / width) as i32;
        let col = (i % width) as i32;
        *mat.at_2d_mut::<u16>(row, col)? = val;
    }

    Ok(mat)
}

/// Convert u8 slice to OpenCV Mat for image processing
pub fn create_mat_from_u8(data: &[u8], width: usize, height: usize) -> OpenCVResult<core::Mat> {
    // Create a new Mat and copy data
    let mut mat = core::Mat::zeros(height as i32, width as i32, core::CV_8UC1)?.to_mat()?;

    // Copy data into the Mat
    for (i, &val) in data.iter().enumerate() {
        let row = (i / width) as i32;
        let col = (i % width) as i32;
        *mat.at_2d_mut::<u8>(row, col)? = val;
    }

    Ok(mat)
}

/// Convert OpenCV Mat back to u16 vector
pub fn mat_to_u16_vec(mat: &core::Mat) -> OpenCVResult<Vec<u16>> {
    let rows = mat.rows();
    let cols = mat.cols();
    let total_pixels = (rows * cols) as usize;

    let mut result = Vec::with_capacity(total_pixels);

    for row in 0..rows {
        for col in 0..cols {
            let value: u16 = *mat.at_2d(row, col)?;
            result.push(value);
        }
    }

    Ok(result)
}

/// Convert OpenCV Mat back to u8 vector
pub fn mat_to_u8_vec(mat: &core::Mat) -> OpenCVResult<Vec<u8>> {
    let rows = mat.rows();
    let cols = mat.cols();
    let total_pixels = (rows * cols) as usize;

    let mut result = Vec::with_capacity(total_pixels);

    for row in 0..rows {
        for col in 0..cols {
            let value: u8 = *mat.at_2d(row, col)?;
            result.push(value);
        }
    }

    Ok(result)
}

/// Copy OpenCV Mat data back to existing u8 slice
pub fn copy_mat_to_u8(mat: &core::Mat, dest: &mut [u8]) -> OpenCVResult<()> {
    let rows = mat.rows();
    let cols = mat.cols();

    for row in 0..rows {
        for col in 0..cols {
            let idx = (row * cols + col) as usize;
            if idx < dest.len() {
                dest[idx] = *mat.at_2d(row, col)?;
            }
        }
    }

    Ok(())
}

/// Create Gaussian template for PSF matching
pub fn create_gaussian_template(size: i32, sigma: f64) -> OpenCVResult<core::Mat> {
    let mut template = core::Mat::zeros(size, size, core::CV_32F)?.to_mat()?;

    let center = size as f64 / 2.0;
    let two_sigma_squared = 2.0 * sigma * sigma;

    for row in 0..size {
        for col in 0..size {
            let x = col as f64 - center;
            let y = row as f64 - center;
            let distance_squared = x * x + y * y;
            let value = (-distance_squared / two_sigma_squared).exp();

            *template.at_2d_mut::<f32>(row, col)? = value as f32;
        }
    }

    // Normalize template
    let mut normalized = core::Mat::default();
    core::normalize(
        &template,
        &mut normalized,
        0.0,
        1.0,
        core::NORM_MINMAX,
        -1,
        &core::no_array(),
    )?;

    Ok(normalized)
}

/// Convert 16-bit astronomical data to normalized 32-bit for OpenCV processing
pub fn normalize_u16_to_f32(data: &[u16]) -> Vec<f32> {
    let max_val = *data.iter().max().unwrap_or(&1) as f32;
    data.iter().map(|&val| val as f32 / max_val).collect()
}

/// OpenCV error wrapper for better error handling
pub fn opencv_to_anyhow(result: OpenCVResult<()>) -> Result<()> {
    result.with_context(|| "OpenCV operation failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u16_mat_conversion() {
        let width = 4;
        let height = 3;
        let data: Vec<u16> = (0..12).map(|i| i * 1000).collect();

        let mat = create_mat_from_u16(&data, width, height).unwrap();
        assert_eq!(mat.rows(), height as i32);
        assert_eq!(mat.cols(), width as i32);

        let recovered = mat_to_u16_vec(&mat).unwrap();
        assert_eq!(data, recovered);
    }

    #[test]
    fn test_u8_mat_conversion() {
        let width = 4;
        let height = 3;
        let data: Vec<u8> = (0..12).collect();

        let mat = create_mat_from_u8(&data, width, height).unwrap();
        assert_eq!(mat.rows(), height as i32);
        assert_eq!(mat.cols(), width as i32);

        let recovered = mat_to_u8_vec(&mat).unwrap();
        assert_eq!(data, recovered);
    }

    #[test]
    fn test_gaussian_template() {
        let template = create_gaussian_template(5, 1.0).unwrap();
        assert_eq!(template.rows(), 5);
        assert_eq!(template.cols(), 5);

        // Center should have highest value
        let center_val: f32 = *template.at_2d(2, 2).unwrap();
        let corner_val: f32 = *template.at_2d(0, 0).unwrap();
        assert!(center_val > corner_val);
    }

    #[test]
    fn test_normalize_u16_to_f32() {
        let data = vec![0u16, 1000, 2000, 3000];
        let normalized = normalize_u16_to_f32(&data);

        assert_eq!(normalized[0], 0.0);
        assert_eq!(normalized[3], 1.0);
        assert!((normalized[1] - 1.0 / 3.0).abs() < 0.001);
    }
}
