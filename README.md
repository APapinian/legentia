# Legentia

Kobo Libra 2 için Rust ile yazılan, tamamen Türkçe arayüzlü e-kitap okuyucu. Şu an M1 aşamasında: geliştirme ortamı, proje iskeleti ve masaüstü simülatörü.

Belgeler: `kobo-rust-okuyucu-proje-aktarim.md` (vizyon), `milestone.md` (plan), `dikkat-edilmesi-gerekenler.md` (kurallar), `docs/device-compatibility.md` (cihaz bilgisi).

## Yapı

- `crates/core`: platformdan bağımsız çekirdek. Çizim yüzeyi, olaylar, E Ink yenileme türleri, Türkçe metin katalogu, uygulama.
- `crates/sim`: Libra 2 ekranını (1264×1680, gri ton) taklit eden masaüstü penceresi.

## Komutlar

Hepsi `nix develop` içinde çalıştırılır (ya da başına `nix develop -c` eklenir).

```sh
cargo run -p legentia-sim                 # simülatör
cargo fmt --all --check                   # biçim denetimi
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace                    # masaüstü testleri

# Kobo hedefi: statik ARMv7 musl; testler qemu-arm ile çalışır
cargo build -p legentia-core --target armv7-unknown-linux-musleabihf
cargo test  -p legentia-core --target armv7-unknown-linux-musleabihf
```

## Simülatör

| Girdi | Karşılığı |
|---|---|
| Fare tıklaması / dokunmatik ekran | Dokunma |
| PageUp, Sol ok | Üst sayfa tuşu (geri) |
| PageDown, Sağ ok, Boşluk | Alt sayfa tuşu (ileri) |
| Esc | Çıkış |

Tam yenileme, ekranın 120 ms ters renkte gösterilmesiyle işaretlenir. Şu an her 6. yenileme tamdır; bu sayı geçicidir. Pencere varsayılan olarak yarı boyuttadır; `LEGENTIA_SIM_SCALE=1` tam boyut verir.

## Kurallar

- Kullanıcıya görünen her metin `crates/core/src/i18n.rs` içindeki `Msg` katalogundan gelir. Arayüz metni yalnızca `Font::draw_msg` ile çizilebilir; `println!`/`eprintln!` clippy tarafından reddedilir.
- Kod, log ve teknik tanımlayıcılar İngilizce olabilir.
- Test fontu (DejaVu Sans) geçicidir; asıl font ve metin düzeni M3'te seçilecek.
