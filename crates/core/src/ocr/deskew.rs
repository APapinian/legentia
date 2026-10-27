//! Eğiklik tespiti ve düzeltme (izdüşüm profili yöntemi).
//!
//! OCR deneyinde kötü taramadaki hatanın asıl nedeni eğiklikti: düzeltmek hatayı
//! %31–69'dan %0'a indirdi (docs/ocr-deney.md).

use crate::pdf::GrayImage;

/// Aranan en büyük eğiklik (derece, iki yönde).
const MAX_ANGLE: f32 = 6.0;
const COARSE_STEP: f32 = 0.25;
const FINE_STEP: f32 = 0.05;
/// Tespit, en uzun kenarı bu kadar piksel olan küçültülmüş görüntüde yapılır.
const PROBE_SIDE: u32 = 1000;
/// Bu değerden koyu pikseller mürekkep sayılır.
const INK: u8 = 140;

/// Görüntüdeki metnin eğikliği (derece, saat yönünün tersi pozitif).
/// `rotate(img, -açı)` metni yataya getirir. Metin yoksa 0.
pub fn estimate_skew(img: &GrayImage) -> f32 {
    let points = ink_points(img);
    if points.len() < 50 {
        return 0.0;
    }
    let score = |deg: f32| projection_score(&points, deg);
    let best = |from: f32, to: f32, step: f32| {
        let mut best = (from, f32::MIN);
        let mut a = from;
        while a <= to + 1e-4 {
            let s = score(a);
            if s > best.1 {
                best = (a, s);
            }
            a += step;
        }
        best.0
    };
    let coarse = best(-MAX_ANGLE, MAX_ANGLE, COARSE_STEP);
    best(coarse - COARSE_STEP, coarse + COARSE_STEP, FINE_STEP)
}

/// Küçültülmüş görüntüdeki mürekkep piksellerinin konumları. Koordinatlar
/// örnekleme adımına bölünür: bölünmezse yalnızca adımın katı olan satırlar dolar
/// ve 0°'de yapay bir tarak deseni en yüksek puanı alır.
fn ink_points(img: &GrayImage) -> Vec<(f32, f32)> {
    let step = (img.width.max(img.height) / PROBE_SIDE).max(1);
    let mut out = Vec::new();
    for y in (0..img.height).step_by(step as usize) {
        for x in (0..img.width).step_by(step as usize) {
            if img.pixels[(y * img.width + x) as usize] < INK {
                out.push(((x / step) as f32, (y / step) as f32));
            }
        }
    }
    out
}

/// Noktaları `deg` açısıyla eğik satırlara izdüşürür; komşu satırlar arasındaki
/// farkların karelerinin toplamı. Satırlar metinle hizalıyken en büyüktür.
fn projection_score(points: &[(f32, f32)], deg: f32) -> f32 {
    let t = deg.to_radians().tan();
    // Görüntü y ekseni aşağı doğru: saat yönünün tersine eğik satırda y, x ile azalır.
    let rows: Vec<i32> = points
        .iter()
        .map(|&(x, y)| (y + x * t).round() as i32)
        .collect();
    let min = rows.iter().copied().min().unwrap_or(0);
    let max = rows.iter().copied().max().unwrap_or(0);
    let mut hist = vec![0f32; (max - min + 1) as usize];
    for r in rows {
        hist[(r - min) as usize] += 1.0;
    }
    hist.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum()
}

/// Görüntüyü merkezi etrafında `deg` derece (saat yönünün tersi pozitif) döndürür;
/// boyut aynı kalır, dışarıda kalan yerler beyaz olur. Çift doğrusal örnekleme.
pub fn rotate(img: &GrayImage, deg: f32) -> GrayImage {
    let (w, h) = (img.width as i64, img.height as i64);
    let (sin, cos) = deg.to_radians().sin_cos();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let sample = |x: i64, y: i64| -> f32 {
        if x < 0 || y < 0 || x >= w || y >= h {
            255.0
        } else {
            img.pixels[(y * w + x) as usize] as f32
        }
    };
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            // Hedef pikselden kaynağa ters dönüşüm (y aşağı doğru).
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let sx = cos * dx - sin * dy + cx - 0.5;
            let sy = sin * dx + cos * dy + cy - 0.5;
            // (source_point ile aynı dönüşüm; piksel merkezi için ±0,5)
            let (x0, y0) = (sx.floor() as i64, sy.floor() as i64);
            let (fx, fy) = (sx - x0 as f32, sy - y0 as f32);
            let top = sample(x0, y0) * (1.0 - fx) + sample(x0 + 1, y0) * fx;
            let bottom = sample(x0, y0 + 1) * (1.0 - fx) + sample(x0 + 1, y0 + 1) * fx;
            pixels.push((top * (1.0 - fy) + bottom * fy).round() as u8);
        }
    }
    GrayImage {
        width: img.width,
        height: img.height,
        pixels,
    }
}

