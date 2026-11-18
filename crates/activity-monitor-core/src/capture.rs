//! Screen capture — port of CaptureManager: GDI BitBlt of the primary screen,
//! BGRA -> RGB8, JPEG at quality 75. Encoded bytes differ from System.Drawing
//! (different encoder); the gate is size band + visual check, never bytes.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use crate::config::CaptureSettings;

/// C# SemaphoreSlim(1,1) + WaitAsync(0): non-blocking try-lock.
/// tokio Mutex so the guard is Send and the future stays spawnable.
static CAPTURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const JPEG_QUALITY: u8 = 75; // hardcoded in C# CaptureScreen

/// C# Math.Min(MaxFramesPerCapture, MaxDurationSeconds * FrameRate)
fn max_frames(cfg: &CaptureSettings) -> i64 {
    cfg.max_frames_per_capture.min(cfg.max_duration_seconds * cfg.frame_rate)
}

/// Port of CaptureFramesAsync. None when the lock is held, on error, or when
/// zero frames were captured.
pub async fn capture_frames(cfg: &CaptureSettings, cancel: &AtomicBool) -> Option<Vec<Vec<u8>>> {
    let _guard = match CAPTURE_LOCK.try_lock() {
        Ok(g) => g,
        Err(_) => {
            tracing::warn!("Capture already in progress, skipping");
            return None;
        }
    };

    if cfg.frame_rate <= 0 {
        tracing::error!("Error during screen capture: FrameRate must be positive");
        return None;
    }
    let max = max_frames(cfg);
    let frame_interval = std::time::Duration::from_secs_f64(1.0 / cfg.frame_rate as f64);
    tracing::info!(max_frames = max, frame_rate = cfg.frame_rate, "Starting capture");

    let mut frames: Vec<Vec<u8>> = vec![];
    for i in 0..max {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match capture_screen(cfg) {
            Some(data) => {
                frames.push(data);
                tracing::debug!(frame = i + 1, max_frames = max, "Captured frame");
            }
            None => {
                tracing::error!(frame = i + 1, "Error capturing frame");
            }
        }
        if i < max - 1 {
            delay_or_cancel(frame_interval, cancel).await;
        }
    }

    tracing::info!(frame_count = frames.len(), "Capture completed");
    if frames.is_empty() { None } else { Some(frames) }
}

async fn delay_or_cancel(interval: std::time::Duration, cancel: &AtomicBool) {
    tokio::select! {
        _ = tokio::time::sleep(interval) => {}
        _ = async {
            while !cancel.load(Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        } => {}
    }
}

/// Port of CaptureScreen: primary screen (or the whole virtual screen when
/// CaptureAllMonitors is set) -> JPEG bytes, None on failure/no screen.
/// `CaptureBackend: "dxgi"` tries Desktop Duplication first and falls back to
/// GDI for that capture on any failure.
pub fn capture_screen(cfg: &CaptureSettings) -> Option<Vec<u8>> {
    match try_capture_screen(cfg) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "Error capturing screen");
            None
        }
    }
}

fn try_capture_screen(cfg: &CaptureSettings) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    if cfg.capture_backend.eq_ignore_ascii_case("dxgi") {
        match try_capture_screen_dxgi(cfg) {
            Err(e) => tracing::warn!(error = %e, "DXGI capture failed, falling back to GDI"),
            Ok(v) => return Ok(v),
        }
    }
    try_capture_screen_gdi(cfg)
}

fn try_capture_screen_dxgi(cfg: &CaptureSettings) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    let frame = crate::dxgi::capture_frame(!cfg.capture_all_monitors)?;
    let Some(f) = frame else { return Ok(None) };
    // Some display drivers (VMs, secure desktops) succeed the acquire but
    // hand back a uniform frame; analyzing black frames forever would be
    // silent data poison, so treat uniform as failure and let GDI take over.
    if crate::dxgi::is_uniform(&f.pixels) {
        tracing::warn!("DXGI frame is uniform (unsupported adapter?); falling back to GDI");
        return Err("DXGI: uniform frame".into());
    }
    tracing::debug!(width = f.width, height = f.height, "DXGI frame captured");
    encode_frame(&f.pixels, f.width, f.height).map(Some)
}

fn try_capture_screen_gdi(cfg: &CaptureSettings) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    unsafe {
        // C# Screen.PrimaryScreen.Bounds — primary display, origin (0,0).
        // CaptureAllMonitors: the virtual screen bounds every monitor, so one
        // BitBlt from the virtual origin grabs all screens in a single frame.
        let (w, h, src_x, src_y) = if cfg.capture_all_monitors {
            (
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
            )
        } else {
            (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN), 0, 0)
        };
        if w <= 0 || h <= 0 {
            return Ok(None); // C# bounds == null -> null frame (skipped, not an error)
        }

        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), src_x, src_y, SRCCOPY)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

        let mut bmi: BITMAPINFO = Default::default();
        bmi.bmiHeader.biSize = core::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // negative = top-down rows
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32; // BGRA
        bmi.bmiHeader.biCompression = BI_RGB.0;

        let mut pixels = vec![0u8; (w as usize) * 4 * (h as usize)];
        let lines = GetDIBits(
            mem_dc,
            bmp,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old);
        DeleteObject(HGDIOBJ(bmp.0));
        DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        if lines == 0 {
            return Err(std::io::Error::last_os_error().into());
        }

        encode_frame(&pixels, w, h).map(Some)
    }
}

