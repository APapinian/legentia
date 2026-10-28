# OCR deneyi (M5, NixOS)

**Tarih:** 2026-10-27
**Araç:** Tesseract 5.5.2 (Apache-2.0), `tur` ve `eng` dil verileri (nixpkgs)
**Belge:** kullanıcının tek sayfalık hukuk metni (A4, 1138 karakter, Source Serif 4). Belge depoda yok.
**Tekrar üretmek için:** `tools/ocr/deney.sh <belge.pdf> <çıktı klasörü>`

## Yöntem

Belge taranmış değil; metin katmanı olan bir PDF. Bu, ölçüm için avantaj sağlıyor: doğru metin PDF'in kendisinden alınabiliyor (`pdftotext`). Deney şu adımlarla yürüdü:

1. Sayfa görüntüye çevrildi ve taramayı taklit eden bozulmalar eklendi.
2. Görüntü Tesseract ile okundu.
3. Sonuç doğru metinle karşılaştırıldı (`tools/ocr/puan.py`).

Karşılaştırmadan önce iki metin de NFC'ye çevrildi, boşluklar birleştirildi ve tırnak işaretleri tek biçime indirildi.

- **Karakter hatası:** Levenshtein uzaklığı / karakter sayısı.
- **Kelime hatası:** aynı ölçü, kelime düzeyinde.
- **Türkçe kelime doğru:** içinde ç, ğ, ı, ö, ş, ü (veya büyükleri) geçen kelimelerin ne kadarının birebir doğru okunduğu.

| Görüntü | Nasıl üretildi |
|---|---|
| temiz-300 | 300 dpi, gri |
| temiz-150 | 150 dpi, gri |
| tarama-300 | 300 dpi, 1,2° eğik, bulanık, gürültülü, soluk |
| kotu-200 | 200 dpi, 2,5° eğik, daha bulanık ve gürültülü, JPEG kalite 35 |
| kotu-200-duzeltilmis | kotu-200, yalnızca eğikliği düzeltilmiş (`-deskew 40%`) |

## Sonuçlar

| Görüntü | Dil | Karakter hatası | Kelime hatası | Türkçe kelime doğru | Süre |
|---|---|---|---|---|---|
| temiz-300 | tur | %0 | %0 | %100 | 1,2 sn |
| temiz-300 | eng | %13 | %60 | %0 | 1,0 sn |
| temiz-150 | tur | %0 | %0 | %100 | 0,9 sn |
| tarama-300 | tur | %0 | %0 | %100 | 1,4 sn |
| tarama-300 | eng | %13 | %60 | %0 | 1,2 sn |
| kotu-200 | tur | %31–69 (gürültü rastgele) | %33–70 | %27–70 | 0,5–0,7 sn |
| **kotu-200-duzeltilmis** | **tur** | **%0** | **%0** | **%100** | 0,9 sn |
| Aranabilir PDF'ten geri okunan (tarama-300) | tur | %4,8 | %15 | %92 | — |

Süreler Ryzen 7 8840HS'te tek sayfa içindir ve Tesseract birden çok çekirdek kullanabilir.

## Bulgular

1. **Türkçe dil verisi şart.** `eng` ile Türkçe harf içeren kelimelerin hiçbiri doğru okunmadı (ı → i, ş → s gibi).
2. **Eğiklik düzeltmesi en önemli ön işleme adımı.** Kötü taramada hatalar harf tanımadan değil, sayfa düzeni çözümlemesinden geliyordu: Tesseract bütün paragrafları ve iki soruyu metin olarak görmeyip atladı. Yalnızca eğikliği düzeltmek hatayı sıfıra indirdi. Bu belgede gürültü temizleme, büyütme ve siyah-beyaza çevirmenin (Otsu, yerel eşik) ek bir etkisi olmadı.
3. **150 dpi bu yazı boyutu için yeterli.** Hata yine sıfır ve okuma %30 daha hızlı. Küçük yazılı belgelerde ayrıca denenmeli.
4. **Tesseract'ın ürettiği aranabilir PDF'te kelimeler bölünüyor.** Metin doğru tanınıyor, ama görünmez katmandan geri okununca kelime ortalarına boşluk giriyor ("annes i", "redde tmiştir"). Bunun nedeni Tesseract'ın her kelimeyi kendi kutusuna sığdırması ve PDF okuyucunun harf aralıklarından boşluk tahmin etmesi. → Kendi PDF yazıcımızda her satır tek bir metin olarak, kelime aralarına gerçek boşluk karakteriyle ve satır genişliğine göre yatay ölçeklemeyle (`Tz`) yazılmalı.
5. **Sayfa düzeni bilgisi kullanılabilir durumda.** hOCR çıktısı 7 paragraf ve 20 satır buldu; bu belgenin yapısıyla uyumlu. Yeniden akıtma (reflow) için paragraf sınırları ve satır kutuları buradan alınabilir.

