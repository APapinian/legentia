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

## Eksikler

- Gerçek bir taranmış kitap sayfasıyla (kâğıt dokusu, gölge, iki sütun, sayfa numarası, dipnot) henüz denenmedi. Kullanıcıdan örnek istenecek.
- Tek sayfa ve tek yazı tipiyle ölçüldü; sonuçlar genellenmemeli.
