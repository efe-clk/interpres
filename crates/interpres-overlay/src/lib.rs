//! Game overlay core: capture a screen region, read its text, translate it.
//! UI-independent; the desktop app draws the result.
//!
//! Only screen capture + OCR: nothing is injected into or read from the game.

pub mod capture;
pub mod change;
pub mod pipeline;

use serde::{Deserialize, Serialize};

/// A screen rectangle in logical (scaled) global coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub use capture::{Capture, CaptureError, default_capture};
pub use pipeline::{Frame, Pipeline, Recognizer};
