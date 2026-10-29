# Milestones — Rust Kobo Okuyucu

Bu dosya, `kobo-rust-okuyucu-proje-aktarim.md` belgesindeki hedefleri ölçülebilir teslimatlara böler. Her kilometre taşı, önceki aşamanın testleri geçmeden tamamlanmış sayılmaz.

**Çalışma sırası:** okuyucu (EPUB) → PDF → diğer özellikler → gerçek cihaz. Geliştirme ve test M10'a kadar NixOS'ta, Libra 2 ölçülerini taklit eden masaüstü simülatöründe yapılır. Modüller yalnızca ihtiyaç duyulan kilometre taşında eklenir.

## Ölçüm yöntemi

- Kabul ölçütlerindeki sayısal hedefler NixOS'ta deneysel olarak ölçülür ve ölçüldükten sonra bu belgeye yazılır. Henüz ölçülmemiş hedefler `[ölçülecek]` olarak işaretlidir.
- Simülatör Libra 2 ekranını taklit eder: 1264×1680, 300 ppi, gri tonlu (bkz. `docs/device-compatibility.md`).
- Bellek ölçümleri cihaza büyük ölçüde aktarılabilir; bu yüzden RAM tavanı masaüstünde de anlamlıdır.
- Süre ölçümleri aktarılamaz: geliştirme bilgisayarı Libra 2'den kat kat hızlıdır. Masaüstündeki süreler yalnızca karşılaştırma ve gerilemeyi yakalamak için kullanılır; gerçek değerler M10'da cihazda ölçülür.

---

## M0 — Cihaz ve ortam keşfi

**Amaç:** Kobo Libra 2 üzerinde uygulama çalıştırmanın gerçek teknik koşullarını doğrulamak.

