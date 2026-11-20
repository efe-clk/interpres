//! Screen capture of a region.
//!
//! On wlroots-based Wayland compositors (Hyprland, Sway) the screencopy protocol is
//! used directly: no permission prompt, ~7 ms per subtitle-sized region. Elsewhere
//! (X11, GNOME/KDE via the portal, Windows) `xcap` is used.

use image::RgbImage;

use crate::Region;

#[derive(Debug, thiserror::Error)]
#[error("screen capture failed: {0}")]
pub struct CaptureError(pub String);

pub trait Capture: Send {
    fn capture(&mut self, region: &Region) -> Result<RgbImage, CaptureError>;
}

/// The fastest capture available on this system.
pub fn default_capture() -> Box<dyn Capture> {
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        match wlroots::Wlroots::new() {
            Ok(c) => return Box::new(c),
            Err(e) => tracing::info!(error = %e, "wlroots screencopy unavailable, using xcap"),
        }
    }
    Box::new(Xcap)
}

/// `xcap`: X11, Windows, and the portal on other Wayland desktops.
pub struct Xcap;

impl Capture for Xcap {
    fn capture(&mut self, r: &Region) -> Result<RgbImage, CaptureError> {
        let err = |e: xcap::XCapError| CaptureError(e.to_string());
        let monitor = xcap::Monitor::from_point(r.x, r.y).map_err(err)?;
        let (mx, my) = (monitor.x().map_err(err)?, monitor.y().map_err(err)?);
        let img = monitor
            .capture_region(
                (r.x - mx).max(0) as u32,
                (r.y - my).max(0) as u32,
                r.width,
                r.height,
            )
            .map_err(err)?;
        Ok(image::DynamicImage::ImageRgba8(img).to_rgb8())
    }
}

#[cfg(target_os = "linux")]
mod wlroots {
    use image::RgbImage;
    use libwayshot_xcap::WayshotConnection;
    use libwayshot_xcap::region::{LogicalRegion, Position, Region as WRegion, Size};

    use super::{Capture, CaptureError};
    use crate::Region;

    pub struct Wlroots(WayshotConnection);

    impl Wlroots {
        pub fn new() -> Result<Self, CaptureError> {
            WayshotConnection::new()
                .map(Self)
                .map_err(|e| CaptureError(e.to_string()))
        }
    }

    impl Capture for Wlroots {
        fn capture(&mut self, r: &Region) -> Result<RgbImage, CaptureError> {
            let region = LogicalRegion {
                inner: WRegion {
                    position: Position { x: r.x, y: r.y },
                    size: Size {
                        width: r.width,
                        height: r.height,
                    },
                },
            };
            self.0
                .screenshot(region, false)
                .map(|img| img.to_rgb8())
                .map_err(|e| CaptureError(e.to_string()))
        }
    }
}
