use std::cell::OnceCell;
use std::path::{Path, PathBuf};

/// Satır içi biçim. Bağlantılar altı çizili gösterilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub link: bool,
}

/// Font ailesi: arayüz ya da `BOOK_FAMILIES` içindeki bir kitap fontu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Ui,
    Book(usize),
}

struct FamilyDef {
    name: &'static str,
    /// Normal, italik, kalın, kalın italik.
    files: [&'static str; 4],
}

/// Hepsi SIL OFL 1.1. Dosyalar flake.nix'teki `fonts` klasöründen gelir.
const UI_FAMILY: FamilyDef = FamilyDef {
    name: "Source Sans 3",
    files: [
        "SourceSans3-Regular.otf",
        "SourceSans3-It.otf",
        "SourceSans3-Bold.otf",
        "SourceSans3-BoldIt.otf",
    ],
};
const BOOK_FAMILIES: &[FamilyDef] = &[
    FamilyDef {
        name: "Literata",
        files: [
            "Literata-Regular.ttf",
            "Literata-Italic.ttf",
            "Literata-Bold.ttf",
            "Literata-BoldItalic.ttf",
        ],
    },
    FamilyDef {
        name: "Source Serif 4",
        files: [
            "SourceSerif4-Regular.otf",
            "SourceSerif4-It.otf",
            "SourceSerif4-Bold.otf",
            "SourceSerif4-BoldIt.otf",
        ],
    },
    FamilyDef {
        name: "Gentium",
        files: [
            "Gentium-Regular.ttf",
            "Gentium-Italic.ttf",
            "Gentium-Bold.ttf",
            "Gentium-BoldItalic.ttf",
        ],
    },
];

pub fn book_family_count() -> usize {
    BOOK_FAMILIES.len()
}

pub fn book_family_name(index: usize) -> &'static str {
    BOOK_FAMILIES[index % BOOK_FAMILIES.len()].name
}

/// Yüklü bir font yüzü (aile + stil). Glif önbelleğinin anahtarıdır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FaceId(u16);

#[derive(Debug)]
pub enum FontError {
    /// Arayüz ya da varsayılan kitap fontu bulunamadı veya okunamadı.
    Missing(PathBuf),
}

/// Yüklenmiş bir yüz: dosya verisi ve tabloları bir kez okunmuş rustybuzz yüzü.
///
/// Veri bilerek program sonuna kadar bellekte tutulur (`Box::leak`): bir yüz
/// yüklendikten sonra hiç bırakılmaz, böylece tablo okuması paragraf başına
/// değil yüz başına bir kez yapılır (Gentium'da yeniden dizme süresini yarıya indirir).
/// En fazla 16 yüz vardır.
struct Loaded {
    data: &'static [u8],
    hb: rustybuzz::Face<'static>,
}

/// Font dosyaları ilk kullanıldıklarında yüklenir; seçilmeyen aileler belleğe girmez.
pub struct Fonts {
    dir: PathBuf,
    faces: Vec<OnceCell<Option<Loaded>>>,
}

impl Fonts {
    /// Arayüz fontunun ve varsayılan kitap fontunun normal yüzü zorunludur;
    /// diğer yüzler eksikse aynı ailenin normal yüzü kullanılır.
    pub fn open(dir: &Path) -> Result<Fonts, FontError> {
        let count = (1 + BOOK_FAMILIES.len()) * 4;
        let fonts = Fonts {
            dir: dir.to_path_buf(),
            faces: (0..count).map(|_| OnceCell::new()).collect(),
        };
        for family in [Family::Ui, Family::Book(0)] {
            let id = Self::id(family, Style::default());
            if fonts.load(id).is_none() {
                return Err(FontError::Missing(fonts.path_of(id)));
            }
        }
        Ok(fonts)
    }

    fn id(family: Family, style: Style) -> FaceId {
        let slot = match family {
            Family::Ui => 0,
            Family::Book(i) => 1 + i % BOOK_FAMILIES.len(),
        };
        let variant = (style.bold as usize) << 1 | style.italic as usize;
        FaceId((slot * 4 + variant) as u16)
    }

    fn path_of(&self, id: FaceId) -> PathBuf {
        let slot = id.0 as usize / 4;
        let def = if slot == 0 {
            &UI_FAMILY
        } else {
            &BOOK_FAMILIES[slot - 1]
        };
        self.dir.join(def.files[id.0 as usize % 4])
    }

    fn load(&self, id: FaceId) -> Option<&Loaded> {
        self.faces[id.0 as usize]
            .get_or_init(|| {
                let path = self.path_of(id);
                let data = std::fs::read(&path).ok()?;
                if rustybuzz::Face::from_slice(&data, 0).is_none() {
                    log::warn!("not a usable font: {}", path.display());
                    return None;
                }
                let data: &'static [u8] = Box::leak(data.into_boxed_slice());
                let hb = rustybuzz::Face::from_slice(data, 0)?;
                Some(Loaded { data, hb })
            })
            .as_ref()
    }

    /// İstenen yüz, yoksa ailenin normal yüzü, o da yoksa arayüz fontu.
    pub(crate) fn face(&self, family: Family, style: Style) -> FaceId {
        let wanted = Self::id(family, style);
        if self.load(wanted).is_some() {
            return wanted;
        }
        let regular = Self::id(family, Style::default());
        if self.load(regular).is_some() {
            return regular;
        }
        Self::id(Family::Ui, Style::default())
    }

    /// `face` ile çözülmüş bir yüzün verisi.
    pub(crate) fn data(&self, id: FaceId) -> &[u8] {
        self.loaded(id).data
    }

    /// `face` ile çözülmüş bir yüzün harf birleştirme nesnesi.
    pub(crate) fn shaper(&self, id: FaceId) -> &rustybuzz::Face<'static> {
        &self.loaded(id).hb
    }

    fn loaded(&self, id: FaceId) -> &Loaded {
        self.load(id)
            .expect("FaceId yalnızca yüklü yüzler için verilir")
    }
}
