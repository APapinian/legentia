//! PDF'e yazma (lopdf, saf Rust): taranmış sayfalara görünmez OCR metin katmanı
//! (aranabilir PDF) ve vurgu/not açıklamaları.
//!
//! Orijinal dosyaya dokunulmaz: çıktı önce geçici dosyaya yazılır, yeniden açılıp
//! doğrulanır, sonra yerine taşınır. Girdiyle aynı yola yazmak reddedilir.
//!
//! Görünmez katman (docs/ocr-deney.md, bulgu 4): Tesseract'ın kendi PDF'inde
//! kelimeler geri okunurken bölünüyordu. Burada her kelime tek parça yazılır;
//! font genişlikleri bizim belirlediğimiz sabit değerdir (`/Widths`), böylece
//! kelime OCR kutusuna tam sığdırılır (`Tz`) ve kelime sonuna gerçek boşluk
//! karakteri eklenir. Metin eğik sayfada da yatay ve satır başına ortak taban
//! çizgisinde yazılır: eğik yazılınca pdftotext satırı ve kelimeleri bölüyordu.

use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use serde::{Deserialize, Serialize};

use super::{Area, PdfDoc, PdfError};
use crate::ocr::{self, OcrWord};

/// Katman fontunun kaynak adı.
const FONT_NAME: &str = "FLegOcr";
/// Her glifin genişliği (glif uzayı, em = 1000).
const GLYPH_WIDTH: f32 = 500.0;

/// WinAnsi'de olmayan karakterler için 128–140 kodları (Differences ile).
const EXTRA: [(u8, char, &str); 13] = [
    (128, 'Ğ', "Gbreve"),
    (129, 'ğ', "gbreve"),
    (130, 'Ş', "Scedilla"),
    (131, 'ş', "scedilla"),
    (132, 'İ', "Idotaccent"),
    (133, 'ı', "dotlessi"),
    (134, '’', "quoteright"),
    (135, '“', "quotedblleft"),
    (136, '”', "quotedblright"),
    (137, '–', "endash"),
    (138, '—', "emdash"),
    (139, '‘', "quoteleft"),
    (140, '…', "ellipsis"),
];

/// Bir sayfanın OCR sonucu, katman yazmak için gereken her şeyle. Kelime kutuları
/// eğikliği düzeltilmiş görüntünün pikselleridir (`dpi` çözünürlükte); `angle`,
/// düzeltmeden önceki eğiklik.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OcrLayer {
    pub page: usize,
    pub dpi: u32,
    pub angle: f32,
    pub image_width: u32,
    pub image_height: u32,
    pub words: Vec<OcrWord>,
}

impl OcrLayer {
    /// Yeniden akıtma için paragraf ve başlık blokları.
    pub fn blocks(&self) -> Vec<crate::epub::Block> {
        ocr::blocks_of(&self.words)
    }
}

/// Vurgu (isteğe bağlı notla). Alan, görüntülenen sayfa koordinatlarıdır (punto, sol üst).
#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    pub page: usize,
    pub area: Area,
    pub note: Option<String>,
}

pub struct PdfEdit {
    doc: Document,
    view: PdfDoc,
    font: Option<ObjectId>,
}

fn invalid(e: impl std::fmt::Display) -> PdfError {
    PdfError::Invalid(e.to_string())
}

impl PdfEdit {
    pub fn open(path: &Path) -> Result<PdfEdit, PdfError> {
        let bytes = std::fs::read(path).map_err(PdfError::Io)?;
        let doc = Document::load_mem(&bytes).map_err(invalid)?;
        let view = PdfDoc::from_bytes(bytes)?;
        Ok(PdfEdit {
            doc,
            view,
            font: None,
        })
    }

    fn page_id(&self, page: usize) -> Result<ObjectId, PdfError> {
        self.doc
            .get_pages()
            .get(&(page as u32 + 1))
            .copied()
            .ok_or(PdfError::PageMissing(page))
    }

