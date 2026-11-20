//! OCR: Windows에 내장된 OCR 엔진(Windows.Media.Ocr)으로 로컬에서 글자를 읽는다. 무료·오프라인.
//! 사용자 프로필 언어(예: 한국어+영어)로 인식하며, 해당 언어팩의 OCR 구성 요소가 필요하다.
//! macOS는 아직 지원하지 않는다(Vision 프레임워크 연동 예정).

use crate::error::{AppError, AppResult};

/// 화면 캡처의 작은 글씨(12~14px)는 OCR 정확도가 낮다. 긴 변이 2000px 이하이면 2배로 키워 인식한다.
pub fn prepare_for_ocr(png: &[u8]) -> Vec<u8> {
    let Ok(img) = image::load_from_memory(png) else { return png.to_vec() };
    if img.width().max(img.height()) > 2000 {
        return png.to_vec();
    }
    let big = img.resize(img.width() * 2, img.height() * 2, image::imageops::FilterType::CatmullRom);
    let mut out = std::io::Cursor::new(Vec::new());
    match big.write_to(&mut out, image::ImageFormat::Png) {
        Ok(()) => out.into_inner(),
        Err(_) => png.to_vec(),
    }
}

#[cfg(windows)]
pub fn recognize_png(png: &[u8]) -> AppResult<String> {
    // WinRT 호출은 별도 스레드에서 MTA로 초기화해 실행한다.
    let data = prepare_for_ocr(png);
    std::thread::spawn(move || -> AppResult<String> {
        use windows::Graphics::Imaging::BitmapDecoder;
        use windows::Media::Ocr::OcrEngine;
        use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
        use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
        let err = |e: windows::core::Error| AppError::new("OCR_FAILED", format!("OCR 실패: {e}"));
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }
        let stream = InMemoryRandomAccessStream::new().map_err(err)?;
        let writer = DataWriter::CreateDataWriter(&stream).map_err(err)?;
        writer.WriteBytes(&data).map_err(err)?;
        writer.StoreAsync().map_err(err)?.join().map_err(err)?;
        writer.FlushAsync().map_err(err)?.join().map_err(err)?;
        writer.DetachStream().map_err(err)?;
        stream.Seek(0).map_err(err)?;
        let decoder = BitmapDecoder::CreateAsync(&stream).map_err(err)?.join().map_err(err)?;
        let bitmap = decoder.GetSoftwareBitmapAsync().map_err(err)?.join().map_err(err)?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()
            .map_err(|_| AppError::new("OCR_UNAVAILABLE", "OCR 엔진을 만들 수 없습니다. Windows 설정 → 언어에서 한국어 언어팩(광학 문자 인식)을 설치하세요"))?;
        let result = engine.RecognizeAsync(&bitmap).map_err(err)?.join().map_err(err)?;
        let mut out = Vec::new();
        for line in result.Lines().map_err(err)? {
            let text = line.Text().map_err(err)?.to_string();
            out.push(join_korean(&text));
        }
        Ok(out.join("\n"))
    })
    .join()
    .map_err(|_| AppError::new("OCR_FAILED", "OCR 처리 중 오류가 발생했습니다"))?
}

#[cfg(not(windows))]
pub fn recognize_png(_png: &[u8]) -> AppResult<String> {
    Err(AppError::new("OCR_UNSUPPORTED", "이 OS에서는 아직 OCR을 지원하지 않습니다(Windows 전용)"))
}

/// Windows OCR은 한글 글자 사이에 공백을 넣는 경우가 있다. 한글-한글 사이의 단일 공백은
/// 원래 띄어쓰기와 구분할 수 없으므로 그대로 두되, 글자마다 공백이 들어간 경우(한 글자 토큰이 연속)만 붙인다.
pub fn join_korean(line: &str) -> String {
    let tokens: Vec<&str> = line.split(' ').collect();
    let is_hangul_char = |t: &str| t.chars().count() == 1 && t.chars().all(|c| ('가'..='힣').contains(&c));
    let singles = tokens.iter().filter(|t| is_hangul_char(t)).count();
    if tokens.len() >= 3 && singles * 10 >= tokens.len() * 7 {
        tokens.concat()
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_images_are_upscaled_for_ocr() {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(300, 100).write_to(&mut buf, image::ImageFormat::Png).unwrap();
        let out = image::load_from_memory(&prepare_for_ocr(buf.get_ref())).unwrap();
        assert_eq!((out.width(), out.height()), (600, 200));
        let mut big = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2400, 100).write_to(&mut big, image::ImageFormat::Png).unwrap();
        assert_eq!(image::load_from_memory(&prepare_for_ocr(big.get_ref())).unwrap().width(), 2400);
    }

    #[test]
    fn joins_spaced_hangul_only_when_obviously_split() {
        assert_eq!(join_korean("안 녕 하 세 요"), "안녕하세요");
        assert_eq!(join_korean("오늘 회의 자료"), "오늘 회의 자료");
        assert_eq!(join_korean("Hello world"), "Hello world");
    }
}
