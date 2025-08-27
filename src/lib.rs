pub mod image_analysis;
pub mod nina_star_detection;
pub mod mtf_stretch;
pub mod accord_imaging;

// Re-export commonly used items
pub use image_analysis::{FitsImage, ImageStatistics};