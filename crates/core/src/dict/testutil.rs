//! Testler için küçük sözlük dosyaları üretir.

use std::path::{Path, PathBuf};

pub fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "legentia-dict-{}-{tag}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `sametypesequence=m` StarDict sözlüğü; `.ifo` yolunu döner. `syn`: (biçim, madde sırası).
pub fn stardict(
    dir: &Path,
    name: &str,
    words: &[(&str, &str)],
    syn: &[(&str, u32)],
    dictzip: bool,
) -> PathBuf {
    let mut sorted: Vec<_> = words.to_vec();
    sorted.sort_by_key(|(w, _)| w.to_lowercase());
    let mut idx = Vec::new();
    let mut data = Vec::new();
    for (w, def) in &sorted {
        idx.extend_from_slice(w.as_bytes());
        idx.push(0);
        idx.extend_from_slice(&(data.len() as u32).to_be_bytes());
        idx.extend_from_slice(&(def.len() as u32).to_be_bytes());
        data.extend_from_slice(def.as_bytes());
    }
    let ifo = dir.join(format!("{name}.ifo"));
    std::fs::write(
        &ifo,
        format!(
            "StarDict's dict ifo file\nversion=2.4.2\nbookname={name} sözlüğü\nwordcount={}\nidxfilesize={}\nsametypesequence=m\n",
            sorted.len(),
            idx.len()
        ),
    )
    .unwrap();
    std::fs::write(ifo.with_extension("idx"), idx).unwrap();
    if !syn.is_empty() {
        let mut s = Vec::new();
        for (w, i) in syn {
            s.extend_from_slice(w.as_bytes());
            s.push(0);
            s.extend_from_slice(&i.to_be_bytes());
        }
        std::fs::write(ifo.with_extension("syn"), s).unwrap();
    }
    if dictzip {
        std::fs::write(
            ifo.with_extension("dict.dz"),
            super::dictzip::compress(&data, 16),
        )
        .unwrap();
    } else {
        std::fs::write(ifo.with_extension("dict"), data).unwrap();
    }
    ifo
}
