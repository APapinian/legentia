//! Libra 2 masaüstü simülatörü. Kullanım: `legentia-sim kitap.epub`
//!
//! Fare tıklaması (ya da dokunmatik ekran) = dokunma. Okurken sol üçte bir
//! önceki sayfa, sağ üçte bir sonraki sayfa, orta içindekiler.
//! PageUp / Sol ok = üst sayfa tuşu (geri), PageDown / Sağ ok / Boşluk = alt tuş (ileri).
//! Esc = çıkış. Tam yenileme, ekranın bir an ters renkte gösterilmesiyle işaretlenir.
//!
//! Geçici geliştirici tuşları (ayar menüsü M4'te): + / - yazı boyutu,
//! F yazı tipi, L satır aralığı, M kenar boşluğu, J iki yana yaslama, H heceleme.
//!
//! LEGENTIA_SIM_SCALE: pencere küçültme katsayısı (tam sayı, varsayılan 2).

use std::process::ExitCode;
use std::time::Duration;

use legentia_core::i18n::{self, Msg, tr};
use legentia_core::{
    App, Book, Canvas, Event, Fonts, LIBRA2_HEIGHT, LIBRA2_WIDTH, PageKey, Refresh, Settings, Text,
    book_family_count, book_family_name,
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

    let mut app = match std::env::args_os().nth(1) {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            let started = std::time::Instant::now();
            let book = Book::open(&path);
            log::info!("opened {} in {:?}", path.display(), started.elapsed());
            App::new(font, LIBRA2_WIDTH, book)
        }
        None => App::with_message(font, LIBRA2_WIDTH, Msg::NoBookGiven),
    };
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
                _ => {
                    if let Some(settings) = adjust(app.settings(), key) {
                        log::info!(
                            "settings: {} {}px, line {}, margin {}, justify {}, hyphenate {}",
                            book_family_name(settings.family),
                            settings.font_px,
                            settings.line_spacing,
                            settings.margin,
                            settings.justify,
                            settings.hyphenate
                        );
                        let started = std::time::Instant::now();
                        let refresh = app.set_settings(settings);
                        log::info!("relayout in {:?}", started.elapsed());
                        if let Some(refresh) = refresh {
                            present(&mut window, &app, &mut canvas, &mut frame, scale, refresh);
                        }
                    }
                }
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
    ExitCode::SUCCESS
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

/// Geliştirici tuşları: ayarın değiştirilmiş bir kopyası.
fn adjust(current: &Settings, key: Key) -> Option<Settings> {
    let mut s = current.clone();
    match key {
        Key::Equal | Key::NumPadPlus => s.font_px = (s.font_px + 4.0).min(80.0),
        Key::Minus | Key::NumPadMinus => s.font_px = (s.font_px - 4.0).max(24.0),
        Key::F => s.family = (s.family + 1) % book_family_count(),
        Key::L => {
            s.line_spacing = if s.line_spacing >= 1.75 {
                1.2
            } else {
                s.line_spacing + 0.2
            }
        }
        Key::M => s.margin = if s.margin >= 150 { 30 } else { s.margin + 40 },
        Key::J => s.justify = !s.justify,
        Key::H => s.hyphenate = !s.hyphenate,
        _ => return None,
    }
    Some(s)
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
