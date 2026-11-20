//! Game overlay core: capture a screen region, read its text, translate it.
//! UI-independent; the desktop app draws the result.
//!
//! Only screen capture + OCR: nothing is injected into or read from the game.

pub mod caption;
pub mod capture;
pub mod change;
pub mod pipeline;
#[cfg(target_os = "linux")]
pub mod wayland;

use serde::{Deserialize, Serialize};

/// A screen rectangle in logical (scaled) global coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }
}

/// Where the caption goes relative to the captured region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Right below the region (above it when there is no room below).
    Below,
    /// Right above the region (below it when there is no room above).
    Above,
    /// A rectangle the user picked.
    Custom(Region),
}

/// The caption rectangle. Below/above never overlap the region, or the next capture
/// would read the caption itself. `screen` is the monitor holding the region.
pub fn caption_area(region: Region, screen: Region, placement: Placement) -> Region {
    let height = region.height.clamp(48, 160);
    let below = region.bottom();
    let above = region.y - height as i32;
    let fits_below = below + height as i32 <= screen.bottom();
    let fits_above = above >= screen.y;
    let y = match placement {
        Placement::Custom(area) => return area,
        Placement::Below if fits_below || !fits_above => {
            if fits_below {
                below
            } else {
                above.max(screen.y)
            }
        }
        Placement::Below => above,
        Placement::Above if fits_above || !fits_below => above.max(screen.y),
        Placement::Above => below,
    };
    Region {
        x: region.x,
        y,
        width: region.width,
        height,
    }
}

pub use capture::{Capture, CaptureError, default_capture};
pub use pipeline::{Frame, Pipeline, Recognizer};

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Region = Region {
        x: 0,
        y: 0,
        width: 1600,
        height: 1000,
    };

    #[test]
    fn caption_goes_below_the_region() {
        let r = Region {
            x: 300,
            y: 600,
            width: 800,
            height: 120,
        };
        assert_eq!(
            caption_area(r, SCREEN, Placement::Below),
            Region {
                x: 300,
                y: 720,
                width: 800,
                height: 120
            }
        );
    }

    #[test]
    fn caption_goes_above_when_no_room_below() {
        let r = Region {
            x: 300,
            y: 880,
            width: 800,
            height: 100,
        };
        assert_eq!(
            caption_area(r, SCREEN, Placement::Below),
            Region {
                x: 300,
                y: 780,
                width: 800,
                height: 100
            }
        );
    }

    #[test]
    fn caption_height_is_bounded() {
        let tall = Region {
            x: 0,
            y: 0,
            width: 400,
            height: 600,
        };
        assert_eq!(caption_area(tall, SCREEN, Placement::Below).height, 160);
        let thin = Region {
            x: 0,
            y: 0,
            width: 400,
            height: 20,
        };
        assert_eq!(caption_area(thin, SCREEN, Placement::Below).height, 48);
    }

    #[test]
    fn above_and_custom_placement() {
        let r = Region {
            x: 300,
            y: 600,
            width: 800,
            height: 120,
        };
        assert_eq!(caption_area(r, SCREEN, Placement::Above).y, 480);
        // No room above: falls back to below.
        let top = Region {
            x: 0,
            y: 10,
            width: 400,
            height: 60,
        };
        assert_eq!(caption_area(top, SCREEN, Placement::Above).y, 70);
        let custom = Region {
            x: 5,
            y: 6,
            width: 700,
            height: 90,
        };
        assert_eq!(caption_area(r, SCREEN, Placement::Custom(custom)), custom);
    }
}
