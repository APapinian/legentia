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
- [ ] Bu projenin kendi lisansını belirle.

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
- [ ] PDF motorunu araştır ve seç: görüntü kalitesi, hız, bellek, ARMv7 musl uyumluluğu ve lisans (ör. MuPDF AGPL, PDFium BSD, saf Rust seçenekleri).
- [ ] Yüksek kaliteli render: gri ton için doğru kenar yumuşatma ve E Ink'e uygun kontrast/gamma.
- [ ] Yakınlaştırma, kenar boşluğu kırpma (otomatik ve elle), sayfaya/genişliğe sığdırma, sütun modu.
- [ ] Metin katmanı olan PDF'lerde metni çıkar ve okuyucunun layout motoruyla yeniden akıt (reflow).
- [ ] Taranmış PDF'ler için OCR: sayfa düzenini (sütunlar, başlıklar, paragraflar) ve metni tanı; sonuca göre yeniden akıt veya bölgeleri yeniden ölçekle.
- [ ] OCR'ı önce NixOS'ta deney amaçlı çalıştır (Türkçe dil paketiyle); doğruluk ve süreyi ölç.
- [ ] OCR metnini PDF'e görünmez, aranabilir metin katmanı olarak göm; orijinal dosyayı koru, yeni dosyayı ayrı yaz.
- [ ] Notları ve vurguları PDF'e standart PDF açıklamaları (annotation) olarak yaz; başka PDF okuyucularında da görünmeli.
- [ ] Cihazda OCR için ön değerlendirme: tek çekirdekli ARMv7 ve kısıtlı RAM'de sayfa başı süre ve bellek tahmini. Uygulama M10'da.

### Kabul ölçütleri
- Metin PDF'i, taranmış PDF, çok sütunlu PDF ve büyük PDF test kümeleri var.
- Yeniden akıtılmış metinde Türkçe karakterler ve paragraf sırası doğru.
- OCR doğruluğu Türkçe örnek sayfalarda ölçülmüş. `[ölçülecek]`
- Büyük PDF'lerde bellek ve sayfa render süresi ölçülmüş. `[ölçülecek]`
- PDF desteğinin sınırları belgelenmiş.
- OCR katmanı gömülmüş PDF başka bir PDF okuyucusunda aranabiliyor ve seçilebiliyor.
- PDF'e yazılan notlar ve vurgular başka bir PDF okuyucusunda görünüyor.
- Yazma işlemi yarıda kesilse bile orijinal PDF bozulmuyor.

**Çıktı:** Simülatörde PDF görüntüleme, reflow ve OCR akışı.

Not: PDF'e not yazma, M6'daki ortak konumlandırma modeline bağlıdır; M5'te PDF tarafının yazma altyapısı, M6'da arayüzü tamamlanır.

---

## M6 — Vurgular ve notlar

**Amaç:** EPUB ve PDF için tek bir konumlandırma modeliyle kalıcı kişisel çalışma araçları sağlamak.

### İşler
- [ ] EPUB ve PDF için ortak içerik konumlandırma modeli tasarla.
- [ ] Metin seçimi, vurgulama ve seçili metne not ekleme/düzenleme.
- [ ] Notları kitap, bölüm ve konum bilgisiyle ilişkilendir.
- [ ] Notları dışa aktarma ve yedekleme/geri yükleme.

### Kabul ölçütleri
- Uygulama kapanıp açılınca notlar ve vurgular korunuyor.
- Yazı ayarı değişince vurgular doğru metne bağlı kalıyor.
- Kitap dosyası değiştiğinde konum eşleşmesinin sınırları ele alınmış.
- Yedekten geri yükleme test edilmiş.

**Çıktı:** Yerel not ve vurgu sistemi.

---

## M7 — Sözlük ve kelime araştırması

**Amaç:** Kelime seçildiğinde hızlı, genişletilebilir ve çevrimdışı çalışabilen sözlük deneyimi sunmak.

### İşler
- [ ] Sözlük sağlayıcıları için ortak Rust arayüzü tanımla.
- [ ] İlk yerel sözlük formatını ve lisans uygunluğunu seç.
- [ ] Türkçe kök bulma: çekimli kelimeden sözlük maddesine ulaş (ör. "kitaplarımızdan" → "kitap").
- [ ] Kelime arama, tanım, dil ve kaynak bilgisini göster; kelimeyi kişisel listeye kaydet.
- [ ] Diğer diller sonradan: Almanca bileşik kelimeler, Rusça çekimler ve Japonca kelime sınırları ayrı ayrı ele alınacak.
- [ ] Çevrimiçi sağlayıcıları timeout, cache ve çevrimdışı hata davranışıyla ekle.

### Kabul ölçütleri
- En az bir Türkçe sözlük tamamen çevrimdışı çalışıyor.
- Çekimli Türkçe kelimeler için kök bulma testleri geçiyor.
- İnternet olmadığında uygulama kilitlenmiyor ve Türkçe açıklama gösteriyor.

**Çıktı:** Yerel ve çevrimiçi sözlük sistemi.

---

## M8 — İnternet, OPDS ve Calibre

**Amaç:** Kitap keşfi ve indirme için açık ağ protokollerini eklemek. Ana kitap aktarım yolu USB'dir; bu aşama ek bir kolaylıktır.

### İşler
- [ ] HTTPS ve TLS sertifika doğrulaması.
- [ ] OPDS katalog tarama ve kitap indirme.
- [ ] Calibre Content Server uyumluluğu.
- [ ] İndirme kuyruğu, ilerleme, iptal, yeniden deneme ve disk alanı kontrolü.
- [ ] Ağ özelliklerini ayarlardan kapatabilme.

### Kabul ölçütleri
- Geçerli katalogdan kitap listelenip indirilebiliyor.
- Hatalı sertifika veya bağlantı hatası güvenli biçimde reddediliyor.
- Kesintili bağlantı yerel kitapları veya notları etkilemiyor.

**Çıktı:** İnternet üzerinden katalog ve kitap indirme.

---

## M9 — Google Drive

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
- [ ] Geri alma/kurtarma planını yaz ve kullanıcıyla gözden geçir.
- [ ] KOReader ve Plato başlatma betiklerini okuyarak (değiştirmeden) Nickel'ı durdurma ve geri başlatma yöntemini incele.
- [ ] Gerekirse SSH'yi kullanıcı onayıyla aç (`.kobo/ssh-disabled` → `ssh-enabled`; geri alma: eski ada döndürmek).
- [ ] CPU, RAM ve framebuffer bilgilerini cihazda salt okuma komutlarıyla doğrula.
- [ ] Uygulamayı `.adds/` altında ayrı bir klasöre USB ile kopyala; KOReader, Plato ve KFMon dosyalarına dokunma.
- [ ] Ayrı bir NickelMenu yapılandırma dosyasıyla başlat; mevcut `nm` dosyalarını düzenleme.
- [ ] Önce statik test ekranı, sonra dokunma ve sayfa tuşları, sonra EPUB ve PDF.
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
