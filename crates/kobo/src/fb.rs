//! Kobo ekranı: `mxc_epdc_fb` framebuffer'ı ve MXCFB v2 E Ink yenilemesi
//! (i.MX6SLL, "Mark 7"). Ekran bilgisi sürücüden okunur, sabit kodlanmaz.
//!
//! Açılışta ekran 8 bit griye geçirilir (KOReader ve Plato gibi; 32 bit'e
//! göre dörtte bir veri), çıkışta eski hâline döner. Tuval framebuffer'a
//! sıradan dosya yazmasıyla aktarılır (mmap yok); yalnızca değişen satırlar
//! yazılır ve yenilenir. `unsafe` yalnızca bu dosyadaki ioctl çağrılarındadır. Yapılar ve sabitler Linux
//! `fb.h` ve Kobo `mxcfb.h` (FBInk'in Mark 7 başlığı) ile aynı düzendedir.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::raw::c_ulong;
use std::os::unix::fs::FileExt;
use std::time::Instant;

use legentia_core::{Canvas, Refresh};

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct Bitfield {
    pub offset: u32,
    pub length: u32,
    pub msb_right: u32,
}

/// `struct fb_var_screeninfo`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct VarScreeninfo {
    pub xres: u32,
    pub yres: u32,
    pub xres_virtual: u32,
    pub yres_virtual: u32,
    pub xoffset: u32,
    pub yoffset: u32,
    pub bits_per_pixel: u32,
    pub grayscale: u32,
    pub red: Bitfield,
    pub green: Bitfield,
    pub blue: Bitfield,
    pub transp: Bitfield,
    pub nonstd: u32,
    pub activate: u32,
    pub height: u32,
    pub width: u32,
    pub accel_flags: u32,
    pub pixclock: u32,
    pub left_margin: u32,
    pub right_margin: u32,
    pub upper_margin: u32,
    pub lower_margin: u32,
    pub hsync_len: u32,
    pub vsync_len: u32,
    pub sync: u32,
    pub vmode: u32,
    pub rotate: u32,
    pub colorspace: u32,
    pub reserved: [u32; 4],
}

/// `struct fb_fix_screeninfo`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct FixScreeninfo {
    pub id: [u8; 16],
    pub smem_start: c_ulong,
    pub smem_len: u32,
    pub kind: u32,
    pub type_aux: u32,
    pub visual: u32,
    pub xpanstep: u16,
    pub ypanstep: u16,
    pub ywrapstep: u16,
    pub line_length: u32,
    pub mmio_start: c_ulong,
    pub mmio_len: u32,
    pub accel: u32,
    pub capabilities: u16,
    pub reserved: [u16; 2],
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct MxcfbRect {
    pub top: u32,
    pub left: u32,
    pub width: u32,
    pub height: u32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct AltBufferData {
    pub phys_addr: u32,
    pub width: u32,
    pub height: u32,
    pub alt_update_region: MxcfbRect,
}

/// `struct mxcfb_update_data` (v2, Mark 7).
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct UpdateData {
    pub update_region: MxcfbRect,
    pub waveform_mode: u32,
    pub update_mode: u32,
    pub update_marker: u32,
    pub temp: i32,
    pub flags: u32,
    pub dither_mode: i32,
    pub quant_bit: i32,
    pub alt_buffer_data: AltBufferData,
}

/// `struct mxcfb_update_marker_data`.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct UpdateMarker {
    pub update_marker: u32,
    pub collision_test: u32,
}

/// Mark 7 dalga biçimleri (`NTX_WFM_MODE_*`).
const WAVEFORM_GC16: u32 = 2;
const WAVEFORM_GL16: u32 = 5;
const UPDATE_MODE_PARTIAL: u32 = 0;
const UPDATE_MODE_FULL: u32 = 1;
const TEMP_USE_AMBIENT: i32 = 0x1000;
/// `fb.h`: ayarı hemen ve zorla uygula; 8 bit gri (`GRAYSCALE_8BIT`).
const FB_ACTIVATE_FORCE: u32 = 128;
const GRAYSCALE_8BIT: u32 = 1;

mod ioctl {
    use super::{FixScreeninfo, UpdateData, UpdateMarker, VarScreeninfo};
    nix::ioctl_read_bad!(get_vscreeninfo, 0x4600, VarScreeninfo);
    nix::ioctl_read_bad!(get_fscreeninfo, 0x4602, FixScreeninfo);
    nix::ioctl_write_ptr_bad!(put_vscreeninfo, 0x4601, VarScreeninfo);
    nix::ioctl_write_ptr!(send_update, b'F', 0x2E, UpdateData);
    nix::ioctl_readwrite!(wait_for_update, b'F', 0x2F, UpdateMarker);
}