    /// Sayfaya görünmez metin katmanı ekler.
    pub fn add_text_layer(&mut self, layer: &OcrLayer) -> Result<(), PdfError> {
        let page_id = self.page_id(layer.page)?;
        let font = self.layer_font();
        self.add_font_resource(page_id, font)?;

        let px_to_pt = 72.0 / layer.dpi as f32;
        // Düzeltilmiş görüntüdeki nokta → eğik asıl sayfadaki yeri (punto, sol üst).
        let to_page = |x: f32, y: f32| {
            let (sx, sy) =
                ocr::source_point(layer.image_width, layer.image_height, -layer.angle, x, y);
            (sx * px_to_pt, sy * px_to_pt)
        };
        let mut ops = String::from("BT 3 Tr\n");
        let words = &layer.words;
        let mut i = 0;
        while i < words.len() {
            // Bir satırın kelimeleri: aynı blok, paragraf ve satır numarası.
            let key = (words[i].block, words[i].paragraph, words[i].line);
            let end = words[i..]
                .iter()
                .position(|w| (w.block, w.paragraph, w.line) != key)
                .map_or(words.len(), |n| i + n);
            let line = &words[i..end];
            i = end;
            // Metin yatay yazılır, satırın kelimeleri ortak bir taban çizgisinde:
            // eğik yazılınca metin çıkaran araçlar (pdftotext) satırı ve kelimeyi bölüyor.
            let ends: Vec<((f32, f32), (f32, f32))> = line
                .iter()
                .map(|w| {
                    let base = (w.top + w.height) as f32 - 0.2 * w.height as f32;
                    (
                        to_page(w.left as f32, base),
                        to_page((w.left + w.width) as f32, base),
                    )
                })
                .collect();
            let baseline =
                ends.iter().map(|(l, r)| (l.1 + r.1) / 2.0).sum::<f32>() / ends.len() as f32;
            for (n, (w, (l, r))) in line.iter().zip(&ends).enumerate() {
                let last = n + 1 == line.len();
                let mut text = w.text.clone();
                if !last {
                    text.push(' ');
                }
                let encoded = encode(&text);
                let (Some((ux0, uy0)), Some((ux1, uy1))) = (
                    self.view.to_user_space(layer.page, l.0, baseline),
                    self.view.to_user_space(layer.page, r.0, baseline),
                ) else {
                    continue;
                };
                let length = ((ux1 - ux0).powi(2) + (uy1 - uy0).powi(2)).sqrt();
                if length <= 0.0 {
                    continue;
                }
                let (cos, sin) = ((ux1 - ux0) / length, (uy1 - uy0) / length);
                let size = (w.height as f32 * px_to_pt).max(1.0);
                // Boşluk hariç kelime kutuya sığsın: boşluk karakteri kutunun dışına taşar.
                let letters = (encoded.len() - usize::from(!last)).max(1) as f32;
                let scale = 100.0 * length / (letters * GLYPH_WIDTH / 1000.0 * size);
                ops.push_str(&format!(
                    "/{FONT_NAME} {size:.2} Tf {scale:.2} Tz {cos:.5} {sin:.5} {:.5} {cos:.5} {ux0:.2} {uy0:.2} Tm ({}) Tj\n",
                    -sin,
                    escape(&encoded)
                ));
            }
        }
        ops.push_str("ET\n");
        self.append_content(page_id, ops.into_bytes())
    }