/// Shared BGRA -> JPEG q75 encoder for both capture backends.
fn encode_frame(bgra: &[u8], w: i32, h: i32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let rgb = bgra_to_rgb(bgra);
    let mut out: Vec<u8> = Vec::new();
    // jpeg-encoder defaults to 4:2:0 chroma below quality 90, same as GDI+
    let encoder = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
    encoder.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)?;
    Ok(out)
}

/// BGRA (little-endian u32) -> RGB8 triplets, alpha dropped — matches the
/// report's expectation of RGB frames.
fn bgra_to_rgb(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() / 4 * 3);
    for px in pixels.chunks_exact(4) {
        out.push(px[2]); // R
        out.push(px[1]); // G
        out.push(px[0]); // B
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_math_matches_csharp() {
        let mk = |max_frames: i64, max_dur: i64, fps: i64| CaptureSettings {
            frame_rate: fps,
            max_duration_seconds: max_dur,
            trigger_on_focus_change: true,
            trigger_on_idle_resume: true,
            max_frames_per_capture: max_frames,
            ..Default::default()
        };
        assert_eq!(max_frames(&mk(30, 30, 1)), 30); // 30 vs 30*1
        assert_eq!(max_frames(&mk(1, 30, 1)), 1); // shipped appsettings: 1 frame
        assert_eq!(max_frames(&mk(100, 2, 5)), 10); // duration-limited: 2*5
    }

    #[test]
    fn bgra_conversion() {
        // BGRA bytes: blue=0x11, green=0x22, red=0x33, alpha=0xFF
        assert_eq!(bgra_to_rgb(&[0x11, 0x22, 0x33, 0xFF]), vec![0x33, 0x22, 0x11]);
    }

    #[test]
    fn live_dxgi_capture_produces_jpeg_or_falls_back() {
        // "dxgi" must always yield a frame: either the duplication path or the
        // automatic GDI fallback (RDP / 8bpp sessions cannot duplicate).
        let base = CaptureSettings::default();
        let primary = capture_screen(&base).expect("capture should succeed on a desktop session");
        assert_eq!(&primary[0..2], &[0xFF, 0xD8], "SOI marker");
        let primary_img = image::load_from_memory(&primary).expect("decode");
        let primary_dims = (primary_img.width(), primary_img.height());

        let dxgi_all = CaptureSettings {
            capture_backend: "dxgi".into(),
            capture_all_monitors: true,
            ..Default::default()
        };
        let frame = capture_screen(&dxgi_all).expect("dxgi capture (or fallback) should produce a frame");
        let all_img = image::load_from_memory(&frame).expect("decode");
        let dims = (all_img.width(), all_img.height());
        assert!(dims.0 >= primary_dims.0 && dims.1 >= primary_dims.1,
            "dxgi frame {dims:?} smaller than primary {primary_dims:?}");
    }

    #[tokio::test]
    async fn live_capture_produces_jpeg() {
        let frame = capture_screen(&CaptureSettings::default()).expect("screen capture should succeed on a desktop session");
        assert!(frame.len() > 1000, "jpeg too small: {}", frame.len());
        assert_eq!(&frame[0..2], &[0xFF, 0xD8], "SOI marker");
        assert_eq!(&frame[frame.len() - 2..], &[0xFF, 0xD9], "EOI marker");
        let primary_img = image::load_from_memory(&frame).expect("decode");
        let primary_dims = (primary_img.width(), primary_img.height());

        // All-monitors frame covers the virtual screen: never smaller than primary
        let all = CaptureSettings { capture_all_monitors: true, ..Default::default() };
        let frame_all = capture_screen(&all).expect("virtual-screen capture should succeed");
        let all_img = image::load_from_memory(&frame_all).expect("decode");
        let all_dims = (all_img.width(), all_img.height());
        assert!(all_dims.0 >= primary_dims.0 && all_dims.1 >= primary_dims.1,
            "virtual screen {all_dims:?} smaller than primary {primary_dims:?}");

        let cfg = CaptureSettings {
            frame_rate: 5,
            max_duration_seconds: 1,
            trigger_on_focus_change: true,
            trigger_on_idle_resume: true,
            max_frames_per_capture: 3,
            ..Default::default()
        };
        let _serial = crate::CAPTURE_TEST_LOCK.lock().unwrap();
        let cancel = AtomicBool::new(false);
        let frames = capture_frames(&cfg, &cancel).await.expect("frames");
        assert_eq!(frames.len(), 3); // min(3, 1*5) = 3
        // second call while "held" is not possible here (guard dropped), but lock path:
        let _g = CAPTURE_LOCK.try_lock().unwrap();
        assert!(capture_frames(&cfg, &cancel).await.is_none(), "lock held -> None");
        drop(_g);
    }
}
