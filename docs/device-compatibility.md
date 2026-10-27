# Cihaz Uyumluluk Notu — Kobo Libra 2

Bu belge M0'ın çıktısıdır. Her bilginin kaynağı yanında yazılıdır. "Doğrulandı" yalnızca kullanıcının kendi cihazından alınan çıktılar için kullanılır.

## 1. Kullanıcının cihazından doğrulananlar (2026-10-27, USB ile, salt okuma)

| Alan | Değer | Kaynak |
|---|---|---|
| Model | Kobo Libra 2 (seri öneki `N418`) | `.kobo/version` |
| Cihaz kodu | 388 | `.kobo/version` |
| Firmware | 4.38.23697 | `.kobo/version` |
| Çekirdek | 4.1.15 | `.kobo/version` |
| Depolama | 30 GB bölüm, 8 GB dolu, 22 GB boş | `df -h` |
| SSH | Firmware'in kendi SSH sunucusu kapalı (`.kobo/ssh-disabled` mevcut) | `ls .kobo` |
| Kurulu ekler | `.adds/kfmon`, `.adds/koreader`, `.adds/nm`, `.adds/plato` | `ls .adds` |
| Başlatma yöntemi | KOReader ve Plato, NickelMenu'deki `generator : main : kfmon` satırıyla **KFMon üzerinden** başlatılıyor. Doğrudan `cmd_spawn` satırları yorum satırı. | `.adds/nm/*` |

Seri numarasının tamamı bu belgeye bilerek yazılmadı.

## 2. Cobalt profilinden gelen bilgiler (aynı model + aynı firmware, bizim cihazda henüz ölçülmedi)

Kaynak: [BandarLabs/Cobalt](https://github.com/BandarLabs/Cobalt) `crates/kobo-profile/src/lib.rs` (`LIBRA_2_388`) ve `docs/DEVICES.md`. Cobalt bu değerleri N418 / 388 / 4.38.23697 / çekirdek `4.1.15-00868-g58a2758be07` üzerinde ölçmüş.

- SoC: NXP i.MX6SLL (`fsl,imx6sll`), ARMv7 hard-float.
- Framebuffer: `mxc_epdc_fb` (MXCFB v2 arayüzü).
- Ekran: 1264×1680, 300 ppi, 32 bpp, BGRA bit sırası; dikey konumda `rotation: 1`, ters dikeyde `rotation: 3`.
- Sanal çözünürlük 1280×1792, stride 5120 bayt.
- Dokunmatik: `Elan Touchscreen`, ham aralık x 0–1680, y 0–1264, dönüşüm `Transpose`.
- Sayfa tuşları: üst tuş kod 193 (geri), alt tuş kod 194 (ileri).
- Wi-Fi uykuda kapanıyor; uygulamadan Nickel'a dönüşte artakalan `/bin/wpa_supplicant` sorun çıkarıyor.
- Aynı cihaz kodu altında iki donanım revizyonu var; ekran geometrisi revizyona göre farklı olabilir.
- Bu firmware'de `.kobo/ssh-disabled` dosyasını `ssh-enabled` olarak yeniden adlandırmak OpenSSH'yi açıyor. Geri almak için adı eski haline döndürmek yeterli. **Kullanıcı onayı olmadan yapılmayacak.**

## 3. Doğrulanmamış genel bilgiler

- İşlemci: tek çekirdekli Cortex-A9, yaklaşık 1 GHz. RAM: yaklaşık 512 MB. → Cihazda `/proc/cpuinfo` ve `free -m` ile doğrulanacak.
- glibc sürümü bilinmiyor. Statik musl kullanılacağı için bu kritik değil (bkz. 4. bölüm).

## 4. Derleme hedefi kararı

- Hedef: `armv7-unknown-linux-musleabihf`, **statik bağlama**, linker `rust-lld`.
- Gerekçe: Cobalt aynı cihazda bu yöntemle çalışıyor. Cihazdaki glibc'ye ve Kobo'nun paylaşılan kütüphanelerine bağımlılık oluşmuyor; Kobo kütüphanesinin üzerine yazma veya onu gölgeleme riski yok.
- C kodu içeren bağımlılıklar için NixOS'ta bir ARM C çapraz derleyicisi gerekebilir. Bu, `flake.nix` içinde çözülecek.

## 5. Geliştirme bilgisayarı (NixOS)

- AMD Ryzen 7 8840HS, 8 çekirdek / 16 iş parçacığı, yaklaşık 13,7 GB RAM, x86_64.
- Wacom dokunmatik ekran ve kalem mevcut. Simülatörde dokunmatik girişi test etmek için kullanılabilir.
- **Uyarı:** Bu bilgisayar Libra 2'den kat kat hızlı. NixOS'ta ölçülen süreler cihaz performansının kanıtı değildir (bkz. `milestone.md`, "Ölçüm yöntemi").

## 6. Açık konular

- [ ] Cihazda CPU, RAM ve çekirdek derleme kimliğini salt okuma komutlarıyla doğrula (SSH gerektirir; M10'da).
- [ ] Kendi uygulamamızı KFMon üzerinden mi, doğrudan `cmd_spawn` ile mi başlatacağımıza karar ver.
- [ ] Uygulama açılırken Nickel'ın nasıl durdurulup sonra yeniden başlatılacağını KOReader/Plato başlatma betiklerini **okuyarak** (değiştirmeden) incele.
