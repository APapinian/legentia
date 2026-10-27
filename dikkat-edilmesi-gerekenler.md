# Dikkat Edilmesi Gerekenler — Rust Kobo Okuyucu

Bu belge, geliştirme sırasında cihazın çalışır durumunu, kullanıcının kitaplarını ve güvenliğini korumak için uyulması gereken kuralları listeler.

## 1. Cihazı ve mevcut kurulumu koru

- Kullanıcı Kobo Libra 2'de KOReader ve Plato kullanıyor; ikisini de NickelMenu → KFMon üzerinden başlatıyor.
- Mevcut KOReader, Plato, KFMon, NickelMenu veya Kobo sistem dosyalarını silme, taşıma ya da üzerine yazma. `.adds/nm/` altındaki mevcut dosyaları düzenleme.
- Yeni uygulamayı `.adds/` altında ayrı bir dizinde tut ve kendine ait ayrı bir NickelMenu yapılandırma dosyasıyla başlat.
- `KoboRoot.tgz` yöntemini kullanma; bu dosya açılışta root olarak `/` üzerine açılır ve cihazı açılmaz hale getirebilir.
- SSH'yi açmak (`.kobo/ssh-disabled` → `ssh-enabled`) yalnızca kullanıcının açık onayıyla yapılır.
- Her cihaz değişikliğinden önce hangi dosyanın değişeceğini, neden gerektiğini ve nasıl geri alınacağını açıkla.
- Otomatik kurulum betiği yazmadan önce manuel ve geri alınabilir bir yöntemle doğrula.
- Kullanıcıdan açık onay almadan sistem bölümlerini, açılış dosyalarını veya firmware'i değiştirme.
- Kurtarma planı olmadan firmware güncellemesi, sistem yaması veya riskli dosya değişikliği önerme.

## 2. Varsayımları gerçek gibi sunma

- Firmware (4.38.23697), çekirdek ve başlatma yöntemi kullanıcının cihazından doğrulandı. Ekran, dokunmatik ve işlemci bilgileri Cobalt'ın aynı model ve firmware'deki ölçümlerinden geliyor; bizim cihazda henüz ölçülmedi. Kaynakları ayırt etmek için `docs/device-compatibility.md`'ye bak.
- Bir crate'in Rust ile yazılmış olması onun Kobo'da çalışacağı anlamına gelmez; ARMv7 musl hedefi için derlendiğini kontrol et.
- Cobalt'ın ölçümleri yol gösterir ama kanıt değildir; aynı cihaz kodu altında iki donanım revizyonu var.
- Bir masaüstü simülatöründe başarılı olmak, Kobo'nun ekran ve dokunmatik katmanının çalıştığını kanıtlamaz.
- Geliştirme bilgisayarı Libra 2'den kat kat hızlıdır; masaüstünde ölçülen süreleri cihaz performansı gibi sunma.
- Araç, API, komut veya dosya yolu hakkında emin değilsen doğrula ya da belirsizliği açıkça belirt.
- Henüz test edilmemiş özellikleri “tamamlandı” olarak işaretleme.

## 3. Rust ve derleme hedefi

- Hedef işlemci mimarisini ve ABI'yi cihazdan doğrula.
- Rust target triple, linker, libc ve sistem kütüphanelerinin uyumunu belgeleyerek seç.
- Mümkünse masaüstü çekirdeği ile Kobo'ya özgü platform katmanını ayır.
- Bağımlılıkların bakım durumunu, lisansını, hedef platform desteğini ve derleme gereksinimlerini incele.
- Gereksiz ağır bağımlılıklardan kaçın; bellek ve ikili dosya boyutunu izle.
- Panik, dosya hatası ve ağ hatalarını kullanıcıya anlaşılır Türkçe mesajlarla göster.
- `cargo fmt`, `cargo clippy` ve `cargo test` kontrollerini düzenli çalıştır.

## 4. E Ink ekran ve performans

