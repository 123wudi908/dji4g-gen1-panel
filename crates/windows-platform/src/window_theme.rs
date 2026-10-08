//! Best-effort theme for the existing Windows non-client frame. No custom decorations.

/// Set the native titlebar's dark-mode preference, preserving Windows caption rendering.
///
/// Unsupported DWM attributes (including older Windows versions) return `false`; callers
/// must keep running. Attribute 19 is a best-effort fallback for early Windows 10 builds,
/// before the current SDK's attribute 20 was used. Neither path changes caption colours,
/// icons, window styles or the system-wide theme.
#[must_use]
pub fn set_titlebar_dark_mode(hwnd: isize, dark: bool) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::HWND,
            Graphics::{
                Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
                Gdi::{RDW_FRAME, RDW_INVALIDATE, RDW_UPDATENOW, RedrawWindow},
            },
        };
        if hwnd == 0 {
            return false;
        }
        let value = i32::from(dark); // Win32 BOOL is four bytes, not Rust bool.
        for attribute in [DWMWA_USE_IMMERSIVE_DARK_MODE as u32, 19] {
            // SAFETY: DWM validates the HWND; value is a live, correctly sized BOOL.
            let status = unsafe {
                DwmSetWindowAttribute(
                    hwnd as HWND,
                    attribute,
                    (&value as *const i32).cast(),
                    std::mem::size_of_val(&value) as u32,
                )
            };
            if status >= 0 {
                // Refresh the non-client frame immediately, including while inactive. These
                // flags neither activate the window nor change its minimized/maximized state.
                // SAFETY: null region/rect requests repaint of this existing window.
                unsafe {
                    RedrawWindow(
                        hwnd as HWND,
                        std::ptr::null(),
                        std::ptr::null_mut(),
                        RDW_FRAME | RDW_INVALIDATE | RDW_UPDATENOW,
                    );
                }
                return true;
            }
        }
        false
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, dark);
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn absent_window_is_a_safe_noop() {
        assert!(!super::set_titlebar_dark_mode(0, true));
        assert!(!super::set_titlebar_dark_mode(0, false));
    }

    #[cfg(windows)]
    #[test]
    fn invalid_window_does_not_fail_startup() {
        assert!(!super::set_titlebar_dark_mode(-1, true));
        assert!(!super::set_titlebar_dark_mode(-1, false));
    }
}
