//! Ekran klavyeli form: katalog ekleme/düzenleme ve katalogda arama. Alana
//! dokunulunca klavye o alana yazar; imleç alanın sonundadır.

use super::keyboard::{KEYBOARD_TOP, Keyboard};
use super::ui::{BOLD, HEADER_BOTTOM, MARGIN, PLAIN, ROW_PX, Rect, draw_button, draw_header};
use super::{App, View};
use crate::canvas::Canvas;
use crate::event::{Event, Refresh};
use crate::i18n::{Msg, tr};
use crate::opds::Search;
use crate::text::Family;

/// Bir alanın yüksekliği (etiket + kutu) ve kutunun yüksekliği.
const FIELD_H: i32 = 150;
const BOX_H: i32 = 80;
const VALUE_PX: f32 = 38.0;
const BUTTON_H: i32 = 100;
const BUTTON_GAP: i32 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum FormKind {
    NewCatalog,
    /// `catalogs` içindeki sırası.
    EditCatalog(usize),
    Search(Search),
    /// Açık kitapta metin araması.
    BookSearch,
    /// Kütüphanede başlık/yazar araması.
    LibrarySearch,
    /// Vurgu ve notlarda arama.
    AnnotationSearch,
    /// Vurgunun (`state.annotations` sırası) etiketi.
    AnnotationTag(usize),
    /// Kitabın (`library` sırası) başlığı, yazarları ve serisi.
    BookInfo(usize),
    /// DeepL API anahtarı.
    DeeplKey,
    /// WebDAV hesabı: adres, kullanıcı, şifre.
    WebDav,
    /// Yeni haber beslemesi.
    NewFeed,
    /// Beslemenin (`news.feeds` sırası) adresi.
    EditFeed(usize),
    /// Wallabag hesabı.
    Wallabag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Field {
    pub label: Msg,
    pub value: String,
    /// Şifre: noktalarla gösterilir.
    pub secret: bool,
}

impl Field {
    pub fn new(label: Msg, value: &str) -> Field {
        Field {
            label,
            value: value.to_string(),
            secret: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Button {
    Cancel,
    Delete,
    Submit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Form {
    pub kind: FormKind,
    pub fields: Vec<Field>,
    /// Klavyenin yazdığı alan.
    pub focus: usize,
    pub keyboard: Keyboard,
    /// Gönderilemediyse nedeni (ör. geçersiz adres).
    pub error: Option<Msg>,
}

impl Form {
    pub fn new(kind: FormKind, fields: Vec<Field>) -> Form {
        Form {
            kind,
            fields,
            focus: 0,
            keyboard: Keyboard::default(),
            error: None,
        }
    }

    fn title(&self) -> Msg {
        match self.kind {
            FormKind::NewCatalog => Msg::NewCatalog,
            FormKind::EditCatalog(_) => Msg::EditCatalog,
            FormKind::Search(_) => Msg::SearchCatalog,
            FormKind::BookSearch => Msg::SearchBook,
            FormKind::LibrarySearch => Msg::SearchLibrary,
            FormKind::AnnotationSearch => Msg::SearchAnnotations,
            FormKind::AnnotationTag(_) => Msg::TagTitle,
            FormKind::BookInfo(_) => Msg::EditBookInfo,
            FormKind::DeeplKey => Msg::DeeplKeyButton,
            FormKind::WebDav => Msg::DavAccount,
            FormKind::NewFeed => Msg::NewFeed,
            FormKind::EditFeed(_) => Msg::EditFeed,
            FormKind::Wallabag => Msg::WallabagAccount,
        }
    }

    pub fn buttons(&self) -> Vec<Button> {
        match self.kind {
            FormKind::EditCatalog(_) | FormKind::EditFeed(_) => {
                vec![Button::Cancel, Button::Delete, Button::Submit]
            }
            _ => vec![Button::Cancel, Button::Submit],
        }
    }

    fn button_label(&self, b: Button) -> Msg {
        match (b, &self.kind) {
            (Button::Cancel, _) => Msg::Cancel,
            (Button::Delete, _) => Msg::Delete,
            (
                Button::Submit,
                FormKind::Search(_)
                | FormKind::BookSearch
                | FormKind::LibrarySearch
                | FormKind::AnnotationSearch,
            ) => Msg::Search,
            (Button::Submit, _) => Msg::Save,
        }
    }
}

/// `i`. alanın dokunma alanı (etiketiyle birlikte) ve yazı kutusu.
pub(super) fn field_rects(width: u32, i: usize) -> (Rect, Rect) {
    let y = HEADER_BOTTOM + i as i32 * FIELD_H;
    let slot = Rect {
        x: 0,
        y,
        w: width as i32,
        h: FIELD_H,
    };
    let value = Rect {
        x: MARGIN,
        y: y + FIELD_H - BOX_H - 15,
        w: width as i32 - 2 * MARGIN,
        h: BOX_H,
    };
    (slot, value)
}

/// Klavyenin üstündeki düğme sırasının `i`. düğmesi (`n` düğmeden).
pub(super) fn button_rect(width: u32, n: usize, i: usize) -> Rect {
    let n = n as i32;
    let w = (width as i32 - 2 * MARGIN - (n - 1) * BUTTON_GAP) / n;
    Rect {
        x: MARGIN + i as i32 * (w + BUTTON_GAP),
        y: KEYBOARD_TOP - BUTTON_H - BUTTON_GAP,
        w,
        h: BUTTON_H,
    }
}

impl App {
    pub(super) fn open_form(&mut self, form: Form) -> Option<Refresh> {
        self.view = View::Form(Box::new(form));
        Some(self.refresh(true))
    }

    pub(super) fn handle_form(&mut self, mut form: Form, event: Event) -> Option<Refresh> {
        let Event::Tap { x, y } = event else {
            return None;
        };
        let buttons = form.buttons();
        if let Some(i) =
            (0..buttons.len()).find(|&i| button_rect(self.width, buttons.len(), i).contains(x, y))
        {
            return self.form_button(form, buttons[i]);
        }
        if let Some(i) =
            (0..form.fields.len()).find(|&i| field_rects(self.width, i).0.contains(x, y))
        {
            form.focus = i;
            form.keyboard = Keyboard::default();
        } else {
            let key = form.keyboard.key_at(self.width, x, y)?;
            let field = form.fields.get_mut(form.focus)?;
            form.keyboard.press(key, &mut field.value);
        }
        self.view = View::Form(Box::new(form));
        Some(self.refresh(false))
    }

    pub(super) fn draw_form(&self, canvas: &mut Canvas, form: &Form) {
        let t = &self.text;
        draw_header(t, canvas, &tr(&form.title()), &tr(&Msg::FormHint), false);
        for (i, field) in form.fields.iter().enumerate() {
            let (slot, r) = field_rects(canvas.width(), i);
            t.draw_msg(canvas, &field.label, MARGIN, slot.y + 45, 30.0);
            let focused = i == form.focus;
            canvas.stroke_rect(
                r.x,
                r.y,
                r.w as u32,
                r.h as u32,
                if focused { 5 } else { 2 },
            );
            let shown = if field.secret {
                "•".repeat(field.value.chars().count())
            } else {
                field.value.clone()
            };
            // Sığmayan uzun değerin sonu görünür (imleç sondadır).
            let max_w = (r.w - 40) as f32;
            let mut chars: Vec<char> = shown.chars().collect();
            let mut text: String = chars.iter().collect();
            let mut cut = false;
            while !chars.is_empty() && t.width_plain(&text, Family::Ui, PLAIN, VALUE_PX) > max_w {
                chars.remove(0);
                cut = true;
                text = std::iter::once('…').chain(chars.iter().copied()).collect();
            }
            if cut && chars.is_empty() {
                text.clear();
            }
            let baseline = r.y + r.h / 2 + (VALUE_PX * 0.35) as i32;
            t.draw_plain(
                canvas,
                &text,
                Family::Ui,
                PLAIN,
                r.x + 20,
                baseline,
                VALUE_PX,
            );
            if focused {
                let x = r.x + 20 + t.width_plain(&text, Family::Ui, PLAIN, VALUE_PX) as i32 + 4;
                canvas.fill_rect(x, r.y + 15, 3, (r.h - 30) as u32, 0);
            }
        }
        if let Some(err) = &form.error {
            let y = HEADER_BOTTOM + form.fields.len() as i32 * FIELD_H + 50;
            let max_w = (canvas.width() as i32 - 2 * MARGIN) as f32;
            for (i, line) in t
                .wrap_plain(&tr(err), Family::Ui, BOLD, ROW_PX, max_w)
                .iter()
                .enumerate()
            {
                t.draw_plain(
                    canvas,
                    line,
                    Family::Ui,
                    BOLD,
                    MARGIN,
                    y + i as i32 * 50,
                    ROW_PX,
                );
            }
        }
        let buttons = form.buttons();
        for (i, b) in buttons.iter().enumerate() {
            let r = button_rect(canvas.width(), buttons.len(), i);
            draw_button(t, canvas, r, &tr(&form.button_label(*b)), ROW_PX);
        }
        form.keyboard.draw(t, canvas);
    }
}
