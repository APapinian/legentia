# EPUB desteği

Bu belge, okuyucunun EPUB'ın neyini gösterdiğini ve neyini (henüz) göstermediğini listeler. Desteklenmeyen bir özellik sessizce yanlış gösterilmez; aşağıdaki gibi davranır.

## Destekleniyor

| Özellik | Not |
|---|---|
| EPUB 2 ve EPUB 3 paketleri | `container.xml` → OPF; meta veri (başlık, yazar, dil), manifest, spine (`linear="no"` öğeler atlanır) |
| İçindekiler | EPUB 3 `nav` (`epub:type="toc"`), yoksa EPUB 2 NCX; yoksa her bölüm "Bölüm N" olarak listelenir |
| Paragraf ve başlıklar | `<p>`, `<div>`, `<li>`, `<blockquote>` vb. blok; `<h1>`–`<h6>` başlık (h1–h2 ortalı, kalın, büyük) |
| Kalın, italik | `<b>`, `<strong>`; `<i>`, `<em>`, `<cite>`, `<var>`, `<dfn>`; iç içe kullanım |
| Bağlantılar | `href` taşıyan `<a>` altı çizili gösterilir. Dokununca kitap içindeki hedefe (başka bölüm ya da aynı bölüm, `id` / eski `<a name>`) gidilir; sol üstteki "← Geri" önceki yere döner (en çok 20 adım). Hedef blok düzeyinde bulunur (paragrafın ortasındaki kimlik paragrafın başı sayılır). Dış bağlantılar (http…) açılmaz. |
| HTML varlıkları | `&nbsp;`, `&laquo;`, sayısal `&#305;` vb. |
| Unicode | NFC'ye normalleştirme (ayrışık "İ", "Ş" birleştirilir); yumuşak tire ve sıfır genişlikli boşluk atılır |
| Türkçe dizgi | `tr` dil etiketiyle harf birleştirme ("fi" bitişik harfi "i"nin noktasını yutmaz), TDK kurallarıyla heceleme, iki yana yaslama |
| Güvenlik | Boyut ve girdi sayısı sınırı; paket dışına çıkan yollar reddedilir; betik çalıştırılmaz, dış kaynak indirilmez |

## Henüz desteklenmiyor

| Özellik | Şu anki davranış | Plan |
|---|---|---|
| CSS (yazı tipi, girinti, hizalama, sınıfla italik) | Yok sayılır; okuyucunun kendi düzeni kullanılır | Seçilmiş birkaç özellik (text-indent, text-align, font-style) sonra |
| Görseller (`<img>`, `<svg>`) | Gösterilmez; yalnızca görsel içeren bölümde "gösterilecek metin yok" yazar | M4 sonrası |
| Tablolar | Her hücre ayrı paragraf olarak akar | Sonra |
| Dipnotlar | Bağlantı gibi izlenir (dipnot sayfasına gidilir, "← Geri" ile dönülür) | Açılır pencere: M11 madde 2 |
| Üst/alt simge (`<sup>`, `<sub>`) | Normal metin gibi | Sonra |
| `<pre>` (önceden biçimlenmiş) | Boşluklar birleştirilir | Sonra |
| Sağdan sola yazı (Arapça harfli Osmanlıca) | Soldan sağa dizilir (yanlış) | Mimari hazır (glifler kaynak konumu taşıyor); bidi M7 öncesi |
| Dikey yazı, CJK satır kuralları | Desteklenmiyor | Japonca sözlük aşamasıyla |
| Türkçe dışı heceleme | Türkçe olmayan kitaplarda heceleme kapalı; q/w/x içeren kelimeler bölünmez | Diğer diller için desen tabanlı heceleme sonra |
| Satır sonu dengeleme (Knuth–Plass), dul/öksüz satır | Açgözlü satır doldurma; yalnızca başlık sayfa sonunda yalnız kalmaz | Sonra |
| DRM'li kitaplar | Açılmaz (şifreli içerik bozuk görünür) | Desteklenmeyecek |
