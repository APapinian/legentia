//! Vurguların ve notların dışa aktarılması: kitabın yanına Markdown dosyası,
//! PDF'te ayrıca standart açıklamalı (annotation) bir kopya. Orijinal değişmez.

use std::path::Path;

use super::annotations::{self, Annotation, Flat};
use super::select::box_areas;
use super::session::Session;
use super::{App, Back, View};
use crate::i18n::{Msg, tr};
use crate::pdf::{Highlight, PdfEdit};

impl Session {
    /// Bölümün ya da PDF sayfasının okura gösterilen adı: EPUB'da içindekilerdeki
    /// son başlık (bölüm o başlığın altında), yoksa "Bölüm N"; PDF'te "Sayfa N".
    pub(super) fn chapter_title(&self, spine: usize) -> String {
        if self.pdf().is_some() {
            return tr(&Msg::PageNumber(spine + 1));
        }
        self.toc()
            .iter()
            .rfind(|e| e.spine_index <= spine)
            .filter(|e| !e.title.trim().is_empty())
            .map_or_else(|| tr(&Msg::ChapterNumber(spine + 1)), |e| e.title.clone())
    }

    /// Vurguların `state.annotations` sıraları, kitaptaki sırayla.
    pub(super) fn annotations_in_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.state.annotations.len()).collect();
        order.sort_by_key(|&i| {
            let a = &self.state.annotations[i];
            (a.spine, a.start, a.end)
        });
        order
    }
}

/// Kitabın vurguları ve notları Markdown olarak: başlık, her bölüm için alt
/// başlık, alıntılar blok alıntı, notlar altında.
fn markdown(title: &str, items: &[(String, &Annotation)]) -> String {
    let mut out = format!("# {title}\n");
    let mut last = None;
    for (heading, a) in items {
        if last != Some(heading) {
            out.push_str(&format!("\n## {heading}\n"));
            last = Some(heading);
        }
        out.push('\n');
        for line in a.quote.lines() {
            out.push_str(&format!("> {line}\n"));
        }
        if let Some(note) = &a.note {
            out.push_str(&format!("\n{}\n", tr(&Msg::NoteLine(note.clone()))));
        }
        if let Some(tag) = &a.tag {
            out.push_str(&format!("\n{}\n", tr(&Msg::TagLine(tag.clone()))));
        }
    }
    out
}

/// Metni geçici dosyaya yazıp yerine taşır; yarıda kesilirse eski dosya kalır.
fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("md.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

impl App {
    /// Okunan kitabın vurgularını ve notlarını dışa aktarır; sonucu anlatan mesaj döner.
    pub(super) fn export_notes(&mut self) -> Msg {
        let Some(s) = &self.session else {
            return Msg::ExportFailed;
        };
        if s.state.annotations.is_empty() {
            return Msg::NoAnnotations;
        }
        let order = s.annotations_in_order();
        let items: Vec<(String, &Annotation)> = order
            .iter()
            .map(|&i| {
                let a = &s.state.annotations[i];
                (s.chapter_title(a.spine), a)
            })
            .collect();
        let md_name = format!("{} - notlar.md", s.name);
        let md_path = s.path.with_file_name(&md_name);
        if let Err(e) = write_atomic(&md_path, &markdown(&s.title(), &items)) {
            log::error!("cannot write {}: {e}", md_path.display());
            return Msg::ExportFailed;
        }
        let mut files = vec![md_name];
        if s.pdf().is_some() {
            match self.export_pdf_annotations() {
                Ok(name) => files.push(name),
                Err(()) => return Msg::ExportFailed,
            }
        }
        Msg::NotesExported {
            count: order.len(),
            files: files.join(", "),
        }
    }

    /// PDF'in vurgularını standart açıklamalarla "<ad> - notlu.pdf" olarak yazar.
    /// Metin katmanı olmayan (OCR'la okunmuş) sayfalardaki vurgular yazılamaz; atlanır.
    fn export_pdf_annotations(&self) -> Result<String, ()> {
        let s = self.session.as_ref().ok_or(())?;
        let pdf = s.pdf().ok_or(())?;
        let mut edit = PdfEdit::open(&s.path).map_err(|e| {
            log::error!("cannot open {} for writing: {e:?}", s.path.display());
        })?;
        let mut skipped = 0;
        for i in s.annotations_in_order() {
            let a = &s.state.annotations[i];
            let text = pdf.page_text(a.spine).unwrap_or_default();
            let Some((start, end)) = annotations::locate(&Flat::new(&text.blocks), a) else {
                skipped += 1;
                continue;
            };
            let areas = box_areas(&text, start, end);
            if areas.is_empty() {
                skipped += 1;
            }
            // Not yalnızca ilk satırın açıklamasına yazılır; okuyucularda bir kez görünsün.
            for (n, area) in areas.into_iter().enumerate() {
                let h = Highlight {
                    page: a.spine,
                    area,
                    note: a.note.clone().filter(|_| n == 0),
                };
                edit.add_highlight(&h).map_err(|e| {
                    log::error!("cannot add highlight on page {}: {e:?}", a.spine);
                })?;
            }
        }
        if skipped > 0 {
            log::warn!("{skipped} highlights have no text layer position; not written to pdf");
        }
        let name = format!("{} - notlu.pdf", s.name);
        let output = s.path.with_file_name(&name);
        edit.save(&output, &s.path).map_err(|e| {
            log::error!("cannot save {}: {e:?}", output.display());
        })?;
        Ok(name)
    }

    pub(super) fn show_export_result(&mut self) {
        let msg = self.export_notes();
        self.view = View::Message {
            msg,
            back: Back::Reading,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Position;
    use crate::app::annotations::Mark;

    #[test]
    fn markdown_groups_quotes_under_chapters() {
        let ann = |quote: &str, note: Option<&str>| Annotation {
            spine: 0,
            start: Position::default(),
            end: Position::default(),
            quote: quote.into(),
            note: note.map(str::to_string),
            created: 0,
            mark: Mark::Gray,
            tag: None,
        };
        let (a, b, c) = (
            ann("Ağaçların gölgesinde", Some("Güzel bir cümle.")),
            ann("birinci satır\nikinci satır", None),
            ann("son", None),
        );
        let md = markdown(
            "Dar Kapı",
            &[
                ("Birinci Bölüm".into(), &a),
                ("Birinci Bölüm".into(), &b),
                ("İkinci Bölüm".into(), &c),
            ],
        );
        assert_eq!(
            md,
            "# Dar Kapı\n\n## Birinci Bölüm\n\n> Ağaçların gölgesinde\n\nNot: Güzel bir cümle.\n\n> birinci satır\n> ikinci satır\n\n## İkinci Bölüm\n\n> son\n"
        );
    }
}
