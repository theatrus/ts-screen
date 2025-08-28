/// OpenCV Canny Edge Detection wrapper
use std::error::Error;

#[cfg(feature = "opencv")]
use crate::opencv_gaussian_blur;
#[cfg(feature = "opencv")]
use crate::opencv_utils::*;
#[cfg(feature = "opencv")]
use opencv::{imgproc, prelude::*};

/// OpenCV-based Canny edge detector
pub struct OpenCVCanny {
    low_threshold: f64,
    high_threshold: f64,
    aperture_size: i32,
    l2_gradient: bool,
}

impl OpenCVCanny {
    /// Create a new Canny edge detector with default parameters
    pub fn new(low_threshold: u8, high_threshold: u8) -> Self {
        Self {
            low_threshold: low_threshold as f64,
            high_threshold: high_threshold as f64,
            aperture_size: 3,   // Standard Sobel kernel size
            l2_gradient: false, // Use L1 gradient (faster)
        }
    }

    /// Create a Canny detector with L2 gradient (more accurate but slower)
    pub fn new_l2(low_threshold: u8, high_threshold: u8) -> Self {
        Self {
            low_threshold: low_threshold as f64,
            high_threshold: high_threshold as f64,
            aperture_size: 3,
            l2_gradient: true,
        }
    }

    /// Apply Canny edge detection to an image
    #[cfg(feature = "opencv")]
    pub fn apply(
        &self,
        image: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // Create Mat from raw data
        let src = create_mat_from_u8(image, width, height)?;

        // Output Mat
        let mut edges = Mat::default();

        // Apply Canny edge detection
        imgproc::canny(
            &src,
            &mut edges,
            self.low_threshold,
            self.high_threshold,
            self.aperture_size,
            self.l2_gradient,
        )?;

        // Convert result back to Vec<u8>
        Ok(mat_to_u8_vec(&edges)?)
    }

    /// Apply Canny with Gaussian blur pre-processing
    #[cfg(feature = "opencv")]
    pub fn apply_with_blur(
        &self,
        image: &[u8],
        width: usize,
        height: usize,
        blur_size: i32,
        sigma: f64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // Create Mat from raw data
        let src = create_mat_from_u8(image, width, height)?;

        // Apply Gaussian blur
        let mut blurred = Mat::default();
        let ksize = opencv::core::Size::new(blur_size, blur_size);
        opencv_gaussian_blur!(
            &src,
            &mut blurred,
            ksize,
            sigma,
            sigma,
            opencv::core::BORDER_DEFAULT
        )?;

        // Apply Canny edge detection
        let mut edges = Mat::default();
        imgproc::canny(
            &blurred,
            &mut edges,
            self.low_threshold,
            self.high_threshold,
            self.aperture_size,
            self.l2_gradient,
        )?;

        // Convert result back to Vec<u8>
        Ok(mat_to_u8_vec(&edges)?)
    }

    /// Fallback implementation when OpenCV is not available
    #[cfg(not(feature = "opencv"))]
    pub fn apply(
        &self,
        image: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Err("OpenCV support not compiled in. Canny edge detection requires OpenCV.".into())
    }

    #[cfg(not(feature = "opencv"))]
    pub fn apply_with_blur(
        &self,
        _image: &[u8],
        _width: usize,
        _height: usize,
        _blur_size: i32,
        _sigma: f64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Err("OpenCV support not compiled in. Canny edge detection requires OpenCV.".into())
    }
}

/// SIS (Simple Image Statistics) thresholding using OpenCV
pub struct OpenCVThreshold;

impl OpenCVThreshold {
    /// Apply SIS thresholding to an image
    #[cfg(feature = "opencv")]
    pub fn apply_sis(image: &[u8], width: usize, height: usize) -> Result<Vec<u8>, Box<dyn Error>> {
        // Create Mat from raw data
        let src = create_mat_from_u8(image, width, height)?;

        // Calculate optimal threshold using Otsu's method (similar to SIS)
        let mut dst = Mat::default();
        let _ = imgproc::threshold(
            &src,
            &mut dst,
            0.0,
            255.0,
            imgproc::THRESH_BINARY | imgproc::THRESH_OTSU,
        )?;

        // Convert result back to Vec<u8>
        Ok(mat_to_u8_vec(&dst)?)
    }

    #[cfg(not(feature = "opencv"))]
    pub fn apply_sis(
        _image: &[u8],
        _width: usize,
        _height: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Err("OpenCV support not compiled in. Thresholding requires OpenCV.".into())
    }
}

/// Noise reduction operations using OpenCV
pub struct OpenCVNoiseReduction;

impl OpenCVNoiseReduction {
    /// Apply Gaussian blur for noise reduction
    #[cfg(feature = "opencv")]
    pub fn gaussian_blur(
        image: &[u8],
        width: usize,
        height: usize,
        sigma: f64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // Create Mat from raw data
        let src = create_mat_from_u8(image, width, height)?;

        // Apply Gaussian blur
        let mut dst = Mat::default();
        let ksize = opencv::core::Size::new(0, 0); // Auto-calculate from sigma
        opencv_gaussian_blur!(
            &src,
            &mut dst,
            ksize,
            sigma,
            sigma,
            opencv::core::BORDER_DEFAULT
        )?;

        // Convert result back to Vec<u8>
        Ok(mat_to_u8_vec(&dst)?)
    }

    /// Apply median filter for noise reduction
    #[cfg(feature = "opencv")]
    pub fn median_blur(
        image: &[u8],
        width: usize,
        height: usize,
        ksize: i32,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // Create Mat from raw data
        let src = create_mat_from_u8(image, width, height)?;

        // Apply median blur
        let mut dst = Mat::default();
        imgproc::median_blur(&src, &mut dst, ksize)?;

        // Convert result back to Vec<u8>
        Ok(mat_to_u8_vec(&dst)?)
    }

    #[cfg(not(feature = "opencv"))]
    pub fn gaussian_blur(
        _image: &[u8],
        _width: usize,
        _height: usize,
        _sigma: f64,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Err("OpenCV support not compiled in. Gaussian blur requires OpenCV.".into())
    }

    #[cfg(not(feature = "opencv"))]
    pub fn median_blur(
        _image: &[u8],
        _width: usize,
        _height: usize,
        _ksize: i32,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Err("OpenCV support not compiled in. Median blur requires OpenCV.".into())
    }
}

/// Binary morphology operations using OpenCV
pub struct OpenCVBinaryMorphology;

impl OpenCVBinaryMorphology {
    /// Apply binary dilation with 3x3 kernel
    #[cfg(feature = "opencv")]
    pub fn dilate_3x3(
        image: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        // Create Mat from raw data
        let src = create_mat_from_u8(image, width, height)?;

        // Create 3x3 structuring element
        let kernel = imgproc::get_structuring_element(
            imgproc::MORPH_RECT,
            opencv::core::Size::new(3, 3),
            opencv::core::Point::new(-1, -1),
        )?;

        // Apply dilation
        let mut dst = Mat::default();
        imgproc::dilate(
            &src,
            &mut dst,
            &kernel,
            opencv::core::Point::new(-1, -1),
            1,
            opencv::core::BORDER_CONSTANT,
            imgproc::morphology_default_border_value()?,
        )?;

        // Convert result back to Vec<u8>
        Ok(mat_to_u8_vec(&dst)?)
    }

    #[cfg(not(feature = "opencv"))]
    pub fn dilate_3x3(
        _image: &[u8],
        _width: usize,
        _height: usize,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        Err("OpenCV support not compiled in. Morphology operations require OpenCV.".into())
    }
}
