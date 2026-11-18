//! DXGI Desktop Duplication capture backend (v1.2 roadmap).
//!
//! Like GDI `BitBlt`, this path is **silent**: no consent picker, no capture
//! border, no notification — only Windows Graphics Capture draws those, which
//! is why WGC stays rejected. DXGI adds GPU-side frames with dirty-rectangle
//! metadata; this backend takes plain one-shot frames (no dirty tracking yet).
//!
//! Any failure (no adapter, RDP/8bpp session, timeout, access lost) returns an
//! error and the caller falls back to GDI for that capture.
//!
//! shortcut: the mouse cursor is not composited into the frame (DXGI reports
//! the pointer shape/position separately; AI analysis does not need it).

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    IDXGIFactory1,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::core::Interface;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

/// One composed desktop frame, top-down BGRA.
pub struct DxgiFrame {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

/// One output's frame, tightly cropped, plus its offset in the virtual screen.
struct OutputFrame {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

/// Virtual-screen geometry `(w, h, origin_x, origin_y)`.
pub fn virtual_bounds() -> (i32, i32, i32, i32) {
    unsafe {
        (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    }
}

/// `primary_only` captures just the output containing (0,0) (the primary
/// display, whose top-left is the virtual-screen origin); otherwise every
/// attached output, composited into the virtual screen. `Ok(None)` only when
/// the desktop has no size at all.
pub fn capture_frame(primary_only: bool) -> Result<Option<DxgiFrame>, Box<dyn std::error::Error>> {
    let (vw, vh, ox, oy) = virtual_bounds();
    let (w, h, ox, oy) = if primary_only {
        (
            unsafe { GetSystemMetrics(SM_CXSCREEN) },
            unsafe { GetSystemMetrics(SM_CYSCREEN) },
            0,
            0,
        )
    } else {
        (vw, vh, ox, oy)
    };
    if w <= 0 || h <= 0 {
        return Ok(None);
    }

    let mut pixels = vec![0u8; (w as usize) * 4 * (h as usize)];
    let mut captured_any = false;

    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1()? };
    let mut adapter_index = 0u32;
    loop {
        let adapter: IDXGIAdapter1 = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(a) => a,
            Err(_) => break, // no more adapters
        };
        adapter_index += 1;

        // One device per adapter; each output's texture lives on its own.
        let mut device_opt: Option<ID3D11Device> = None;
        let mut context_opt: Option<ID3D11DeviceContext> = None;
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device_opt),
                None,
                Some(&mut context_opt),
            )?
        };
        let (Some(device), Some(context)) = (device_opt, context_opt) else {
            continue;
        };

        let mut output_index = 0u32;
        loop {
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(o) => o,
                Err(_) => break,
            };
            output_index += 1;
            let desc = unsafe { output.GetDesc()? };
            if !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            let rect = unsafe { desc.DesktopCoordinates };
            if primary_only && !(rect.left <= 0 && rect.top <= 0 && rect.right > 0 && rect.bottom > 0)
            {
                continue;
            }
            let output1: IDXGIOutput1 = output.cast()?;
            match duplicate_one(&device, &context, &output1, &rect) {
                Ok(Some(frame)) => {
                    blit_into(&mut pixels, w, h, ox, oy, &frame);
                    captured_any = true;
                }
                Ok(None) => return Err("DXGI: output produced no frame".into()),
                // Not duplicable (RDP, 8bpp, access lost): leave black —
                // same as a sleeping display; the frame still composes.
                Err(_) => continue,
            }
        }
    }

    if !captured_any {
        return Err("DXGI: no attached output produced a frame".into());
    }
    Ok(Some(DxgiFrame { width: w, height: h, pixels }))
}

/// Acquire exactly one frame from this output and stage it to CPU memory.
/// Returns Ok(None) on a plain timeout (nothing changed to present).
fn duplicate_one(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    output: &IDXGIOutput1,
    rect: &windows::Win32::Foundation::RECT,
) -> Result<Option<OutputFrame>, Box<dyn std::error::Error>> {
    let dup: IDXGIOutputDuplication = unsafe { output.DuplicateOutput(device)? };
    let mut info = Default::default();
    let mut resource: Option<IDXGIResource> = None;
    let acquired = unsafe { dup.AcquireNextFrame(500, &mut info, &mut resource) };
    // DXGI only guarantees the desktop resource stays valid until
    // ReleaseFrame, so staging + copying must happen while it is acquired.
    let result = stage_frame(device, context, &acquired, resource.as_ref(), rect);
    // ReleaseFrame must run once per successful acquire, before the next one.
    let _ = unsafe { dup.ReleaseFrame() };
    result
}

