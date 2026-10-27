# Rust ile Kobo Libra 2 için Türkçe e-kitap okuyucu — Proje Aktarım Belgesi

## 1. Projenin amacı

Kobo Libra 2 gibi eski Kobo e-kitap okuyucularında çalışabilecek, **tamamen Türkçe arayüzlü**, Rust ile sıfırdan geliştirilen bir e-kitap okuyucu oluşturmak.

Uygulama KOReader'ın kodunu temel almayacak. Hedef, kendi okuma deneyimini ve uygulama mimarisini geliştirmek. Mevcut KOReader kurulumu korunmalı; yeni uygulama ilk aşamada ayrı ve geri alınabilir biçimde başlatılmalı.

## 2. Kullanıcının mevcut durumu

- Hedef cihaz: **Kobo Libra 2**.
- Firmware **4.38.23697**, çekirdek 4.1.15, cihaz kodu 388 (doğrulandı; ayrıntılar `docs/device-compatibility.md`).
- Kullanıcı cihazda zaten **KOReader** ve **Plato** kullanıyor; ikisi de **NickelMenu → KFMon** üzerinden başlatılıyor.
- Cihazda 22 GB boş alan var. Kitaplar USB ile aktarılacak.
- Kullanıcı ayrıca NixOS ve Rust ile çalışmış durumda; geliştirme ortamı NixOS dizüstü bilgisayar.
- Kullanıcı uygulamanın baştan sona Türkçe olmasını istiyor; diğer arayüz dilleri sonra eklenecek.
- Projenin sıfırdan Rust ile geliştirilmesini istiyor; KOReader fork'u istemiyor. Amaç Plato'nun bir kopyasını yapmak değil, kendi Rust mimarisini kurup test etmek.
- Google Drive için istenen kapsam henüz netleşmedi: sadece Drive'dan kitap indirip okumak mı, yoksa kitaplar, notlar ve okuma ilerlemesi için iki yönlü senkronizasyon mu? İkisini de destekleyecek şekilde tasarım düşünülebilir; kapsam kullanıcıyla netleştirilmeli.

