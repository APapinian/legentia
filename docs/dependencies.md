# Bağımlılıklar

Doğrudan kullanılan crate'ler. Lisanslar `cargo metadata` ile alınmıştır (2026-10-27). Tüm dolaylı bağımlılıklar da izin verici lisanslıdır (MIT, Apache-2.0, Zlib, Unlicense, Unicode-3.0); GPL zorunluluğu getiren yok.

| Crate | Sürüm | Lisans | Nerede | Neden | ARMv7 musl |
|---|---|---|---|---|---|
| zip | 9.1.2 | MIT | core | EPUB paketini açmak. Yalnızca `deflate-flate2-zlib-rs` (saf Rust) etkin. | Derleniyor, testler qemu'da geçiyor |
| quick-xml | 0.42.0 | MIT | core | OPF, NCX, nav ve XHTML okumak. `escape-html` ile HTML varlıkları (`&nbsp;` vb.). | Derleniyor, testler qemu'da geçiyor |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | core | Kitap metnini NFC'ye çevirmek (ayrışık "İ", "Ş"). | Derleniyor, testler qemu'da geçiyor |
| fontdue | 0.9.5 | MIT OR Apache-2.0 OR Zlib | core | Geçici glif çizimi. M3'te yerine asıl layout yığını gelecek. | Derleniyor, testler qemu'da geçiyor |
| log | 0.4.34 | MIT OR Apache-2.0 | core, sim | Teknik log. | Derleniyor |
| minifb | 0.29.0 | MIT OR Apache-2.0 | sim | Masaüstü penceresi (yalnızca X11 arka ucu). Cihazda kullanılmaz. | Gerekmiyor |
| env_logger | 0.11.11 | MIT OR Apache-2.0 | sim | Simülatörde log çıktısı. | Gerekmiyor |

Geçici test fontu DejaVu Sans (Bitstream Vera lisansı) Nix'ten gelir; depoya girmez.
