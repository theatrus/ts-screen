use fitrs::Fits;
use std::path::Path;

fn main() {
    let path = "files2/Bubble Nebula/2025-08-17/LIGHT/2025-08-17_21-13-23_OIII_-10.00_300.00s_0005.fits";
    
    let fits = Fits::open(Path::new(path)).expect("Failed to open FITS file");
    let hdu = fits.get(0).expect("No primary HDU");
    
    // Look for pixel size and focal length
    let headers_of_interest = vec![
        "XPIXSZ", "YPIXSZ", "PIXSIZE", "PIXSCALE",
        "FOCALLEN", "FOCAL", "TELESCOP", "INSTRUME",
        "XBINNING", "YBINNING", "BINNING"
    ];
    
    println!("Key headers from FITS file:");
    for key in headers_of_interest {
        if let Some(value) = hdu.value(key) {
            println!("{}: {:?}", key, value);
        }
    }
    
    // Check for image scale calculation parameters
    if let (Some(xpixsz), Some(focallen)) = (hdu.value("XPIXSZ"), hdu.value("FOCALLEN")) {
        if let (fitrs::HeaderValue::RealFloatingNumber(px), fitrs::HeaderValue::RealFloatingNumber(fl)) = (xpixsz, focallen) {
            let image_scale = (*px / *fl) * 206.265;  // arcsec/pixel
            println!("\nCalculated image scale: {:.3} arcsec/pixel", image_scale);
        }
    }
}