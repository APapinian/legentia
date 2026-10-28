//! Vurgular ve notlar: EPUB'da ve PDF'te tek konumlandırma modeli.
//!
//! Vurgu, bölümün (PDF'te sayfanın) metin bloklarında `[start, end)` aralığıdır.
//! Blok/bayt konumu yazı ayarından bağımsızdır; PDF'te sayfa görüntüsü ve
//! yeniden akıtılmış metin aynı blokları kullanır. Alıntı da saklanır: kitap
//! dosyası değişip aralık kayarsa alıntı bölümde yeniden aranır.

use serde::{Deserialize, Serialize};

use super::session::Position;
use crate::epub::Block;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// EPUB'da bölüm sırası, PDF'te sayfa sırası.
    pub spine: usize,
    pub start: Position,
    pub end: Position,
    /// Vurgulanan metin; bloklar arası "\n" ile.
    pub quote: String,
    pub note: Option<String>,
    /// Oluşturulma zamanı (Unix saniyesi).
    pub created: u64,
}

/// Bölümün metni tek dizgede: bloklar "\n" ile ayrılır. Konumla dizin arasında
/// çeviri yapar.
pub(super) struct Flat {
    pub text: String,
    /// Her bloğun `text` içindeki başlangıcı.
    starts: Vec<usize>,
}

impl Flat {
    pub fn new(blocks: &[Block]) -> Self {
        let mut text = String::new();
        let mut starts = Vec::with_capacity(blocks.len());
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 {
                text.push('\n');
            }
            starts.push(text.len());
            text.push_str(&b.text);
        }
        Self { text, starts }
    }

    /// Geçerli bir konumsa (blok var, bayt karakter sınırında) dizini.
    pub fn index(&self, pos: Position) -> Option<usize> {
        let start = *self.starts.get(pos.block)?;
        let end = self
            .starts
            .get(pos.block + 1)
            .map_or(self.text.len(), |s| s - 1);
        let i = start + pos.offset;
        (i <= end && self.text.is_char_boundary(i)).then_some(i)
    }

    pub fn position(&self, index: usize) -> Position {
        let block = self
            .starts
            .partition_point(|&s| s <= index)
            .saturating_sub(1);
        Position {
            block,
            offset: index - self.starts.get(block).copied().unwrap_or(0),
        }
    }
}

/// Vurgunun bu bölümdeki aralığı. Kayıtlı aralıktaki metin alıntıyla uyuşmuyorsa
/// (kitap dosyası değişmiş) alıntının eski yere en yakın geçtiği yer seçilir;
/// hiç geçmiyorsa `None` (vurgu silinmez, yalnızca çizilmez).
pub(super) fn locate(flat: &Flat, a: &Annotation) -> Option<(Position, Position)> {
    let (start, end) = (flat.index(a.start), flat.index(a.end));
    if let (Some(s), Some(e)) = (start, end)
        && flat.text.get(s..e) == Some(a.quote.as_str())
    {
        return Some((a.start, a.end));
    }
    if a.quote.is_empty() {
        return None;
    }
    let was = start.unwrap_or(0);
    let found = flat
        .text
        .match_indices(a.quote.as_str())
        .map(|(i, _)| i)
        .min_by_key(|&i| i.abs_diff(was))?;
    Some((flat.position(found), flat.position(found + a.quote.len())))
}

pub(super) fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::epub::BlockKind;

    fn blocks(texts: &[&str]) -> Vec<Block> {
        texts
            .iter()
            .map(|t| Block {
                kind: BlockKind::Paragraph,
                text: t.to_string(),
                spans: Vec::new(),
                ..Block::default()
            })
            .collect()
    }

    fn pos(block: usize, offset: usize) -> Position {
        Position { block, offset }
    }

    fn ann(start: Position, end: Position, quote: &str) -> Annotation {
        Annotation {
            spine: 0,
            start,
            end,
            quote: quote.into(),
            note: None,
            created: 0,
        }
    }

    #[test]
    fn positions_and_indices_round_trip_across_blocks() {
        let flat = Flat::new(&blocks(&["Ağaç", "gölge"]));
        assert_eq!(flat.text, "Ağaç\ngölge");
        assert_eq!(flat.index(pos(1, 1)), Some(8));
        assert_eq!(flat.position(8), pos(1, 1));
        assert_eq!(
            flat.index(pos(0, 2)),
            None,
            "ğ'nin ortası karakter sınırı değil"
        );
        assert_eq!(flat.index(pos(0, 7)), None, "blok dışı");
        assert_eq!(flat.index(pos(2, 0)), None);
    }

    #[test]
    fn a_matching_range_is_kept() {
        let flat = Flat::new(&blocks(&["Ağaçların gölgesinde", "çay içtik."]));
        let a = ann(pos(0, 13), pos(1, 4), "gölgesinde\nçay");
        assert_eq!(locate(&flat, &a), Some((a.start, a.end)));
    }

    #[test]
    fn a_shifted_quote_is_found_nearest_to_its_old_place() {
        // Eski dosya: ["Ağaçların gölgesinde", "çay içtik."], vurgu ikinci bloğun başında.
        // Yeni dosyada başa paragraf eklendi; "çay" iki kez geçiyor.
        let flat = Flat::new(&blocks(&[
            "Önsöz.",
            "Ağaçların gölgesinde",
            "çay içtik, bir çay daha.",
        ]));
        let a = ann(pos(1, 0), pos(1, 4), "çay");
        assert_eq!(locate(&flat, &a), Some((pos(2, 0), pos(2, 4))));
    }

    #[test]
    fn a_vanished_quote_is_not_placed() {
        let flat = Flat::new(&blocks(&["Bambaşka bir metin."]));
        assert_eq!(locate(&flat, &ann(pos(0, 0), pos(0, 4), "gölge")), None);
    }
}
