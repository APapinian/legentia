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
- [ ] EPUB ZIP paketini güvenli biçimde aç (path traversal, zip bomb ve boyut sınırları).
- [ ] Container/OPF, manifest ve spine bilgisini işle.
- [ ] XHTML içeriğini güvenli biçimde parse et.
- [ ] Kitap başlığını, yazarını ve bölüm listesini göster.
- [ ] Bir bölümün metnini göster; önceki/sonraki bölüme geç.
- [ ] Bozuk veya desteklenmeyen dosyalarda anlaşılır Türkçe hata göster.
- [ ] Kullanılacak crate'lerin lisansını ve ARMv7 musl uyumluluğunu kontrol et.

### Kabul ölçütleri
- Türkçe karakterli kitaplar dahil en az 5 farklı gerçek EPUB açılıyor.
- Bozuk dosya testleri (kesik ZIP, eksik OPF, hatalı XHTML) uygulamayı çökertmiyor.
- Kitap açma süresi ve bellek kullanımı ölçülmüş. `[ölçülecek]`

**Çıktı:** Simülatörde çalışan EPUB metin görüntüleyici.

---

## M3 — Metin düzeni (layout) ve sayfalama

**Amaç:** Türkçe için doğru, ileride diğer yazı sistemlerine genişletilebilir bir metin düzeni motoru kurmak.

### İşler
- [ ] Font yükleme, ölçüm ve harf birleştirme (shaping) altyapısını seç; seçimde sağdan sola (Osmanlıca/Arapça) ve CJK (Japonca) desteğinin sonradan eklenebilmesini şart koş.
- [ ] Türkçe karakterleri tam kapsayan, lisansı uygun (ör. OFL) fontları seç ve belgele.
- [ ] Unicode satır kırma kurallarını uygula.
- [ ] Türkçe heceleme (tireleme) ekle; iki yana yaslı metinde boşluk dağılımını kontrol et.
- [ ] Punto, yazı tipi, satır aralığı, kenar boşlukları ve hizalamayı ayarlanabilir yap.
- [ ] Paragraf, başlık, italik/kalın ve temel bağlantı biçimlendirmesini uygula.
- [ ] Ekran boyutuna göre sayfalama yap; sayfa konumunu kararlı bir içerik konumuyla ilişkilendir.
- [ ] E Ink için yenileme planı: sayfa çevirmede kısmi yenileme, belirli aralıkla tam yenileme.
- [ ] Çok uzun kelimeler, tablolar, görseller, dipnotlar ve CSS sınırlamalarını test et.

### Kabul ölçütleri
- Yazı ayarları değişince sayfalama yeniden hesaplanıyor ve okuma konumu korunuyor.
- `İ/ı`, `I/i`, `ğ, ü, ş, ö, ç` ve Unicode normalizasyon testleri geçiyor.
- Türkçe heceleme için örnek kelime listesiyle birim testleri var.
- Sayfa çevirme süresi ve bölüm sayfalama süresi ölçülmüş. `[ölçülecek]`
- Desteklenmeyen EPUB özellikleri belgelenmiş.

**Çıktı:** Simülatörde okunabilir sayfalama.

---

## M4 — Temel okuyucu ve tam Türkçe arayüz

**Amaç:** Günlük okumada kullanılabilecek ilk sürümü simülatörde tamamlamak.

### İşler
- [ ] Yerel depolama katmanını seç (SQLite veya gerekçelendirilmiş alternatif); şema ve migration stratejisini belirle.
- [ ] Kütüphane ekranı: USB ile `/mnt/onboard` altına kopyalanan kitapları tara ve listele (simülatörde bir klasör).
- [ ] Okuma ekranı, menüler ve ayarlar.
- [ ] İçindekiler, bölüm/sayfa gezinmesi, okuma konumu ve yer imleri.
- [ ] Ayarların ve ilerlemenin kalıcı saklanması.
- [ ] E Ink'e uygun kontrast ve ters renk seçeneklerini değerlendir.
- [ ] Türkçe imla, tarih, sayı biçimi ve hata mesajlarını gözden geçir; Türkçe sıralama ve aramayı uygula.

### Kabul ölçütleri
- Kullanıcıya görünen İngilizce metin kalmıyor.
- Ayarlar, okuma konumu, yer imleri ve kütüphane bilgileri yeniden açılışta korunuyor.
- Türkçe karakterli dosya adları ve kitap başlıkları doğru sıralanıyor.
- Bellek tavanı belirlenmiş ve büyük kitaplarla test edilmiş. `[ölçülecek]`

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