## Tasarıma etkisi

- **OCR akışı:** sayfayı görüntüye çevirme → eğikliği bulma ve düzeltme → Tesseract (`tur`) → hOCR'den paragraf, satır ve kelime kutuları → (a) yeniden akıtılabilir metin, (b) PDF'e görünmez metin katmanı.
- **Eğiklik tespiti** kendi Rust kodumuzla yapılacak (izdüşüm profili yöntemi). Cihazda da gerekli.
- **Cihazda OCR (M10), henüz doğrulanmadı:** Libra 2'nin tek çekirdekli ARMv7 işlemcisinde sayfa başına süre tahminen onlarca saniye olur. Bu yüzden OCR arka planda, iptal edilebilir biçimde ve sayfa sayfa çalışmalı. Tesseract C++ olduğundan cihaza taşımak için iki seçenek değerlendirilecek: saf Rust bir OCR motoru ya da Tesseract'ı çapraz derlemek. İkisi de kalite (bu deneydeki ölçüler), hız ve bellek açısından karşılaştırılacak.
- **Dil verisi:** `tur.traineddata` yaklaşık 16 MB (tessdata, Apache-2.0); 22 GB'lık cihaz için sorun değil. Osmanlıca (`ota`) verisi nixpkgs'teki Tesseract'ta yok; ayrıca araştırılacak.

## Uygulamadaki OCR akışı (Rust eğiklik düzeltmesiyle)

Akış şöyle: hayro ile çizim (200 dpi) → `ocr::deskew` (izdüşüm profili, saf Rust) → Tesseract `tur`, TSV → paragraf blokları. Ölçüm aracı: `cargo run --release -p legentia-core --example ocr_check -- <belge.pdf> <sayfa> [eğiklik°] [dpi]`. Bu araç sayfayı bilinen bir açıyla eğip doğru metinle (sayfanın metin katmanı) karşılaştırır.

| Belge, sayfa | Uygulanan eğiklik | Bulunan | Düzeltmesiz hata | Düzeltmeli hata | Düzeltme süresi |
|---|---|---|---|---|---|
| Hukuk metni, s.1 | 2,5° | 2,50° | %0 | %0 | 58 ms |
| Hukuk metni, s.1 | −4° | −4,00° | **%83** | **%0** | 61 ms |
| KPSS 2020, s.3 (iki sütun) | 3° | 2,95° | %7,7 | %7,2 | 68 ms |

- KPSS'te kalan fark okuma sırasından geliyor (sınav düzeni); harf tanıma hatası değil.
- İlk sürümdeki bir hata gerçek sayfada yakalandı: büyük görüntüler 2 pikselde bir örneklenirken koordinatlar ölçeklenmiyordu. Bu, 0°'de yapay bir tarak deseni oluşturuyor ve her zaman 0° bulunuyordu. Düzeltildi; büyük sayfa testi eklendi.
- OCR sonucu kitap başına önbelleğe yazılır (`ocr/<kimlik>/<sayfa>.json`); sayfa ikinci kez açıldığında Tesseract çalışmaz.
- Şimdilik OCR sayfa açılırken çalışıyor ve arayüzü bekletiyor (masaüstünde ~1 sn). Cihazda arka planda çalışmalı (M10).

## Cihazda OCR ön değerlendirmesi (M5)

**Tarih:** 2026-10-28. Belge: KPSS 2020, s.4 (iki sütun, ~330 kelime). Tesseract 5.5.3, `tur`, `OMP_THREAD_LIMIT=1` (cihaz tek çekirdekli). Her değer üç ölçümün ortancası; ilk (soğuk) çalıştırma sayılmadı.

