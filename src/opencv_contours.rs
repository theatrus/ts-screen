use crate::accord_imaging::{Blob, Rectangle};
#[cfg(feature = "opencv")]
use crate::opencv_utils::*;
use anyhow::Result;
#[cfg(feature = "opencv")]
use opencv::{core, imgproc};

/// Advanced star contour analysis using OpenCV
#[derive(Debug, Clone)]
pub struct StarContour {
    pub contour_points: Vec<(i32, i32)>,
    pub area: f64,
    pub perimeter: f64,
    pub circularity: f64,
    pub convexity: f64,
    pub bounding_rect: Rectangle,
    pub centroid: (f64, f64),
}

/// Enhanced blob detector using OpenCV contour analysis
pub struct OpenCVBlobDetector {
    pub min_area: f64,
    pub max_area: f64,
    pub min_circularity: f64,
    pub min_convexity: f64,
}

impl Default for OpenCVBlobDetector {
    fn default() -> Self {
        Self {
            min_area: 10.0,
            max_area: 10000.0,
            min_circularity: 0.3, // More lenient than perfect circle
            min_convexity: 0.5,   // Allow some concavity for realistic stars
        }
    }
}

#[cfg(feature = "opencv")]
impl OpenCVBlobDetector {
    /// Analyze star contours with sophisticated shape analysis
    pub fn analyze_star_contours(
        &self,
        binary_image: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<StarContour>> {
        let mat = create_mat_from_u8(binary_image, width, height)?;
        let mut contours = core::Vector::<core::Vector<core::Point>>::new();

        imgproc::find_contours_def(
            &mat,
            &mut contours,
            imgproc::RETR_EXTERNAL,
            imgproc::CHAIN_APPROX_SIMPLE,
        )?;

        let mut stars = Vec::new();

        for i in 0..contours.len() {
            let contour = contours.get(i)?;

            // Calculate area and perimeter
            let area = imgproc::contour_area(&contour, false)?;
            let perimeter = imgproc::arc_length(&contour, true)?;

            // Skip tiny or huge contours
            if area < self.min_area || area > self.max_area {
                continue;
            }

            // Calculate circularity: 4π*Area/Perimeter²
            let circularity = if perimeter > 0.0 {
                4.0 * std::f64::consts::PI * area / (perimeter * perimeter)
            } else {
                0.0
            };

            if circularity < self.min_circularity {
                continue;
            }

            // Calculate convexity: Area/ConvexArea
            let mut hull = core::Vector::<core::Point>::new();
            imgproc::convex_hull_def(&contour, &mut hull)?;

            let convex_area = imgproc::contour_area(&hull, false)?;

            let convexity = if convex_area > 0.0 {
                area / convex_area
            } else {
                0.0
            };

            if convexity < self.min_convexity {
                continue;
            }

            // Calculate bounding rectangle
            let bounding_rect = imgproc::bounding_rect(&contour)?;
            let rect = Rectangle {
                x: bounding_rect.x,
                y: bounding_rect.y,
                width: bounding_rect.width,
                height: bounding_rect.height,
            };

            // Calculate centroid using image moments
            let moments = imgproc::moments(&contour, false)?;
            let centroid = if moments.m00 > 0.0 {
                (moments.m10 / moments.m00, moments.m01 / moments.m00)
            } else {
                (
                    bounding_rect.x as f64 + bounding_rect.width as f64 / 2.0,
                    bounding_rect.y as f64 + bounding_rect.height as f64 / 2.0,
                )
            };

            // Extract contour points
            let contour_points: Vec<(i32, i32)> = (0..contour.len())
                .map(|j| {
                    let pt = contour.get(j).unwrap_or(core::Point::new(0, 0));
                    (pt.x, pt.y)
                })
                .collect();

            stars.push(StarContour {
                contour_points,
                area,
                perimeter,
                circularity,
                convexity,
                bounding_rect: rect,
                centroid,
            });
        }

        // Sort by area (largest first) for consistent ordering
        stars.sort_by(|a, b| {
            b.area
                .partial_cmp(&a.area)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(stars)
    }

    /// Convert StarContour results back to simple Blob format for compatibility
    pub fn star_contours_to_blobs(contours: &[StarContour]) -> Vec<Blob> {
        contours
            .iter()
            .map(|star| Blob {
                rectangle: star.bounding_rect,
            })
            .collect()
    }

    /// Enhanced star quality assessment based on shape analysis
    pub fn assess_star_quality(&self, contour: &StarContour) -> f64 {
        // Quality score based on multiple criteria
        let circularity_score = contour.circularity;
        let convexity_score = contour.convexity;

        // Penalize very elongated stars (likely double stars or artifacts)
        let aspect_ratio = contour.bounding_rect.width as f64 / contour.bounding_rect.height as f64;
        let aspect_score = if aspect_ratio > 1.0 {
            1.0 / aspect_ratio
        } else {
            aspect_ratio
        };

        // Area-based score (favor medium-sized stars)
        let area_score = if contour.area > 50.0 && contour.area < 500.0 {
            1.0
        } else if contour.area > 500.0 {
            500.0 / contour.area // Penalize too large
        } else {
            contour.area / 50.0 // Penalize too small
        };

        // Combined quality score (0.0 to 1.0)
        (circularity_score * 0.4 + convexity_score * 0.3 + aspect_score * 0.2 + area_score * 0.1)
            .min(1.0)
    }
}

/// Fallback implementation when OpenCV is not available
#[cfg(not(feature = "opencv"))]
impl OpenCVBlobDetector {
    /// Fallback to simple blob detection from accord_imaging
    pub fn analyze_star_contours(
        &self,
        binary_image: &[u8],
        width: usize,
        height: usize,
    ) -> Result<Vec<StarContour>> {
        // Use existing blob counter for fallback
        use crate::accord_imaging::BlobCounter;
        let mut blob_counter = BlobCounter::new();
        blob_counter.process_image(binary_image, width, height);
        let blobs = blob_counter.get_objects_information();

        // Convert blobs to StarContour format with estimated values
        let star_contours: Vec<StarContour> = blobs
            .into_iter()
            .map(|blob| {
                let area = (blob.rectangle.width * blob.rectangle.height) as f64;
                let perimeter = 2.0 * (blob.rectangle.width + blob.rectangle.height) as f64;
                let circularity = if perimeter > 0.0 {
                    4.0 * std::f64::consts::PI * area / (perimeter * perimeter)
                } else {
                    0.0
                };

                StarContour {
                    contour_points: vec![], // Not available in fallback
                    area,
                    perimeter,
                    circularity,
                    convexity: 0.8, // Estimate
                    bounding_rect: blob.rectangle,
                    centroid: (
                        blob.rectangle.x as f64 + blob.rectangle.width as f64 / 2.0,
                        blob.rectangle.y as f64 + blob.rectangle.height as f64 / 2.0,
                    ),
                }
            })
            .collect();

        Ok(star_contours)
    }

    pub fn star_contours_to_blobs(contours: &[StarContour]) -> Vec<Blob> {
        contours
            .iter()
            .map(|star| Blob {
                rectangle: star.bounding_rect,
            })
            .collect()
    }

    pub fn assess_star_quality(&self, contour: &StarContour) -> f64 {
        // Simple fallback quality assessment
        contour.circularity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_detector_creation() {
        let detector = OpenCVBlobDetector::default();
        assert_eq!(detector.min_area, 10.0);
        assert_eq!(detector.min_circularity, 0.3);
    }

    #[test]
    fn test_contour_analysis_fallback() {
        let detector = OpenCVBlobDetector::default();
        let binary_image = vec![0u8; 100]; // 10x10 empty image

        let result = detector.analyze_star_contours(&binary_image, 10, 10);
        assert!(result.is_ok());

        let contours = result.unwrap();
        // Should find no contours in empty image
        assert_eq!(contours.len(), 0);
    }

    #[test]
    fn test_star_quality_assessment() {
        let detector = OpenCVBlobDetector::default();

        let star = StarContour {
            contour_points: vec![],
            area: 100.0,
            perimeter: 35.4, // Roughly circular
            circularity: 0.8,
            convexity: 0.9,
            bounding_rect: Rectangle {
                x: 10,
                y: 10,
                width: 10,
                height: 10,
            },
            centroid: (15.0, 15.0),
        };

        let quality = detector.assess_star_quality(&star);
        assert!(quality > 0.0 && quality <= 1.0);
    }
}
