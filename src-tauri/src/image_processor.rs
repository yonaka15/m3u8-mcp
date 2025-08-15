use base64::{Engine as _, engine::general_purpose};
use image::codecs::jpeg::JpegEncoder;

pub fn process_screenshot(png_data: Vec<u8>, quality: u8, max_width: u32) -> Result<String, String> {
    // Load PNG image from bytes
    let img = image::load_from_memory(&png_data)
        .map_err(|e| format!("Failed to load image: {}", e))?;
    
    // Resize if needed
    let processed_img = if img.width() > max_width {
        let ratio = max_width as f32 / img.width() as f32;
        let new_height = (img.height() as f32 * ratio) as u32;
        img.resize_exact(max_width, new_height, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    
    // Convert to RGB8 (remove alpha channel for JPEG)
    let rgb_img = processed_img.to_rgb8();
    
    // Encode as JPEG with specified quality
    let mut jpeg_buffer = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut jpeg_buffer, quality);
    encoder.encode(
        rgb_img.as_raw(),
        rgb_img.width(),
        rgb_img.height(),
        image::ColorType::Rgb8.into()
    ).map_err(|e| format!("Failed to encode JPEG: {}", e))?;
    
    // Convert to base64
    let base64_string = general_purpose::STANDARD.encode(&jpeg_buffer);
    
    Ok(base64_string)
}