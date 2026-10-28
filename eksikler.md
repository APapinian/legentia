# Eksikler

Geliştirme sırasında fark edilen, ama hemen düzeltilmeyen eksikler ve sınırlar. Bunlar en sonda, nihai sürüm Kobo Libra 2 üzerinde denenirken gözden geçirilecek (kullanıcı kararı, 2026-10-28): her madde cihazda bakılıp düzeltilecek, bırakılacak ya da v2'ye aktarılacak.

Uygulama bittikten sonraki güncellemeye bilerek bırakılanlar burada değil, `milestone.md` → "v2'ye bırakılanlar" bölümünde.

Yeni bir eksik bulununca bu dosyaya eklenir; giderilen madde silinir ve ne yapıldığı commit mesajına yazılır.

## Cihazda ayarlanacak ya da doğrulanacak

- **E Ink yenilemesi:** 6 kısmi yenilemeden sonra bir tam yenileme yapılıyor (`PARTIALS_PER_FULL`). Sayı geçici; cihazdaki gölgelenmeye bakılarak ayarlanacak. (M3)
- **Kontrast ve gamma:** gece modu ve PDF kenar yumuşatması cihaz ekranında değerlendirilecek. (M4, M5)
- **Uzun basma:** zamanlayıcı yok; uzun basma ancak parmak kalkınca ya da kıpırdayınca tanınıyor. Cihazda doğal hissettiriyor mu bakılacak. (M6)
- **Süreler ve bellek:** kitap açma, sayfalama ve sayfa çizme süreleri yalnızca masaüstünde ölçüldü. Cihazda yeniden ölçülecek. (M2, M3, M5)
- **İndirme ilerlemesi:** `Tick` 500 ms'de bir gönderiliyor, ekran %10 adımlarla yenileniyor. Cihazda ekranın fazla mı, az mı yenilendiğine bakılacak. (M8)
- **TLS:** gerçek HTTPS yalnızca masaüstünde ve qemu-arm'da denendi. Cihazın saati yanlışsa sertifika doğrulaması başarısız olur; bu durumun mesajı cihazda denenecek. (M8)
- **Sözlükler:** bütün biçimler yalnızca kendi test sözlüklerimizle denendi. Kullanıcının gerçek sözlükleriyle (KOReader klasöründekiler) denenecek. (M7)
- **OCR:** gerçek taranmış bir kitap sayfasıyla henüz denenmedi. Cihaza taşınması M10 işi. (M5)

## İşlev eksikleri

- **Kütüphanede arama yok.** Türkçe sıralama var ama kütüphanede kitap aramak için alan yok. (M4)
- **Katalog şifresi düz metin:** `catalogs.json` dosyasında açık duruyor. (M8)
- **İndirme kuyruğu kalıcı değil:** yalnızca uygulama açıkken tutuluyor; uygulama kapanınca sıradakiler unutuluyor. (M8)
- **Kitap ayrıntısında kapak yok:** yalnızca başlık, yazar, biçim ve özet gösteriliyor. (M8)
- **Kütüphaneden ayarlara gidilemiyor:** "İnternet" ayarı da dahil, ayarlara yalnızca bir kitap açıkken menüden ulaşılıyor. İnternet kapalıyken çıkan mesaj bu yolu tarif ediyor. (M8)

## Bilinen hatalar ve sınırlar

- **Gutenberg'de aynı başlıklı girdiler:** kitap sayfasında resimli ve resimsiz baskı aynı başlıkla listeleniyor, ayırt edilemiyor; ikisi de aynı dosya adına iniyor (biri inince öbürü "Aç" gösteriyor). (M8)
- **Yarım indirme dosyası:** uygulama indirme sırasında kapanırsa `İndirilenler` klasöründe `.indiriliyor` uzantılı yarım dosya kalabiliyor. Kütüphane bu dosyayı göstermiyor; kitap yeniden indirilince üzerine yazılıyor. (M8)
- **İptal ve hemen yeniden indirme:** iptal edilen indirme bir sonraki veri parçasına kadar yarım dosyasını silmiyor. Aynı kitap hemen yeniden indirilirse iki iş aynı yarım dosyaya denk gelebilir (düşük olasılık, denenmedi). (M8)
- **Notlu PDF kütüphanede ayrı kitap:** dışa aktarılan "- notlu.pdf" kütüphanede ayrı bir kitap olarak görünüyor. (M6)
- **Değişen PDF yeni kitap sayılıyor:** PDF'in kimliği dosya adı + boyut olduğundan, değişen PDF'in konum ve notları eşleşmiyor. (M6)
- **Digest yalnızca MD5:** SHA-256 Digest isteyen sunucuya bağlanılamaz ("Sunucunun yanıtı anlaşılamadı" mesajı çıkar). Calibre MD5 kullandığı için sorun değil. (M8)
- **Yetkili isteklerde iki gidiş-dönüş:** şifre ilk istekte gönderilmediği için yetki isteyen katalogda her sayfa iki istekle açılıyor; sunucunun nonce'u yeniden kullanılmıyor. Cihazda yavaş hissettirirse bakılacak. (M8)
- **Sağdan sola yazı yok:** bidi desteği henüz yok; altyapı buna uygun kuruldu. (M3)