- E Ink ekranı normal LCD gibi ele alma; ekran yenileme davranışı farklıdır.
- Gereksiz tam ekran yenilemelerinden kaçın; ghosting, titreme ve pil maliyetini gerçek cihazda ölç.
- Hızlı sayfa çevirme, dokunma tepkisi ve uzun kitaplarda bellek kullanımını test et.
- Sürekli animasyon, sürekli ekran çizimi veya sürekli polling kullanma.
- Ağ isteklerini ve ağır işlemleri okuma etkileşimini bloke etmeyecek biçimde tasarla.
- Uygulama uyku/uyanma, uzun süre açık kalma ve düşük pil durumlarını yönetmeli.
- Sınırlı RAM nedeniyle büyük kitapları veya görselleri gereksiz yere belleğe yükleme.

## 5. EPUB doğruluğu ve güvenliği

- EPUB yalnızca düz metin değildir; ZIP, OPF, XHTML, CSS, görseller, yazı tipleri, içindekiler ve bağlantılar içerebilir.
- İlk sürümde hangi EPUB özelliklerinin desteklendiğini açıkça belgele.
- Güvenilmeyen EPUB dosyalarını ayrıştırırken path traversal, zip bomb, aşırı boyut ve kaynak tüketimi risklerini dikkate al.
- EPUB içindeki HTML veya betikleri yerel sistemde çalıştırma.
- Harici kaynakları kullanıcı izni olmadan indirme.
- Bozuk veya beklenmedik girdiler uygulamayı çökertmemeli.
- İçerik konumlarını yalnızca sayfa numarasıyla saklama; yazı tipi veya ekran ayarı değişince sayfa numarası değişebilir. Kararlı bölüm/konum işaretleri kullan.
- Türkçe `İ/ı`, `I/i`, `ğ, ü, ş, ö, ç` karakterlerini ve Unicode normalizasyonunu test et.

## 6. Tam Türkçe deneyim

- Menü, ayar, düğme, hata, yardım ve kurulum belgeleri Türkçe olmalı.
- Kullanıcıya görünen sabit İngilizce dizeler bırakma.
- Kod, log ve teknik tanımlayıcıların İngilizce olması kabul edilebilir; kullanıcıya gösterilen mesajlar ayrı tutulmalı.
- Türkçe karakterli dosya adları, arama ve sıralama test edilmeli.
- Arama ve sıralamada Türkçe büyük/küçük harf kurallarını bilinçli ele al.
- Sözlük sonuçlarında dil, kaynak, lisans ve güncellik bilgisi mümkün olduğunda gösterilmeli.

## 7. Sözlükler ve çevrimiçi kaynaklar

- Sözlük içeriklerinin lisansını kontrol et; telifli veri kümelerini izinsiz uygulamaya gömme.
- Çevrimdışı sözlükleri ayrı paketler halinde kurma/güncelleme olanağı düşün.
- Çevrimiçi sağlayıcıların API şartlarını, kullanım limitlerini ve ücretlerini kontrol et.
- API anahtarlarını kaynak koduna veya Git deposuna koyma.
- Ağ yanıtlarını doğrula; zaman aşımı, hata, boş sonuç ve kota aşımını ele al.
- Kelime araması başarısız olduğunda uygulama kilitlenmemeli; yerel sözlük çalışmaya devam etmeli.
- Çeviri ve sözlük anlamlarını kesin/tek doğru gibi sunma; bağlam farklarını koru.

## 8. İnternet, TLS ve Google Drive

- Wi-Fi donanımının bulunması, modern HTTPS/TLS veya OAuth'un sorunsuz çalışacağını garanti etmez.
- TLS sertifika doğrulamasını kapatma ve geçersiz sertifikaları sessizce kabul etme.
- OAuth akışının cihazın eski tarayıcısında çalışacağını varsayma; uygun yetkilendirme veya eşleştirme yöntemini araştır.
- Token, refresh token, parola ve kişisel verileri loglama.
- Kullanıcı bağlantıyı kaldırdığında token'ları ve ilgili sırları güvenli biçimde temizle.
- En az yetki ilkesini uygula; Drive'da yalnızca gerekli kapsamları iste.
- Senkronizasyon sırasında yerel notları veya kitap dosyalarını otomatik olarak silme.
- Çakışma çözümü olmadan iki yönlü senkronizasyonu etkinleştirme.
- Büyük indirmelerde ilerleme, iptal, yeniden deneme ve disk alanı kontrolü ekle.
- Çevrimdışı durumda okuma, notlar ve yerel sözlükler çalışmaya devam etmeli.
- İnternet özelliğini kullanmak zorunlu olmamalı; kullanıcı ağ erişimini kapatabilmeli.

