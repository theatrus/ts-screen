use anyhow::Result;
use bumpalo::Bump;
use byteorder::{BigEndian, ReadBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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
    pub mad: Option<f64>,
}

/// FITS image data structure
pub struct FitsImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u16>, // Keep as 16-bit unsigned integers
}

#[allow(dead_code)]
impl FitsImage {
    /// Load FITS image data from file
    pub fn from_file(path: &Path) -> Result<Self> {
        use crate::fits::read_fits_metadata;

        // Use existing FITS reader for metadata
        let metadata = read_fits_metadata(path)?;

        // Get image dimensions from metadata
        let (width, height) = if let Some(img_info) = metadata.image_info {
            (img_info.width as usize, img_info.height as usize)
        } else {
            return Err(anyhow::anyhow!("No image information found in FITS file"));
        };

        // Read the raw data
        let data = read_fits_data(path, width, height)?;

        Ok(FitsImage {
            width,
            height,
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

        let median = if self.data.len() % 2 == 0 {
            let mid = self.data.len() / 2;
            (sorted_data[mid - 1] as f64 + sorted_data[mid] as f64) / 2.0
        } else {
            sorted_data[self.data.len() / 2] as f64
        };

        let variance: f64 = self
            .data
            .iter()
            .map(|&x| {
                let diff = x as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / self.data.len() as f64;
        let std_dev = variance.sqrt();

        let min = *sorted_data.first().unwrap_or(&0) as f64;
        let max = *sorted_data.last().unwrap_or(&65535) as f64;

        // Calculate MAD using N.I.N.A.'s histogram-based approach
        let mad = self.calculate_mad_histogram(median);

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

        // Return statistics without star detection
        // (star detection is now handled by dedicated modules)
        ImageStatistics {
            width: self.width,
            height: self.height,
            mean: stats.mean,
            median: stats.median,
            std_dev: stats.std_dev,
            min: stats.min,
            max: stats.max,
            star_count: None,
            hfr: None,
            fwhm: None,
            mad: stats.mad,
        }
    }

    /// Calculate MAD using N.I.N.A.'s histogram-based approach
    fn calculate_mad_histogram(&self, median: f64) -> f64 {
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
            } else if idx_up < 65536 {
                occurrences += pixel_counts[idx_up as usize];
            }

            // Check if we've found the median of deviations
            if occurrences as f64 > medianlength {
                // The median absolute deviation is the current distance from median
                return (idx_up as f64 - median).abs();
            }

            // Step outward
            idx_down -= 1;
            idx_up += 1;

            // Safety check
            if idx_down < 0 && idx_up >= 65536 {
                break;
            }
        }

        // Fallback to simple MAD calculation
        let mut deviations: Vec<f64> = self
            .data
            .iter()
            .map(|&x| (x as f64 - median).abs())
            .collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());

        if deviations.len() % 2 == 0 {
            let mid = deviations.len() / 2;
            (deviations[mid - 1] + deviations[mid]) / 2.0
        } else {
            deviations[deviations.len() / 2]
        }
    }
}

/// Read FITS data directly from file
fn read_fits_data(path: &Path, width: usize, height: usize) -> Result<Vec<u16>> {
    let mut file = File::open(path)?;

    // FITS files have 2880-byte blocks for headers
    // Skip headers to get to data
    let mut header = vec![0u8; 2880];
    let mut data_start = 0u64;

    loop {
        file.read_exact(&mut header)?;
        data_start += 2880;

        // Check for END card
        if header.windows(3).any(|w| w == b"END") {
            break;
        }
    }

    // Seek to start of data
    file.seek(SeekFrom::Start(data_start))?;

    // Read image data as 16-bit big-endian values
    let mut data = Vec::with_capacity(width * height);
    for _ in 0..(width * height) {
        let value = file.read_u16::<BigEndian>()?;
        data.push(value);
    }

    Ok(data)
}