    /// Sayfaya vurgu açıklaması (ve varsa not) ekler.
    pub fn add_highlight(&mut self, h: &Highlight) -> Result<(), PdfError> {
        let page_id = self.page_id(h.page)?;
        let a = h.area;
        let corner = |x: f32, y: f32| {
            self.view
                .to_user_space(h.page, x, y)
                .ok_or(PdfError::PageMissing(h.page))
        };
        let (ulx, uly) = corner(a.x, a.y)?;
        let (urx, ury) = corner(a.x + a.w, a.y)?;
        let (llx, lly) = corner(a.x, a.y + a.h)?;
        let (lrx, lry) = corner(a.x + a.w, a.y + a.h)?;
        let xs = [ulx, urx, llx, lrx];
        let ys = [uly, ury, lly, lry];
        let (x0, x1) = (
            xs.iter().copied().fold(f32::MAX, f32::min),
            xs.iter().copied().fold(f32::MIN, f32::max),
        );
        let (y0, y1) = (
            ys.iter().copied().fold(f32::MAX, f32::min),
            ys.iter().copied().fold(f32::MIN, f32::max),
        );

        // Görünüm akışı: rengi çarparak (Multiply) koyulaştıran gri dikdörtgen.
        let gs = self.doc.add_object(
            dictionary! { "Type" => "ExtGState", "BM" => "Multiply", "CA" => 1.0, "ca" => 1.0 },
        );
        let appearance = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![x0.into(), y0.into(), x1.into(), y1.into()],
                "Resources" => dictionary! { "ExtGState" => dictionary! { "GS0" => gs } },
            },
            format!(
                "q /GS0 gs 1 0.9 0.3 rg {x0} {y0} {} {} re f Q",
                x1 - x0,
                y1 - y0
            )
            .into_bytes(),
        );
        let ap = self.doc.add_object(appearance);
        let mut annot = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Highlight",
            "Rect" => vec![x0.into(), y0.into(), x1.into(), y1.into()],
            "QuadPoints" => vec![ulx.into(), uly.into(), urx.into(), ury.into(), llx.into(), lly.into(), lrx.into(), lry.into()],
            "C" => vec![1.0.into(), 0.9.into(), 0.3.into()],
            "F" => 4,
            "AP" => dictionary! { "N" => ap },
            "T" => text_string("Legentia"),
        };
        if let Some(note) = &h.note {
            annot.set("Contents", text_string(note));
        }
        let annot_id = self.doc.add_object(annot);
        let annots = {
            let page = self.doc.get_dictionary(page_id).map_err(invalid)?;
            page.get(b"Annots").ok().cloned()
        };
        match annots {
            Some(Object::Reference(id)) => {
                let arr = self
                    .doc
                    .get_object_mut(id)
                    .and_then(Object::as_array_mut)
                    .map_err(invalid)?;
                arr.push(Object::Reference(annot_id));
            }
            Some(Object::Array(mut arr)) => {
                arr.push(Object::Reference(annot_id));
                self.doc
                    .get_dictionary_mut(page_id)
                    .map_err(invalid)?
                    .set("Annots", arr);
            }
            _ => {
                self.doc
                    .get_dictionary_mut(page_id)
                    .map_err(invalid)?
                    .set("Annots", vec![Object::Reference(annot_id)]);
            }
        }
        Ok(())
    }

    /// Çıktıyı geçici dosyaya yazar, yeniden açıp doğrular, sonra yerine taşır.
    pub fn save(mut self, output: &Path, input: &Path) -> Result<(), PdfError> {
        let same = match (std::fs::canonicalize(input), std::fs::canonicalize(output)) {
            (Ok(a), Ok(b)) => a == b,
            _ => input == output,
        };
        if same {
            return Err(PdfError::Invalid(
                "refusing to overwrite the original".into(),
            ));
        }
        let tmp = output.with_extension("pdf.tmp");
        let mut file = std::fs::File::create(&tmp).map_err(PdfError::Io)?;
        self.doc.save_to(&mut file).map_err(invalid)?;
        file.sync_all().map_err(PdfError::Io)?;
        drop(file);
        let check = std::fs::read(&tmp).map_err(PdfError::Io).and_then(|bytes| {
            Document::load_mem(&bytes).map_err(invalid)?;
            PdfDoc::from_bytes(bytes).map(|_| ())
        });
        if let Err(e) = check {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        std::fs::rename(&tmp, output).map_err(PdfError::Io)
    }

    /// Katman fontu (bir kez oluşturulur): standart Helvetica, sabit genişlikler,
    /// Türkçe harfler için Differences ve ToUnicode tablosu.
    fn layer_font(&mut self) -> ObjectId {
        if let Some(id) = self.font {
            return id;
        }
        let mut differences: Vec<Object> = vec![Object::Integer(EXTRA[0].0 as i64)];
        differences.extend(
            EXTRA
                .iter()
                .map(|(_, _, name)| Object::Name(name.as_bytes().to_vec())),
        );
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
             /CMapName /Legentia-OCR def /CMapType 2 def\n\
             1 begincodespacerange <20> <FF> endcodespacerange\n",
        );
        let mapped: Vec<(u8, char)> = (32u8..=255)
            .filter_map(|code| decode(code).map(|c| (code, c)))
            .collect();
        for chunk in mapped.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
            for (code, c) in chunk {
                let mut units = [0u16; 2];
                let hex: String = c
                    .encode_utf16(&mut units)
                    .iter()
                    .map(|u| format!("{u:04X}"))
                    .collect();
                cmap.push_str(&format!("<{code:02X}> <{hex}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap CMapName currentdict /CMap defineresource pop end end\n");
        let to_unicode = self
            .doc
            .add_object(Stream::new(Dictionary::new(), cmap.into_bytes()));
        let id = self.doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "FirstChar" => 32,
            "LastChar" => 255,
            "Widths" => vec![Object::Integer(GLYPH_WIDTH as i64); 224],
            "Encoding" => dictionary! { "Type" => "Encoding", "BaseEncoding" => "WinAnsiEncoding", "Differences" => differences },
            "ToUnicode" => to_unicode,
        });
        self.font = Some(id);
        id
    }

    /// Sayfanın font kaynaklarına katman fontunu ekler. Sayfanın kendi kaynağı
    /// yoksa üst düğümlerden miras aldığı kaynak kopyalanır (miras bozulmasın).
    fn add_font_resource(&mut self, page_id: ObjectId, font: ObjectId) -> Result<(), PdfError> {
        let has_own = self
            .doc
            .get_dictionary(page_id)
            .map_err(invalid)?
            .has(b"Resources");
        if !has_own {
            let inherited = {
                let (direct, refs) = self.doc.get_page_resources(page_id).map_err(invalid)?;
                match (direct, refs.first()) {
                    (Some(d), _) => d.clone(),
                    (None, Some(id)) => self.doc.get_dictionary(*id).map_err(invalid)?.clone(),
                    (None, None) => Dictionary::new(),
                }
            };
            self.doc
                .get_dictionary_mut(page_id)
                .map_err(invalid)?
                .set("Resources", inherited);
        }
        let resources = self
            .doc
            .get_or_create_resources(page_id)
            .and_then(Object::as_dict_mut)
            .map_err(invalid)?;
        let fonts = resources.get(b"Font").ok().cloned();
        match fonts {
            Some(Object::Reference(id)) => {
                self.doc
                    .get_dictionary_mut(id)
                    .map_err(invalid)?
                    .set(FONT_NAME, font);
            }
            Some(Object::Dictionary(mut d)) => {
                d.set(FONT_NAME, font);
                resources.set("Font", d);
            }
            _ => resources.set("Font", dictionary! { FONT_NAME => font }),
        }
        Ok(())
    }

    /// Sayfanın içeriğini `q … Q` ile sarar ve sonuna yeni akışı ekler; mevcut
    /// içeriğin bıraktığı dönüşümler katmanı kaydırmaz.
    fn append_content(&mut self, page_id: ObjectId, content: Vec<u8>) -> Result<(), PdfError> {
        let existing: Vec<Object> = match self
            .doc
            .get_dictionary(page_id)
            .map_err(invalid)?
            .get(b"Contents")
        {
            Ok(Object::Reference(id)) => vec![Object::Reference(*id)],
            Ok(Object::Array(arr)) => arr.clone(),
            _ => Vec::new(),
        };
        let open = self
            .doc
            .add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let mut body = b"\nQ\nq\n".to_vec();
        body.extend_from_slice(&content);
        body.extend_from_slice(b"Q\n");
        let close = self.doc.add_object(Stream::new(Dictionary::new(), body));
        let mut list = vec![Object::Reference(open)];
        list.extend(existing);
        list.push(Object::Reference(close));
        self.doc
            .get_dictionary_mut(page_id)
            .map_err(invalid)?
            .set("Contents", list);
        Ok(())
    }
}

