# Kobo Libra 2'ye kurulum, kaldırma ve kurtarma

M10'un çıktısıdır. Durum (2026-10-28): plan yazıldı, cihazda henüz denenmedi. "Doğrulanmadı" işaretli adımlar ilk denemede kontrol edilecek.

## Neye dokunulur, neye dokunulmaz

Cihaza yalnızca **yeni** dosyalar eklenir:

| Yol (Kobo'nun kökünden) | İçerik |
|---|---|
| `.adds/legentia/legentia-kobo` | program (statik ARMv7, ~11 MB) |
| `.adds/legentia/legentia.sh` | başlatma betiği |
| `.adds/legentia/fonts/` | fontlar ve OFL lisans metinleri (~7 MB) |
| `.adds/nm/legentia` | NickelMenu'ye "Legentia" satırı |

Program çalışırken kendisi şunları oluşturur: `.adds/legentia/veri/` (ayar, okuma konumu, notlar, önbellekler), `legentia.log`, `betik.log`. Kütüphanedeki "Yedekle" `Legentia yedek.json` dosyasını kütüphane köküne yazar; indirilen kitaplar `İndirilenler/` klasörüne iner.

**Dokunulmayanlar:** `.adds/koreader`, `.adds/plato`, `.adds/kfmon`, `.adds/nm` içindeki mevcut dosyalar (`doc`, `kfmon`, `koreader`, `plato`), `.kobo/` (Nickel'ın veritabanı ve ayarları; SSH dosyası dahil), kitaplar. Program kitapları ve sözlükleri yalnızca okur.

## Kurulum

**Kısa yol:** Kobo'yu USB ile bağlayıp (nix develop içinde) `tools/kobo/kur.sh /run/media/$USER/KOBOeReader`. Betik aygıtın Kobo olduğunu (`.kobo/version`; Libra 2 değilse sorar) denetler, paketi kurar, yalnızca Legentia'nın dosyalarını kopyalar ve her birini doğrular; güncellemede `veri/` korunur. Elle kurulum adımları:

1. Bilgisayarda (nix develop içinde): `tools/kobo/paketle.sh target/kobo-paket`
2. Kobo'yu USB ile bağlayın, bağlantıyı onaylayın.
3. Paketteki `.adds` klasörünün içeriğini Kobo'nun `.adds` klasörüne kopyalayın. Bu yalnızca yukarıdaki yeni yolları ekler; aynı adlı bir dosya varsa durup bakın.
4. Kobo'yu güvenle çıkarın. NickelMenu ayarları Nickel açılırken okunur. "Legentia" menüde görünmezse cihazı yeniden başlatın.

## Başlatma ve çıkış

- **İlk denemeden önce:** Kobo USB'ye bağlı olmasın. Wi-Fi kapalı olsun (uçak modu). Cihaz dikey tutulsun.
- Ana ekranın sağ altındaki NickelMenu'den **Legentia**'yı seçin. Betik Nickel'ı durdurur ve Legentia'yı başlatır.
- **Çıkış:** kütüphanedeki **Çık** düğmesi ya da **güç tuşu**. İkisi de okuma konumunu kaydeder.
- **Çıkıştan sonra cihaz yeniden başlar** (yaklaşık yarım dakika). Program nasıl biterse bitsin (çıkış, çökme, bekçi) bu olur. Nickel'ı elle geri başlatmak Wi-Fi ve USB durumuna bağlı ayrıntılar istiyor; bunlar doğrulanana kadar yeniden başlatmak en güvenli yol.
- **Bekçi:** ilk denemelerde program 15 dakika sonra kendiliğinden durdurulur ve cihaz yeniden başlar. Ekran ya da dokunma çalışmazsa cihaz böylece kendiliğinden kurtulur. Süre `legentia.sh`'deki `DENEME_SURESI` ile değişir (0 = sınırsız).

## Kurtarma

Sırasıyla:

1. **Ekran değişmiyor ya da dokunma çalışmıyor:** güç tuşuna basın; program kaydedip çıkar, cihaz yeniden başlar.
2. **Güç tuşu da işe yaramıyor:** en çok 15 dakika bekleyin; bekçi programı durdurur, cihaz yeniden başlar.
3. **Cihaz hiç tepki vermiyor:** güç tuşunu uzun süre (yaklaşık 15–20 sn) basılı tutarak kapatın, sonra açın. Legentia kendiliğinden başlamaz; cihaz her zamanki gibi Nickel'a açılır. *(Libra 2'de bu sürenin tam değeri doğrulanmadı.)*
4. **Sorun tekrarlıyor:** Legentia'yı kaldırın (aşağıda). Günlükleri saklayın: `.adds/legentia/legentia.log` ve `betik.log`.

Legentia açılışta kendiliğinden çalışmaz; yalnızca menüden seçilince başlar. Bu yüzden yeniden başlatma her zaman Nickel'a döner.

## Güncelleme

- **Bilgisayardan:** `tools/kobo/kur.sh <Kobo klasörü>` (veri/ korunur).
- **Cihazdan:** Araçlar → "Güncellemeleri denetle". GitHub'daki (`APapinian/legentia`) son sürüm bundan yeniyse notları gösterilir; "İndir ve kur" programı ve özetini indirir, SHA-256'yı doğrular, `.adds/legentia/legentia-kobo.yeni` olarak bırakır. Kurulum Legentia bir sonraki açılışta `legentia.sh`'dedir: eski program `legentia-kobo.eski` olarak saklanır; yeni sürüm 20 saniye içinde hatayla kapanırsa eskisine dönülür (sahte programlarla denendi). Yalnız program güncellenir; fontlar ve betik `kur.sh` ile.
- **Sürüm yayınlamak:** `tools/kobo/surum.sh <klasör>` `legentia-kobo` ve `legentia-kobo.sha256`'yı hazırlar; etiket Cargo sürümüyle (`v0.1.0`) aynı olmalı. Betik hiçbir şey yüklemez.

## Kaldırma

Kısa yol: `tools/kobo/kur.sh --kaldir <Kobo klasörü>` (okuma konumu ve notlar `veri/`'de kalır; onları da silmek için `--verileri-de-sil`). Elle: USB ile bağlayıp şunları silin:
- `.adds/legentia/` (programla birlikte `veri/` da gider; önce kütüphanedeki "Yedekle" ile yedek alabilirsiniz)
- `.adds/nm/legentia`

Başka hiçbir dosya değişmediği için bunlar yeterli.

## İlk denemede bakılacaklar

Sırasıyla:
1. Ekran açılıyor mu, kütüphane görünüyor mu?
2. Dokunma doğru yere düşüyor mu?
3. Sayfa tuşları çalışıyor mu?
4. Bir EPUB, bir PDF.
5. "Çık" ve güç tuşu.

Sonuçlar `eksikler.md` ve `milestone.md`'ye yazılır. Ardından Nickel'ı yeniden başlatarak dönmek, uyku/uyanma ve Wi-Fi ele alınır.
