//! Windows-only system calls: graphics adapter list (DXGI), GPU usage
//! (performance counters) and password encryption (Data Protection API).

#![cfg(windows)]

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE, IDXGIFactory1};
use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData};
use windows::Win32::System::Performance::*;
use windows::core::{PCWSTR, w};

/// (name, PCI vendor id, dedicated video memory in bytes)
pub fn dxgi_adapters() -> Vec<(String, u32, u64)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(f) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        let mut i = 0u32;
        while let Ok(a) = f.EnumAdapters1(i) {
            i += 1;
            if let Ok(d) = a.GetDesc1() {
                if (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                    continue;
                }
                let end = d.Description.iter().position(|c| *c == 0).unwrap_or(d.Description.len());
                let name = String::from_utf16_lossy(&d.Description[..end]);
                if name.contains("Microsoft Basic Render") {
                    continue;
                }
                out.push((name, d.VendorId, d.DedicatedVideoMemory as u64));
            }
            if i > 16 {
                break;
            }
        }
    }
    out
}

/// Total 3D-engine use across all processes, from Windows performance
/// counters. Works for NVIDIA, AMD and Intel.
pub struct GpuCounter {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

unsafe impl Send for GpuCounter {}

impl GpuCounter {
    pub fn new() -> Option<Self> {
        Self::engine(w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"))
    }

    /// The video encoder engine, on any GPU vendor.
    pub fn encoder() -> Option<Self> {
        Self::engine(w!("\\GPU Engine(*engtype_VideoEncode)\\Utilization Percentage"))
    }

    fn engine(path: PCWSTR) -> Option<Self> {
        unsafe {
            let mut q = PDH_HQUERY::default();
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut q) != 0 {
                return None;
            }
            let mut c = PDH_HCOUNTER::default();
            if PdhAddEnglishCounterW(q, path, 0, &mut c) != 0 {
                let _ = PdhCloseQuery(q);
                return None;
            }
            let _ = PdhCollectQueryData(q);
            Some(Self { query: q, counter: c })
        }
    }

    pub fn sample(&mut self) -> Option<f32> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let _ = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size == 0 {
                return None;
            }
            let n = (size as usize) / std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() + 1;
            let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = vec![std::mem::zeroed(); n];
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(buf.as_mut_ptr())) != 0 {
                return None;
            }
            let mut total = 0.0f64;
            for item in buf.iter().take(count as usize) {
                total += item.FmtValue.Anonymous.doubleValue;
            }
            Some(total.min(100.0) as f32)
        }
    }
}

impl Drop for GpuCounter {
    fn drop(&mut self) {
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}

pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, w!("Stream Preflight"), None, None, None, 0, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
        Some(v)
    }
}

pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    unsafe {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, 0, &mut out).ok()?;
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut _)));
        Some(v)
    }
}

/// (name, width, height, current Hz, highest Hz at that resolution, primary)
pub fn monitors() -> Vec<(String, u32, u32, u32, u32, bool)> {
    use windows::Win32::Graphics::Gdi::*;
    let mut out = Vec::new();
    unsafe {
        let mut i = 0u32;
        loop {
            let mut dd = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
            if !EnumDisplayDevicesW(PCWSTR::null(), i, &mut dd, 0).as_bool() {
                break;
            }
            i += 1;
            if (dd.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP).0 == 0 {
                continue;
            }
            let dev = PCWSTR(dd.DeviceName.as_ptr());
            let mut cur = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
            if !EnumDisplaySettingsW(dev, ENUM_CURRENT_SETTINGS, &mut cur).as_bool() {
                continue;
            }
            let (w, h, hz) = (cur.dmPelsWidth, cur.dmPelsHeight, cur.dmDisplayFrequency);
            let mut max = hz;
            let mut m = 0u32;
            loop {
                let mut dm = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
                if !EnumDisplaySettingsW(dev, ENUM_DISPLAY_SETTINGS_MODE(m), &mut dm).as_bool() {
                    break;
                }
                if dm.dmPelsWidth == w && dm.dmPelsHeight == h {
                    max = max.max(dm.dmDisplayFrequency);
                }
                m += 1;
                if m > 4000 {
                    break;
                }
            }
            // Second call gives the monitor's own name.
            let mut mon = DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
            let name = if EnumDisplayDevicesW(dev, 0, &mut mon, 0).as_bool() {
                let end = mon.DeviceString.iter().position(|c| *c == 0).unwrap_or(mon.DeviceString.len());
                String::from_utf16_lossy(&mon.DeviceString[..end])
            } else {
                "Monitor".to_string()
            };
            let primary = (dd.StateFlags & DISPLAY_DEVICE_PRIMARY_DEVICE).0 != 0;
            out.push((name, w, h, hz, max, primary));
        }
    }
    out
}

/// Steam's install folder from the registry.
pub fn steam_path() -> Option<std::path::PathBuf> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Valve\\Steam").ok()?;
    let p: String = k.get_value("SteamPath").ok()?;
    Some(std::path::PathBuf::from(p.replace('/', "\\")))
}
