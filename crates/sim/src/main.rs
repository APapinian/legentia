//! Libra 2 masaüstü simülatörü. Kullanım: `legentia-sim <kitap klasörü | kitap.epub>`
//! Kitap verilirse bulunduğu klasör kütüphane olur ve kitap doğrudan açılır.
//!
//! Fare tıklaması (ya da dokunmatik ekran) = dokunma. Okurken sol üçte bir
//! önceki sayfa, sağ üçte bir sonraki sayfa, orta menü.
//! PageUp / Sol ok = üst sayfa tuşu (geri), PageDown / Sağ ok / Boşluk = alt tuş (ileri).
//! Esc = çıkış. Tam yenileme, ekranın bir an ters renkte gösterilmesiyle işaretlenir.
//!
//! LEGENTIA_SIM_SCALE: pencere küçültme katsayısı (tam sayı, varsayılan 2).
//! LEGENTIA_DATA: ayar ve okuma kayıtlarının klasörü (varsayılan ~/.local/share/legentia).

use std::process::ExitCode;
use std::time::Duration;

use legentia_core::i18n::{self, Msg, tr};
use legentia_core::{
    App, Canvas, Event, Fonts, LIBRA2_HEIGHT, LIBRA2_WIDTH, PageKey, Refresh, Store, Text,
};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

const FULL_REFRESH_FLASH: Duration = Duration::from_millis(120);

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    i18n::install_panic_hook();

    let Some(font) = load_fonts() else {
        return ExitCode::FAILURE;
    };
    let scale = std::env::var("LEGENTIA_SIM_SCALE")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&s| s >= 1)
        .unwrap_or(2);

    let (w, h) = (LIBRA2_WIDTH / scale, LIBRA2_HEIGHT / scale);
    let mut window = match Window::new(
        &tr(&Msg::WindowTitle),
        w as usize,
        h as usize,
        WindowOptions::default(),
    ) {
        Ok(win) => win,
        Err(e) => {
            log::error!("window creation failed: {e}");
            i18n::show_error(&Msg::WindowFailed);
            return ExitCode::FAILURE;
        }
    };
    window.set_target_fps(60);

    let Some(arg) = std::env::args_os().nth(1).map(std::path::PathBuf::from) else {
        i18n::show_error(&Msg::Usage);
        return ExitCode::FAILURE;
    };
    let (root, book) = if arg.is_dir() {
        (arg, None)
    } else {
        let parent = arg.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        (parent, Some(arg))
    };
    let store = Store::new(&data_dir());
    log::info!("library {}, data {}", root.display(), store.dir().display());
    let started = std::time::Instant::now();
    let mut app = App::new(font, LIBRA2_WIDTH, store, &root);
    log::info!("library scanned in {:?}", started.elapsed());
    if let Some(book) = book {
        app.open_book(&book);
    }
    let mut canvas = Canvas::new(LIBRA2_WIDTH, LIBRA2_HEIGHT);
    let mut frame = vec![0u32; (w * h) as usize];
    let refresh = app.start();
    present(&mut window, &app, &mut canvas, &mut frame, scale, refresh);

    let mut was_down = false;
    while window.is_open() && !window.is_key_down(Key::Escape) {
        window.update();

        let mut events = Vec::new();
        let down = window.get_mouse_down(MouseButton::Left);
        if down
            && !was_down
            && let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard)
        {
            let x = ((mx as u32) * scale).min(LIBRA2_WIDTH - 1);
            let y = ((my as u32) * scale).min(LIBRA2_HEIGHT - 1);
            events.push(Event::Tap { x, y });
        }
        was_down = down;

        for key in window.get_keys_pressed(KeyRepeat::No) {
            match key {
                Key::PageUp | Key::Left => events.push(Event::Key(PageKey::Back)),
                Key::PageDown | Key::Right | Key::Space => {
                    events.push(Event::Key(PageKey::Forward))
                }
                _ => {}
            }
        }

        for event in events {
            let started = std::time::Instant::now();
            let handled = app.handle(event);
            log::info!("event: {event:?}, handled in {:?}", started.elapsed());
            if let Some(refresh) = handled {
                present(&mut window, &app, &mut canvas, &mut frame, scale, refresh);
            }
        }
    }
    app.shutdown();
    log::info!(
        "peak memory (VmHWM): {}",
        peak_rss().unwrap_or_else(|| "?".into())
    );
    ExitCode::SUCCESS
}

fn peak_rss() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM"))?;
    Some(
        line.split_whitespace()
            .skip(1)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn load_fonts() -> Option<Text> {
    let Ok(dir) = std::env::var("LEGENTIA_FONTS") else {
        i18n::show_error(&Msg::FontMissing);
        return None;
    };
    match Fonts::open(std::path::Path::new(&dir)) {
        Ok(fonts) => Some(Text::new(fonts)),
        Err(e) => {
            log::error!("cannot load fonts: {e:?}");
            i18n::show_error(&Msg::FontMissing);
            None
        }
    }
}

fn data_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("LEGENTIA_DATA") {
        return dir.into();
    }
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    home.join(".local/share/legentia")
}

fn present(
    window: &mut Window,
    app: &App,
    canvas: &mut Canvas,
    frame: &mut [u32],
    scale: u32,
    refresh: Refresh,
) {
    let started = std::time::Instant::now();
    app.draw(canvas);
    log::info!("refresh: {refresh:?}, drawn in {:?}", started.elapsed());
    let (w, h) = window.get_size();
    if refresh == Refresh::Full {
        downscale(canvas, frame, scale, true);
        let _ = window.update_with_buffer(frame, w, h);
        std::thread::sleep(FULL_REFRESH_FLASH);
    }
    downscale(canvas, frame, scale, false);
    if let Err(e) = window.update_with_buffer(frame, w, h) {
        log::error!("present failed: {e}");
    }
}

/// `scale`×`scale` blokların ortalamasıyla küçültür ve 0RGB'ye çevirir.
fn downscale(canvas: &Canvas, frame: &mut [u32], scale: u32, invert: bool) {
    let src = canvas.pixels();
    let cw = canvas.width();
    let fw = cw / scale;
    for (i, out) in frame.iter_mut().enumerate() {
        let (fx, fy) = (i as u32 % fw, i as u32 / fw);
        let mut sum = 0u32;
        for dy in 0..scale {
            let row = ((fy * scale + dy) * cw + fx * scale) as usize;
            sum += src[row..row + scale as usize]
                .iter()
                .map(|&p| p as u32)
                .sum::<u32>();
        }
        let mut g = sum / (scale * scale);
        if invert {
            g = 255 - g;
        }
        *out = (g << 16) | (g << 8) | g;
    }
}
