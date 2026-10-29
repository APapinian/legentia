//! Geliştirici aracı: pencere açmadan ekran görüntüsü alır. Kütüphaneyi açar,
//! ilk kitabı okur, menü ve yazı ayarlarını gezer; her ekranı PGM olarak yazar.
//! Kayıtlar geçici bir veri klasörüne gider; kullanıcının verisine dokunulmaz.
//!
//!     cargo run --release -p legentia-core --example screenshots -- <kütüphane> <çıktı klasörü> [kitap sırası] [reflow] [sutun]
//!
//! `reflow` verilirse PDF'ler yeniden akıtılmış, `sutun` verilirse sütun sütun açılır,
//! `izgara` verilirse kütüphane kapak ızgarası olarak (kapaklar beklenir), `raf`
//! verilirse raflar olarak çekilir.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::path::{Path, PathBuf};

use legentia_core::{App, Canvas, Event, Fonts, LIBRA2_HEIGHT, LIBRA2_WIDTH, PageKey, Store, Text};

// Ekran ölçüleri (crates/core/src/app/ui.rs ve screens.rs ile aynı).
const HEADER_BOTTOM: u32 = 90 + 175;
/// Kütüphane listesi araç çubuğunun (100 piksel) altından başlar.
const LIBRARY_TOP: u32 = HEADER_BOTTOM + 100;
const LIBRARY_ROW: u32 = 120;
const MENU_TOP: u32 = 90;
const MENU_ROW: u32 = 100;

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [library, out, rest @ ..] = args.as_slice() else {
        eprintln!("usage: screenshots <library> <out dir> [book index]");
        std::process::exit(2);
    };
    let book: u32 = rest
        .first()
        .and_then(|s| s.to_str()?.parse().ok())
        .unwrap_or(0);
    std::fs::create_dir_all(out).expect("output dir");
    let fonts = Fonts::open(Path::new(
        &std::env::var("LEGENTIA_FONTS").expect("LEGENTIA_FONTS"),
    ))
    .expect("fonts");
    let data = std::env::temp_dir().join(format!("legentia-screens-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let flag = |name: &str| rest.iter().any(|a| a.as_os_str() == name);
    let mut settings = Vec::new();
    if flag("reflow") {
        settings.push(r#""pdf_reflow": true"#);
    }
    if flag("sutun") {
        settings.push(r#""pdf_view": "Columns""#);
    }
    if flag("izgara") {
        settings.push(r#""library_view": "Covers""#);
    }
    if flag("raf") {
        settings.push(r#""library_view": "Shelves""#);
    }
    if !settings.is_empty() {
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::write(
            data.join("settings.json"),
            format!(r#"{{"version": 1, "data": {{{}}}}}"#, settings.join(", ")),
        )
        .expect("settings");
    }

    let mut app = App::new(Text::new(fonts), LIBRA2_WIDTH, Store::new(&data), library);
    let mut canvas = Canvas::new(LIBRA2_WIDTH, LIBRA2_HEIGHT);
    let mut shot = |app: &App, name: &str| {
        app.draw(&mut canvas);
        let path = out.join(format!("{name}.pgm"));
        write_pgm(&canvas, &path);
        println!("{}", path.display());
    };
    let tap = |app: &mut App, x: u32, y: u32| {
        app.handle(Event::Tap { x, y });
    };
    let middle = (LIBRA2_WIDTH / 2, LIBRA2_HEIGHT / 2);
    let menu_item = |i: u32| (LIBRA2_WIDTH / 2, MENU_TOP + i * MENU_ROW + MENU_ROW / 2);

    app.start();
    while app.wants_ticks() {
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.handle(Event::Tick);
    }
    shot(&app, "1-kutuphane");
    tap(
        &mut app,
        LIBRARY_ROW,
        LIBRARY_TOP + book * LIBRARY_ROW + LIBRARY_ROW / 2,
    );
    shot(&app, "2-icindekiler");
    tap(&mut app, LIBRARY_ROW, HEADER_BOTTOM + 3 * 66 + 33);
    shot(&app, "3-okuma");
    for _ in 0..2 {
        app.handle(Event::Key(PageKey::Forward));
    }
    tap(&mut app, middle.0, middle.1);
    shot(&app, "4-menu");
    let (x, y) = menu_item(1);
    tap(&mut app, x, y); // yer imi koy
    shot(&app, "5-yer-imli-sayfa");
    tap(&mut app, middle.0, middle.1);
    let (x, y) = menu_item(2);
    tap(&mut app, x, y);
    shot(&app, "6-yer-imleri");
    tap(&mut app, BACK_BUTTON_X, 90 + 36);
    tap(&mut app, middle.0, middle.1);
    let (x, y) = menu_item(3);
    tap(&mut app, x, y);
    shot(&app, "7-yazi-ayarlari");
    app.handle(Event::Key(PageKey::Forward)); // PDF ayarları sayfası
    shot(&app, "8-pdf-ayarlari");
    let _ = std::fs::remove_dir_all(&data);
}

/// "Geri" düğmesinin ortası (sağ kenardan 80 + 85 piksel içeride).
const BACK_BUTTON_X: u32 = LIBRA2_WIDTH - 80 - 85;

fn write_pgm(canvas: &Canvas, path: &Path) {
    let mut bytes = format!("P5\n{} {}\n255\n", canvas.width(), canvas.height()).into_bytes();
    bytes.extend_from_slice(canvas.pixels());
    std::fs::write(path, bytes).expect("write pgm");
}
