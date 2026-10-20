//! Checks saved window positions before restoring them.
//!
//! eframe already places the window on-screen. The session can still refer
//! to a monitor that was unplugged, so check before moving the window there.

/// Why a Wayland window cannot be kept above others from the app, and what
/// to do instead.
pub fn on_top_unavailable(locale: crate::i18n::Locale) -> std::borrow::Cow<'static, str> {
    use crate::i18n::gettext;
    gettext(
        locale,
        "On Wayland, use your desktop's Keep Above shortcut or window rule.",
    )
}

/// The message box shown when eframe could not create an OpenGL context on
/// Windows, and the page its Yes button opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingOpenGl {
    pub title: String,
    pub text: String,
    pub page: &'static str,
}

/// The Store page of the OpenCL, OpenGL & Vulkan Compatibility Pack.
const COMPATIBILITY_PACK_PAGE: &str = "ms-windows-store://pdp/?ProductId=9NQPSL29BFFF";
/// The download guide, which documents the OpenGL 2.0 requirement.
const DOWNLOAD_PAGE: &str = "https://spotifast.rocks/download/";

/// Says why no window opened and where the fix is.
///
/// Snapdragon graphics drivers have no OpenGL of their own. Windows provides
/// it on ARM through Microsoft's compatibility pack, a Store app that
/// translates it to Direct3D 12, so ARM points at that pack. Elsewhere the
/// usual cause is a virtual machine or remote session with only the
/// Microsoft Basic Display Adapter, which offers OpenGL 1.1.
pub fn missing_opengl_message(locale: crate::i18n::Locale, arm: bool) -> MissingOpenGl {
    use crate::i18n::gettext;
    let title = gettext(locale, "Spotifast cannot open its window").into_owned();
    if arm {
        return MissingOpenGl {
            title,
            text: gettext(
                locale,
                "On Windows on ARM, the OpenGL that Spotifast draws with comes from Microsoft's OpenCL, OpenGL & Vulkan Compatibility Pack. Open its Microsoft Store page to install or repair it?",
            )
            .into_owned(),
            page: COMPATIBILITY_PACK_PAGE,
        };
    }
    let text = gettext(
        locale,
        // Translators: Keep {url} exactly as written. It becomes the address
        // of the Spotifast download page.
        "Spotifast needs a graphics driver with OpenGL 2.0 or newer, and this PC or virtual machine does not provide one. Install the graphics driver, or turn on GPU acceleration for the virtual machine. Open {url} to read more?",
    );
    MissingOpenGl {
        title,
        text: text.replace("{url}", DOWNLOAD_PAGE),
        page: DOWNLOAD_PAGE,
    }
}

/// Shows [`missing_opengl_message`] and opens its page on Yes, so a missing
/// OpenGL does not close the app without a word.
#[cfg(windows)]
pub fn report_missing_opengl(locale: crate::i18n::Locale) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONERROR, MB_YESNO, MessageBoxW};

    let message = missing_opengl_message(locale, cfg!(target_arch = "aarch64"));
    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let title = wide(&message.title);
    let text = wide(&message.text);
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call,
    // and the box has no owner window.
    let answer = unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONERROR,
        )
    };
    if answer == IDYES
        && let Err(error) = open::that(message.page)
    {
        log::warn!("cannot open {}: {error}", message.page);
    }
}

