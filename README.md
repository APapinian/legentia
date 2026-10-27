# Legentia

Kobo Libra 2 için Rust ile yazılan, tamamen Türkçe arayüzlü e-kitap okuyucu. Şu an M4 tamamlandı: simülatörde kütüphane, içindekiler, okuma, menü, yer imleri ve yazı ayarları; okuma konumu ve ayarlar kalıcı.

Belgeler: `kobo-rust-okuyucu-proje-aktarim.md` (vizyon), `milestone.md` (plan), `dikkat-edilmesi-gerekenler.md` (kurallar), `docs/device-compatibility.md` (cihaz bilgisi), `docs/epub-support.md` (desteklenen EPUB özellikleri), `docs/dependencies.md` (bağımlılıklar, fontlar ve lisansları).

## Yapı

- `crates/core`: platformdan bağımsız çekirdek. Çizim yüzeyi, olaylar, E Ink yenileme türleri, Türkçe metin katalogu, EPUB okuma (`epub/`), metin motoru (`text/`: fontlar, harf birleştirme, satır kırma, Türkçe heceleme), kütüphane taraması, Türkçe sıralama, JSON kayıt (`storage.rs`), uygulama ekranları (`app/`).
- `crates/sim`: Libra 2 ekranını (1264×1680, gri ton) taklit eden masaüstü penceresi.

## Komutlar

Hepsi `nix develop` içinde çalıştırılır (ya da başına `nix develop -c` eklenir).

```sh
cargo run -p legentia-sim -- <klasör | kitap.epub>   # simülatör
cargo fmt --all --check                   # biçim denetimi
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                    # masaüstü testleri

# Bir klasördeki tüm EPUB'ları dene (salt okuma; süre, bellek, hata raporu)
cargo run --release -p legentia-core --example epub_check -- /run/media/$USER/KOBOeReader

# Pencere açmadan ekran görüntüleri (PGM): kütüphane, içindekiler, okuma, menü, ayarlar
cargo run --release -p legentia-core --example screenshots -- <kütüphane> <çıktı klasörü> [kitap sırası]

# Kobo hedefi: statik ARMv7 musl; testler qemu-arm ile çalışır
cargo build -p legentia-core --target armv7-unknown-linux-musleabihf
cargo test  -p legentia-core --target armv7-unknown-linux-musleabihf
```

## Simülatör

| Girdi | Karşılığı |
|---|---|
| Fare tıklaması / dokunmatik ekran | Dokunma: içindekilerde satırı açar; okurken sol üçte bir önceki sayfa, sağ üçte bir sonraki sayfa, orta menü |
| PageUp, Sol ok | Üst sayfa tuşu (geri) |
| PageDown, Sağ ok, Boşluk | Alt sayfa tuşu (ileri) |
| Esc | Çıkış |

Okurken ekranın ortasına dokunmak menüyü açar: içindekiler, yer imi koy/kaldır, yer imleri, yazı ayarları, kütüphane.

Klasör verilirse kütüphane ekranı açılır; kitap verilirse bulunduğu klasör kütüphane olur ve kitap açılır. Ayarlar ve okuma kayıtları `~/.local/share/legentia` altına yazılır (`LEGENTIA_DATA` ile değiştirilebilir).

Tam yenileme, ekranın 120 ms ters renkte gösterilmesiyle işaretlenir. Şu an her 6. yenileme tamdır; bu sayı geçicidir. Pencere varsayılan olarak yarı boyuttadır; `LEGENTIA_SIM_SCALE=1` tam boyut verir.

## Kurallar

- Kullanıcıya görünen her metin `crates/core/src/i18n.rs` içindeki `Msg` katalogundan gelir. Arayüz metni yalnızca `Font::draw_msg` ile çizilebilir; `println!`/`eprintln!` clippy tarafından reddedilir.
- Kitap içeriği (başlık, yazar, metin) arayüz metni değildir; crate içinden `Font::draw_content` ile çizilir.
- Kod, log ve teknik tanımlayıcılar İngilizce olabilir.
- Fontlar `LEGENTIA_FONTS` klasöründen gelir (`nix develop` ayarlar); hepsi SIL OFL 1.1.
- `flake.nix` değişince `nix develop` kabuğundan çıkıp yeniden girin; yoksa eski ortam değişkenleri kalır.
