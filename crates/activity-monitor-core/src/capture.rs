//! Screen capture — port of CaptureManager: GDI BitBlt of the primary screen,
//! BGRA -> RGB8, JPEG at quality 75. Encoded bytes differ from System.Drawing
//! (different encoder); the gate is size band + visual check, never bytes.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

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
        match capture_screen() {
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

/// Port of CaptureScreen: primary screen -> JPEG bytes, None on failure/no screen.
pub fn capture_screen() -> Option<Vec<u8>> {
    match try_capture_screen() {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(error = %e, "Error capturing screen");
            None
        }
    }
}

fn try_capture_screen() -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
    unsafe {
        // C# Screen.PrimaryScreen.Bounds — primary display, origin (0,0)
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w <= 0 || h <= 0 {
            return Ok(None); // C# bounds == null -> null frame (skipped, not an error)
        }

        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));
        BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), 0, 0, SRCCOPY).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

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

        let rgb = bgra_to_rgb(&pixels);
        let mut out: Vec<u8> = Vec::new();
        // jpeg-encoder defaults to 4:2:0 chroma below quality 90, same as GDI+
        let encoder = jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY);
        encoder.encode(&rgb, w as u16, h as u16, jpeg_encoder::ColorType::Rgb)?;
        Ok(Some(out))
    }
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

    #[tokio::test]
    async fn live_capture_produces_jpeg() {
        let frame = capture_screen().expect("screen capture should succeed on a desktop session");
        assert!(frame.len() > 1000, "jpeg too small: {}", frame.len());
        assert_eq!(&frame[0..2], &[0xFF, 0xD8], "SOI marker");
        assert_eq!(&frame[frame.len() - 2..], &[0xFF, 0xD9], "EOI marker");

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
