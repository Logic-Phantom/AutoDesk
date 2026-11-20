//! 이미지 변환: 여러 이미지를 한 번에 변환·리사이즈·압축한다. 원본은 절대 덮어쓰지 않고 별도 폴더에 저장한다.

use crate::error::{AppError, AppResult};
use crate::fsutil;
use image::{imageops::FilterType, DynamicImage, ImageDecoder, ImageFormat, ImageReader};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageJob {
    pub files: Vec<String>,
    /// keep | jpg | png | webp
    pub format: String,
    /// none | fit | exact | cover | percent
    pub resize: String,
    pub width: u32,
    pub height: u32,
    pub percent: u32,
    /// JPG 품질(1~100)
    pub quality: u8,
    /// 0이면 제한 없음. JPG에서만 품질을 낮춰 맞춘다.
    pub max_kb: u64,
    /// 비우면 첫 파일 폴더 아래 `AutoDesk 변환` 폴더
    pub output_dir: String,
    pub suffix: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResult {
    pub source: String,
    pub output: Option<String>,
    pub before_bytes: u64,
    pub after_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub note: Option<String>,
    pub error: Option<String>,
}

pub const SUPPORTED_INPUT: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// EXIF 회전 정보를 반영해 연다(휴대폰 사진이 눕지 않게).
pub fn open_oriented(path: &Path) -> AppResult<DynamicImage> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder().map_err(|e| AppError::new("IMAGE_DECODE", format!("이미지를 읽을 수 없습니다: {e}")))?;
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| AppError::new("IMAGE_DECODE", format!("이미지를 읽을 수 없습니다: {e}")))?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img)
}

pub fn resize(img: DynamicImage, job: &ImageJob) -> DynamicImage {
    let (w, h) = (job.width.max(1), job.height.max(1));
    match job.resize.as_str() {
        // 비율 유지, 상자 안에 들어가게(확대는 하지 않음)
        "fit" if img.width() > w || img.height() > h => img.resize(w, h, FilterType::Lanczos3),
        // 정확히 w×h(비율 무시)
        "exact" => img.resize_exact(w, h, FilterType::Lanczos3),
        // 비율 유지 + 넘치는 부분 가운데 기준으로 잘라 정확히 w×h
        "cover" => img.resize_to_fill(w, h, FilterType::Lanczos3),
        "percent" => {
            let p = job.percent.clamp(1, 400) as f32 / 100.0;
            let nw = ((img.width() as f32) * p).round().max(1.0) as u32;
            let nh = ((img.height() as f32) * p).round().max(1.0) as u32;
            img.resize_exact(nw, nh, FilterType::Lanczos3)
        }
        _ => img,
    }
}

fn encode(img: &DynamicImage, format: &str, quality: u8) -> AppResult<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    match format {
        "jpg" => {
            let rgb = img.to_rgb8(); // JPG는 투명도 없음 → 흰 배경 대신 그대로 RGB 변환
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.clamp(1, 100));
            rgb.write_with_encoder(enc).map_err(|e| AppError::new("IMAGE_ENCODE", e.to_string()))?;
        }
        "png" => {
            let enc = image::codecs::png::PngEncoder::new_with_quality(
                &mut buf,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::Adaptive,
            );
            img.write_with_encoder(enc).map_err(|e| AppError::new("IMAGE_ENCODE", e.to_string()))?;
        }
        "webp" => {
            // image 크레이트의 WebP 인코더는 무손실만 지원한다.
            img.write_to(&mut buf, ImageFormat::WebP).map_err(|e| AppError::new("IMAGE_ENCODE", e.to_string()))?;
        }
        _ => return Err(AppError::invalid("지원하지 않는 출력 형식입니다")),
    }
    Ok(buf.into_inner())
}

/// JPG를 목표 용량 이하가 될 때까지 품질을 이분 탐색으로 낮춘다(최저 품질 20).
fn encode_jpg_under(img: &DynamicImage, max_bytes: u64, start_quality: u8) -> AppResult<(Vec<u8>, u8, bool)> {
    let first = encode(img, "jpg", start_quality)?;
    if first.len() as u64 <= max_bytes {
        return Ok((first, start_quality, true));
    }
    let (mut lo, mut hi) = (20u8, start_quality);
    let mut best: Option<(Vec<u8>, u8)> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let data = encode(img, "jpg", mid)?;
        if data.len() as u64 <= max_bytes {
            best = Some((data, mid));
            lo = mid + 1;
        } else {
            if mid == 0 {
                break;
            }
            hi = mid - 1;
        }
    }
    match best {
        Some((d, q)) => Ok((d, q, true)),
        None => {
            let d = encode(img, "jpg", 20)?;
            Ok((d, 20, false))
        }
    }
}

fn output_format(src: &Path, job: &ImageJob) -> String {
    match job.format.as_str() {
        "keep" => match ext_of(src).as_str() {
            "jpg" | "jpeg" => "jpg".into(),
            "webp" => "webp".into(),
            _ => "png".into(), // gif/bmp는 png로
        },
        f => f.to_string(),
    }
}

