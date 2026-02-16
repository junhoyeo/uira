use base64::Engine;
use std::path::Path;
use uira_core::ImageSource;

use crate::ProviderError;

pub(crate) fn normalize_image_source(source: &ImageSource) -> Result<ImageSource, ProviderError> {
    match source {
        ImageSource::Base64 { .. } | ImageSource::Url { .. } => Ok(source.clone()),
        ImageSource::FilePath { path } => {
            let bytes = std::fs::read(path).map_err(|err| {
                ProviderError::InvalidResponse(format!(
                    "failed to read image file '{}': {}",
                    path, err
                ))
            })?;

            let media_type = detect_media_type(path, &bytes).ok_or_else(|| {
                ProviderError::InvalidResponse(format!(
                    "unsupported image format for '{}'; supported formats: PNG, JPEG, GIF, WebP",
                    path
                ))
            })?;

            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            Ok(ImageSource::Base64 {
                media_type: media_type.to_string(),
                data,
            })
        }
    }
}

pub(crate) fn image_source_to_data_url(source: &ImageSource) -> Result<String, ProviderError> {
    let normalized = normalize_image_source(source)?;
    match normalized {
        ImageSource::Base64 { media_type, data } => {
            Ok(format!("data:{};base64,{}", media_type, data))
        }
        ImageSource::Url { url } => Ok(url),
        ImageSource::FilePath { .. } => unreachable!("file paths are normalized above"),
    }
}

fn detect_media_type(path: &str, bytes: &[u8]) -> Option<&'static str> {
    media_type_from_extension(path).or_else(|| media_type_from_header(bytes))
}

fn media_type_from_extension(path: &str) -> Option<&'static str> {
    let ext = Path::new(path)
        .extension()
        .and_then(|v| v.to_str())?
        .to_ascii_lowercase();

    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn media_type_from_header(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 8 && bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']) {
        return Some("image/png");
    }

    if bytes.len() >= 3 && bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }

    if bytes.len() >= 6 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        return Some("image/gif");
    }

    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }

    None
}