### Referans projeler (yalnızca okunacak)
- [Cobalt](https://github.com/BandarLabs/Cobalt): Kobo için Rust SDK, çalışma ortamı ve simülatör. Kullanıcının cihazıyla aynı model ve firmware'de donanım testi yapılmış. AGPL-3.0.
- [Plato](https://github.com/baskerville/plato): Kobo için Rust ile yazılmış belge okuyucu. AGPL-3.0.
- Karar: ikisi de yalnızca referans. Kodları kopyalanmayacak; uygulama kendi Rust koduyla yazılacak.

## 3. Temel gereksinimler

### Zorunlu
- Arayüz, ayarlar, yardım, hata mesajları ve kullanıcıya görünen metinler Türkçe.
- Kobo Libra 2'nin donanım ve yazılım kısıtlarına uygun çalışma.
- Rust tabanlı uygulama.
- EPUB okuma ve sayfalama.
- E Ink ekran için kontrollü yenileme ve düşük pil tüketimi.
- Dokunmatik ekran desteği.
- Okuma konumu, yer imleri, vurgular ve notların yerel kaydı.
- KOReader ve mevcut Kobo sistem kurulumunu bozmadan, geri alınabilir kurulum.
- İnternet olmadan temel okuma işlevlerinin çalışması.

### İstenen gelişmiş özellikler
- Çoklu sözlük desteği: Türkçe, İngilizce, Almanca, Fransızca, Rusça, Japonca ve gerektiğinde Osmanlı Türkçesi gibi diller.
- Çevrimdışı sözlük paketleri ve çevrimiçi sözlük/API sağlayıcıları.
- Kelime arama, tanım, çeviri, telaffuz veya ses desteği mümkünse.
- İnternet üzerinden kaynak arama ve araştırma paneli.
- Google Drive entegrasyonu: kitap listeleme/indirme; isteğe bağlı notlar ve okuma ilerlemesi senkronizasyonu.
- Calibre ve OPDS kataloglarıyla bağlantı.
- Kitaplıkta yazar, seri, etiket, tür ve okuma durumuna göre düzenleme.
- Alıntı ve notları dışa aktarma.
- EPUB'dan hemen sonra **yüksek kaliteli PDF desteği**: iyi görüntüleme ve ölçeklendirme, kenar kırpma, metin PDF'lerinde yeniden akıtma; taranmış PDF'lerde OCR ile metni ve sayfa düzenini anlayıp buna göre ölçeklendirme; OCR sonucunu PDF'e aktarma.
- Libra 2 ekranını taklit eden masaüstü simülatörü (zorunlu; tüm geliştirme ve test önce burada yapılır).

## 4. Önerilen mimari

Bileşenleri ayrı modüller olarak tasarla:

1. **Uygulama çekirdeği (Rust):** Uygulama durumu, gezinme, ayarlar, kitaplık ve iş akışları.
2. **EPUB katmanı:** EPUB arşivini açma, OPF/manifest/spine ve XHTML/CSS içeriğini işleme.
3. **Metin düzenleme ve sayfalama:** Font ölçümleri, satır sarma, sayfa hesaplama, Türkçe karakterler ve RTL/özel yazım durumları için gereken altyapı.
4. **Kobo platform katmanı:** Ekran, dokunmatik giriş, cihaz bilgisi, uyku/uyanma ve E Ink yenileme davranışı. Önce Libra 2 üzerinde gerçekten kullanılabilir API'ler araştırılmalı; donanım erişimi varsayılmamalı.
5. **Yerel veri katmanı:** SQLite veya uygun, hafif bir depolama katmanı; kitap meta verileri, son konum, yer imleri, notlar, vurgular ve senkronizasyon kuyruğu.
6. **Sözlük sağlayıcıları:** Ortak bir sağlayıcı arayüzü; yerel sözlük paketleri ile çevrimiçi sağlayıcıları birbirinden ayır.
7. **Ağ katmanı:** HTTPS, zaman aşımı, yeniden deneme, çevrimdışı durum, önbellek ve veri tüketimini sınırlama.
8. **Bulut eşitleme:** Google Drive OAuth/API istemcisi, kimlik doğrulama akışı ve çakışma çözümü. OAuth'un Kobo'nun eski web bileşenlerinde sorun çıkarabileceği göz önünde bulundurulmalı; bilgisayardan yetkilendirme/eşleştirme gibi alternatifler araştırılmalı.
9. **Masaüstü simülatörü:** Mümkünse aynı çekirdek kodu ile geliştirilen, gerçek cihazdan bağımsız test arayüzü.

İlk sürüm için sade ve kaynak açısından tutumlu bir arayüz tercih et. Kobo Libra 2'nin sınırlı CPU/RAM'i ve E Ink ekranı nedeniyle ağır web arayüzlerinden, gereksiz animasyonlardan ve sürekli ağ bağlantısından kaçın.

## 5. Tasarım ilkeleri

- Türkçe birinci sınıf dil desteği olmalı; çeviri sonradan eklenen bir yama olmamalı.
- Okuma ekranı sade, yüksek kontrastlı ve göz yormayan yapıda olmalı.
- Yazı tipi, punto, satır aralığı, kenar boşluğu ve hizalama ayarlanabilmeli.
- Sayfa çevirme hızlı ve tutarlı olmalı; ekran yenileme sıklığı kullanıcı tarafından ayarlanabilmeli.
- Ağ özellikleri çevrimdışı okuma deneyimini bozmamalı.
- Kullanıcı verileri yerel olarak saklanmalı; bulut eşitleme açıkça etkinleştirilmeli.
- Mevcut KOReader veya NickelMenu kurulumu üzerine yazılmamalı.
- Kurulum ve kaldırma adımları açık, yedekleme ve kurtarma yolu belli olmalı.

## 6. Gerçekçi geliştirme aşamaları

Aşamaların ayrıntılı listesi, işleri ve kabul ölçütleri `milestone.md` içindedir; tek kaynak orasıdır. Özet sıra:

1. Cihaz keşfi (büyük ölçüde tamamlandı).
2. NixOS ortamı, proje iskeleti ve Libra 2 simülatörü.
3. EPUB, metin düzeni ve tam Türkçe temel okuyucu.
4. PDF: görüntüleme, ölçeklendirme, reflow ve OCR.
5. Notlar ve vurgular, sözlük, OPDS/Calibre, Google Drive.
6. En son: gerçek Kobo Libra 2 üzerinde çalıştırma.

## 7. Teknik riskler ve uyarılar

- **Rust kullanmak tek başına Kobo uyumluluğu sağlamaz.** Cihazın ABI'si, libc sürümü, mimarisi ve sistem API'leri doğrulanmalı.
- Kobo cihazında genel amaçlı masaüstü Linux arayüzlerinin çalışacağını varsayma.
- E Ink ekranı normal bir monitör gibi ele alma; yenileme modları, ghosting ve pil maliyeti test edilmeli.
- Web/Drive entegrasyonu Wi-Fi donanımından fazlasını gerektirir: TLS sertifikaları, HTTPS, OAuth, API uyumluluğu ve güvenli token depolama.
- Google OAuth akışı eski gömülü tarayıcıda çalışmayabilir. Alternatif yetkilendirme mimarisi tasarlanmalı.
- Sıfırdan tam EPUB/CSS motoru yazmak büyük iştir. Kütüphane kullanımıyla “sıfırdan uygulama” arasındaki sınır kullanıcıyla açıkça belirlenmeli; kullanılması düşünülen Rust crate'lerinin lisansı, bakımı ve hedef platform uyumluluğu kontrol edilmeli.
- KOReader'dan kod kopyalanmayacak; ancak davranış ve kullanıcı deneyimi referans alınabilir.
- Donanım/firmware ayrıntıları doğrulanmadan derleme komutları veya kurulum dosyaları kesinmiş gibi sunulmamalı.

## 8. İlk somut teslimat

İlk olarak aşağıdakileri içeren bir teknik keşif ve başlangıç deposu hazırla:

1. Libra 2 donanım/firmware uyumluluk notu. (Hazır: `docs/device-compatibility.md`)
2. Rust hedef üçlüsü (`armv7-unknown-linux-musleabihf`, statik) ve NixOS geliştirme ortamı.
3. Yalnızca gereken parçaları içeren Cargo proje iskeleti ve Türkçe metin katalogu.
4. Libra 2 ekranını taklit eden masaüstü simülatörü.
5. Simülatörde çalışan en küçük EPUB açma ve metin gösterme prototipi.
6. Bilinen riskler ve doğrulanmamış varsayımlar listesi.

Önce teknik keşif yap; kaynak veya araç desteğini doğrulamadan çalışmayan örnek kodu “hazır” diye sunma.

## 9. Kullanıcıdan netleştirilecekler

Yanıtlananlar:
- Firmware: 4.38.23697. Kitap aktarımı: USB. Format sırası: önce EPUB, hemen ardından PDF. Masaüstü simülatörü: evet, zorunlu. Cihazda çalıştırma: en son.
- PDF'e aktarma: ikisi de. OCR metni aranabilir katman olarak gömülecek; notlar ve vurgular PDF'e yazılacak.
- OCR: önce NixOS'ta deney amaçlı; sonra cihaz uygulamasının parçası olacak.
- Cobalt ve Plato yalnızca referans; kodları kullanılmayacak. Uygulama kendi Rust koduyla yazılacak.
- Proje lisansı: MIT.
- PDF motoru: saf Rust (olgunluktan çok hız ve sade derleme öncelikli); MuPDF (AGPL) kullanılmayacak.

Açık kalanlar:
- Drive kullanım kapsamı: yalnızca kitap indirme mi, yoksa iki yönlü senkronizasyon mu?
- Çok dilli sözlüklerin öncelik sırası.

## 10. Projenin kısa tanımı

“KOReader'dan bağımsız, Rust ile sıfırdan geliştirilen; Kobo Libra 2 üzerinde çalışan; tamamen Türkçe, E Ink için optimize edilmiş, gelişmiş çevrimdışı/çevrimiçi sözlükleri ve isteğe bağlı Google Drive senkronizasyonunu destekleyen e-kitap okuyucu.”

Öncelik sırası: **Türkçe EPUB okuyucu (simülatörde) → PDF ve OCR → notlar, sözlük, internet/Drive → gerçek cihazda güvenli çalıştırma.**
