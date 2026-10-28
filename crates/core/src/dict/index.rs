//! Sözlük biçimlerinin ortak dizini: indirgenmiş anahtarlar tek dizgede,
//! (anahtar konumu, uzunluğu, öğe) üçlüleri anahtara göre sıralı. Madde başına
//! 12 bayt + anahtar; 100 bin maddelik sözlük birkaç MB tutar.

pub(crate) struct Index {
    keys: String,
    entries: Vec<(u32, u32, u32)>,
}

impl Index {
    /// (anahtar, öğe) çiftlerinden; anahtarlar [`super::key`] ile indirgenmiş olmalı.
    pub fn build(pairs: impl IntoIterator<Item = (String, u32)>) -> Index {
        let mut keys = String::new();
        let mut entries = Vec::new();
        for (k, item) in pairs {
            entries.push((keys.len() as u32, k.len() as u32, item));
            keys.push_str(&k);
        }
        let key = |e: &(u32, u32, u32)| &keys[e.0 as usize..(e.0 + e.1) as usize];
        entries.sort_by(|a, b| key(a).cmp(key(b)).then(a.2.cmp(&b.2)));
        entries.dedup_by(|a, b| key(a) == key(b) && a.2 == b.2);
        Index { keys, entries }
    }

    fn key(&self, e: &(u32, u32, u32)) -> &str {
        &self.keys[e.0 as usize..(e.0 + e.1) as usize]
    }

    /// Anahtarın öğeleri.
    pub fn find(&self, key: &str) -> Vec<u32> {
        let start = self.entries.partition_point(|e| self.key(e) < key);
        self.entries[start..]
            .iter()
            .take_while(|e| self.key(e) == key)
            .map(|e| e.2)
            .collect()
    }

    pub fn contains(&self, key: &str) -> bool {
        let i = self.entries.partition_point(|e| self.key(e) < key);
        self.entries.get(i).is_some_and(|e| self.key(e) == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_items_of_a_key() {
        let idx = Index::build([
            ("çay".to_string(), 2),
            ("ağaç".to_string(), 0),
            ("çay".to_string(), 1),
            ("çay".to_string(), 1),
        ]);
        assert_eq!(idx.find("çay"), vec![1, 2]);
        assert_eq!(idx.find("ağaç"), vec![0]);
        assert!(idx.find("yok").is_empty());
        assert!(idx.contains("ağaç") && !idx.contains("ağ"));
    }
}
