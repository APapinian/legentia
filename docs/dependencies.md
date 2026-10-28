# Bağımlılıklar

Doğrudan kullanılan crate'ler. Lisanslar `cargo metadata` ile alınmıştır (2026-10-27). Tüm dolaylı bağımlılıklar da izin verici lisanslıdır (MIT, Apache-2.0, BSD-3-Clause, Zlib, Unlicense, Unicode-3.0); GPL zorunluluğu getiren yok ve hepsi projenin MIT lisansıyla uyumlu.

| Crate | Sürüm | Lisans | Nerede | Neden | ARMv7 musl |
|---|---|---|---|---|---|
| zip | 9.1.2 | MIT | core | EPUB paketini açmak. Yalnızca `deflate-flate2-zlib-rs` (saf Rust) etkin. | Testler qemu'da geçiyor |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | core | Sözlüklerde dictzip (`.dict.dz`) ve gzip (`.idx.gz`) açmak. Yalnızca `zlib-rs` (saf Rust) arka ucu; zip ile aynı. | Derleniyor |
| ripemd | 0.1.3 | MIT OR Apache-2.0 | core | MDict anahtar dizini karıştırmasını ("Encrypted=2") çözmek için RIPEMD-128 (RustCrypto, saf Rust). | Derleniyor |
| quick-xml | 0.42.0 | MIT | core | OPF, NCX, nav ve XHTML okumak. `escape-html` ile HTML varlıkları (`&nbsp;` vb.). | Testler qemu'da geçiyor |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | core | Kitap metnini NFC'ye çevirmek (ayrışık "İ", "Ş"). | Testler qemu'da geçiyor |
| rustybuzz | 0.20.1 | MIT | core | Harf birleştirme (shaping), kerning, bitişik harfler, dile özgü kurallar (`tr`). | Testler qemu'da geçiyor |
| swash | 0.2.10 | Apache-2.0 OR MIT | core | Glif çizimi (hinting açık), font ölçüleri. | Testler qemu'da geçiyor |
| unicode-linebreak | 0.1.5 | Apache-2.0 | core | Unicode satır kırma noktaları (UAX #14). | Testler qemu'da geçiyor |
| hayro | 0.8.0 | Apache-2.0 OR MIT | core | PDF ayrıştırma ve çizme (saf Rust; JPEG 2000, JBIG2, CCITT çözücüleri dahil). Bozuk girdide paniğe karşı sarılı. | Testler qemu'da geçiyor |
| serde, serde_json | 1.0 | MIT OR Apache-2.0 | core | Ayar ve okuma kayıtları (JSON). | Testler qemu'da geçiyor |
| log | 0.4.34 | MIT OR Apache-2.0 | core, sim | Teknik log. | Derleniyor |
| minifb | 0.29.0 | MIT OR Apache-2.0 | sim | Masaüstü penceresi (yalnızca X11 arka ucu). Cihazda kullanılmaz. | Gerekmiyor |
| env_logger | 0.11.11 | MIT OR Apache-2.0 | sim | Simülatörde log çıktısı. | Gerekmiyor |

Türkçe heceleme kendi kodumuzdur (`crates/core/src/text/hyphen.rs`, TDK kuralları); dış desen dosyası yoktur. Türkçe kök bulma da kendi kodumuzdur (`crates/core/src/dict/stem.rs`). Uygulamayla sözlük verisi dağıtılmaz; kullanıcının kendi sözlük dosyaları okunur.

## Masaüstü araçları (cihaza girmez)

| Araç | Sürüm | Lisans | Kullanım |
|---|---|---|---|
| Tesseract | 5.5.3 (nixpkgs, `tur` `eng` `osd` dil verileri) | Apache-2.0 | OCR deneyi ve simülatörde "Taranmış sayfaları OCR ile oku"; `LEGENTIA_TESSERACT` ile bulunur. Cihaz için OCR motoru M10'da seçilecek. |

## Fontlar

Nix'ten gelir (`flake.nix` → `LEGENTIA_FONTS`), depoya girmez. Cihaz paketine M10'da eklenecek; OFL, fontların uygulamayla birlikte dağıtılmasına izin verir (lisans metni yanında dağıtılmalı).

| Font | Sürüm | Lisans | Kullanım |
|---|---|---|---|
| Literata | 3.103 | SIL OFL 1.1 | Varsayılan kitap fontu (e-kitap okuma için tasarlandı) |
| Source Serif 4 | 4.005 | SIL OFL 1.1 | Kitap fontu seçeneği |
| Gentium | 7.000 | SIL OFL 1.1 | Kitap fontu seçeneği (geniş Latin/Kiril/Yunan kapsamı) |
| Source Sans 3 | 3.052 | SIL OFL 1.1 | Arayüz fontu |
