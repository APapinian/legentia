//! Legentia çekirdeği: platformdan bağımsız uygulama mantığı.
//!
//! Platform sözleşmesi bu crate'in tiplerinden oluşur. Bir platform
//! (masaüstü simülatörü, ileride Kobo):
//! 1. [`Canvas`] boyutunda bir ekran sağlar,
//! 2. donanım olaylarını [`Event`] olarak [`App::handle`]'a verir,
//! 3. dönen [`Refresh`] isteğine göre [`App::draw`] ile çizip ekrana basar.
//!
//! Ekran yalnızca bir olay geldiğinde yeniden çizilir; sürekli çizim yoktur.

mod app;
mod canvas;
pub mod epub;
mod event;
pub mod i18n;
mod text;

pub use app::{App, Position, Settings};
pub use canvas::Canvas;
pub use epub::{Book, EpubError};
pub use event::{Event, PageKey, Refresh};
pub use text::{Family, FontError, Fonts, Span, Style, Text, book_family_count, book_family_name};

/// Kobo Libra 2 ekranı, dikey konum (bkz. docs/device-compatibility.md).
pub const LIBRA2_WIDTH: u32 = 1264;
pub const LIBRA2_HEIGHT: u32 = 1680;
pub const LIBRA2_PPI: u32 = 300;
