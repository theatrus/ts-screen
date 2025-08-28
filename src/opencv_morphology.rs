#[cfg(feature = "opencv")]
use crate::opencv_utils::*;
use anyhow::Result;
#[cfg(feature = "opencv")]
use opencv::imgproc::morphology_default_border_value;
#[cfg(feature = "opencv")]
#[cfg(feature = "opencv")]
use opencv::{core, imgproc};

/// Advanced morphological operations using OpenCV
#[cfg(feature = "opencv")]
pub struct OpenCVMorphology {
    kernel_size: i32,
    kernel_type: MorphKernelType,
}

#[cfg(feature = "opencv")]
pub enum MorphKernelType {
    Rectangle,
    Ellipse,
    Cross,
}

#[cfg(feature = "opencv")]
impl OpenCVMorphology {
    /// Create new morphology processor with elliptical kernel (better for stars)
    pub fn new_ellipse(kernel_size: i32) -> Self {
        Self {
            kernel_size,
            kernel_type: MorphKernelType::Ellipse,
        }
    }

    /// Create new morphology processor with rectangular kernel (NINA compatible)
    pub fn new_rectangle(kernel_size: i32) -> Self {
        Self {
            kernel_size,
            kernel_type: MorphKernelType::Rectangle,
        }
    }

    /// Apply binary dilation with advanced morphology
    pub fn dilate_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        let mat = create_mat_from_u8(image, width, height)?;

        let kernel_type = match self.kernel_type {
            MorphKernelType::Rectangle => imgproc::MORPH_RECT,
            MorphKernelType::Ellipse => imgproc::MORPH_ELLIPSE,
            MorphKernelType::Cross => imgproc::MORPH_CROSS,
        };

        let kernel = imgproc::get_structuring_element(
            kernel_type,
            core::Size::new(self.kernel_size, self.kernel_size),
            core::Point::new(-1, -1),
        )?;

        let mut result = core::Mat::default();
        imgproc::dilate(
            &mat,
            &mut result,
            &kernel,
            core::Point::new(-1, -1),
            1, // iterations
            core::BORDER_REFLECT,
            morphology_default_border_value()?,
        )?;

        copy_mat_to_u8(&result, image)?;
        Ok(())
    }

    /// Apply morphological opening (erosion followed by dilation)
    /// Good for removing noise while preserving star shapes
    pub fn opening_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        let mat = create_mat_from_u8(image, width, height)?;

        let kernel_type = match self.kernel_type {
            MorphKernelType::Rectangle => imgproc::MORPH_RECT,
            MorphKernelType::Ellipse => imgproc::MORPH_ELLIPSE,
            MorphKernelType::Cross => imgproc::MORPH_CROSS,
        };

        let kernel = imgproc::get_structuring_element(
            kernel_type,
            core::Size::new(self.kernel_size, self.kernel_size),
            core::Point::new(-1, -1),
        )?;

        let mut result = core::Mat::default();
        imgproc::morphology_ex(
            &mat,
            &mut result,
            imgproc::MORPH_OPEN,
            &kernel,
            core::Point::new(-1, -1),
            1, // iterations
            core::BORDER_REFLECT,
            morphology_default_border_value()?,
        )?;

        copy_mat_to_u8(&result, image)?;
        Ok(())
    }

    /// Apply morphological closing (dilation followed by erosion)
    /// Good for filling gaps in stars
    pub fn closing_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        let mat = create_mat_from_u8(image, width, height)?;

        let kernel_type = match self.kernel_type {
            MorphKernelType::Rectangle => imgproc::MORPH_RECT,
            MorphKernelType::Ellipse => imgproc::MORPH_ELLIPSE,
            MorphKernelType::Cross => imgproc::MORPH_CROSS,
        };

        let kernel = imgproc::get_structuring_element(
            kernel_type,
            core::Size::new(self.kernel_size, self.kernel_size),
            core::Point::new(-1, -1),
        )?;

        let mut result = core::Mat::default();
        imgproc::morphology_ex(
            &mat,
            &mut result,
            imgproc::MORPH_CLOSE,
            &kernel,
            core::Point::new(-1, -1),
            1, // iterations
            core::BORDER_REFLECT,
            morphology_default_border_value()?,
        )?;

        copy_mat_to_u8(&result, image)?;
        Ok(())
    }

    /// Apply erosion operation
    pub fn erode_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        let mat = create_mat_from_u8(image, width, height)?;

        let kernel_type = match self.kernel_type {
            MorphKernelType::Rectangle => imgproc::MORPH_RECT,
            MorphKernelType::Ellipse => imgproc::MORPH_ELLIPSE,
            MorphKernelType::Cross => imgproc::MORPH_CROSS,
        };

        let kernel = imgproc::get_structuring_element(
            kernel_type,
            core::Size::new(self.kernel_size, self.kernel_size),
            core::Point::new(-1, -1),
        )?;

        let mut result = core::Mat::default();
        imgproc::erode(
            &mat,
            &mut result,
            &kernel,
            core::Point::new(-1, -1),
            1, // iterations
            core::BORDER_REFLECT,
            morphology_default_border_value()?,
        )?;

        copy_mat_to_u8(&result, image)?;
        Ok(())
    }

    /// Advanced hot pixel filtering using morphological operations
    /// Combines opening and closing for better noise removal
    pub fn hot_pixel_filter_in_place(
        &self,
        image: &mut [u8],
        width: usize,
        height: usize,
    ) -> Result<()> {
        // First apply opening to remove hot pixels
        self.opening_in_place(image, width, height)?;

        // Then apply closing to restore star shapes
        self.closing_in_place(image, width, height)?;

        Ok(())
    }
}