/// Karakteri katman kodlamasına çevirir; kodlanamayan karakter '?' olur.
fn encode(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| match c {
            ' '..='~' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            _ => EXTRA.iter().find(|e| e.1 == c).map_or(b'?', |e| e.0),
        })
        .collect()
}

fn decode(code: u8) -> Option<char> {
    match code {
        32..=126 | 160..=255 => Some(code as char),
        _ => EXTRA.iter().find(|e| e.0 == code).map(|e| e.1),
    }
}

/// PDF metin dizisi için `(`, `)` ve `\` kaçışı; 8 bitlik baytlar sekizlik yazılır.
fn escape(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                out.push('\\');
                out.push(b as char);
            }
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

/// Unicode metin dizisi (UTF-16BE, BOM'lu) — açıklama notları için.
fn text_string(s: &str) -> Object {
    let mut bytes = vec![0xFE, 0xFF];
    for u in s.encode_utf16() {
        bytes.extend_from_slice(&u.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

#[cfg(test)]
mod tests {
    use super::super::testutil::{minimal_pdf, pdf_with_content};
    use super::*;
    use crate::epub::Block;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("legentia-write-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn word(text: &str, left: u32, line: u32) -> OcrWord {
        OcrWord {
            text: text.into(),
            confidence: 95.0,
            left,
            top: 200 + line * 60,
            width: 40 * text.chars().count() as u32,
            height: 40,
            block: 1,
            paragraph: 1,
            line,
        }
    }

    fn text_of(blocks: &[Block]) -> String {
        blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn text_layer_is_extracted_with_turkish_letters_and_real_spaces() {
        let dir = temp_dir("layer");
        let input = dir.join("tarama.pdf");
        std::fs::write(&input, pdf_with_content(&["0 0 1 rg 72 600 200 50 re f"])).unwrap();
        let layer = OcrLayer {
            page: 0,
            dpi: 200,
            angle: 0.0,
            image_width: 1700,
            image_height: 2200,
            words: vec![
                word("Ağaçların", 200, 0),
                word("gölgesinde", 620, 0),
                word("İŞÇİ", 200, 1),
                word("ılık", 400, 1),
            ],
        };
        let mut edit = PdfEdit::open(&input).unwrap();
        edit.add_text_layer(&layer).unwrap();
        let output = dir.join("aranabilir.pdf");
        edit.save(&output, &input).unwrap();

        let doc = PdfDoc::open(&output).unwrap();
        let text = text_of(&doc.page_blocks(0).unwrap());
        assert_eq!(text, "Ağaçların gölgesinde İŞÇİ ılık");
        // Görünmez katman sayfanın görüntüsünü değiştirmemeli.
        let before = PdfDoc::open(&input).unwrap().render(0, 1.0).unwrap();
        let after = doc.render(0, 1.0).unwrap();
        assert_eq!(before.pixels, after.pixels);
    }

    #[test]
    fn skewed_layer_follows_the_page_angle() {
        let dir = temp_dir("skew");
        let input = dir.join("tarama.pdf");
        std::fs::write(&input, pdf_with_content(&["0 0 1 rg 72 600 200 50 re f"])).unwrap();
        let layer = OcrLayer {
            page: 0,
            dpi: 200,
            angle: 3.0,
            image_width: 1700,
            image_height: 2200,
            words: vec![word("eğik", 300, 0), word("satır", 520, 0)],
        };
        let mut edit = PdfEdit::open(&input).unwrap();
        edit.add_text_layer(&layer).unwrap();
        let output = dir.join("aranabilir.pdf");
        edit.save(&output, &input).unwrap();
        // 3° eğik metin hâlâ yatay sayılır (sınır ~5,7°) ve doğru okunur.
        assert_eq!(
            text_of(&PdfDoc::open(&output).unwrap().page_blocks(0).unwrap()),
            "eğik satır"
        );
    }

    #[test]
    fn highlight_note_is_stored_as_an_annotation() {
        let dir = temp_dir("highlight");
        let input = dir.join("belge.pdf");
        std::fs::write(&input, minimal_pdf("Merhaba")).unwrap();
        let mut edit = PdfEdit::open(&input).unwrap();
        let note = "Önemli: ağaç gölgesi";
        edit.add_highlight(&Highlight {
            page: 0,
            area: Area {
                x: 70.0,
                y: 50.0,
                w: 120.0,
                h: 30.0,
            },
            note: Some(note.into()),
        })
        .unwrap();
        let output = dir.join("notlu.pdf");
        edit.save(&output, &input).unwrap();

        let doc = Document::load(&output).unwrap();
        let page = doc
            .get_dictionary(*doc.get_pages().get(&1).unwrap())
            .unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        let annot = doc
            .get_dictionary(annots[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(
            annot.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Highlight"
        );
        let Object::String(bytes, _) = annot.get(b"Contents").unwrap() else {
            panic!("not yok")
        };
        let units: Vec<u16> = bytes[2..]
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), note);
        // Sayfa koordinatı (sol üst) PDF koordinatına (sol alt) çevrilmiş olmalı: 792 - 50 = 742.
        let rect: Vec<f32> = annot
            .get(b"Rect")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o.as_float().unwrap())
            .collect();
        assert!((rect[3] - 742.0).abs() < 0.01, "{rect:?}");
        PdfDoc::open(&output).unwrap().render(0, 1.0).unwrap();
    }

    #[test]
    fn never_overwrites_the_original() {
        let dir = temp_dir("same");
        let input = dir.join("belge.pdf");
        let original = minimal_pdf("x");
        std::fs::write(&input, &original).unwrap();
        let edit = PdfEdit::open(&input).unwrap();
        assert!(edit.save(&input, &input).is_err());
        assert_eq!(std::fs::read(&input).unwrap(), original);
    }

    #[test]
    fn encoding_round_trips_and_escapes() {
        let text = "Ağ(ç)ı\\İ“…”";
        let bytes = encode(text);
        let back: String = bytes.iter().map(|&b| decode(b).unwrap()).collect();
        assert_eq!(back, text);
        assert_eq!(escape(b"(a)\\"), "\\(a\\)\\\\");
        assert_eq!(encode("中"), b"?");
    }
}
