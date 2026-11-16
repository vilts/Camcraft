//! The window and taskbar icon (#1459): the maintainer's logotype, generated to
//! `assets/logo/icons/` by `cargo xtask logo`, decoded here for winit.
//! Provenance: our own artwork; winit's `Icon::from_rgba` docs and its Windows
//! extension `WindowAttributesExtWindows::with_taskbar_icon` (#1754).

use winit::window::{Icon, WindowAttributes};

/// The 128 px render of the logotype, embedded at build time.
const PNG_128: &[u8] = include_bytes!("../../../../assets/logo/icons/camcraft-128.png");

/// The 256 px render, for the Windows taskbar button (`ICON_BIG`, 256 px is winit's ceiling).
#[cfg(windows)]
const PNG_256: &[u8] = include_bytes!("../../../../assets/logo/icons/camcraft-256.png");

/// The 128 px icon's pixels as RGBA8 with its width and height, or why it does not decode.
pub fn decode() -> Result<(Vec<u8>, u32, u32), String> {
    decode_png(PNG_128)
}

fn decode_png(png: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .map_err(|e| format!("logotype icon: {e}"))?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Ok((img.into_raw(), w, h))
}

/// The winit window icon; `None` (and a log line) when it cannot be made: the window opens without one.
pub fn window_icon() -> Option<Icon> {
    let made = decode().and_then(|(rgba, w, h)| Icon::from_rgba(rgba, w, h).map_err(|e| format!("logotype icon: {e}")));
    match made {
        Ok(i) => Some(i),
        Err(e) => {
            diag_warn!("{e} - the window opens without an icon");
            None
        }
    }
}

/// The 256 px taskbar icon; `None` (and a log line) when it cannot be made.
#[cfg(windows)]
pub fn taskbar_icon() -> Option<Icon> {
    let made = decode_png(PNG_256).and_then(|(rgba, w, h)| Icon::from_rgba(rgba, w, h).map_err(|e| format!("logotype icon: {e}")));
    match made {
        Ok(i) => Some(i),
        Err(e) => {
            diag_warn!("{e} - the taskbar button shows the default icon");
            None
        }
    }
}

/// Sets the window icon (title bar, `ICON_SMALL`) and, on Windows, the taskbar and Alt+Tab icon (`ICON_BIG`).
/// winit's `with_window_icon` alone sets only the small one on Windows, so the taskbar showed no icon (#1754).
pub fn with_icons(attrs: WindowAttributes) -> WindowAttributes {
    let attrs = attrs.with_window_icon(window_icon());
    #[cfg(windows)]
    let attrs = winit::platform::windows::WindowAttributesExtWindows::with_taskbar_icon(attrs, taskbar_icon());
    attrs
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_logotype_icon_decodes_square_and_opaque() -> Result<(), String> {
        let (rgba, w, h) = super::decode()?;
        assert_eq!((w, h), (128, 128));
        assert_eq!(rgba.len(), 128 * 128 * 4);
        assert!(rgba.chunks(4).all(|p| p[3] == 255));
        assert!(super::window_icon().is_some());
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn the_taskbar_icon_decodes_at_256_and_both_icons_reach_the_window_attributes() -> Result<(), String> {
        let (rgba, w, h) = super::decode_png(super::PNG_256)?;
        assert_eq!((w, h), (256, 256));
        assert_eq!(rgba.len(), 256 * 256 * 4);
        assert!(super::taskbar_icon().is_some());
        let attrs = super::with_icons(winit::window::Window::default_attributes());
        assert!(attrs.window_icon.is_some());
        Ok(())
    }
}
