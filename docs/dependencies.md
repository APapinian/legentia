# Bağımlılıklar

Doğrudan kullanılan crate'ler. Lisanslar `cargo metadata` ile alınmıştır (2026-10-27). Tüm dolaylı bağımlılıklar da izin verici lisanslıdır (MIT, Apache-2.0, BSD-3-Clause, Zlib, Unlicense, Unicode-3.0, ISC; webpki-roots'taki sertifika verisi CDLA-Permissive-2.0); GPL zorunluluğu getiren yok ve hepsi projenin MIT lisansıyla uyumlu.

| Crate | Sürüm | Lisans | Nerede | Neden | ARMv7 musl |
|---|---|---|---|---|---|
| zip | 9.1.2 | MIT | core | EPUB paketini açmak. Yalnızca `deflate-flate2-zlib-rs` (saf Rust) etkin. | Testler qemu'da geçiyor |
| flate2 | 1.1.10 | MIT OR Apache-2.0 | core | Sözlüklerde dictzip (`.dict.dz`) ve gzip (`.idx.gz`) açmak. Yalnızca `zlib-rs` (saf Rust) arka ucu; zip ile aynı. | Derleniyor |
| ripemd | 0.1.3 | MIT OR Apache-2.0 | core | MDict anahtar dizini karıştırmasını ("Encrypted=2") çözmek için RIPEMD-128 (RustCrypto, saf Rust). | Derleniyor |
| ureq | 3.5.0 | MIT OR Apache-2.0 | core | HTTP(S) istemcisi (eşzamanlı, küçük). Yalnızca `rustls` özelliği. | Testler qemu'da geçiyor |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | core (ureq üzerinden) | TLS. Sertifika doğrulaması rustls-webpki 0.103 (ISC) ile; kapatılmaz. | Testler qemu'da geçiyor; gerçek HTTPS qemu'da denendi |
| ring | 0.17.14 | Apache-2.0 AND ISC | core (rustls üzerinden) | TLS şifrelemesi. C/assembly içerir: ARMv7 için flake'teki clang ile derlenir (libc gerekmez). Saf Rust seçeneği (rustls-rustcrypto) 2024'ten beri bakımsız alfa olduğu için seçilmedi. | Derleniyor, qemu'da çalışıyor |
| webpki-roots | 1.0.11 | CDLA-Permissive-2.0 | core (ureq üzerinden) | Mozilla'nın kök sertifikaları, uygulamaya gömülü; cihazın eski sertifika paketine bağlı kalınmaz. Güncel kalması için bağımlılık düzenli güncellenmeli. | Derleniyor |
| quick-xml | 0.42.0 | MIT | core | OPF, NCX, nav ve XHTML okumak. `escape-html` ile HTML varlıkları (`&nbsp;` vb.). | Testler qemu'da geçiyor |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | core | Kitap metnini NFC'ye çevirmek (ayrışık "İ", "Ş"). | Testler qemu'da geçiyor |
| rustybuzz | 0.20.1 | MIT | core | Harf birleştirme (shaping), kerning, bitişik harfler, dile özgü kurallar (`tr`). | Testler qemu'da geçiyor |
| swash | 0.2.10 | Apache-2.0 OR MIT | core | Glif çizimi (hinting açık), font ölçüleri. | Testler qemu'da geçiyor |
| unicode-linebreak | 0.1.5 | Apache-2.0 | core | Unicode satır kırma noktaları (UAX #14). | Testler qemu'da geçiyor |
| hayro | 0.8.0 | Apache-2.0 OR MIT | core | PDF ayrıştırma ve çizme (saf Rust; JPEG 2000, JBIG2, CCITT çözücüleri dahil). Bozuk girdide paniğe karşı sarılı. | Testler qemu'da geçiyor |
| serde, serde_json | 1.0 | MIT OR Apache-2.0 | core | Ayar ve okuma kayıtları (JSON). | Testler qemu'da geçiyor |
| image | 0.25.10 | MIT OR Apache-2.0 | core | EPUB görsellerini (JPEG, PNG) çözüp ölçeklemek. Yalnızca `png` ve `jpeg` özellikleri; hayro aynı sürümü kullandığı için yeni crate eklenmedi (JPEG çözücü zune-jpeg, PNG çözücü png). | Derleniyor |
| md-5 | 0.11.0 | MIT OR Apache-2.0 | core | Calibre'nin istediği Digest yetkilendirmesi için MD5 (RustCrypto, saf Rust). lopdf de aynı sürümü kullandığı için yeni kod eklenmedi. | Testler qemu'da geçiyor |
| nix | 0.31.3 | MIT | core, kobo | core: indirmeden önce diskteki boş alanı ölçmek (`statvfs`, `fs` özelliği; bizim kodda `unsafe` yok). kobo: framebuffer ve E Ink yenilemesi ioctl'leri (`ioctl` özelliği); `unsafe` yalnızca `crates/kobo/src/fb.rs`'deki dört çağrıda. | Testler qemu'da geçiyor |
| log | 0.4.34 | MIT OR Apache-2.0 | core, sim | Teknik log. | Derleniyor |
| minifb | 0.29.0 | MIT OR Apache-2.0 | sim | Masaüstü penceresi (yalnızca X11 arka ucu). Cihazda kullanılmaz. | Gerekmiyor |
| env_logger | 0.11.11 | MIT OR Apache-2.0 | sim, kobo | Log çıktısı (cihazda programın yanındaki `legentia.log` dosyasına). | Derleniyor |

Türkçe heceleme kendi kodumuzdur (`crates/core/src/text/hyphen.rs`, TDK kuralları). Diğer diller (İngilizce, Almanca, Fransızca, İspanyolca, İtalyanca) için TeX hyph-utf8 desenleri değiştirilmeden gömülüdür (`crates/core/hyph/`, ~350 KB; Almanca 272 KB); Liang algoritması kendi kodumuz (`text/liang.rs`). Desen lisansları (MIT ve İngilizce için telifsiz kullanım izni) `crates/core/hyph/LISANSLAR.md`'de; yalnızca LPPL'li Rusça alınmadı. Türkçe kök bulma da kendi kodumuzdur (`crates/core/src/dict/stem.rs`). Uygulamayla sözlük verisi dağıtılmaz; kullanıcının kendi sözlük dosyaları okunur.

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
