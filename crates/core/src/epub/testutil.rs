//! Testler için bellekte EPUB üretimi.

use std::io::{Cursor, Write};

use super::{Book, EpubError};

pub fn open_bytes(bytes: Vec<u8>) -> Result<Book, EpubError> {
    Book::from_reader(Box::new(Cursor::new(bytes)))
}

pub fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
    let bytes: Vec<(&str, &[u8])> = entries.iter().map(|(n, b)| (*n, b.as_bytes())).collect();
    zip_of_bytes(&bytes)
}

/// İkili girdiler (görseller) de içerebilen ZIP.
pub fn zip_of_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, body) in entries {
        w.start_file(*name, opts).unwrap();
        w.write_all(body).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub fn container(opf_path: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="{opf_path}" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#
    )
}

pub fn opf_doc(manifest: &str, spine: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Deneme</dc:title></metadata>
  <manifest>{manifest}</manifest>
  <spine>{spine}</spine>
</package>"#
    )
}

/// `w`×`h` boyutunda, her pikseli `gray` olan PNG.
pub fn png(w: u32, h: u32, gray: u8) -> Vec<u8> {
    let img = image::GrayImage::from_pixel(w, h, image::Luma([gray]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

pub fn epub_with_chapter(body: &str) -> Vec<u8> {
    let opf = opf_doc(
        r#"<item id="c" href="c.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="c"/>"#,
    );
    zip_of(&[
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", &container("OEBPS/content.opf")),
        ("OEBPS/content.opf", &opf),
        ("OEBPS/c.xhtml", body),
    ])
}

pub fn epub3_sample() -> Vec<u8> {
    let opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Çalıkuşu</dc:title>
    <dc:creator>Reşat Nuri Güntekin</dc:creator>
    <dc:language>tr</dc:language>
    <dc:identifier id="uid">urn:uuid:calikusu-0001</dc:identifier>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="b1" href="metin/bolum1.xhtml" media-type="application/xhtml+xml"/>
    <item id="b2" href="metin/bolum2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="b1"/><itemref idref="b2"/></spine>
</package>"#;
    let nav = r##"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
  <nav epub:type="landmarks"><ol><li><a href="metin/bolum2.xhtml">Yanlış</a></li></ol></nav>
  <nav epub:type="toc"><ol>
    <li><a href="metin/bolum1.xhtml">Birinci Kısım</a></li>
    <li><a href="metin/bolum2.xhtml">İkinci Kısım</a>
      <ol><li><a href="metin/bolum2.xhtml#agac">Ağaçlar</a></li></ol></li>
  </ol></nav></body></html>"##;
    let ch1 = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>1</title></head><body>\
        <h1>Birinci Kısım</h1><p>İğne ile kuyu kazmak;\n şöyle ölçülü, çok&nbsp;güzel.</p></body></html>";
    let ch2 = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>İkinci Kısım</h1><p id=\"agac\">Ağaçlar.</p></body></html>";
    zip_of(&[
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", &container("OEBPS/content.opf")),
        ("OEBPS/content.opf", opf),
        ("OEBPS/nav.xhtml", nav),
        ("OEBPS/metin/bolum1.xhtml", ch1),
        ("OEBPS/metin/bolum2.xhtml", ch2),
    ])
}

pub fn epub2_sample() -> Vec<u8> {
    let opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Kuyucaklı Yusuf</dc:title></metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="a" href="a.html" media-type="application/xhtml+xml"/>
    <item id="b" href="b.html" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="a"/><itemref idref="b"/></spine>
</package>"#;
    let ncx = r#"<?xml version="1.0"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><navMap>
  <navPoint id="p1"><navLabel><text>Bir</text></navLabel><content src="a.html"/></navPoint>
  <navPoint id="p2"><navLabel><text>İki</text></navLabel><content src="b.html"/></navPoint>
</navMap></ncx>"#;
    zip_of(&[
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", &container("content.opf")),
        ("content.opf", opf),
        ("toc.ncx", ncx),
        ("a.html", "<html><body><p>bir</p></body></html>"),
        ("b.html", "<html><body><p>iki</p></body></html>"),
    ])
}

/// Kapaklı EPUB: EPUB3 biçiminde `cover-image` özelliğiyle, değilse EPUB2'nin
/// `<meta name="cover">`ıyla. Kapak 60×90, koyu gri.
pub fn epub_with_cover(epub3: bool) -> Vec<u8> {
    let (meta, props) = if epub3 {
        ("", r#" properties="cover-image""#)
    } else {
        (r#"<meta name="cover" content="kapak"/>"#, "")
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Kapaklı</dc:title>{meta}</metadata>
  <manifest>
    <item id="kapak" href="resim/kapak.png" media-type="image/png"{props}/>
    <item id="c" href="c.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c"/></spine>
</package>"#
    );
    let cover = png(60, 90, 40);
    zip_of_bytes(&[
        ("mimetype", b"application/epub+zip".as_slice()),
        (
            "META-INF/container.xml",
            container("OEBPS/content.opf").as_bytes(),
        ),
        ("OEBPS/content.opf", opf.as_bytes()),
        (
            "OEBPS/c.xhtml",
            b"<html><body><p>metin</p></body></html>".as_slice(),
        ),
        ("OEBPS/resim/kapak.png", &cover),
    ])
}
