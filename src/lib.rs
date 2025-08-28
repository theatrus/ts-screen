pub mod image_analysis;
pub mod nina_star_detection;
pub mod mtf_stretch;
pub mod accord_imaging;
pub mod hocus_focus_star_detection;

#[cfg(test)]
mod test_star_detection;

// Re-export commonly used items
pub use image_analysis::{FitsImage, ImageStatistics};