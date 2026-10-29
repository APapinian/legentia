//! Seçili metni DeepL ile çevirme (kullanıcının kendi DeepL API anahtarıyla).
//! Anahtar Araçlar'dan girilir ve veri klasöründe `deepl.json`'da tutulur;
//! kütüphane klasörüne yazılan yedeğe girmez. Anahtar yoksa seçim çubuğunda
//! "Çevir" görünmez. Hedef dil arayüz dilidir (Türkçe → TR, English →
//! EN-US); kaynak dili DeepL bulur. Ücretsiz anahtarlar (`:fx` ile biten)
//! api-free.deepl.com'a gider.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::form::{Field, Form, FormKind};
use super::jobs::Job;
use super::lookup::OnlineSearch;
use super::{App, View};
use crate::dict::{Article, Definition, Markup};
use crate::event::Refresh;
use crate::i18n::{Language, Msg, tr};
use crate::net::Net;

pub(super) const DEEPL_FILE: &str = "deepl.json";
/// Bir seferde çevrilecek en çok karakter (DeepL isteği 128 KiB'a kadar).
const MAX_CHARS: usize = 5000;
const ANSWER_LIMIT: u64 = 1 << 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct DeeplKey {
    pub key: String,
}

/// Anahtarın gideceği sunucu: ücretsiz anahtarlar `:fx` ile biter.
fn base_for(key: &str) -> &'static str {
    if key.trim().ends_with(":fx") {
        "https://api-free.deepl.com"
    } else {
        "https://api.deepl.com"
    }
}

fn target_lang(language: Language) -> &'static str {
    match language {
        Language::Turkish => "TR",
        Language::English => "EN-US",
    }
}

fn request_body(text: &str, target: &str) -> String {
    serde_json::json!({ "text": [text], "target_lang": target }).to_string()
}

/// Yanıttaki çeviri ve bulunan kaynak dil.
fn parse_answer(json: &[u8]) -> Option<(String, String)> {
    let v: Value = serde_json::from_slice(json).ok()?;
    let t = v.pointer("/translations/0")?;
    let text = t.get("text")?.as_str()?.to_string();
    let lang = t
        .get("detected_source_language")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((text, lang))
}

/// Ayrı iş parçacığında: çeviri isteği.
fn translate(
    net: &Net,
    base: &str,
    key: &str,
    text: &str,
    target: &str,
) -> Result<Option<Definition>, Msg> {
    let headers = [
        ("Authorization", format!("DeepL-Auth-Key {}", key.trim())),
        ("Content-Type", "application/json".to_string()),
    ];
    let body = request_body(text, target).into_bytes();
    let (status, answer) = net
        .send(
            "POST",
            &format!("{base}/v2/translate"),
            &headers,
            body,
            ANSWER_LIMIT,
        )
        .map_err(|e| {
            log::warn!("deepl: {e:?}");
            e.message()
        })?;
    match status {
        200..=299 => {}
        401 | 403 => return Err(Msg::DeeplBadKey),
        456 => return Err(Msg::DeeplQuota),
        code => return Err(Msg::NetStatus(code)),
    }
    let (translation, lang) = parse_answer(&answer).ok_or(Msg::NetInvalid)?;
    Ok(Some(Definition {
        form: text.to_string(),
        articles: vec![(
            tr(&Msg::SourceDeepl(lang)),
            Article {
                headword: String::new(),
                parts: vec![(Markup::Plain, translation)],
            },
        )],
    }))
}

impl App {
    pub(super) fn can_translate(&self) -> bool {
        self.deepl_key.is_some()
    }

    /// Seçili metni çevirir (arka planda; sonuç tanım ekranında).
    pub(super) fn translate_selection(&mut self, text: &str) -> Option<Refresh> {
        let text: String = text.trim().chars().take(MAX_CHARS).collect();
        if !self.settings.network {
            self.show_message(Msg::NetworkOff);
            return Some(self.refresh(true));
        }
        let key = self.deepl_key.clone()?;
        let base = self
            .endpoints
            .deepl
            .clone()
            .unwrap_or_else(|| base_for(&key).to_string());
        let target = target_lang(self.settings.language);
        let net = self.browser.net();
        let t = text.clone();
        let job = Job::spawn(move |_| translate(&net, &base, &key, &t, target));
        self.online = Some(OnlineSearch::translation(text, job));
        self.view = View::Searching;
        Some(self.refresh(true))
    }

    pub(super) fn deepl_button(&self) -> super::ui::Rect {
        let u = self.upload_button();
        super::ui::Rect {
            y: u.y + u.h + 40,
            ..u
        }
    }

    pub(super) fn open_deepl_form(&mut self) -> Option<Refresh> {
        let mut field = Field::new(Msg::FieldDeeplKey, "");
        field.secret = true;
        self.open_form(Form::new(FormKind::DeeplKey, vec![field]))
    }

    /// Formdaki anahtarı kaydeder; boşsa siler.
    pub(super) fn save_deepl_key(&mut self, key: &str) {
        let key = key.trim();
        let file = self.store.dir().join(DEEPL_FILE);
        if key.is_empty() {
            self.deepl_key = None;
            let _ = std::fs::remove_file(file);
        } else {
            self.deepl_key = Some(key.to_string());
            self.store.save(
                DEEPL_FILE,
                &DeeplKey {
                    key: key.to_string(),
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_keys_go_to_the_free_host_and_answers_parse() {
        assert_eq!(base_for("abc:fx"), "https://api-free.deepl.com");
        assert_eq!(base_for("abc"), "https://api.deepl.com");
        let body: Value = serde_json::from_str(&request_body("Ağaç", "EN-US")).unwrap();
        assert_eq!(body["text"][0], "Ağaç");
        assert_eq!(body["target_lang"], "EN-US");
        let answer = br#"{"translations":[{"detected_source_language":"TR","text":"Tree"}]}"#;
        assert_eq!(parse_answer(answer), Some(("Tree".into(), "TR".into())));
        assert_eq!(parse_answer(b"{}"), None);
    }
}