## 9. Veriler, yedekleme ve gizlilik

- Kitaplar, notlar, vurgular, ayarlar ve okuma ilerlemesi için açık bir veri modeli kur.
- Veritabanı şeması değişikliklerinde migration kullan.
- Yedekleme ve geri yüklemeyi erken aşamada tasarla.
- Senkronizasyon veya içe aktarma sırasında mevcut kullanıcı verisinin üzerine sessizce yazma.
- Kullanıcıya hangi verinin cihazda, hangisinin bulutta tutulduğunu açıkça anlat.
- Hata kayıtlarında kitap metni, özel notlar veya kimlik bilgileri bulunmamalı.
- Kısmi indirme veya kesilen yazma işlemi kitap dosyasını bozarsa önceki sağlam dosyayı koru.

## 10. Lisanslar ve açık kaynak

- Her Rust crate'inin ve sözlük veri kaynağının lisansını kontrol et.
- KOReader kodunu kopyalama; yalnızca davranışını ve kullanıcı deneyimini referans al.
- Cobalt ve Plato (AGPL-3.0) yalnızca referanstır. Onlardan kod kopyalama; mimarilerini ve cihaz ölçümlerini okuyup kendi Rust kodunu yaz.
- PDF'e yazarken (OCR katmanı, notlar) orijinal dosyanın üzerine doğrudan yazma; önce ayrı dosyaya yaz, doğrula, sonra değiştir.
- Bir kütüphane veya kod parçası kullanılacaksa lisans ve atıf koşullarını yerine getir.
- Uygulamanın kendi lisansı ve dağıtım yöntemi konusunda karar vermeden önce kullanılan bağımlılıkların yükümlülüklerini gözden geçir.
- Google Drive gibi üçüncü taraf markalarını veya hizmetlerini resmi ortaklık varmış gibi tanıtma.

## 11. Test ve teslimat disiplini

Her kilometre taşında:
1. Ne değiştiğini özetle.
2. Değişen dosyaları listele.
3. Derleme ve test sonuçlarını bildir.
4. Gerçek Kobo üzerinde test edildiyse bunu açıkça söyle; edilmediyse belirt.
5. Bilinen sorunları ve sonraki adımı yaz.
6. Geri alma adımlarını gerektiğinde sun.

Bir özelliği yalnızca derleniyor diye bitmiş sayma. Kullanıcı deneyimi, cihaz uyumluluğu, çevrimdışı davranış ve hata durumları da test edilmeli.

## 12. Kaçınılması gereken yanlış yaklaşımlar

- İlk günden tam EPUB ve PDF motoru, Drive senkronizasyonu ve çoklu sözlükleri tek seferde yazmaya çalışma.
- Kobo uyumluluğunu doğrulamadan büyük GUI framework'ünü seçme.
- Her şey için sürekli internet bağlantısı gerektirme.
- E Ink ekranı sürekli yeniden çizme.
- Mevcut KOReader kurulumunu kaldırarak yer açma.
- Kimlik bilgilerini koda gömme.
- Başarısız OAuth/TLS bağlantısını güvenlik kontrollerini kapatarak “çözme”.
- Gerçek cihazda test edilmemiş kodu çalışan cihaz uygulaması olarak sunma.

## Altın kural

**Her şeyi önce NixOS'taki Libra 2 simülatöründe geliştir ve test et: önce Türkçe EPUB okuyucu, sonra PDF, sonra diğer özellikler. Gerçek cihaza en son, güvenli ve geri alınabilir biçimde geç.** Mevcut KOReader, Plato, KFMon ve NickelMenu kurulumu her aşamada korunmalı.