### Masaüstünde ölçülen (Ryzen 7 8840HS, tek iş parçacığı)

| Çözünürlük | Görüntü | Tesseract süresi | Tesseract en yüksek bellek |
|---|---|---|---|
| 150 dpi | 1240×1801 | 1,58 sn | 80 MB |
| 200 dpi (uygulamanın şu anki değeri) | 1654×2402 | 1,77 sn | 98 MB |
| 300 dpi | 2480×3601 | 2,14 sn | 149 MB |

Uygulama akışının geri kalanı (`ocr_check`, 2° eğik sayfa): hayro ile çizim 23 ms (150 dpi) / 43 ms (200 dpi), eğiklik bulma ve düzeltme 55 / 82 ms. Yani sürenin %95'inden fazlası Tesseract'ta. Karakter hatası 150 dpi'de %8,2, 200 dpi'de %7,5; fark okuma sırasından (sınav düzeni), iki çözünürlükte de harf tanıma hatası yok denecek kadar az.

### Cihaz için tahmin (ölçülmedi)

- **Süre:** Libra 2'nin işlemcisi tek çekirdekli Cortex-A9, ~1 GHz (Cobalt'ın bilgisi, bizim cihazda doğrulanmadı). Saat hızı ~5 kat, saat başına iş ~3–4 kat düşük; Tesseract'ın LSTM'i x86'da AVX2/FMA kullanırken A9'un NEON birimi dar. Bu yüzden 15–40 kat yavaşlama bekleniyor: **sayfa başına kabaca 25–70 saniye**. Bu bir tahmindir; M10'da cihazda ölçülecek.
- **Bellek:** büyük ölçüde aktarılabilir. 150 dpi'de ~80 MB, 200 dpi'de ~100 MB; buna PDF çiziminin geçici tepesi (çoğu sayfada 20–40 MB, tam sayfa görüntülü taramada 150 MB'a kadar) eklenir. ~512 MB RAM'li cihazda tek sayfa sığar; iki sayfa aynı anda işlenmemeli.
- **Pil:** 25–70 saniye tam yük, sayfa başına okumadan çok daha pahalı. Tüm kitabı toplu OCR'lamak (ör. 300 sayfa → 2–6 saat) yalnızca şarjdayken mantıklı.

### Sonuçlar ve M10 için kararlar

1. **Cihazda 150 dpi.** Bu yazı boyutunda doğruluk aynı, bellek %20 daha az, süre %10 daha kısa. Küçük yazılı belgelerde 200 dpi'ye çıkılabilir (ayar ya da otomatik: x yüksekliğine göre).
2. **OCR ayrı bir süreçte, arka planda.** Arayüz hiç beklememeli: sayfa önce görüntü olarak gösterilir, OCR bitince yeniden akıtılmış metne geçilir. Süreç öldürülerek iptal edilir; Tesseract zaten ayrı süreç olarak çalışıyor.
3. **Önden okuma:** okunan sayfa bitince sonraki 1–2 sayfa sırayla OCR'lanır; sonuçlar mevcut önbelleğe (`ocr/<kimlik>/<sayfa>.json`) yazılır.
4. **Toplu OCR ("Aranabilir PDF olarak kaydet")** yalnızca şarjdayken ya da pil belirli bir düzeyin üstündeyken, kesilip kaldığı yerden devam edebilecek biçimde.
5. **Denenecek hızlandırma:** `tessdata_fast` modelleri (daha küçük LSTM; hız/doğruluk farkı Türkçe için ölçülmeli) ve Tesseract'ı ARMv7 musl için çapraz derleme (`nixpkgs` `pkgsCross`). Saf Rust OCR motoru seçeneği hâlâ açık; aynı ölçütlerle karşılaştırılacak.

## Eksikler

- Gerçek bir taranmış kitap sayfasıyla (kâğıt dokusu, gölge, iki sütun, sayfa numarası, dipnot) henüz denenmedi. Kullanıcıdan örnek istenecek.
- Tek sayfa ve tek yazı tipiyle ölçüldü; sonuçlar genellenmemeli.
