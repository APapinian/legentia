//! Libra 2 masaüstü simülatörü.
//!
//! Fare tıklaması (ya da dokunmatik ekran) = dokunma.
//! PageUp / Sol ok = üst sayfa tuşu (geri), PageDown / Sağ ok / Boşluk = alt tuş (ileri).
//! Esc = çıkış. Tam yenileme, ekranın bir an ters renkte gösterilmesiyle işaretlenir.
//!
//! LEGENTIA_SIM_SCALE: pencere küçültme katsayısı (tam sayı, varsayılan 2).

use std::process::ExitCode;
use std::time::Duration;

use legentia_core::i18n::{self, Msg, tr};
use legentia_core::{App, Canvas, Event, Font, LIBRA2_HEIGHT, LIBRA2_WIDTH, PageKey, Refresh};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};

const FULL_REFRESH_FLASH: Duration = Duration::from_millis(120);

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    i18n::install_panic_hook();

    let Some(font) = load_font() else {
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

    let mut app = App::new(font);
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
            log::info!("event: {event:?}");
            if let Some(refresh) = app.handle(event) {
                present(&mut window, &app, &mut canvas, &mut frame, scale, refresh);
            }
        }
    }
    ExitCode::SUCCESS
}

fn load_font() -> Option<Font> {
    let Ok(path) = std::env::var("LEGENTIA_FONT") else {
        i18n::show_error(&Msg::FontMissing);
        return None;
    };
    let font = std::fs::read(&path).ok().and_then(|b| Font::from_bytes(&b));
    if font.is_none() {
        log::error!("cannot load font from {path}");
        i18n::show_error(&Msg::FontInvalid);
    }
    font
}

fn present(
    window: &mut Window,
    app: &App,
    canvas: &mut Canvas,
    frame: &mut [u32],
    scale: u32,
    refresh: Refresh,
) {
    log::info!("refresh: {refresh:?}");
    app.draw(canvas);
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