/// `rotate(img, deg)` çıktısındaki (x, y) noktasının girdideki yeri. `rotate` ile
/// aynı ters dönüşüm kullanılır; böylece OCR kutuları asıl (eğik) sayfaya geri taşınır.
pub fn source_point(width: u32, height: u32, deg: f32, x: f32, y: f32) -> (f32, f32) {
    let (sin, cos) = deg.to_radians().sin_cos();
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let (dx, dy) = (x - cx, y - cy);
    (cos * dx - sin * dy + cx, sin * dx + cos * dy + cy)
}

/// Eğikliği bulup düzeltir; düzeltilmiş görüntü ve bulunan açı.
/// 0,1°'den küçük eğiklikte görüntü olduğu gibi kopyalanır.
pub fn deskew(img: &GrayImage) -> (GrayImage, f32) {
    let angle = estimate_skew(img);
    if angle.abs() < 0.1 {
        let copy = GrayImage {
            width: img.width,
            height: img.height,
            pixels: img.pixels.clone(),
        };
        return (copy, angle);
    }
    (rotate(img, -angle), angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Metin sayfası taklidi: kelimeler halinde koyu çubuklardan satırlar.
    fn page() -> GrayImage {
        page_of(1)
    }

    /// `k` katı büyüklükte sayfa; `k` > 1 örnekleme adımını devreye sokar.
    fn page_of(k: u32) -> GrayImage {
        let (w, h) = (900u32 * k, 1200u32 * k);
        let mut pixels = vec![255u8; (w * h) as usize];
        for line in 0..25 {
            let top = (100 + line * 40) * k;
            let mut x = 80 * k;
            let mut word = 0;
            while x < 820 * k {
                let len = (30 + (word * 37 % 50)) * k;
                for y in top..top + 14 * k {
                    for xx in x..(x + len).min(820 * k) {
                        pixels[(y * w + xx) as usize] = 20;
                    }
                }
                x += len + 14 * k;
                word += 1;
            }
        }
        GrayImage {
            width: w,
            height: h,
            pixels,
        }
    }

    #[test]
    fn straight_page_has_no_skew() {
        assert!(estimate_skew(&page()).abs() < 0.15);
    }

    #[test]
    fn finds_and_corrects_known_rotations() {
        for angle in [-4.0f32, -1.2, 2.5, 5.0] {
            let tilted = rotate(&page(), angle);
            let found = estimate_skew(&tilted);
            assert!(
                (found - angle).abs() < 0.2,
                "{angle}° döndürüldü, {found}° bulundu"
            );
            let (fixed, _) = deskew(&tilted);
            assert!(estimate_skew(&fixed).abs() < 0.2);
        }
    }

    #[test]
    fn large_pages_are_sampled_without_bias_towards_zero() {
        // 1800x2400: örnekleme adımı 2. Hata: koordinatlar adıma bölünmeyince hep 0° bulunuyordu.
        let tilted = rotate(&page_of(2), -3.0);
        let found = estimate_skew(&tilted);
        assert!(
            (found + 3.0).abs() < 0.2,
            "-3° döndürüldü, {found}° bulundu"
        );
    }

    #[test]
    fn source_point_undoes_rotation() {
        // Döndürülmüş görüntüdeki koyu piksellerin kaynaktaki yeri de koyu olmalı.
        let img = page();
        let rotated = rotate(&img, 3.0);
        let dark: Vec<(u32, u32)> = (0..rotated.height)
            .flat_map(|y| (0..rotated.width).map(move |x| (x, y)))
            .filter(|&(x, y)| rotated.pixels[(y * rotated.width + x) as usize] < 40)
            .step_by(997)
            .collect();
        assert!(dark.len() > 20);
        for (x, y) in dark {
            let (sx, sy) = source_point(img.width, img.height, 3.0, x as f32 + 0.5, y as f32 + 0.5);
            let p = img.pixels[(sy as u32 * img.width + sx as u32) as usize];
            assert!(p < 128, "({x}, {y}) → ({sx}, {sy}) mürekkep değil");
        }
    }

    #[test]
    fn blank_page_is_left_alone() {
        let blank = GrayImage {
            width: 100,
            height: 100,
            pixels: vec![255; 10_000],
        };
        let (out, angle) = deskew(&blank);
        assert_eq!(angle, 0.0);
        assert_eq!(out.pixels, blank.pixels);
    }

    #[test]
    fn rotation_keeps_size_and_fills_white() {
        let img = rotate(&page(), 10.0);
        assert_eq!((img.width, img.height), (900, 1200));
        assert_eq!(img.pixels[0], 255, "köşe dışarıdan gelir, beyaz olmalı");
    }
}