/// Minimize the active window without leaving egui's macOS viewport flag stale
/// when the user later restores it from the Dock.
pub fn minimize_window(ctx: &egui::Context) {
    #[cfg(target_os = "macos")]
    {
        use objc2::{MainThreadMarker, sel};
        use objc2_app_kit::NSApplication;
        use objc2_foundation::NSObjectNSDelayedPerforming;

        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let window = app.currentEvent().and_then(|event| event.window(mtm));
        if let Some(window) = window.or_else(|| app.keyWindow()) {
            // Let egui finish this frame before AppKit minimizes the window.
            // SAFETY: NSWindow's selector takes one optional sender argument.
            unsafe {
                window.performSelector_withObject_afterDelay(sel!(miniaturize:), None, 0.0);
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    #[cfg(target_os = "macos")]
    let _ = ctx;
}

/// The active backend matters: a Wayland session can also host X11 windows.
/// winit's Wayland backend cannot change a window's stacking level.
pub fn supports_window_level(display: raw_window_handle::RawDisplayHandle) -> bool {
    !matches!(display, raw_window_handle::RawDisplayHandle::Wayland(_))
}

/// Whether the mini player can keep its window without a taskbar entry.
/// Windows and X11 can; Wayland has no protocol for it, and macOS has the
/// Dock instead of a taskbar.
pub fn supports_hiding_from_taskbar(display: raw_window_handle::RawDisplayHandle) -> bool {
    use raw_window_handle::RawDisplayHandle;
    matches!(
        display,
        RawDisplayHandle::Windows(_) | RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_)
    )
}

/// Keeps an X11 window out of the taskbar with `_NET_WM_STATE_SKIP_TASKBAR`.
///
/// winit can only do this on Windows, so this asks the window manager
/// directly. A window that is not mapped yet carries the state in its
/// property, which the window manager reads when it maps the window; a
/// mapped one asks the window manager, as EWMH requires. Any other handle is
/// left alone.
#[cfg(target_os = "linux")]
pub fn skip_x11_taskbar(window: raw_window_handle::RawWindowHandle) {
    use raw_window_handle::RawWindowHandle;
    let window = match window {
        RawWindowHandle::Xlib(handle) => match u32::try_from(handle.window) {
            Ok(window) => window,
            Err(_) => return,
        },
        RawWindowHandle::Xcb(handle) => handle.window.get(),
        _ => return,
    };
    if let Err(error) = x11::skip_taskbar(window) {
        log::warn!("could not hide the mini player from the taskbar: {error}");
    }
}

#[cfg(target_os = "linux")]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, MapState, PropMode,
    };
    use x11rb::wrapper::ConnectionExt as _;

    /// `_NET_WM_STATE_ADD` in a `_NET_WM_STATE` client message.
    const ADD: u32 = 1;
    /// The request comes from an ordinary application.
    const FROM_APPLICATION: u32 = 1;

    pub(super) fn skip_taskbar(window: u32) -> Result<(), Box<dyn std::error::Error>> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;
        let state = connection
            .intern_atom(false, b"_NET_WM_STATE")?
            .reply()?
            .atom;
        let skip = connection
            .intern_atom(false, b"_NET_WM_STATE_SKIP_TASKBAR")?
            .reply()?
            .atom;
        let mapped =
            connection.get_window_attributes(window)?.reply()?.map_state != MapState::UNMAPPED;
        if mapped {
            let event = ClientMessageEvent::new(32, window, state, message_data(skip));
            connection.send_event(
                false,
                root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?;
        } else {
            connection.change_property32(
                PropMode::APPEND,
                window,
                state,
                AtomEnum::ATOM,
                &[skip],
            )?;
        }
        connection.flush()?;
        Ok(())
    }

    /// The `_NET_WM_STATE` request that adds one state.
    pub(super) fn message_data(atom: u32) -> [u32; 5] {
        [ADD, atom, 0, FROM_APPLICATION, 0]
    }
}

/// The main window's smallest size with no side panel open. The page and
/// its top bar fit beside the narrowest sidebar from here.
pub const MAIN_MIN_SIZE: [f32; 2] = [760.0, 520.0];

/// A remembered main window size to open at, or `None` for the default. Under
/// the minimum is not a size anyone chose, since the window cannot be dragged
/// that small, and opening at it leaves a window too small to find (#704).
pub fn usable_size(size: Option<[f32; 2]>) -> Option<[f32; 2]> {
    size.filter(|size| size[0] >= MAIN_MIN_SIZE[0] && size[1] >= MAIN_MIN_SIZE[1])
}

/// The main window's default size when no saved geometry is available.
pub const DEFAULT_MAIN_SIZE: [f32; 2] = [1800.0, 1000.0];