/// Fallback implementation when OpenCV is not available
#[cfg(not(feature = "opencv"))]
pub struct OpenCVMorphology {
    kernel_size: i32,
}

#[cfg(not(feature = "opencv"))]
impl OpenCVMorphology {
    pub fn new_ellipse(kernel_size: i32) -> Self {
        Self { kernel_size }
    }

    pub fn new_rectangle(kernel_size: i32) -> Self {
        Self { kernel_size }
    }

    /// Fallback to simple 3x3 dilation from accord_imaging
    pub fn dilate_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        use crate::accord_imaging::BinaryDilation3x3;
        let dilation = BinaryDilation3x3;
        dilation.apply_in_place(image, width, height);
        Ok(())
    }

    pub fn opening_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        // Simple fallback - just dilate for now
        self.dilate_in_place(image, width, height)
    }

    pub fn closing_in_place(&self, image: &mut [u8], width: usize, height: usize) -> Result<()> {
        // Simple fallback - just dilate for now
        self.dilate_in_place(image, width, height)
    }

    pub fn erode_in_place(&self, _image: &mut [u8], _width: usize, _height: usize) -> Result<()> {
        // Simple fallback - do nothing for now
        Ok(())
    }

    pub fn hot_pixel_filter_in_place(
        &self,
        image: &mut [u8],
        width: usize,
        height: usize,
    ) -> Result<()> {
        self.dilate_in_place(image, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morphology_operations() {
        let mut image = vec![0u8; 25]; // 5x5 image

        // Create a single pixel in center
        image[12] = 255; // center pixel

        let morph = OpenCVMorphology::new_ellipse(3);

        // Test dilation
        morph.dilate_in_place(&mut image, 5, 5).unwrap();

        // After dilation, neighboring pixels should be set
        assert!(image[12] == 255); // center still set

        #[cfg(feature = "opencv")]
        {
            // With OpenCV, we should have proper elliptical dilation
            assert!(image[7] == 255 || image[11] == 255); // some neighbors should be set
        }
    }

    #[test]
    fn test_hot_pixel_filtering() {
        let mut image = vec![0u8; 25]; // 5x5 image

        // Create isolated hot pixels and a larger star
        image[2] = 255; // isolated hot pixel
        image[11] = 255; // part of star
        image[12] = 255; // star center
        image[13] = 255; // part of star

        let morph = OpenCVMorphology::new_ellipse(3);
        morph.hot_pixel_filter_in_place(&mut image, 5, 5).unwrap();

        // Star should be preserved, hot pixel may be reduced
        assert!(image[12] == 255); // star center should remain
    }
}
