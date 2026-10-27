/// Platformdan gelen girdi olayları. Koordinatlar ekran pikselidir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Tap { x: u32, y: u32 },
    Key(PageKey),
}

/// Libra 2'deki fiziksel sayfa tuşları (üst: geri, alt: ileri).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKey {
    Back,
    Forward,
}

/// E Ink yenileme türü. Kısmi yenileme hızlıdır ama gölge (ghosting) bırakır;
/// tam yenileme gölgeyi temizler ama ekranı bir kez yanıp söndürür.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    Partial,
    Full,
}