static FIXED_SIZE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the main window keeps one size it was given, as demo shots ask,
/// so the interface must not raise its minimum.
pub fn fixed_size() -> bool {
    FIXED_SIZE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Pins the main window to the size it is created with.
pub fn set_fixed_size(on: bool) {
    FIXED_SIZE.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The outer window rectangle Windows gives a maximized window on a monitor
/// whose work area is `work` `[left, top, right, bottom]` and whose sizing
/// frame adds `border` `[left, top, right, bottom]` around an empty client
/// rectangle.
///
/// `AdjustWindowRectExForDpi` puts the caption in `border[1]` (`top`), while
/// a maximized window hangs its sizing border outside `work` on all four
/// sides and keeps the caption inside `work`. The top sizing border has the
/// same thickness as the bottom one (`border[3]`).
#[cfg(any(windows, test))]
fn maximized_outer_rect(work: [i32; 4], border: [i32; 4]) -> [i32; 4] {
    [
        work[0] + border[0],
        work[1] - border[3],
        work[2] + border[2],
        work[3] + border[3],
    ]
}

/// Maximizes a hidden Win32 window without showing it.
///
/// `ShowWindow(SW_MAXIMIZE)` activates and displays the window even when
/// `winit` created it hidden (`with_visible(false)`), and `winit` then hides
/// it again with `ShowWindow(SW_HIDE)`. Calling that before the OpenGL
/// context exists flashes an unpainted white window on screen. Setting
/// `WS_MAXIMIZE` and moving the hidden window to the monitor's maximized
/// rectangle sends `WM_NCCALCSIZE` and `WM_SIZE` (`SIZE_MAXIMIZED`) so
/// `winit` and `egui` see the maximized geometry on the first frame, while
/// the window stays hidden until `eframe` reveals it after painting.
///
/// # Safety
/// `hwnd` must be a valid window handle owned by the calling thread, or `0`.
#[cfg(windows)]
pub unsafe fn maximize_hidden(hwnd: isize) {
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    use windows_sys::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongW, GetWindowPlacement, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongW, SetWindowPos, WINDOWPLACEMENT, WS_MAXIMIZE,
    };

    if hwnd == 0 {
        return;
    }
    let hwnd = hwnd as HWND;
    // Ask Windows for the placement before setting `WS_MAXIMIZE`, so its
    // internal checkpoint records the normal restore rectangle that is on
    // the window now rather than the maximized rectangle set below.
    let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
    placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(hwnd, &mut placement) } == 0 {
        return;
    }
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return;
    }
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return;
    }
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let mut border = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if dpi != 0 {
        unsafe { AdjustWindowRectExForDpi(&mut border, style, 0, ex_style, dpi) };
    }
    let work = info.rcWork;
    let [left, top, right, bottom] = maximized_outer_rect(
        [work.left, work.top, work.right, work.bottom],
        [border.left, border.top, border.right, border.bottom],
    );
    unsafe {
        SetWindowLongW(hwnd, GWL_STYLE, (style | WS_MAXIMIZE) as i32);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            left,
            top,
            right - left,
            bottom - top,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_opengl_points_arm_at_the_store_pack_and_others_at_the_guide() {
        let arm = missing_opengl_message(crate::i18n::Locale::English, true);
        assert_eq!(arm.page, COMPATIBILITY_PACK_PAGE);
        assert!(arm.text.contains("Compatibility Pack"));

        let x64 = missing_opengl_message(crate::i18n::Locale::English, false);
        assert_eq!(x64.title, "Spotifast cannot open its window");
        assert_eq!(x64.page, "https://spotifast.rocks/download/");
        assert!(x64.text.contains("OpenGL 2.0"));
        assert!(x64.text.contains("virtual machine"));
        assert!(x64.text.contains("https://spotifast.rocks/download/"));
        assert!(!x64.text.contains("{url}"));
    }

    #[test]
    fn only_the_wayland_backend_lacks_window_level_control() {
        use raw_window_handle::{
            RawDisplayHandle, WaylandDisplayHandle, XcbDisplayHandle, XlibDisplayHandle,
        };
        let wayland = WaylandDisplayHandle::new(std::ptr::NonNull::dangling());
        assert!(!supports_window_level(RawDisplayHandle::Wayland(wayland)));
        assert!(supports_window_level(RawDisplayHandle::Xlib(
            XlibDisplayHandle::new(None, 0)
        )));
        assert!(supports_window_level(RawDisplayHandle::Xcb(
            XcbDisplayHandle::new(None, 0)
        )));
    }

    #[test]
    fn only_windows_and_x11_can_hide_the_mini_player_from_the_taskbar() {
        use raw_window_handle::{
            AppKitDisplayHandle, RawDisplayHandle, WaylandDisplayHandle, WindowsDisplayHandle,
            XcbDisplayHandle, XlibDisplayHandle,
        };
        let wayland = WaylandDisplayHandle::new(std::ptr::NonNull::dangling());
        assert!(!supports_hiding_from_taskbar(RawDisplayHandle::Wayland(
            wayland
        )));
        assert!(!supports_hiding_from_taskbar(RawDisplayHandle::AppKit(
            AppKitDisplayHandle::new()
        )));
        assert!(supports_hiding_from_taskbar(RawDisplayHandle::Windows(
            WindowsDisplayHandle::new()
        )));
        assert!(supports_hiding_from_taskbar(RawDisplayHandle::Xlib(
            XlibDisplayHandle::new(None, 0)
        )));
        assert!(supports_hiding_from_taskbar(RawDisplayHandle::Xcb(
            XcbDisplayHandle::new(None, 0)
        )));
    }

    /// EWMH's layout: add, the state, no second state, from an application.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_skip_taskbar_request_adds_one_state_as_an_application() {
        assert_eq!(x11::message_data(42), [1, 42, 0, 1, 0]);
    }

    /// Needs an X server (`DISPLAY`), so it is not part of the ordinary
    /// suite. The window is never mapped, so nothing appears on screen.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "needs an X server"]
    fn an_unmapped_x11_window_keeps_its_states_and_skips_the_taskbar() {
        use x11rb::connection::Connection;
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, WindowClass};
        use x11rb::wrapper::ConnectionExt as _;

        let (connection, screen) = x11rb::connect(None).expect("an X server");
        let root = connection.setup().roots[screen].root;
        let window = connection.generate_id().unwrap();
        connection
            .create_window(
                0,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .unwrap();
        let atom = |name: &[u8]| {
            connection
                .intern_atom(false, name)
                .unwrap()
                .reply()
                .unwrap()
                .atom
        };
        let state = atom(b"_NET_WM_STATE");
        let above = atom(b"_NET_WM_STATE_ABOVE");
        let skip = atom(b"_NET_WM_STATE_SKIP_TASKBAR");
        connection
            .change_property32(
                x11rb::protocol::xproto::PropMode::REPLACE,
                window,
                state,
                AtomEnum::ATOM,
                &[above],
            )
            .unwrap();
        connection.flush().unwrap();

        x11::skip_taskbar(window).unwrap();

        let states: Vec<u32> = connection
            .get_property(false, window, state, AtomEnum::ATOM, 0, 16)
            .unwrap()
            .reply()
            .unwrap()
            .value32()
            .unwrap()
            .collect();
        connection.destroy_window(window).unwrap();
        connection.flush().unwrap();
        assert_eq!(states, vec![above, skip]);
    }

    /// A remembered size under the minimum is not one anyone chose, since the
    /// window cannot be dragged that small. It is not restored, so the window
    /// opens at its default size instead.
    #[test]
    fn a_remembered_size_below_the_minimum_is_not_restored() {
        assert_eq!(usable_size(Some([64.0, 64.0])), None);
        assert_eq!(usable_size(Some([MAIN_MIN_SIZE[0] - 1.0, 800.0])), None);
        assert_eq!(usable_size(Some([1240.0, MAIN_MIN_SIZE[1] - 1.0])), None);
        assert_eq!(usable_size(None), None);
        for size in [MAIN_MIN_SIZE, [1240.0, 800.0], [1920.0, 1129.0]] {
            assert_eq!(usable_size(Some(size)), Some(size));
        }
    }

    #[test]
    fn maximized_outer_rect_hangs_the_sizing_frame_outside_the_work_area() {
        assert_eq!(
            maximized_outer_rect([0, 0, 2560, 1392], [-8, -31, 8, 8]),
            [-8, -8, 2568, 1400]
        );
        assert_eq!(
            maximized_outer_rect([100, 40, 2020, 1080], [0, 0, 0, 0]),
            [100, 40, 2020, 1080]
        );
    }

    #[cfg(windows)]
    #[test]
    fn maximize_hidden_maximizes_without_showing_or_losing_the_restore_rect() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetWindowPlacement, IsWindowVisible, SW_SHOWMAXIMIZED,
            WINDOWPLACEMENT, WS_OVERLAPPEDWINDOW,
        };

        unsafe { maximize_hidden(0) };

        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPEDWINDOW,
                120,
                140,
                900,
                600,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        assert!(!hwnd.is_null());

        unsafe { maximize_hidden(hwnd as isize) };

        let visible = unsafe { IsWindowVisible(hwnd) };
        let mut placement: WINDOWPLACEMENT = unsafe { std::mem::zeroed() };
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let ok = unsafe { GetWindowPlacement(hwnd, &mut placement) };
        unsafe { DestroyWindow(hwnd) };

        assert_eq!(visible, 0, "hidden window must never be shown");
        assert_ne!(ok, 0);
        assert_eq!(placement.showCmd, SW_SHOWMAXIMIZED as u32);
        assert_eq!(
            [
                placement.rcNormalPosition.left,
                placement.rcNormalPosition.top,
                placement.rcNormalPosition.right,
                placement.rcNormalPosition.bottom,
            ],
            [120, 140, 1020, 740]
        );
    }
}