pub fn output_dir(job: &ImageJob) -> AppResult<PathBuf> {
    if !job.output_dir.trim().is_empty() {
        return Ok(PathBuf::from(job.output_dir.trim()));
    }
    let first = job.files.first().ok_or_else(|| AppError::invalid("이미지를 선택하세요"))?;
    Ok(Path::new(first).parent().unwrap_or(Path::new(".")).join("AutoDesk 변환"))
}

pub fn process_one(src: &Path, out_dir: &Path, job: &ImageJob) -> ImageResult {
    let before = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let mut r = ImageResult {
        source: src.to_string_lossy().into(),
        output: None,
        before_bytes: before,
        after_bytes: 0,
        width: 0,
        height: 0,
        note: None,
        error: None,
    };
    let run = || -> AppResult<(PathBuf, u64, u32, u32, Option<String>)> {
        if !SUPPORTED_INPUT.contains(&ext_of(src).as_str()) {
            return Err(AppError::invalid("지원하지 않는 이미지 형식입니다"));
        }
        let img = resize(open_oriented(src)?, job);
        let fmt = output_format(src, job);
        let mut note = None;
        let data = if fmt == "jpg" && job.max_kb > 0 {
            let (d, q, ok) = encode_jpg_under(&img, job.max_kb * 1024, job.quality)?;
            note = Some(if ok { format!("품질 {q}로 맞춤") } else { format!("품질을 최저(20)로 낮춰도 {}KB를 넘습니다 — 크기를 줄여 보세요", job.max_kb) });
            d
        } else {
            if job.max_kb > 0 {
                note = Some("용량 제한은 JPG에서만 적용됩니다".into());
            }
            if fmt == "webp" {
                note = Some("WebP는 무손실로 저장됩니다(용량을 줄이려면 JPG 권장)".into());
            }
            encode(&img, &fmt, job.quality)?
        };
        std::fs::create_dir_all(out_dir)?;
        let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
        let name = format!("{stem}{}.{fmt}", job.suffix);
        let dest = fsutil::unique_path(out_dir, &name);
        if fsutil::dunce_canon(&dest) == fsutil::dunce_canon(src) {
            return Err(AppError::invalid("원본을 덮어쓸 수 없습니다"));
        }
        std::fs::write(&dest, &data)?;
        Ok((dest, data.len() as u64, img.width(), img.height(), note))
    };
    match run() {
        Ok((dest, size, w, h, note)) => {
            r.output = Some(dest.to_string_lossy().into());
            r.after_bytes = size;
            r.width = w;
            r.height = h;
            r.note = note;
        }
        Err(e) => r.error = Some(e.message),
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job() -> ImageJob {
        ImageJob {
            files: vec![],
            format: "jpg".into(),
            resize: "none".into(),
            width: 100,
            height: 100,
            percent: 50,
            quality: 85,
            max_kb: 0,
            output_dir: String::new(),
            suffix: String::new(),
        }
    }

    fn sample(path: &Path, w: u32, h: u32) {
        let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([(x * 7 % 255) as u8, (y * 13 % 255) as u8, ((x + y) % 255) as u8]));
        img.save(path).unwrap();
    }

    #[test]
    fn converts_png_to_jpg_without_touching_original() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("a.png");
        sample(&src, 400, 300);
        let before = std::fs::read(&src).unwrap();
        let r = process_one(&src, &d.path().join("out"), &job());
        assert!(r.error.is_none(), "{:?}", r.error);
        assert!(r.output.as_ref().unwrap().ends_with("a.jpg"));
        assert_eq!(std::fs::read(&src).unwrap(), before);
    }

    #[test]
    fn resize_modes() {
        let img = DynamicImage::new_rgb8(400, 200);
        let mut j = job();
        j.resize = "fit".into();
        assert_eq!(resize(img.clone(), &j).dimensions_tuple(), (100, 50));
        j.resize = "cover".into();
        assert_eq!(resize(img.clone(), &j).dimensions_tuple(), (100, 100));
        j.resize = "exact".into();
        j.width = 30;
        j.height = 70;
        assert_eq!(resize(img.clone(), &j).dimensions_tuple(), (30, 70));
        j.resize = "percent".into();
        assert_eq!(resize(img.clone(), &j).dimensions_tuple(), (200, 100));
        // 작은 이미지는 fit으로 확대하지 않음
        j.resize = "fit".into();
        j.width = 1000;
        j.height = 1000;
        assert_eq!(resize(img, &j).dimensions_tuple(), (400, 200));
    }

    #[test]
    fn jpg_size_limit_lowers_quality() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("big.png");
        sample(&src, 1200, 900);
        let mut j = job();
        j.max_kb = 60;
        let r = process_one(&src, &d.path().join("out"), &j);
        assert!(r.error.is_none());
        assert!(r.after_bytes <= 60 * 1024 || r.note.as_deref().unwrap_or("").contains("넘습니다"));
    }

    #[test]
    fn name_collision_uses_new_name() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("c.png");
        sample(&src, 50, 50);
        let out = d.path().join("out");
        let a = process_one(&src, &out, &job());
        let b = process_one(&src, &out, &job());
        assert_ne!(a.output, b.output);
    }

    trait Dim {
        fn dimensions_tuple(&self) -> (u32, u32);
    }
    impl Dim for DynamicImage {
        fn dimensions_tuple(&self) -> (u32, u32) {
            (self.width(), self.height())
        }
    }
}