fn stage_frame(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    acquired: &windows::core::Result<()>,
    resource: Option<&IDXGIResource>,
    rect: &windows::Win32::Foundation::RECT,
) -> Result<Option<OutputFrame>, Box<dyn std::error::Error>> {
    acquired
        .as_ref()
        .map_err(|e| Box::new(e.clone()) as Box<dyn std::error::Error>)?;
    let Some(resource) = resource else { return Ok(None) };
    let texture = resource.cast::<ID3D11Texture2D>()?;

    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: desc.Width,
        Height: desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: 0,
    };
    let mut staging_opt: Option<ID3D11Texture2D> = None;
    unsafe { device.CreateTexture2D(&staging_desc, None, Some(&mut staging_opt)) };
    let Some(staging) = staging_opt else {
        return Err("DXGI: CreateTexture2D returned no texture".into());
    };
    unsafe { context.CopyResource(&staging, &texture) };

    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))? };
    let row_bytes = (desc.Width as usize) * 4;
    let mut pixels = vec![0u8; row_bytes * desc.Height as usize];
    for row in 0..desc.Height as usize {
        unsafe {
            std::ptr::copy_nonoverlapping(
                (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                pixels.as_mut_ptr().add(row * row_bytes),
                row_bytes,
            );
        }
    }
    unsafe { context.Unmap(&staging, 0) };

    Ok(Some(OutputFrame {
        x: rect.left,
        y: rect.top,
        width: desc.Width as i32,
        height: desc.Height as i32,
        pixels,
    }))
}

/// Copy one output frame into the composed virtual-screen buffer.
/// True when every sampled RGB value is identical — the signature of a
/// driver that "supports" duplication but delivers a blank surface.
pub fn is_uniform(bgra: &[u8]) -> bool {
    if bgra.len() < 4 {
        return true;
    }
    let first = &bgra[0..3];
    let step = (bgra.len() / 4 / 1024).max(1);
    bgra
        .chunks_exact(4)
        .step_by(step)
        .all(|p| &p[0..3] == first)
}

fn blit_into(dst: &mut [u8], dst_w: i32, dst_h: i32, ox: i32, oy: i32, frame: &OutputFrame) {
    let x = frame.x - ox;
    let y = frame.y - oy;
    if x < 0 || y < 0 || x + frame.width > dst_w || y + frame.height > dst_h {
        return; // output outside the virtual screen; skip
    }
    let row_bytes = (frame.width as usize) * 4;
    for row in 0..frame.height as usize {
        let src = row * row_bytes;
        let dst_start = ((y as usize + row) * dst_w as usize + x as usize) * 4;
        dst[dst_start..dst_start + row_bytes].copy_from_slice(&frame.pixels[src..src + row_bytes]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: i32, y: i32, w: i32, h: i32, v: u8) -> OutputFrame {
        OutputFrame {
            x,
            y,
            width: w,
            height: h,
            pixels: vec![v; (w * h * 4) as usize],
        }
    }

    #[test]
    fn blit_places_single_monitor_at_origin() {
        let mut dst = vec![0u8; 8 * 8 * 4];
        blit_into(&mut dst, 8, 8, 0, 0, &frame(0, 0, 8, 8, 0xAB));
        assert_eq!(dst[0], 0xAB);
        assert_eq!(dst[8 * 4 * 7 + 7 * 4 + 3], 0xAB); // bottom-right pixel
    }

    #[test]
    fn blit_places_second_monitor_right_of_first() {
        let mut dst = vec![0u8; 16 * 4 * 4];
        blit_into(&mut dst, 16, 4, 0, 0, &frame(0, 0, 8, 4, 0x11));
        blit_into(&mut dst, 16, 4, 0, 0, &frame(8, 0, 8, 4, 0x22));
        assert_eq!(dst[0], 0x11); // left monitor
        assert_eq!(dst[8 * 4], 0x22); // right monitor starts at x=8
        assert_eq!(dst[16 * 4 - 1], 0x22); // right monitor reaches the end
    }

    #[test]
    fn blit_skips_frames_outside_virtual_screen() {
        let mut dst = vec![0u8; 8 * 4 * 4];
        blit_into(&mut dst, 8, 4, 0, 0, &frame(8, 0, 4, 4, 0xFF));
        assert!(dst.iter().all(|b| *b == 0), "out-of-bounds blit must not write");
    }

    #[test]
    fn blit_handles_negative_origin() {
        // virtual screen origin at (-8, 0); monitor at (-8,0) maps to x=0
        let mut dst = vec![0u8; 8 * 4 * 4];
        blit_into(&mut dst, 8, 4, -8, 0, &frame(-8, 0, 8, 4, 0x77));
        assert_eq!(dst[0], 0x77);
    }

    #[test]
    fn is_uniform_detects_black_and_content() {
        // all-zero frame (VM driver symptom)
        assert!(is_uniform(&vec![0u8; 64 * 64 * 4]));
        // one differing pixel sampled?
        let mut px = vec![0u8; 64 * 64 * 4];
        for c in px.iter_mut() {
            *c = 0x20;
        }
        assert!(is_uniform(&px), "uniform non-black frame is still uniform");
        px[32 * 4] = 0x21; // sampled: step = 4096/1024 = 4 -> pixel 32 is a multiple of 4 -> sampled
        assert!(!is_uniform(&px));
    }

    #[test]
    fn virtual_bounds_is_sane() {
        let (w, h, ox, oy) = virtual_bounds();
        assert!(w > 0 && h > 0, "virtual screen must exist on a desktop session");
        assert!(oy >= 0, "virtual origin y is not negative on this box: {oy}");
    }
}