/// Piksellerin framebuffer'daki yerleşimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    /// Satır uzunluğu (bayt).
    pub stride: u32,
    pub bits_per_pixel: u32,
    /// 32 bit'te renk kanallarının bit konumları (r, g, b) ve varsa saydamlık.
    pub shifts: [u32; 3],
    pub alpha: Option<u32>,
}

impl Layout {
    fn of(var: &VarScreeninfo, fix: &FixScreeninfo) -> Layout {
        Layout {
            width: var.xres,
            height: var.yres,
            stride: fix.line_length,
            bits_per_pixel: var.bits_per_pixel,
            shifts: [var.red.offset, var.green.offset, var.blue.offset],
            alpha: (var.transp.length > 0).then_some(var.transp.offset),
        }
    }

    /// Gri tuvali bu yerleşime çevirir; `out` en az `stride * height` bayt.
    pub fn pack(&self, gray: &[u8], gray_width: u32, out: &mut [u8]) {
        let rows = (gray.len() as u32 / gray_width.max(1)).min(self.height);
        let cols = gray_width.min(self.width) as usize;
        for y in 0..rows as usize {
            let src = &gray[y * gray_width as usize..][..cols];
            let dst = &mut out[y * self.stride as usize..];
            match self.bits_per_pixel {
                8 => dst[..cols].copy_from_slice(src),
                _ => {
                    let alpha = self.alpha.map_or(0, |a| 0xffu32 << a);
                    for (x, &g) in src.iter().enumerate() {
                        let g = g as u32;
                        let px = self.shifts.iter().fold(alpha, |p, &s| p | g << s);
                        dst[x * 4..x * 4 + 4].copy_from_slice(&px.to_le_bytes());
                    }
                }
            }
        }
    }
}

pub struct Framebuffer {
    file: File,
    pub layout: Layout,
    /// Sürücünün dönüşü (`fb_var_screeninfo.rotate`); dokunma ve tuşlar buna göre çevrilir.
    pub rotate: u32,
    /// Açılıştaki ayar; 8 bit'e geçildiyse çıkışta geri yüklenir.
    original: Option<VarScreeninfo>,
    /// Yazılacak kare ve ekranda olduğu bilinen kare (yalnızca değişen satırlar yazılır).
    next: Vec<u8>,
    shown: Option<Vec<u8>>,
    marker: u32,
    /// Son gönderilen yenilemenin işareti; yeni kare yazılmadan bitmesi beklenir.
    pending: Option<u32>,
}

#[derive(Debug)]
pub enum FbError {
    Io(std::io::Error),
    Ioctl(nix::Error),
    /// Desteklenmeyen piksel derinliği.
    Depth(u32),
    /// Beklenmeyen ekran boyutu (dönüş ya da başka bir ekran).
    Size(u32, u32),
}

impl std::fmt::Display for FbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FbError::Io(e) => write!(f, "{e}"),
            FbError::Ioctl(e) => write!(f, "ioctl: {e}"),
            FbError::Depth(bpp) => write!(f, "unsupported depth: {bpp} bpp"),
            FbError::Size(w, h) => write!(f, "unexpected screen {w}x{h} (rotation?)"),
        }
    }
}

fn read_info(file: &File) -> Result<(VarScreeninfo, FixScreeninfo), FbError> {
    let mut var = VarScreeninfo::default();
    let mut fix = FixScreeninfo::default();
    // SAFETY: fd açık bir framebuffer; yapılar çekirdeğin beklediği düzende
    // (`fb.h`) ve çağrı boyunca yaşıyor.
    unsafe {
        ioctl::get_vscreeninfo(file.as_raw_fd(), &mut var).map_err(FbError::Ioctl)?;
        ioctl::get_fscreeninfo(file.as_raw_fd(), &mut fix).map_err(FbError::Ioctl)?;
    }
    Ok((var, fix))
}

fn put_info(file: &File, var: &VarScreeninfo) -> Result<(), FbError> {
    let mut var = *var;
    var.activate = FB_ACTIVATE_FORCE;
    // SAFETY: yukarıdaki gibi.
    unsafe { ioctl::put_vscreeninfo(file.as_raw_fd(), &var) }
        .map(|_| ())
        .map_err(FbError::Ioctl)
}

