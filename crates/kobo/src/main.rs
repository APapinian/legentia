//! Legentia, Kobo Libra 2. Kullanım: `legentia-kobo [kütüphane klasörü]`
//! (varsayılan `/mnt/onboard`). Nickel'ı durdurup geri başlatmak başlatma
//! betiğinin işidir; bu program yalnızca ekranı ve girdiyi kullanır.
//!
//! Klasörler programın yanında: `fonts/` (ya da LEGENTIA_FONTS), `veri/`
//! (ayar ve okuma kayıtları; ya da LEGENTIA_DATA), günlük `legentia.log`.
//! Ekran 8 bit griye geçirilir; LEGENTIA_FB8=0 ise 32 bit'te kalır.
//! Çıkış: kütüphanedeki "Çık" ya da güç tuşu (kayıtlar yazılır).

mod fb;
mod input;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use legentia_core::i18n::{self, Msg};
use legentia_core::{App, Canvas, Event, Fonts, LIBRA2_HEIGHT, LIBRA2_WIDTH, Refresh, Store, Text};

use input::Input;

/// Arka plan işi sürerken `Event::Tick` aralığı.
const TICK: Duration = Duration::from_millis(500);
const FRAMEBUFFER: &str = "/dev/fb0";
const DEFAULT_LIBRARY: &str = "/mnt/onboard";

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn dir_from_env(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var).map_or_else(|| exe_dir().join(default), PathBuf::from)
}

fn main() -> ExitCode {
    let mut logger =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Ok(file) = std::fs::File::create(exe_dir().join("legentia.log")) {
        logger.target(env_logger::Target::Pipe(Box::new(file)));
    }
    logger.init();
    i18n::install_panic_hook();
    std::panic::catch_unwind(run).unwrap_or_else(|_| {
        i18n::show_error(&Msg::Crashed);
        ExitCode::FAILURE
    })
}

fn run() -> ExitCode {
    let fonts = dir_from_env("LEGENTIA_FONTS", "fonts");
    let text = match Fonts::open(&fonts) {
        Ok(f) => Text::new(f),
        Err(e) => {
            log::error!("cannot load fonts from {}: {e:?}", fonts.display());
            i18n::show_error(&Msg::FontMissing);
            return ExitCode::FAILURE;
        }
    };
    let gray8 = std::env::var("LEGENTIA_FB8").map_or(true, |v| v != "0");
    let mut screen = match fb::Framebuffer::open(FRAMEBUFFER, (LIBRA2_WIDTH, LIBRA2_HEIGHT), gray8)
    {
        Ok(s) => s,
        Err(e) => {
            log::error!("cannot open {FRAMEBUFFER}: {e}");
            i18n::show_error(&Msg::ScreenFailed);
            return ExitCode::FAILURE;
        }
    };

    let root = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from(DEFAULT_LIBRARY), PathBuf::from);
    let store = Store::new(&dir_from_env("LEGENTIA_DATA", "veri"));
    log::info!("library {}, data {}", root.display(), store.dir().display());
    let started = Instant::now();
    let mut app = App::new(text, LIBRA2_WIDTH, store, &root);
    log::info!("library scanned in {:?}", started.elapsed());

    // Dikey dönüş (Nickel'ın bıraktığı: 1 ya da ters dikey 3) ve okuma yataya
    // dönünce kullanılacak dönüş. Yatayın hangi yana olduğu ivmeölçerden gelir;
    // gelmediyse (elle "Yatay") dikeyin bir sonrakisi. 0/2 seçimi cihazda doğrulanmadı.
    let portrait_rotate = screen.rotate;
    let mut landscape_rotate = (portrait_rotate + 1) % 4;
    log::info!("screen rotation {portrait_rotate}");
    let rotation = Arc::new(AtomicU32::new(portrait_rotate));
    let (tx, rx) = channel();
    if input::spawn_readers(&tx, &rotation) == 0 {
        log::error!("no input devices");
    }
    drop(tx);

    let mut canvas = Canvas::new(LIBRA2_WIDTH, LIBRA2_HEIGHT);
    let refresh = app.start();
    present(&mut screen, &app, &mut canvas, refresh);

    let clock = Instant::now();
    let mut last_tick = Instant::now();
    loop {
        let wait = if app.wants_ticks() {
            TICK.saturating_sub(last_tick.elapsed())
        } else {
            Duration::from_secs(3600)
        };
        let mut events = Vec::new();
        match rx.recv_timeout(wait) {
            Ok(Input::Power) => {
                log::info!("power button: exit");
                break;
            }
            Ok(Input::Touch { phase, x, y }) => events.push(Event::Touch {
                phase,
                x,
                y,
                ms: clock.elapsed().as_millis() as u64,
            }),
            Ok(Input::SecondTouch { phase, x, y }) => {
                events.push(Event::SecondTouch { phase, x, y })
            }
            Ok(Input::Page(key)) => events.push(Event::Key(key)),
            Ok(Input::Tilt {
                landscape,
                positive,
            }) => {
                if landscape {
                    landscape_rotate = if positive {
                        (portrait_rotate + 1) % 4
                    } else {
                        (portrait_rotate + 3) % 4
                    };
                }
                events.push(Event::Tilt { landscape });
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                log::error!("all input devices closed");
                break;
            }
        }
        if app.wants_ticks() && last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            events.push(Event::Tick);
        }
        for event in events {
            let started = Instant::now();
            let handled = app.handle(event);
            if event != Event::Tick {
                log::info!("event: {event:?}, handled in {:?}", started.elapsed());
            }
            let size = app.screen_size();
            if size != (canvas.width(), canvas.height()) {
                // Okuma yatay ya da dikey döndü: sürücünün dönüşü değişir.
                let target = if size.0 > size.1 {
                    landscape_rotate
                } else {
                    portrait_rotate
                };
                if screen.set_rotation(target) {
                    rotation.store(target, Ordering::Relaxed);
                } else {
                    log::error!("cannot rotate screen to {target}");
                }
                canvas = Canvas::new(size.0, size.1);
            }
            if let Some(refresh) = handled {
                present(&mut screen, &app, &mut canvas, refresh);
            }
        }
        if app.wants_exit() {
            break;
        }
    }
    app.shutdown();
    app.draw_farewell(&mut canvas);
    screen.present(&canvas, Refresh::Full);
    screen.restore();
    log::info!("exit");
    ExitCode::SUCCESS
}

fn present(screen: &mut fb::Framebuffer, app: &App, canvas: &mut Canvas, refresh: Refresh) {
    let started = Instant::now();
    app.draw(canvas);
    let drawn = started.elapsed();
    screen.present(canvas, refresh);
    log::info!(
        "refresh: {refresh:?}, drawn in {drawn:?}, presented in {:?}",
        started.elapsed() - drawn
    );
}