### İşler
- [x] Kobo Libra 2 firmware/işletim sistemi sürümünü kaydet. (4.38.23697, çekirdek 4.1.15, cihaz kodu 388)
- [x] KOReader'ın NickelMenu üzerinden nasıl başlatıldığını, mevcut dosyaları değiştirmeden incele. (NickelMenu → KFMon üreticisi)
- [x] Rust target triple ve derleme/linkleme yöntemini seç; seçim için kaynak göster. (`armv7-unknown-linux-musleabihf`, statik, `rust-lld`; kaynak: Cobalt)
- [x] Ekran ve dokunmatik arayüzlerini belirle. (Cobalt'ın aynı model ve firmware üzerindeki ölçümleri; bizim cihazda M10'da doğrulanacak)
- [ ] Cihaz mimarisi, CPU ve RAM'i cihazda doğrula. (SSH gerektirir; M10'a ertelendi)
- [ ] Geri alma/kurtarma planı yaz. (M10'dan önce)

### Kabul ölçütleri
- Cihaz ve firmware bilgileri belgelenmiş.
- Seçilen derleme hedefi gerekçelendirilmiş.
- Doğrulanmamış varsayımlar açıkça işaretlenmiş.

**Çıktı:** `docs/device-compatibility.md`

---

## M1 — NixOS geliştirme ortamı, iskelet ve simülatör

**Amaç:** Tekrarlanabilir bir Rust geliştirme ortamı ve Libra 2 ekranını taklit eden bir masaüstü penceresi oluşturmak.

### İşler
- [x] NixOS için `flake.nix` ve geliştirme shell'i hazırla; ARMv7 musl hedefini de ekle.
- [x] Cargo workspace'i yalnızca şimdi gereken parçalarla kur: uygulama çekirdeği, metin katalogu (i18n), masaüstü platform katmanı.
- [x] Platform arayüzünü tanımla: ekrana çizim, kısmi/tam yenileme isteği, dokunma ve tuş olayları. Masaüstü uygulaması bu arayüzü bir pencereyle gerçekleştirir.
- [x] Simülatörde Libra 2 ölçülerini kullan (1264×1680, gri tonlu); E Ink yenilemesini görünür biçimde işaretle (ör. tam yenilemede kısa bir yanıp sönme).
- [x] Dokunmayı fare ve Wacom dokunmatik ekranla; sayfa tuşlarını klavyeyle taklit et. (Kullanıcı simülatörde denedi, 2026-10-27)
- [x] Türkçe metin katalogunu kur: kullanıcıya görünen her metin tek bir katalogdan gelsin, kodda sabit metin kalmasın.
- [x] Hata yönetimi ve loglama yaklaşımını belirle; panikte bile Türkçe anlaşılır mesaj göster. (`log` + `env_logger`; panik kancası Türkçe mesaj verir)
- [x] Biçimlendirme, lint ve test komutlarını tanımla; README'de açıkla.
- [x] Lisans kararı: Cobalt ve Plato (AGPL-3.0) yalnızca referans olarak okunacak; kodları kullanılmayacak. Uygulama kendi Rust koduyla yazılacak.
- [x] Bu projenin kendi lisansını belirle. (MIT, `LICENSE`)

### Kabul ölçütleri
- Temiz ortamda `nix develop` sonrası proje derleniyor.
- `cargo fmt --check`, `cargo clippy` ve `cargo test` çalışıyor.
- ARMv7 musl hedefi için derleme başarılı (cihazda çalıştırılmadan).
- Simülatör penceresi açılıyor; Türkçe bir test ekranı gösteriyor; dokunma ve tuş olayları loglanıyor.
- Kodda kullanıcıya görünen sabit metin yok; bunu yakalayan bir test veya lint var.

**Çıktı:** Derlenebilir en küçük proje, `flake.nix`, çalışan simülatör.

---

## M2 — EPUB prototipi

**Amaç:** EPUB dosyası açıp içeriğini simülatörde gösterebilen en küçük dikey dilimi oluşturmak.

### İşler
- [x] EPUB ZIP paketini güvenli biçimde aç (path traversal, zip bomb ve boyut sınırları).
- [x] Container/OPF, manifest ve spine bilgisini işle.
- [x] XHTML içeriğini güvenli biçimde parse et.
- [x] Kitap başlığını, yazarını ve bölüm listesini göster.
- [x] Bir bölümün metnini göster; önceki/sonraki bölüme geç.
- [x] Bozuk veya desteklenmeyen dosyalarda anlaşılır Türkçe hata göster.
- [x] Kullanılacak crate'lerin lisansını ve ARMv7 musl uyumluluğunu kontrol et. (`docs/dependencies.md`)
- [x] Kitap metnini NFC'ye normalleştir. (Gerçek bir kitapta "İ" `I` + U+0307 olarak yazılmıştı.)

### Kabul ölçütleri
- Türkçe karakterli kitaplar dahil en az 5 farklı gerçek EPUB açılıyor.
- Bozuk dosya testleri (kesik ZIP, eksik OPF, hatalı XHTML) uygulamayı çökertmiyor.
- Kitap açma süresi ve bellek kullanımı ölçülmüş.

### Ölçümler (2026-10-27)
Araç: `cargo run --release -p legentia-core --example epub_check -- <klasör>`. Kullanıcının Kobo'sundaki 8 Türkçe EPUB, salt okuma.

| Ölçüm | NixOS (x86_64, release) | qemu-arm (ARMv7 musl, release) |
|---|---|---|
| Hatasız açılan kitap | 8/8 | 8/8 |
| Kitap açma (OPF + içindekiler) | 8,7–18,7 ms | yalnızca işlevsel test |
| Tüm bölümleri okuma, en yavaş kitap | 94 ms | yalnızca işlevsel test |
| En yüksek bellek (VmHWM) | 3,4 MB | 16,6 MB (qemu ek yükü dahil) |

Süreler cihaz performansı değildir (bkz. "Ölçüm yöntemi"). Bellek ölçümü yalnızca ayrıştırmayı kapsar; ekran tamponu ve font önbelleği dahil değil.

**Çıktı:** Simülatörde çalışan EPUB metin görüntüleyici.

---

## M3 — Metin düzeni (layout) ve sayfalama

**Amaç:** Türkçe için doğru, ileride diğer yazı sistemlerine genişletilebilir bir metin düzeni motoru kurmak.

### İşler
- [x] Font yükleme, ölçüm ve harf birleştirme (shaping) altyapısını seç; seçimde sağdan sola (Osmanlıca/Arapça) ve CJK (Japonca) desteğinin sonradan eklenebilmesini şart koş. (rustybuzz + swash; glifler kaynak bayt konumu taşır, satırlar bayt aralığıyla tanımlı. Bidi henüz yok.)
- [x] Türkçe karakterleri tam kapsayan, lisansı uygun (ör. OFL) fontları seç ve belgele. (Literata, Source Serif 4, Gentium, arayüz: Source Sans 3; `docs/dependencies.md`)
- [x] Unicode satır kırma kurallarını uygula. (unicode-linebreak, UAX #14)
- [x] Türkçe heceleme (tireleme) ekle; iki yana yaslı metinde boşluk dağılımını kontrol et. (TDK kuralları; boşluk en fazla 3 katına genişler, fazlası sola yaslanır)
- [x] Punto, yazı tipi, satır aralığı, kenar boşlukları ve hizalamayı ayarlanabilir yap. (`Settings`; arayüzü M4'te, şimdilik simülatörde geliştirici tuşları)
- [x] Paragraf, başlık, italik/kalın ve temel bağlantı biçimlendirmesini uygula.
- [x] Ekran boyutuna göre sayfalama yap; sayfa konumunu kararlı bir içerik konumuyla ilişkilendir. (`Position { block, offset }`)
- [x] E Ink için yenileme planı: sayfa çevirmede kısmi yenileme, belirli aralıkla tam yenileme. (Her 6. çevirme ve her bölüm/ekran değişimi tam; sayı cihazda ayarlanacak)
- [x] Çok uzun kelimeler, tablolar, görseller, dipnotlar ve CSS sınırlamalarını test et. (Uzun kelime testli; diğerlerinin davranışı `docs/epub-support.md`)
- [x] `tr` dil etiketini harf birleştiriciye ver: Literata'da "fi" bitişik harfi Türkçede uygulanmıyor (testli).
- [x] Yabancı kelimeleri Türkçe kurallarla bölme (q/w/x içerenler). Gerçek kitapta "Luxembo-urg" hatası görüldü.

### Kabul ölçütleri
- Yazı ayarları değişince sayfalama yeniden hesaplanıyor ve okuma konumu korunuyor. (testli)
- `İ/ı`, `I/i`, `ğ, ü, ş, ö, ç` ve Unicode normalizasyon testleri geçiyor. (her font ailesi ve stil için glif kapsamı testli)
- Türkçe heceleme için örnek kelime listesiyle birim testleri var.
- Sayfa çevirme süresi ve bölüm sayfalama süresi ölçülmüş.
- Desteklenmeyen EPUB özellikleri belgelenmiş. (`docs/epub-support.md`)

### Ölçümler (2026-10-27, NixOS x86_64, release, Libra 2 ekranı 1264×1680)
"Dar Kapı", 6. bölüm (18 sayfa), Literata 40 px, iki yana yaslı, heceleme açık.

| Ölçüm | Süre |
|---|---|
| Bölümü okuyup tamamını sayfalara dizme | 15 ms |
| Sayfa çevirme (olay işleme) | < 0,01 ms |
| Bir sayfayı çizme (glif önbelleği dolu / boş) | 2,6 ms / 7 ms |

Süreler cihaz performansı değildir; Libra 2'de kat kat yavaş olacak (M10'da ölçülecek). Glif önbelleği en fazla 4096 glif tutar.

**Çıktı:** Simülatörde okunabilir sayfalama.

---

## M4 — Temel okuyucu ve tam Türkçe arayüz

**Amaç:** Günlük okumada kullanılabilecek ilk sürümü simülatörde tamamlamak.

### İşler
- [x] Yerel depolama katmanını seç (SQLite veya gerekçelendirilmiş alternatif); şema ve migration stratejisini belirle. (Sürüm numaralı JSON dosyaları, `crates/core/src/storage.rs`. Gerekçe: veri küçük, okunabilir ve yedeklenebilir, saf Rust; atomik yazma (geçici dosya + yeniden adlandırma). Bozuk ya da daha yeni sürümlü dosya `.bozuk` olarak kenara alınır, üzerine yazılmaz. Notlar/arama (M6) SQLite gerektirirse JSON'dan aktarılır.)
- [x] Kütüphane ekranı: USB ile `/mnt/onboard` altına kopyalanan kitapları tara ve listele (simülatörde bir klasör). (Gizli klasörler atlanır; başlıklar yol/boyut/tarih anahtarlı önbellekte)
- [x] Okuma ekranı, menüler ve ayarlar. (Ekranın ortası menü; yazı ayarları ekranı önizlemeli, "Tamam"la uygulanır)
- [x] İçindekiler, bölüm/sayfa gezinmesi, okuma konumu ve yer imleri. (Yer imli sayfada köşe işareti)
- [x] Ayarların ve ilerlemenin kalıcı saklanması. (Konum her 10 sayfa çevirmede, bölüm değişince, menü açılınca ve kapanışta kaydedilir)
- [x] E Ink'e uygun kontrast ve ters renk seçeneklerini değerlendir. (Gece modu: ters renk; kontrast ayarı cihazda değerlendirilecek)
- [x] Türkçe imla, tarih, sayı biçimi ve hata mesajlarını gözden geçir; Türkçe sıralama ve aramayı uygula. (Yüzde "%45", ondalık virgül "1,4", Türk alfabesiyle sıralama, `I/ı`–`İ/i` dönüşümü. Kütüphanede arama henüz yok.)
- [x] Kitap kimliği: EPUB'ın kendi tanımlayıcısı (ISBN/UUID), yoksa dosya adı + boyut; kitap taşınsa da konum ve yer imleri korunur.
- [x] Pencere açmadan ekran görüntüsü alan geliştirici aracı (`examples/screenshots.rs`).

### Kabul ölçütleri
- Kullanıcıya görünen İngilizce metin kalmıyor. (Tüm arayüz metni `Msg` katalogundan; font adları ve kitap içeriği hariç)
- Ayarlar, okuma konumu, yer imleri ve kütüphane bilgileri yeniden açılışta korunuyor. (testli)
- Türkçe karakterli dosya adları ve kitap başlıkları doğru sıralanıyor. (testli)
- Bellek tavanı belirlenmiş ve büyük kitaplarla test edilmiş.

### Ölçümler (2026-10-27, NixOS x86_64, release)
| Ölçüm | Değer |
|---|---|
| Kütüphane taraması, 8 kitap, önbellek boş | 12 ms |
| Uygulamanın en yüksek belleği (fontlar, ekran tamponu, glif önbelleği, en büyük kitap ~500 bin karakter) | 12,6 MB |
| Sayfa çevirme, kayıtsız / kayıtlı (her 10'da bir, `fsync`) | < 0,01 ms / ~10 ms |

**Çıktı:** Simülatörde Türkçe temel okuyucu.

---

## M5 — PDF: görüntüleme, ölçeklendirme ve OCR

**Amaç:** PDF'leri yüksek kalitede göstermek; taranmış PDF'lerde bile metni anlayıp 6 inç ekrana uygun biçimde yeniden düzenleyebilmek.

### İşler
- [x] PDF motorunu araştır ve seç: görüntü kalitesi, hız, bellek, ARMv7 musl uyumluluğu ve lisans. (Saf Rust: hayro 0.8, MIT/Apache; ARMv7 musl'da derleniyor, testler qemu'da geçiyor. PDF yazma için lopdf (MIT) değerlendirilecek.)
- [x] Yüksek kaliteli render: gri ton için doğru kenar yumuşatma. (E Ink'e uygun kontrast/gamma ayarı cihazda değerlendirilecek.)
- [ ] Yakınlaştırma, kenar boşluğu kırpma (otomatik ve elle), sayfaya/genişliğe sığdırma, sütun modu. (Yapıldı: otomatik kırpma, yalnızca görünen alan çizilir. "PDF görünümü" ayarı: sayfaya sığdır / genişliğe sığdır / sütun sütun; "PDF yakınlaştırma" %100–200. Ekrana sığmayan sayfa ekranlara bölünür (alt alta en az %5 örtüşme, yan yana örtüşmesiz); sayfa tuşları ekran ekran ilerler, ayar değişince okuma sırasındaki yer korunur. Sütunlar mürekkep profilinden bulunur: sütunlar arası ayırıcı çizgi, sütunları aşan talimat kutusu ve seyrek sütunlar destekli; 2020 KPSS PDF'inin soru sayfalarında görsel olarak doğrulandı (2016 ve 2024'te yalnızca sütun sayısına bakıldı); tek sütunlu metin sayfaları (2020 s.9, 2022 PDF'i) bölünmüyor. Bilinen sınırlar: neredeyse boş kapak sayfaları sütun modunda yanlış bölünebilir; içerik boyunun %10'undan fazlasını kaplayan, sütunları aşan başlık/özet varsa sütun bulunmaz. Kalan: elle kırpma (sürükleme hareketi gelince).)
- [x] Metin katmanı olan PDF'lerde metni çıkar ve okuyucunun layout motoruyla yeniden akıt (reflow). (`pdf/text.rs`: glif konumu, boyutu ve genişliğinden satır ve paragraf; satır sonu tiresi birleştirilir, büyük yazı başlık olur; görünmez OCR katmanı da okunur. "PDF metnini yeniden akıt" ayarı; metni olmayan sayfa görüntü olarak kalır. Hukuk metninde karakter hatası %0. Sınav kitapçığı gibi karmaşık düzenler yeniden akıtmaya uygun değil.)
- [x] Taranmış PDF'ler için OCR: sayfa düzenini (sütunlar, başlıklar, paragraflar) ve metni tanı; sonuca göre yeniden akıt. (`ocr/`: Rust'ta eğiklik düzeltme + Tesseract TSV → paragraf/başlık; "Taranmış sayfaları OCR ile oku" ayarı; sonuç önbelleğe yazılır. Bölgeleri yeniden ölçekleme sütun moduyla birlikte yapılacak.)
- [x] OCR'ı önce NixOS'ta deney amaçlı çalıştır (Türkçe dil paketiyle); doğruluk ve süreyi ölç. (`docs/ocr-deney.md`: `tur` ile temiz ve hafif bozuk görüntüde %0 hata; kötü taramada eğiklik düzeltmesi hatayı %31–69'dan %0'a indirdi)
- [x] OCR metnini PDF'e görünmez, aranabilir metin katmanı olarak göm; orijinal dosyayı koru, yeni dosyayı ayrı yaz. (`pdf/write.rs`, lopdf; menüde "Aranabilir PDF olarak kaydet". 2° eğik görüntü PDF'inde pdftotext ile geri okuma: karakter hatası %4,75, Türkçe kelimeler %100. Kelimeler eğik yazılınca %16'ydı; satır başına yatay ortak taban çizgisine geçildi.)
- [x] Notları ve vurguları PDF'e standart PDF açıklamaları (annotation) olarak yaz; başka PDF okuyucularında da görünmeli. (Yazıcı: Highlight + not + görünüm akışı. Arayüz M6'da: "Notları dışa aktar" → "<ad> - notlu.pdf".)
- [x] Cihazda OCR için ön değerlendirme: tek çekirdekli ARMv7 ve kısıtlı RAM'de sayfa başı süre ve bellek tahmini. Uygulama M10'da. (`docs/ocr-deney.md`: masaüstünde tek iş parçacığıyla sayfa başı 1,6–1,8 sn, 80–100 MB; cihazda tahminen 25–70 sn. Karar: 150 dpi, ayrı süreçte arka planda, iptal edilebilir, önden okumalı; toplu OCR yalnızca şarjda.)

### Kabul ölçütleri
- Metin PDF'i, taranmış PDF, çok sütunlu PDF ve büyük PDF test kümeleri var.
- Yeniden akıtılmış metinde Türkçe karakterler ve paragraf sırası doğru.
- OCR doğruluğu Türkçe örnek sayfalarda ölçülmüş. (`docs/ocr-deney.md`: hukuk metni ve KPSS sayfaları; gerçek taranmış kitap sayfasıyla henüz denenmedi)
- Büyük PDF'lerde bellek ve sayfa render süresi ölçülmüş. (aşağıdaki ara ölçümler: 7 PDF, 1528 sayfa)
- PDF desteğinin sınırları belgelenmiş.
- OCR katmanı gömülmüş PDF başka bir PDF okuyucusunda aranabiliyor ve seçilebiliyor.
- PDF'e yazılan notlar ve vurgular başka bir PDF okuyucusunda görünüyor. (Poppler/pdftoppm ile görüldü; not içeriği `Contents` alanında, testli)
- Yazma işlemi yarıda kesilse bile orijinal PDF bozulmuyor.

### Ara ölçümler (2026-10-27, NixOS x86_64, release, Libra 2 genişliği 1264 px)
`cargo run --release -p legentia-core --example pdf_check -- <klasör>`; kullanıcının Kobo'sundaki 7 PDF, 1528 sayfa.

| Ölçüm | Değer |
|---|---|
| Hatasız çizilen sayfa | 1528/1528 (JBIG2 ve JPEG 2000 içeren taranmış kitap dahil) |
| Belge açma | 145–463 ms |
| Sayfa çizme, ortanca | 12–23 ms (metin PDF'leri), 99 ms (taranmış) |
| En yavaş sayfa | 367 ms (taranmış) |
| En yüksek bellek | 32–41 MB çoğu belge, 78 MB (10 MB'lık dosya), 150 MB (taranmış) |

Bellek incelemesi (`PDF_CHECK_MEM=1`, sayfa başına en yüksek bellek, 15 KPSS PDF'i): ortanca 21–37 MB; tepe değerler (75–150 MB) yalnızca tam sayfa 300 dpi görüntü içeren sayfalarda görülüyor (ör. 2480×3425 CMYK görüntü tek başına ~34 MB açılıyor). Bu tepe geçicidir, sayfa çizilince geri verilir; uygulamanın kalıcı belleği küçüktür (önbellekte tek sayfa görüntüsü, ~2 MB). Görüntüleri düşük çözünürlükte açmak hayro'da değişiklik gerektirir (ileride). Dosyayı bellek eşlemesi (mmap) ile açmak `unsafe` gerektirdiği ve kazancı dosya boyutuyla sınırlı kaldığı için yapılmadı.

**Çıktı:** Simülatörde PDF görüntüleme, reflow ve OCR akışı.

Not: PDF'e not yazma, M6'daki ortak konumlandırma modeline bağlıdır; M5'te PDF tarafının yazma altyapısı, M6'da arayüzü tamamlanır.

---

## M6 — Vurgular ve notlar

**Amaç:** EPUB ve PDF için tek bir konumlandırma modeliyle kalıcı kişisel çalışma araçları sağlamak.

### İşler
- [x] EPUB ve PDF için ortak içerik konumlandırma modeli tasarla. (`app/annotations.rs`: vurgu = bölüm/PDF sayfası + blok metninde bayt aralığı + alıntı + not; yazı ayarından bağımsız, PDF'te görüntü ve yeniden akıtma aynı blokları kullanır. Dosya değişip aralık kayarsa alıntı en yakın yerde yeniden bulunur, bulunamazsa vurgu silinmez, çizilmez. PDF metninde her karakterin sayfa kutusu taşınır (`pdf::PageText`): görüntüde seçim, çizim ve PDF açıklaması için.)
- [x] Metin seçimi, vurgulama ve seçili metne not ekleme/düzenleme. (Kararlar, 2026-10-28: seçim uzun bas + sürükle; not metni Türkçe ekran klavyesiyle. Yapıldı: ham dokunmadan dokunma/uzun basma/sürükleme tanıma (`event.rs`, zamanlayıcısız: uzun basma süre dolduktan sonraki ilk hareket ya da kaldırmada tanınır); simülatörde fare. EPUB'da ve yeniden akıtılmış PDF'te seçim (`app/select.rs`): uzun basma kelimeyi seçer ("İstanbul'dan" tek kelime), sürükleme kelime kelime uzatır; işlem çubuğu (Vurgula / Vazgeç), vurguya dokununca (Vurguyu sil / Vazgeç); vurgu açık gri, seçim koyu gri zemin. PDF sayfa görüntüsünde de aynı seçim ve vurgu: ekran noktası sayfa koordinatına çevrilip karakter kutularında aranır; sayfa metni yalnızca uzun basınca ya da sayfada vurgu varsa çıkarılır (KPSS sayfası ~1 ms, masaüstü). Vurgu, sayfa çizildikten sonra dikdörtgen koyulaştırılarak çizilir; görüntüdeki yazı siyah kalır. Görüntüde yapılan vurgu yeniden akıtmada da görünür. Notlar: Türkçe Q ekran klavyesi (`app/keyboard.rs`: ç ğ ı ö ş ü, Türkçe büyük harf i→İ ı→I, rakam/işaret katmanı; imleç metnin sonunda), not ekranı (`app/note.rs`); seçimde "Not ekle" vurgulayıp not ekranını açar, vurguya dokununca not kutuda gösterilir ve "Notu düzenle"; notlu vurgunun sonunda küçük işaret; boş kaydedilen not kaldırılır, vurgu kalır. Kalan: taranmış (OCR'lı) sayfa görüntüsünde seçim; notta imleci metin ortasına taşıma.)
- [x] Notları kitap, bölüm ve konum bilgisiyle ilişkilendir. (Menüde "Vurgular ve notlar": kitap sırasıyla alıntı, bölüm adı (içindekilerden; PDF'te sayfa) ve not; dokununca o yere gidilir.)
- [x] Notları dışa aktarma ve yedekleme/geri yükleme. (Karar: Markdown, PDF'lerde standart PDF açıklamaları (ayrı kopya), JSON yedek. Yapıldı: menüde "Notları dışa aktar" kitabın yanına "<ad> - notlar.md" (bölüm başlıkları, alıntılar, notlar), PDF'te ayrıca "<ad> - notlu.pdf" (satır başına Highlight açıklaması, not ilkinde; orijinal değişmez; KPSS 2020'de Poppler ile görüldü). Taranmış (OCR'lı) sayfalardaki vurgular PDF'e yazılamaz, Markdown'a yazılır. Yedek: kütüphane ekranında "Yedek" → kütüphane klasörüne "Legentia yedek.json" (ayarlar + her kitabın konumu, yer imleri, vurguları, notları); geri yükleme siler değil birleştirir (`app/backup.rs`).)

### Kabul ölçütleri
- Uygulama kapanıp açılınca notlar ve vurgular korunuyor. (testli)
- Yazı ayarı değişince vurgular doğru metne bağlı kalıyor. (testli; konum blok/bayt olduğu için sayfalamadan bağımsız)
- Kitap dosyası değiştiğinde konum eşleşmesinin sınırları ele alınmış. (Alıntı en yakın yerde yeniden aranır; metin değiştiyse ya da alıntı silindiyse vurgu çizilmez ama listede ve yedekte kalır. PDF'te kimlik dosya adı + boyut olduğundan değişen PDF yeni kitap sayılır. Testli.)
- Yedekten geri yükleme test edilmiş. (Yeni kurulumda geri yükleme ve birleştirme testli)

**Bilinen sınırlar:** taranmış (OCR'lı) sayfa görüntüsünde seçim yapılamıyor (yeniden akıtmada yapılabiliyor); notta imleç yalnızca metnin sonunda; uzun basma, zamanlayıcı olmadığı için parmak kalkınca ya da kıpırdayınca tanınıyor (cihazda değerlendirilecek); dışa aktarılan "- notlu.pdf" kütüphanede ayrı kitap olarak görünür.

**Çıktı:** Yerel not ve vurgu sistemi.

---

## M7 — Sözlük ve kelime araştırması

**Amaç:** Kelime seçildiğinde hızlı, genişletilebilir ve çevrimdışı çalışabilen sözlük deneyimi sunmak.

### İşler
- [x] Sözlük sağlayıcıları için ortak Rust arayüzü tanımla. (`dict::Dictionary`: ad, `contains`, `lookup` → maddeler (biçim etiketli metin: düz, HTML, Pango, XDXF, DSL). Ortak dizin: Türkçe küçük harf + şapkasız anahtar, madde başına 12 bayt. `Dictionaries::define`: kök bulma + bütün sözlüklerde arama. StarDict: `.ifo/.idx(.gz)/.syn/.dict(.dz)`, 32/64 bit konum, `sametypesequence`, dictzip parça parça açılır; bozuk dosya hata verir, ötekileri engellemez. DSL (Lingvo): `.dsl/.dsl.dz`, UTF-16 LE/BE ve UTF-8, `#NAME`, çok satırlı madde başı, `{…}` ve `(…)` söz dizimi; dizin açılışta akışla kurulur (57 MB, 200 bin kart: 0,22 sn, 26 MB, masaüstü; cihazda ilk aramada birkaç saniye beklenir, gerekirse dizin diske önbelleklenecek); `_abrv.dsl` atlanır. Kobo dicthtml: `dicthtml*.zip`, gzip'li ya da düz `PREFIX.html`, `<a name>` madde başı ve `<variant>` çekimli biçimler; dizin açılışta HTML taranarak kurulur (Kobo'nun önek kuralına ve marisa-trie `words` dosyasına bağımlı değil). Kobo'nun resmî sözlükleri AES ile şifreli; anahtarı belgelenmemiş ve açmak kopya korumasını aşmak olur: desteklenmez, hatayla atlanır. MDict: `.mdx` sürüm 1.2 ve 2.0, UTF-8/UTF-16, sıkıştırmasız ve zlib bloklar, "Encrypted=2" dizin karıştırması (RIPEMD-128), `@@@LINK=` yönlendirmesi; desteklenmeyen: "Encrypted=1" (kullanıcıya özel kayıt kodu, kopya koruması), LZO, GBK/Big5. MDict yalnızca kendi ürettiğimiz test dosyalarıyla denendi; gerçek bir .mdx ile doğrulanacak. Klasörler: `.adds/koreader/data/dict`, `.kobo/dict`, `Sözlükler`.)
- [ ] İlk yerel sözlük formatını ve lisans uygunluğunu seç. (Karar, 2026-10-28: GoldenDict'in okuduğu biçimler: StarDict, DSL, MDict; ayrıca Kobo dicthtml. Uygulamayla sözlük dağıtılmaz; kullanıcının kendi dosyaları okunur. Deneme: KOReader'daki sözlükler.)
- [x] Türkçe kök bulma: çekimli kelimeden sözlük maddesine ulaş (ör. "kitaplarımızdan" → "kitap"). (Karar, 2026-10-28: kendi kurallarımız. `dict/stem.rs`: ek kalıplarından (ünlü uyumu ve ünsüz değişimiyle açılan) sondan soyma; ünsüz yumuşaması, ünlü düşmesi, çift ünsüz ve -yor daralmasının geri alınması; fiillerde mastar. Adaylar sözlükte doğrulanır, az soyulmuş önce. 27 örnek kelimeyle testli; kelime başına 10–175 µs, masaüstü.)
- [x] Kelime arama, tanım, dil ve kaynak bilgisini göster; kelimeyi kişisel listeye kaydet. (Yapıldı: seçim çubuğunda "Sözlük"; kelime ya da sözlükte bulunan kökü bütün sözlüklerde aranır, "“toplumun” → toplum" gösterilir; her madde başlık + sözlük adı + içerik olarak tam ekran, okuyucunun dizgisiyle; HTML/Pango/XDXF/DSL biçimlendirmesinden kalın, italik ve paragraflar korunur (`dict/render.rs`). Sözlükler kütüphane köküne göre `.adds/koreader/data/dict` ve `Sözlükler` klasörlerinde aranır, ilk aramada açılır. Kişisel kelime listesi: tanım ekranında "Listeye ekle / Listeden çıkar", menüde "Kelime listem" (Türk alfabesine göre; metindeki biçim ve kitapla), dokununca yeniden bakılır; `words.json`, yedeğe dahil. Kaynak olarak sözlük adı gösterilir; sözlüğün dil bilgisi v2'de.)
- [ ] Diğer diller sonradan: Almanca bileşik kelimeler, Rusça çekimler ve Japonca kelime sınırları ayrı ayrı ele alınacak.
- [x] Çevrimiçi sağlayıcıları timeout, cache ve çevrimdışı hata davranışıyla ekle. (M8'de yapıldı, 2026-10-28. Kullanıcı kararı: TDK ve Vikisözlük ayrı sağlayıcı, ayarlardan açılır ("PDF ve internet" sayfası; varsayılan TDK açık, Vikisözlük kapalı); yalnızca kelime cihazdaki sözlüklerde yoksa sorulur. TDK şartları: resmî API ya da kullanım şartı yayımlanmamış; sitenin kendi kullandığı belgelenmemiş `sozluk.gov.tr/gts` ucu kullanılıyor, kaynak "TDK Güncel Türkçe Sözlük" diye gösteriliyor. Uygulama dağıtılacaksa TDK'dan yazılı izin istemek en güvenlisi. Vikisözlük: MediaWiki API'si, CC BY-SA; çekim sayfaları "Türkçe sözcükler" kategorisiyle ayıklanıyor, bütün kök adayları tek istekte soruluyor. TDK yalnızca sözlükteki biçimi tanıdığından kök adayları sırayla deneniyor (en çok 6); Vikisözlük de açıksa önce o soruluyor ve bulduğu kök TDK'da ilk deneniyor. Arama arka planda, "Vazgeç" ile bırakılabiliyor; zaman aşımları `net.rs`'deki gibi. Bulunan sonuçlar veri klasöründe `cevrimici-sozluk/` altında önbellekleniyor, sonra çevrimdışı da açılıyor. Gerçek sunucularda denendi: ağaçlar → ağaç, gölgeler → gölge, geldi → gelmek (TDK + Vikisözlük, kelime başına ~2 sn). Kök ipucu iyileştirmesi yalnızca taklit sunucuyla doğrulandı: denemeler sırasında TDK art arda isteklerden sonra boş yanıt vermeye başladı.)

### Kabul ölçütleri
- En az bir Türkçe sözlük tamamen çevrimdışı çalışıyor. (Bütün biçimler çevrimdışı; test sözlükleriyle testli. Kullanıcının gerçek sözlükleriyle (KOReader) denenecek.)
- Çekimli Türkçe kelimeler için kök bulma testleri geçiyor. (27 kelime)
- İnternet olmadığında uygulama kilitlenmiyor ve Türkçe açıklama gösteriyor. (Arama arka planda; ulaşılamazsa "cihazdaki sözlüklerde yok; çevrimiçi sözlüğe de ulaşılamadı" ve nedeni gösteriliyor. Testli.)

**Çıktı:** Yerel ve çevrimiçi sözlük sistemi.

---

## M8 — İnternet, OPDS ve Calibre

**Amaç:** Kitap keşfi ve indirme için açık ağ protokollerini eklemek. Ana kitap aktarım yolu USB'dir; bu aşama ek bir kolaylıktır.

### İşler
- [x] HTTPS ve TLS sertifika doğrulaması. (Karar, 2026-10-28: rustls + ring; saf Rust sağlayıcı bakımsız alfa olduğu için bırakıldı. ureq istemcisi, gömülü Mozilla kök sertifikaları; ARMv7'de ring flake'teki clang ile derleniyor. Gerçek sunucularda denendi (masaüstü ve qemu-arm): Gutenberg OPDS açılıyor, adı tutmayan sertifika reddediliyor. `net.rs`: zaman aşımları (bağlantı 15 sn, yanıt 30 sn), Basic ve Digest yetkilendirme (5. kısım), hatalar Türkçe (bağlantı yok, zaman aşımı, sertifika, yetki, sunucu kodu, boyut, disk); indirme geçici dosyaya, iptal ve hatada iz bırakmaz. Not: Standard Ebooks OPDS'i artık üyelik istiyor (401).)
- [x] OPDS katalog tarama ve kitap indirme. (Yapıldı: `opds.rs` OPDS 1.2 ayrıştırma: gezinme ve kitap girdileri, EPUB/PDF bağlantıları (EPUB önce; satılık ve okunamayan biçimler elenir), `next` sayfalama, OpenSearch (şablon ya da açıklama dosyası), göreli adres çözme; HTTPS katalogdan gelen düz HTTP bağlantılar HTTPS'e yükseltilir. Gerçek Gutenberg kataloğunda denendi: kök, popüler, kitap sayfası, arama. Katalog ekranları: kütüphane başlığında "Kataloglar"; Gutenberg ve Standard Ebooks hazır gelir, kullanıcı ekran klavyeli formla katalog ekler, uzun basarak düzenler/siler (`catalogs.json`; şifre bu dosyada düz metin durur). Katalog sayfalarında gezinme, geri yığını, "Sonraki sayfa", arama ve kitap ayrıntısı (başlık, yazar, özet, biçim). İstek arka plan iş parçacığında yürür, platformun `Tick` olaylarıyla yoklanır, beklerken "Vazgeç" ile bırakılır. Kullanıcı adı ve şifre yalnızca katalogun kendi sunucusuna (şema + ad + port) gönderilir. Yerel taklit sunucuyla testli (yetki, OPDS olmayan yanıt, kapalı sunucu, vazgeçme); gerçek Gutenberg'de gezinme ve arama, Standard Ebooks'ta üyelik (401) mesajı görüldü. Gerçek Gutenberg'den iki kitap indirilip kütüphanede açıldı (arama → kitap sayfası → İndir).)
- [x] Calibre Content Server uyumluluğu. (Digest yetkilendirme (MD5, `qop=auth`; RFC 2617 örneğiyle testli). Şifre artık hiçbir sunucuya ilk istekte gönderilmiyor: sunucu 401 ile Digest ya da Basic istediğinde o yöntemle yeniden deneniyor, böylece düz HTTP'deki Calibre'ye şifre açık gitmiyor. Adres alanı kısaltma kabul ediyor: şema yoksa `http://`, yol yoksa `/opds` ekleniyor ("192.168.1.5:8080" → "http://192.168.1.5:8080/opds"). Gerçek calibre-server 9.14 (nixpkgs, geçici, `--enable-auth`) ile uygulama üzerinden denendi: kısaltılmış adresle katalog ekleme, yanlış şifrede yetki mesajı, kök katalog, "By En yeni" gezinmesi, arama ve Türkçe başlıklı kitabın indirilip kütüphaneye eklenmesi. Calibre'nin kendi sınırı: kullanıcı şifresi yalnızca ASCII karakter içerebiliyor.)
- [x] İndirme kuyruğu, ilerleme, iptal, yeniden deneme ve disk alanı kontrolü. (Kitap ayrıntısında "İndir"; kitaplar sırayla, arka planda `<kütüphane>/İndirilenler/Yazar - Başlık.epub` olarak iner (FAT32'ye uymayan karakterler temizlenir), bu sırada gezinmeye devam edilir. Katalog başlığında "İndirmeler (n)": ilerleme (yüzde ve MB), İptal, Yeniden dene, Aç. E Ink ilerlemede yalnızca %10 adımlarla yenilenir. Dosya zaten varsa indirilmez, "Aç" çıkar (kullanıcı kararı). Bitince kütüphane yeniden taranır. Boş alan `statvfs` ile her 1 MB'da ölçülür; diskte 20 MB'tan az yer kalacaksa indirme durur, yarım dosya silinir. Sınırlar: kuyruk yalnızca uygulama açıkken tutulur; uygulama indirme sırasında kapanırsa `.indiriliyor` uzantılı yarım dosya kalabilir (kütüphane onu göstermez, aynı kitap yeniden indirilince üzerine yazılır). Gutenberg'in kitap sayfasında aynı başlıklı iki girdi (resimli/resimsiz baskı) ayırt edilmeden listelenir ve aynı dosya adına iner.)
- [x] Ağ özelliklerini ayarlardan kapatabilme. (Ayarlar → "PDF ve internet" sayfası; kapalıyken kataloglar açılmaz, hiç bağlantı kurulmaz.)

### Kabul ölçütleri
- Geçerli katalogdan kitap listelenip indirilebiliyor.
- Hatalı sertifika veya bağlantı hatası güvenli biçimde reddediliyor.
- Kesintili bağlantı yerel kitapları veya notları etkilemiyor.

**Çıktı:** İnternet üzerinden katalog ve kitap indirme.

---

## M9 — Google Drive

**Kullanıcı kararı (2026-10-28): v2'ye bırakıldı.** Ana aktarım yolu USB; OPDS ve Calibre M8'de hazır. Aşağıdaki işler v2'de ele alınacak; araştırma sonuçları "v2'ye bırakılanlar" bölümünde.

**Amaç:** Drive kitaplık erişimini güvenli ve kararlı biçimde eklemek.

### İşler
- [ ] Google Drive API ve OAuth gereksinimlerini doğrula.
- [ ] Cihaza uygun yetkilendirme akışını belirle; gömülü tarayıcıda OAuth'un çalışacağını varsayma.
- [ ] Önce Drive'daki dosyaları listeleme ve indirme.
- [ ] Token saklama, yenileme, iptal ve bağlantıyı kaldırma akışı.
- [ ] İstenirse notlar ve okuma ilerlemesi için senkronizasyon ve çakışma çözümü.

### Kabul ölçütleri
- Kullanıcı kendi hesabını açık rızasıyla bağlayıp bağlantıyı kaldırabiliyor.
- Token'lar loglara yazılmıyor.
- İki yönlü senkronizasyon varsa çakışmalar sessizce veri kaybettirmiyor.

**Çıktı:** Drive kitaplık entegrasyonu; iki yönlü senkronizasyon ayrı bir alt kilometre taşı olabilir.

---

## M10 — Kobo Libra 2 üzerinde çalıştırma

**Amaç:** Simülatörde doğrulanmış uygulamayı gerçek cihazda güvenli ve geri alınabilir biçimde çalıştırmak.

### İşler
- [x] Kobo platform katmanı (`crates/kobo`, cihaz olmadan; 2026-10-28). Ekran: `/dev/fb0` bilgisi ioctl ile okunur (sabit kodlanmaz), boyut 1264×1680 değilse çizmeden Türkçe hatayla çıkar. Tuval sıradan dosya yazmasıyla aktarılır (mmap yok), 32 bit BGRA ya da 8 bit gri. E Ink yenilemesi MXCFB v2 (Mark 7): kısmi = GL16, tam = GC16, her kareden önce bir öncekinin bitmesi beklenir. `unsafe` yalnızca nix'in ioctl makrolarıyla dört çağrıda (kullanıcı kararı). Girdi: `/dev/input/event*` her biri ayrı iş parçacığında; adında "touch" geçen aygıt dokunma (çok noktalı B ve tek noktalı protokol, yalnızca ilk parmak), eksenler yer değiştirir (Cobalt ölçümü). Tuşlar: 193 geri, 194 ileri, güç (116) = kaydedip çık. Kütüphaneye "Çık" düğmesi eklendi (kullanıcı kararı). Yapı boyutları, ioctl numaraları (`0x4048462E`, `0xC008462F`), piksel dönüşümü, olay ayrıştırma ve dokunma eşlemesi testli; testler qemu-arm'da (32 bit düzen) da geçiyor. qemu-arm'da başlatıldığında ikili fontları yükledi, bilgisayarın kendi framebuffer'ını (1920×1200) doğru okudu ve çizmeyi reddetti. Cihazda henüz çalıştırılmadı.)
- [ ] Geri alma/kurtarma planını yaz ve kullanıcıyla gözden geçir. (Yazıldı: `docs/device-install.md`; kullanıcı onayı bekleniyor.)
- [x] KOReader ve Plato başlatma betiklerini okuyarak (değiştirmeden) Nickel'ı durdurma ve geri başlatma yöntemini incele. (2026-10-28, USB ile salt okuma. İkisi de Nickel'ın ortamından Wi-Fi değişkenlerini alıyor, `sync` sonrası Nickel ve yardımcılarını `killall -TERM` ile durduruyor, Nickel'ın kapanmasını bekliyor. Çıkışta Nickel'ı ya kendi betikleriyle (Wi-Fi'ı kapatıp, `PLATFORM`'a göre) yeniden başlatıyor ya da cihazı yeniden başlatıyor. Karar: ilk denemelerde dönüş yolu yeniden başlatma (`tools/kobo/legentia.sh`); NickelMenu'den doğrudan `cmd_spawn`, KFMon kullanılmıyor. Betik kendi kodumuz; Plato/KOReader'dan kopya yok.)
- [ ] Gerekirse SSH'yi kullanıcı onayıyla aç (`.kobo/ssh-disabled` → `ssh-enabled`; geri alma: eski ada döndürmek).
- [ ] CPU, RAM ve framebuffer bilgilerini cihazda salt okuma komutlarıyla doğrula.
- [ ] Uygulamayı `.adds/` altında ayrı bir klasöre USB ile kopyala; KOReader, Plato ve KFMon dosyalarına dokunma.
- [ ] Ayrı bir NickelMenu yapılandırma dosyasıyla başlat; mevcut `nm` dosyalarını düzenleme.
- [ ] Önce statik test ekranı, sonra dokunma ve sayfa tuşları, sonra EPUB ve PDF.
  - 1. deneme (2026-10-28):
    - Çalışanlar: program açıldı, kütüphane 284 ms'de tarandı, EPUB ve PDF çalıştı.
    - Dokunma yanlış yere düştü. Neden: Nickel ekranı `rotate: 3` (ters dikey) bırakmış; Cobalt yalnızca 1'i ölçmüştü.
    - Sayfa tuşları beklenen gibi değildi; aynı nedenle yerleri değişmiş olmalı.
    - Her ekran güncellemesi 1,6–2,3 sn sürdü (32 bit, kare başına 8,6 MB yazma). Çizim hızlıydı (sayfa başına 11–24 ms; yazı tipi önbelleği boşken ~2 sn).
    - Güç tuşu programı düzgün kapattı, ama betikteki `reboot` cihazı yeniden başlatmadı; kullanıcı cihazı zorla kapattı.
    - Kullanıcı isteği: okurken ortaya dokununca üst ve alt çubuklu menü.
  - Düzeltmeler (2. deneme için):
    - Dönüş 3'te dokunma iki eksende aynalanıyor, sayfa tuşlarının yeri değişiyor.
    - Ekran 8 bit griye geçiriliyor; yalnızca değişen satırlar yazılıp yenileniyor; her aşamanın süresi günlüğe yazılıyor.
    - Boyut kontrolü ekran ayarı değiştirilmeden önce yapılıyor.
    - Çıkışta "yeniden başlatılıyor" ekranı; betik `/sbin/reboot`'u günlüğe yazarak çağırıyor, 20 sn içinde olmazsa `reboot -f`.
    - Okuma menüsü üst ve alt çubuk oldu.
- [ ] Uyku/uyanma, Wi-Fi'ın Nickel'a geri verilmesi ve düşük pil senaryolarını dene.
- [ ] Masaüstünde belirlenen süre ve bellek hedeflerini cihazda yeniden ölç.
- [ ] OCR'ı cihaz uygulamasına taşı; M5'teki ön değerlendirmeye göre arka planda, kesilebilir ve pil durumunu gözeten biçimde çalıştır.

### Kabul ölçütleri
- Uygulama gerçek Libra 2 üzerinde başlıyor; ekran ve dokunma doğru çalışıyor.
- Uygulama kapandığında Nickel normal biçimde kullanılabiliyor.
- KOReader, Plato ve varsayılan Kobo okuyucusu çalışır durumda kalıyor.
- Kurulum ve kaldırma adımları test edilmiş.

**Çıktı:** Geri alınabilir cihaz kurulumu ve `docs/device-install.md`.

---

## M11 — Ek özellikler

Kullanıcı isteği (2026-10-28): 50 maddelik öneri listesi milestone olarak kaydedildi; ilk 10 madde önce yapılacak. Büyüklük tahmini: K küçük, O orta, B büyük.

**Kararlar ve notlar**
- Z-Library ve Anna's Archive kaynak olarak eklenmeyecek (kullanıcı kararı; telifli eserleri izinsiz dağıtıyorlar). Yasal ek kaynak önerileri: Vikikaynak (Türkçe, EPUB), Internet Archive / Open Library (kamu malı), Feedbooks kamu malı kataloğu, ManyBooks; Kültür Bakanlığı e-kitapları (şartları doğrulanacak).
- Uygulama dağıtımı için kullanıcının önerisi: Cobalt'ın uygulama mağazası (github.com/BandarLabs/Cobalt). Mağazaya yalnızca Cobalt SDK'sı ve çalışma ortamıyla yazılmış, Cobalt deposuna PR ile giren uygulamalar girebiliyor; Cobalt AGPL-3.0. **Kullanıcı kararı (2026-10-28): Cobalt'a girilmeyecek; kendi dağıtımımız** (GitHub sürümleri / MobileRead, NickelMenu ile kurulum; madde 48 ve 50). Gerekçe, Cobalt SDK belgesinden: uygulama kendi klasörü dışında dosya açamıyor (kitaplara erişim için gereken `shared-files` yetkisi "ayrılmış"), serbest çizim ve yazı tipi seçimi yok, ham dokunma koordinatı yok, ekran yenileme denetimi yok; kod Cobalt deposuna AGPL altında girip Cobalt anahtarıyla imzalanıyor. Legentia'nın dizgi, PDF, seçim ve dokunma özellikleri bu modelde yapılamaz; M1'deki MIT ve "Cobalt kodu kullanılmayacak" kararlarıyla da çelişir.

### Okuma deneyimi
- [x] 1. Kitap içinde metin arama, sonuçlar arasında gezinme (O) (Kullanıcı kararı: sonuç listesi. Üst çubukta "Ara"; bütün bölümler/PDF sayfaları taranır (en çok 300 sonuç), sonuçlar kelimenin çevresi ve bölüm adıyla listelenir; dokununca sayfa açılır, bulunan yer gri işaretli, sol üstte "← Sonuçlar". Büyük/küçük harf ve şapka fark etmez; noktalı/noktasız i aynı sayılır (PDF'in dili bilinmez, İngilizce "Index" Türkçe kuralla bulunamıyordu). Arama eşzamanlı: cihazda büyük kitapta birkaç saniye sürebilir. Testli.)
- [x] 2. Dipnotlar sayfadan çıkmadan açılır pencerede (O) (`epub:type="noteref"`/`role="doc-noteref"` ya da en çok 5 karakterlik bağlantı metni dipnot sayılır; not altta pencerede, "Notun yerine git" ile notun sayfasına gidilir, dışına dokunmak kapatır. Testli.)
- [x] 3. Kitap içi bağlantılar ve "geri dön" (O) (Dokunulan bağlantının hedef bölümü açılır, `id`/`<a name>` taşıyan bloğun sayfasına gidilir; sol üstte "← Geri" (en çok 20 adım). Dipnot işaretleri küçük olduğundan bağlantının iki yanına 30 px dokunma payı. Dış bağlantılar açılmaz. Testli.)
- [x] 4. EPUB görselleri ve tabloları, dokununca büyütme (B) (JPEG/PNG görseller sayfaya sığdırılır (en çok 2 kat büyür), SVG kapaklardaki `<image>` dahil; uzun basınca tam ekran, dokununca kapanır. Kısa dokunma yerine uzun basma: sayfayı kaplayan kapakta dokunarak sayfa çevrilemezdi. GIF/SVG çizimi yerine alt metin. Tablolar: satır başına paragraf, hücreler " | ", başlık hücresi kalın. `image` crate'i hayro'nun sürümüyle (yeni crate yok). Testli; cihazda görsel çözme süresi ölçülecek.)
- [x] 5. Kitaba özel yazı ayarı (K) (Kullanıcı kararı: her değişiklik kitaba özel. Kitap açıkken ayarlar ekranındaki yazı ve PDF görünüm ayarları yalnız o kitaba yazılır (`BookState.layout`, yedeğe girer); gece modu, dokunma düzeni, internet ve çevrimiçi sözlük genel kalır. Başlıktaki "Varsayılan yap" ayarları bütün kitapların varsayılanı yapar ve kitabın kendi ayarını siler. Testli.)
- [x] 6. Yazı kalınlığı ayarı (K) (Yazı ayarlarında Normal / Biraz kalın / Kalın / Çok kalın; yalnızca kitap metni. Çizilmiş glifin kenarları koyulaştırılır (gamma), en üst düzeyde bir piksel genişletilir. swash'ın kontur kalınlaştırması denendi, bileşik gliflerde (Literata'nın "i"si) bileşenleri atladığı için bırakıldı. "Gece modu" üçüncü sayfaya taşındı (yazı sayfasında önizleme düğmelere biniyordu). Testli; E Ink'te görünüşü cihazda değerlendirilecek.)
- [x] 7. Yatay mod, PDF'te iki sayfa yan yana (O) (Kullanıcı kararları: yalnızca okuma ekranı ve üstüne açılanlar döner, ayarlar/listeler/formlar dikey kalır; döndürmeyi sürücü yapar. 7a yapıldı: ayarlarda "Okurken ekran yönü" Otomatik/Dikey/Yatay ve "Yatayda EPUB" iki/tek sütun; çekirdekte ekran boyutu değişken, yatayda tek geniş sütun; Kobo'da ivmeölçer (açılıştaki baskın eksen dikey sayılır, yön 3 ardışık okumada kararlı olunca bildirilir) ve framebuffer dönüşü (çıkışta geri yüklenir); simülatörde R tuşu. 7b yapıldı: yatayda EPUB iki sütun (bölüm sütun genişliğinde dizilir, iki sütun yan yana, çevirme iki sütun atlar; seçim, vurgu, bağlantı ve görsel her iki sütunda). 7c yapıldı: yatayda "Sayfaya sığdır" görünümündeki PDF iki sayfa yan yana (1–2, 3–4…; her biri ekranın yarısına sığar, çevirme iki sayfa atlar, alt bilgi "Sayfa 3–4/120"). Testli; yatay okumanın tamamı cihazda denenmedi.)
- [x] 8. Bölümün ve kitabın kalan okuma süresi tahmini (K) (Alt bilgide "bölüm 12 dk · kitap 3 sa 20 dk". Hız kullanıcının ileri çevirmelerinden öğrenilir (sayfada 3 sn–10 dk kalınanlar; menü ve sözlükte geçen süre sayılmaz), kitaplar arası `okuma-hizi.json`; en az 3 ölçümden sonra gösterilir. Kitabın kalanı EPUB'da bölüm dosyası boyutlarından (açık bölümün karakter/bayt oranıyla), PDF görüntüsünde ekran başına süreden. Testli.)
- [x] 9. Kaydırarak sayfa çevirme, iki parmakla punto değiştirme (O) (Okurken yatay kaydırma (en az 150 px, dikeyin iki katından fazla) sola ileri, sağa geri; liste ekranlarında sayfa değiştirir. İki parmak 1,15 kattan fazla açılınca punto bir adım büyür, 0,87'den fazla kapanınca küçülür (PDF görüntüsünde yakınlaştırma); kitaba özel ayar olarak. Kobo girdisi ilk iki yuvayı izler. Simülatörde + / - tuşları. Testli; Libra 2'nin iki parmağı bildirdiği cihazda doğrulanmadı.)
- [x] 10. Dokunma bölgelerini özelleştirme (K) (Ayarlara üçüncü sayfa "Okuma ve ekran"; "Dokunma düzeni": sol geri/sağ ileri, ters, tek el (sol kenar şeridi geri, üst şerit menü, gerisi ileri). Testli.)
- [x] 11. Sayfa tuşlarının yönünü ayardan değiştirme (K) ("Okuma ve ekran" sayfasında "Sayfa tuşları": üst geri / üst ileri; bütün ekranlarda geçerli. Testli.)
- [x] 12. Otomatik sayfa çevirme zamanlayıcısı (K) (Kullanıcı kararı: menüden başlat, süre ayarlı. Alt çubukta "Otomatik çevir" / "Otomatiği durdur"; aralık ayarlarda 10–120 sn ya da "okuma hızıma göre" (sayfadaki karakter ÷ öğrenilen hız; hız bilinmiyorsa 30 sn). Dokunma/tuş durdurur, kitap sonunda durur; otomatik çevirmeler okuma hızına katılmaz. Testli.)
- [ ] 13. Sağdan sola yazı (Osmanlıca, Arapça) (B)
- [ ] 14. Japonca dikey yazı ve furigana (B)
- [x] 15. İngilizce ve diğer diller için heceleme (O) (Kullanıcı kararı: TeX hyph-utf8 desenleri. İngilizce, Almanca, Fransızca, İspanyolca, İtalyanca; Liang algoritması kendi kodumuz, desenler gömülü ve ilk kullanımda ayrıştırılır. Lisanslar dil dil kontrol edildi (MIT / telifsiz izin); Rusça yalnızca LPPL olduğu için alınmadı. TeX çıktısıyla aynı bölünmeler testli.)

### Cihaz (Libra 2)
- [ ] 16. Ön ışık ve sıcak ışık ayarı (O)
- [ ] 17. Uyku/uyanma; uyku ekranında kitabın kapağı (O)
- [ ] 18. Üst çubukta pil ve saat (K)
- [ ] 19. Otomatik uyku zamanlayıcısı (K)
- [ ] 20. Uygulama içinden Wi-Fi açma, kapama ve ağ seçme (B)
- [ ] 21. Nickel'a yeniden başlatmadan, hızlı dönüş (O)
- [ ] 22. Ekran görüntüsü alma (K)

### Kütüphane
- [x] 23. Kütüphanede arama ve sıralama (K) (Kullanıcı kararı: başlığın altında araç çubuğu. Ara (başlık/yazar; büyük/küçük harf, şapka, noktalı i fark etmez; yeniden dokununca kaldırılır), sıra: son okunan / başlık / yazar / eklenme (dokundukça değişir, kalıcı). Testli.)
- [x] 24. Kapaklı ızgara görünümü (O) (Araç çubuğunda "Kapaklar"/"Liste" düğmesi; 3×3 ızgara, kapağın altında başlık ve ilerleme/durum. Kapak EPUB'da OPF'teki `cover-image` ya da EPUB2 `<meta name="cover">`, PDF'te ilk sayfa; arka planda üretilip `veri/kapaklar/<kimlik>.png` olarak saklanır, bellekte yalnızca görünen sayfanınkiler. Kapağı olmayan kitapta başlıklı düz kapak. Masaüstünde gerçek kitaplarla denendi; cihazda üretim süresi ölçülecek.)
- [x] 25. Koleksiyonlar ve raflar (O) (Kullanıcı kararı: kitaplık görünümü, klasörler raf. Araç çubuğundaki görünüm düğmesi Liste → Kapaklar → Raflar arasında döner. Her klasör bir raf (ana klasör önce, sonra Türkçe ada göre, yanında kitap sayısı); kitaplar rafta sırt sırta, ad sırtta aşağıdan yukarı en çok iki satır. Sırt boyu ve tonu kitaptan kitaba değişir; bitmiş kitabın sırtında üstte koyu bant. Sığmayan kitaplar rafın alt sırasına geçer, sayfa çevrildikçe alttaki raflar gelir. Sırta dokununca kitap açılır, uzun basınca kitap işlemleri; rafa koymak = "Klasöre taşı". Arama, süzgeç ve sıralama raflarda da geçerli.)
- [x] 26. Okuma durumu: okunacak / okunuyor / bitti (K) (Araç çubuğunda süzgeç: hepsi / okunuyor / okunacak / bitti. Kitaba uzun basınca durum seçilir; seçilmediyse ilerlemeden çıkar (açılmamış okunacak, açılmış okunuyor, %100 bitti). Kitap kaydında, yedeğe girer. Testli.)
- [ ] 27. Seri bilgisi ve seri sırası (O)
- [x] 28. Cihazdan kitap silme ve taşıma (K) (Kitaba uzun basınca "Kitabı sil" (onaylı; yalnızca dosya silinir, okuma konumu ve notlar kayıtta kalır) ve "Klasöre taşı" (kök ve gizli olmayan alt klasörler, iki düzey; hedefte aynı adlı dosya varsa taşınmaz). Testli.)
- [ ] 29. Kitap bilgilerini düzenleme (O)
- [ ] 30. FB2, TXT, HTML (O)
- [ ] 31. DRM'siz MOBI/AZW3 (B)
- [ ] 32. Çizgi roman (CBZ/CBR) (O)
- [ ] 33. DjVu (B)

### Not ve öğrenme
- [x] 34. Vurgu türleri: altı çizili, çerçeveli, etiketli (K) (Vurguya dokununca çubukta "Tür": gri → altı çizili → çerçeveli; seçilen tür yeni vurgulara da uygulanır (`highlight` ayarı). "Etiket": kısa etiket yazılır, boş bırakılınca kalkar; vurgu listesinde gösterilir ve aramaya girer, Markdown dışa aktarmada "Etiket:" satırı. Eski kayıtlar gri olarak açılır. Notlu PDF'e aktarmada tür yok sayılıyor, hepsi vurgu olarak yazılıyor.)
- [x] 35. Notlarda ve vurgularda arama (K) (Vurgular ekranının başlığında "Ara": alıntıda ve not metninde, kitap içi aramayla aynı harf kurallarıyla; yeniden dokununca kaldırılır. Testli.)
- [x] 36. Kelime listesinden Anki kartları (CSV) (K) (Kelime listesi başlığında "Anki'ye aktar": kütüphane klasörüne "Legentia kelimeler.csv" (Anki başlık satırlarıyla, noktalı virgüllü). Ön yüz kelime; arka yüz cihazdaki sözlüklerden ilk tanım (en çok 600 karakter), metindeki biçim ve kitap. Testli; Anki'de içe aktarma elle denenmedi.)
- [ ] 37. Cihazda aralıklı tekrarlı kelime çalışması (O)
- [ ] 38. Seçili cümleyi çevrimiçi çevirme (O)
- [x] 39. Seçili kelimeyi Türkçe Vikipedi'de arama (K) (Seçim çubuğunda "Vikipedi": önce Vikipedi araması (çekimli biçimi de bulur: "İstanbul'dan" → İstanbul), sonra maddenin özeti tanım ekranında, kaynak "Vikipedi (CC BY-SA)". Arka planda, "Vazgeç" ile bırakılır; internet kapalıyken açıklama. Gerçek API'nin yanıt biçimi curl ile doğrulandı; uygulama testi taklit sunucuyla.)
- [x] 40. Okuma istatistikleri (O) (Kütüphanedeki "Yedek" düğmesi "Araçlar" oldu; içinde "Okuma istatistikleri": bugün, son 7 gün, toplam (süre ve sayfa), art arda okunan gün, dakikada kelime (okuma hızından, 7 karakter = 1 kelime), son 14 günün çubuk grafiği, en çok okunan 5 kitap. Süre okuma ekranında sayılır (sayfa çevrilince, okumadan çıkınca, kapanışta); 10 dakikadan uzun bekleme ara sayılır; otomatik çevirmede süre sayılır ama hız ölçümüne katılmaz. `istatistik.json`, günler UTC.)

### Bağlantı ve eşitleme
- [ ] 41. KOReader eşitleme sunucusuyla okuma konumu eşitleme (O)
- [ ] 42. Calibre "kablosuz aygıt" bağlantısı (B)
- [ ] 43. Cihazda küçük web sayfası; telefondan/bilgisayardan kitap yükleme (O)
- [ ] 44. WebDAV / Nextcloud kütüphanesi (O)
- [ ] 45. Dropbox (uygulamaya özel klasör; şartlar doğrulanacak) (O–B)
- [ ] 46. Web makalelerini EPUB olarak okuma (Wallabag) (O)
- [ ] 47. RSS haber okuyucu (O)

### Sistem
- [ ] 48. Uygulama içinden güncelleme (O)
- [ ] 49. İngilizce arayüz (K)
- [x] 50. Tek komutla kurma ve kaldırma betiği (K) (`tools/kobo/kur.sh`: aygıtın Kobo olduğunu denetler (Libra 2 değilse sorar), paketler, yalnızca Legentia dosyalarını kopyalar ve doğrular; güncellemede `veri/` korunur. `--kaldir` (veri/ kalır), `--verileri-de-sil`. Sahte Kobo klasöründe denendi; diğer eklentilerin dosyalarına dokunmadığı doğrulandı.)

---

## v2'ye bırakılanlar

Kullanıcı kararı (2026-10-28): uygulama bittikten sonra v2 güncellemesinde ele alınacak.

- Sözlük: LZO sıkıştırmalı ve GBK/Big5 kodlamalı MDict; DSL dizininin diske önbelleklenmesi; sözlüğün dil bilgisinin gösterilmesi; diğer diller için kök bulma (Almanca, Rusça, Japonca).
- Sözlük: şifreli sözlükler (Kobo'nun resmî dicthtml'leri, kayıt kodlu MDict) kopya koruması nedeniyle desteklenmeyecek; v2'de de ancak kullanıcının yasal olarak açabildiği bir yol varsa.
- Vurgu/not: taranmış (OCR'lı) sayfa görüntüsünde seçim; notta imleci metin ortasına taşıma.
- Google Drive (M9). Araştırma (2026-10-28, Google belgeleri): tarayıcısız cihaz akışı (google.com/device + kod) yalnızca `drive.file` ve `drive.appdata` izinlerini kabul ediyor; `drive.file` uygulamanın kendi oluşturmadığı dosyaları göremez, yani Drive'daki mevcut kitaplar listelenemez (ama not/ilerleme/yedek eşitlemesine yeter). Bütün Drive'ı okumak (`drive.readonly`) "kısıtlı" izin: herkese açık uygulamada Google doğrulaması gerekir, cihaz akışında hiç verilmez; ancak bilgisayarda tarayıcıyla (loopback) alınabilir. "Test" durumundaki projede yetki 7 günde sona erer; "Yayında" ama doğrulanmamış projede giriş uyarısı çıkar, en çok 100 kullanıcı. Her yol kullanıcının kendi Google Cloud projesini ve istemci kimliğini gerektirir. Seçenekler: bilgisayarda bir kez giriş + USB ile yetki dosyası; bağlantıyla paylaşılan klasör + API anahtarı (OAuth'suz, ama bağlantıyı bilen herkes görür); cihazda kodla giriş yalnızca eşitleme için.
- PDF: sütun modunda neredeyse boş sayfaların (kapak) yanlış bölünmesi; sütunları aşan uzun başlıklı sayfalarda sütun bulma.

## Her sürüm için genel tamamlanma ölçütleri

- [ ] Derleme ve testler geçiyor.
- [ ] Kullanıcıya görünen metinler Türkçe ve katalogdan geliyor.
- [ ] Yeni özellik simülatörde test edildi; cihazda doğrulanmadığı açıkça belirtildi (M10'a kadar).
- [ ] Hatalar ve bilinen sınırlamalar belgelenmiş.
- [ ] Kullanıcı verileri için yedekleme/geri yükleme düşünülmüş.
- [ ] Bağımlılık lisansları ve kaynakları kaydedilmiş.
- [ ] Bellek, ekran yenileme ve çevrimdışı davranış değerlendirilmiş.

## Öncelik sırası

**M0 → M1 → M2 → M3 → M4 → M5 → M6 → M7 → M8 → M9 → M10**

Bilinen risk: cihaza özgü sorunlar (performans, yenileme davranışı, TLS) ancak M10'da ortaya çıkar. Bu risk kullanıcının bilinçli tercihidir. Etkisini azaltmak için her aşamada ARMv7 musl derlemesi de kontrol edilir.
