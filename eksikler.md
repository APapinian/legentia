# Eksikler

Geliştirme sırasında fark edilen, ama hemen düzeltilmeyen eksikler ve sınırlar. Bunlar en sonda, nihai sürüm Kobo Libra 2 üzerinde denenirken gözden geçirilecek (kullanıcı kararı, 2026-10-28): her madde cihazda bakılıp düzeltilecek, bırakılacak ya da v2'ye aktarılacak.

Uygulama bittikten sonraki güncellemeye bilerek bırakılanlar burada değil, `milestone.md` → "v2'ye bırakılanlar" bölümünde.

Yeni bir eksik bulununca bu dosyaya eklenir; giderilen madde silinir ve ne yapıldığı commit mesajına yazılır.

## Cihazda ayarlanacak ya da doğrulanacak

- **E Ink yenilemesi:** 6 kısmi yenilemeden sonra bir tam yenileme yapılıyor (`PARTIALS_PER_FULL`). Sayı geçici; cihazdaki gölgelenmeye bakılarak ayarlanacak. (M3)
- **Kobo platform katmanı, 1. denemeden sonra** (M10). Düzeltildi ama cihazda henüz doğrulanmadı:
  - dönüş 3'te dokunma ve sayfa tuşları;
  - 8 bit ekran ve yalnızca değişen satırları yazma (hız);
  - `reboot`.
  2. denemede doğrulananlar (2026-10-28): `reboot` çalışıyor, 8 bit geçişi başarılı. Ekrana yazma tam ekranda 1,6–2,3 sn'den 360 ms'ye indi; yarım ekran ~210 ms. Hâlâ yavaş: framebuffer'a dosya yazma ~6 MB/sn. Sonraki adım mmap olabilir (`unsafe` gerektirir, kullanıcı kararı).
  Hâlâ bakılacaklar:
  - Kısmi yenilemede GL16'nın mı REAGL'in mi daha az iz bıraktığı.
  - Uygulama açıkken cihaz döndürülürse dokunmanın ters düşmesi (ivmeölçer okunmuyor).
  - Dönüş 0 ve 2'de (yatay) programın çizmeden çıkması.
  - Yazı tipi önbelleği boşken ilk sayfanın ~2 sn'de çizilmesi.
  - 8 bit'e geçişin ekranı ters renk göstermesi ihtimali.
  Doğrulananlar (1. deneme): dokunmatik aygıt adla bulunuyor ("Elan Touchscreen"); 16 MB'lık ikili 11 MB'a küçültüldü (strip).
- **İki parmak hareketi cihazda denenmedi:** Elan dokunmatiğin ikinci parmağı çok noktalı yuva 1'de bildirdiği varsayıldı. Kaydırma ve iki parmak eşikleri (150 px, %15) elle ayarlanabilir; cihazda bakılacak. (M11)
- **Yatay okuma cihazda denenmedi:** yatayda dokunma eşlemesi (dönüş 0 ve 2) geometriden çıkarıldı. Yatayın hangi yana olacağı (ivmeölçerin işaretine göre 0 mı, 2 mi) ve yatayda sayfa tuşlarının anlamı da tahmin. İvmeölçerin eksenleri açılışta kalibre ediliyor; ham değerler günlüğe yazılıyor. Dönüş başarısız olursa görüntü kırpılır. (M11)
- **Yatayda yan yana PDF:** seçim, vurgu ve arama işareti yalnızca soldaki sayfada; sağ sayfanın metni çıkarılmıyor. Yüzde sol sayfaya göre. Yalnızca "Sayfaya sığdır" görünümünde (genişliğe sığdır ve sütun modunda tek sayfa). (M11)
- **Güç tuşu uyku yerine çıkış yapıyor:** şimdilik güvenlik yolu; uyku/uyanma M10'da ayrıca ele alınacak.
- **Kontrast ve gamma:** gece modu ve PDF kenar yumuşatması cihaz ekranında değerlendirilecek. (M4, M5)
- **Uzun basma:** zamanlayıcı yok; uzun basma ancak parmak kalkınca ya da kıpırdayınca tanınıyor. Cihazda doğal hissettiriyor mu bakılacak. (M6)
- **Süreler ve bellek:** kitap açma, sayfalama ve sayfa çizme süreleri yalnızca masaüstünde ölçüldü. Cihazda yeniden ölçülecek. (M2, M3, M5)
- **İndirme ilerlemesi:** `Tick` 500 ms'de bir gönderiliyor, ekran %10 adımlarla yenileniyor. Cihazda ekranın fazla mı, az mı yenilendiğine bakılacak. (M8)
- **TLS:** gerçek HTTPS yalnızca masaüstünde ve qemu-arm'da denendi. Cihazın saati yanlışsa sertifika doğrulaması başarısız olur; bu durumun mesajı cihazda denenecek. (M8)
- **Sözlükler:** bütün biçimler yalnızca kendi test sözlüklerimizle denendi. Kullanıcının gerçek sözlükleriyle (KOReader klasöründekiler) denenecek. (M7)
- **OCR:** gerçek taranmış bir kitap sayfasıyla henüz denenmedi. Cihaza taşınması M10 işi. (M5)