impl Framebuffer {
    /// Açar; ekran `size` değilse hiçbir şeyi değiştirmeden hata döner (ör.
    /// bilgisayarın kendi framebuffer'ı). `gray8` ise ekranı 8 bit griye
    /// geçirir (dörtte bir veri); geçiş olmazsa ya da boyut/dönüş değişirse
    /// eski ayarla devam eder.
    pub fn open(path: &str, size: (u32, u32), gray8: bool) -> Result<Framebuffer, FbError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(FbError::Io)?;
        let (mut var, mut fix) = read_info(&file)?;
        log::info!("framebuffer {var:?} {fix:?}");
        if (var.xres, var.yres) != size {
            return Err(FbError::Size(var.xres, var.yres));
        }
        let mut original = None;
        if gray8 && var.bits_per_pixel != 8 {
            let mut gray = var;
            gray.bits_per_pixel = 8;
            gray.grayscale = GRAYSCALE_8BIT;
            let plain = Bitfield {
                offset: 0,
                length: 8,
                msb_right: 0,
            };
            (gray.red, gray.green, gray.blue) = (plain, plain, plain);
            gray.transp = Bitfield::default();
            let switched = put_info(&file, &gray).and_then(|_| read_info(&file));
            match switched {
                Ok((v, f))
                    if v.bits_per_pixel == 8
                        && (v.xres, v.yres, v.rotate) == (var.xres, var.yres, var.rotate) =>
                {
                    log::info!("switched to 8 bpp gray: {v:?} {f:?}");
                    original = Some(var);
                    (var, fix) = (v, f);
                }
                other => {
                    log::warn!(
                        "8 bpp switch failed ({other:?}); keeping {} bpp",
                        var.bits_per_pixel
                    );
                    if let Err(e) = put_info(&file, &var) {
                        log::error!("cannot restore screen settings: {e}");
                    }
                    (var, fix) = read_info(&file)?;
                }
            }
        }
        let layout = Layout::of(&var, &fix);
        if layout.bits_per_pixel != 8 && layout.bits_per_pixel != 32 {
            return Err(FbError::Depth(layout.bits_per_pixel));
        }
        Ok(Framebuffer {
            file,
            next: vec![0; (layout.stride * layout.height) as usize],
            layout,
            rotate: var.rotate,
            original,
            shown: None,
            marker: 0,
            pending: None,
        })
    }

    /// Açılıştaki ekran ayarını (piksel derinliği) geri yükler.
    pub fn restore(&mut self) {
        self.wait_pending();
        if let Some(var) = self.original.take() {
            match put_info(&self.file, &var) {
                Ok(()) => log::info!("screen restored to {} bpp", var.bits_per_pixel),
                Err(e) => log::error!("cannot restore screen settings: {e}"),
            }
        }
    }

    fn wait_pending(&mut self) {
        if let Some(marker) = self.pending.take() {
            let mut data = UpdateMarker {
                update_marker: marker,
                collision_test: 0,
            };
            // SAFETY: yukarıdaki gibi; yapı `mxcfb.h` düzeninde.
            if let Err(e) = unsafe { ioctl::wait_for_update(self.file.as_raw_fd(), &mut data) } {
                log::warn!("wait for update {marker}: {e}");
            }
        }
    }

    /// Tuvali ekrana yazar ve yenilemeyi ister; yalnızca değişen satırlar
    /// yazılıp yenilenir (tam yenilemede bütün ekran yanıp söner). Hata
    /// günlüğe yazılır: ekran bir kez yenilenemedi diye okuma bozulmamalı.
    pub fn present(&mut self, canvas: &Canvas, refresh: Refresh) {
        let t0 = Instant::now();
        self.wait_pending();
        let t1 = Instant::now();
        self.layout
            .pack(canvas.pixels(), canvas.width(), &mut self.next);
        let stride = self.layout.stride as usize;
        let rows = match &self.shown {
            Some(shown) => changed_rows(shown, &self.next, stride),
            None => Some((0, self.layout.height as usize)),
        };
        let t2 = Instant::now();
        let full = refresh == Refresh::Full;
        let Some((top, bottom)) = rows.or(full.then_some((0, 0))) else {
            log::info!(
                "present: nothing changed (wait {:?}, pack {:?})",
                t1 - t0,
                t2 - t1
            );
            return;
        };
        let range = top * stride..bottom * stride;
        if let Err(e) = self
            .file
            .write_all_at(&self.next[range.clone()], range.start as u64)
        {
            log::error!("framebuffer write: {e}");
            self.shown = None;
            return;
        }
        match &mut self.shown {
            Some(shown) => shown[range].copy_from_slice(&self.next[top * stride..bottom * stride]),
            None => self.shown = Some(self.next.clone()),
        }
        let t3 = Instant::now();
        self.marker = self.marker.wrapping_add(1).max(1);
        let region = if full {
            (0, self.layout.height)
        } else {
            (top as u32, (bottom - top) as u32)
        };
        let update = UpdateData {
            update_region: MxcfbRect {
                top: region.0,
                left: 0,
                width: self.layout.width,
                height: region.1,
            },
            waveform_mode: if full { WAVEFORM_GC16 } else { WAVEFORM_GL16 },
            update_mode: if full {
                UPDATE_MODE_FULL
            } else {
                UPDATE_MODE_PARTIAL
            },
            update_marker: self.marker,
            temp: TEMP_USE_AMBIENT,
            ..UpdateData::default()
        };
        // SAFETY: yukarıdaki gibi.
        match unsafe { ioctl::send_update(self.file.as_raw_fd(), &update) } {
            Ok(_) => self.pending = Some(self.marker),
            Err(e) => log::error!("send update: {e}"),
        }
        log::info!(
            "present: rows {top}..{bottom}, wait {:?}, pack+diff {:?}, write {:?}, update {:?}",
            t1 - t0,
            t2 - t1,
            t3 - t2,
            t3.elapsed()
        );
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        self.restore();
    }
}

