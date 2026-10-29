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

## FB2, TXT ve HTML

Bu dosyalar açılırken bellekte EPUB'a çevrilir (`crates/core/src/epub/convert.rs`); dosyaya yazılmaz, sonrası EPUB'la aynıdır.

| Biçim | Ne yapılır |
|---|---|
| FB2, `.fb2.zip` | Üst düzey her `section` bir bölüm ve içindekiler girdisi; `title` başlık, `epigraph`/`cite`/`annotation` alıntı, `poem` dizeleri, `emphasis`/`strong`, tablolar; `binary` görseller (base64), `coverpage` kapak; yazar, dil, kimlik, `sequence` seri olur; ikinci (`notes`) gövde dipnot bölümü, `type="note"` bağlantılar dipnot açar. Kodlama UTF-8, bildirilmişse Windows-1251, değilse Windows-1254. |
| TXT | Boş satırla ayrılan paragraflar (boş satır çok azsa her satır bir paragraf). "Bölüm 3", "BİRİNCİ KISIM", "Chapter IV" gibi kısa satırlar bölüm başı ve içindekiler girdisi; başlıksız uzun metin ~100 KB'lık bölümlere bölünür. UTF-8 değilse Windows-1254. Başlık dosya adıdır. |
| HTML | Gövde tek bölüm; başlık `<title>`'dan. Yanındaki görsel dosyaları okunmaz (yer tutucu ya da alt metin). |

## Henüz desteklenmiyor

| Özellik | Şu anki davranış | Plan |
|---|---|---|
| CSS (yazı tipi, girinti, hizalama, sınıfla italik) | Yok sayılır; okuyucunun kendi düzeni kullanılır | Seçilmiş birkaç özellik (text-indent, text-align, font-style) sonra |
| Görseller | JPEG ve PNG gösterilir (`<img>` ve kapak sayfalarındaki `<svg><image>`); genişliğe ve sayfa yüksekliğine sığar, en çok 2 kat büyür; uzun basınca tam ekran. GIF, SVG çizimi ve diğer biçimler gösterilmez: yerinde alt metin ya da "[Görsel gösterilemiyor]". Satır içi görsel paragrafı böler. | GIF/SVG sonra |
| Tablolar | Her satır bir paragraf, hücreler " \| " ile ayrılır; başlık hücreleri kalın. Izgara çizilmez, sütunlar hizalanmaz. | Izgara sonra |
| Dipnotlar | `epub:type="noteref"` / `role="doc-noteref"` ya da en çok 5 karakterlik bağlantı metni ("1", "[12]", "*") dipnot sayılır; not altta açılır pencerede kitabın yazı tipiyle gösterilir (hedef blok + ardından gelen kimliksiz bloklar, en çok 6 blok / 2000 karakter). "Notun yerine git" nota gider. Kısa metinli ama dipnot olmayan bağlantı da pencerede açılır; pencerede "Notun yerine git" var. | |
| Üst/alt simge (`<sup>`, `<sub>`) | Normal metin gibi | Sonra |
| `<pre>` (önceden biçimlenmiş) | Boşluklar birleştirilir | Sonra |
| Sağdan sola yazı (Arapça harfli Osmanlıca) | Soldan sağa dizilir (yanlış) | Mimari hazır (glifler kaynak konumu taşıyor); bidi M7 öncesi |
| Dikey yazı, CJK satır kuralları | Desteklenmiyor | Japonca sözlük aşamasıyla |
| Türkçe dışı heceleme | İngilizce, Almanca, Fransızca, İspanyolca ve İtalyanca kitaplar TeX hyph-utf8 desenleriyle (Liang) hecelenir; diğer dillerde heceleme kapalı. Türkçe kitapta q/w/x içeren kelimeler bölünmez | Rusça (desen yalnızca LPPL) ve diğer diller sonra |
| Satır sonu dengeleme (Knuth–Plass), dul/öksüz satır | Açgözlü satır doldurma; yalnızca başlık sayfa sonunda yalnız kalmaz | Sonra |
| DRM'li kitaplar | Açılmaz (şifreli içerik bozuk görünür) | Desteklenmeyecek |