## İşlev eksikleri

- **TDK erişimi resmî değil:** belgelenmemiş `sozluk.gov.tr/gts` ucu kullanılıyor. Adres ya da biçim değişirse TDK araması çalışmaz. Kısa sürede çok istek gelince TDK boş yanıt dönüyor (denemede görüldü); uygulama o zaman "Sunucunun yanıtı anlaşılamadı" diyor. (M8)
- **Yalnızca TDK açıkken derin çekimler:** kök adayları sırayla en çok 6 kez deneniyor; "kardeşlerimizden" gibi kelimelerde kök bu sınıra girmiyor ve bulunamıyor. Vikisözlük de açıksa onun bulduğu kök kullanılıyor. (M8)
- **Vikimedya kullanıcı aracısı:** Vikimedya, isteklerde iletişim bilgisi içeren bir User-Agent istiyor; bizimki yalnızca "Legentia/sürüm". Engellenirse buraya iletişim adresi eklenecek. (M8)
- **Çevrimiçi sözlük önbelleği sınırsız:** `cevrimici-sozluk/` kelime başına küçük bir dosya; temizleme seçeneği yok. (M8)
- **Kitap içi arama eşzamanlı:** bütün bölümler arama sırasında okunur; cihazda büyük kitapta (ya da yüzlerce sayfalık PDF'te) ekran birkaç saniye bekleyebilir. Yavaşsa arka plana alınacak. (M11)
- **EPUB görselleri:** GIF ve SVG çizimleri gösterilmiyor; tablolar ızgara olarak değil satır satır akıyor; satır içi küçük görseller paragrafı bölüyor; gece modunda görseller de ters dönüyor. Görsel çözme süresi (büyük JPEG) cihazda ölçülecek. (M11)
- **Kütüphanede arama yok.** Türkçe sıralama var ama kütüphanede kitap aramak için alan yok. (M4)
- **Raflar:** bir kitap yalnızca bir rafta olabilir (klasör); boş klasörler raf olarak görünmez, yeni raf için bilgisayardan klasör açmak gerekiyor. Aynı kimlikli kopyalar aynı sırt rengini alıyor. (M11)
- **İstatistik günleri UTC:** cihazın saat dilimi okunmuyor; Türkiye'de gün gece 03:00'te değişiyor. İstatistikler yedeğe girmiyor. Okuma ekranı açık bırakılıp 10 dakikadan uzun sayfa çevrilmezse o süre hiç sayılmıyor (ara sayılıyor). (M11)
- **Vurgu türleri notlu PDF'e geçmiyor:** altı çizili ve çerçeveli vurgular "notlu PDF"te de sarı/gri vurgu (Highlight) olarak yazılıyor; çerçeve, alt satırdaki harflerin uzantılarına değebiliyor. (M11)
- **Kapak önbelleği:** kitap dosyası değişse de (aynı kimlikle) eski kapak kalır; silinen kitapların kapakları `kapaklar/`'da durur. Cihazda kapak üretme süresi (özellikle büyük PDF'in ilk sayfası) ölçülecek. Araç çubuğundaki süzgeç düğmeleri dar; yazılar sığıyor ama sıkışık. (M11)
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