/// `old` ile `new` arasında değişen satırların aralığı (`üst..alt`); aynıysa `None`.
pub fn changed_rows(old: &[u8], new: &[u8], stride: usize) -> Option<(usize, usize)> {
    let differs = |y: &usize| old[y * stride..][..stride] != new[y * stride..][..stride];
    let rows = new.len() / stride.max(1);
    let top = (0..rows).find(differs)?;
    let bottom = (0..rows).rev().find(differs)? + 1;
    Some((top, bottom))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn structs_match_the_kernel_layout() {
        assert_eq!(size_of::<VarScreeninfo>(), 160);
        let fix = if cfg!(target_pointer_width = "64") {
            80
        } else {
            68
        };
        assert_eq!(size_of::<FixScreeninfo>(), fix);
        assert_eq!(size_of::<UpdateData>(), 72);
        assert_eq!(size_of::<UpdateMarker>(), 8);
    }

    #[test]
    fn only_changed_rows_are_found() {
        let old = vec![0u8; 12];
        let mut new = old.clone();
        assert_eq!(changed_rows(&old, &new, 3), None);
        new[4] = 1; // satır 1
        new[9] = 1; // satır 3
        assert_eq!(changed_rows(&old, &new, 3), Some((1, 4)));
        new[4] = 0;
        assert_eq!(changed_rows(&old, &new, 3), Some((3, 4)));
    }

    #[test]
    fn ioctl_numbers_match_the_c_macros() {
        // _IOW('F', 0x2E, struct mxcfb_update_data), _IOWR('F', 0x2F, marker)
        assert_eq!(
            nix::request_code_write!(b'F', 0x2E, size_of::<UpdateData>()) as u32,
            0x4048_462E
        );
        assert_eq!(
            nix::request_code_readwrite!(b'F', 0x2F, size_of::<UpdateMarker>()) as u32,
            0xC008_462F
        );
    }

    #[test]
    fn gray_pixels_are_packed_as_bgra_and_gray8() {
        // Libra 2: 32 bit, kırmızı 16, yeşil 8, mavi 0, saydamlık 24.
        let bgra = Layout {
            width: 3,
            height: 2,
            stride: 20,
            bits_per_pixel: 32,
            shifts: [16, 8, 0],
            alpha: Some(24),
        };
        let gray = [0u8, 0x80, 0xff, 1, 2, 3];
        let mut out = vec![0xaa; 40];
        bgra.pack(&gray, 3, &mut out);
        assert_eq!(
            &out[..12],
            &[
                0, 0, 0, 0xff, 0x80, 0x80, 0x80, 0xff, 0xff, 0xff, 0xff, 0xff
            ]
        );
        assert_eq!(&out[12..20], &[0xaa; 8], "satır sonu dolgusu değişmez");
        assert_eq!(&out[20..24], &[1, 1, 1, 0xff]);

        let g8 = Layout {
            bits_per_pixel: 8,
            stride: 4,
            alpha: None,
            ..bgra
        };
        let mut out = vec![9u8; 8];
        g8.pack(&gray, 3, &mut out);
        assert_eq!(out, [0, 0x80, 0xff, 9, 1, 2, 3, 9]);
    }
}
