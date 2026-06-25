//! A small, backend-agnostic RGBA color type.
//!
//! Stored as four `u8` channels (straight, non-premultiplied alpha). This is the
//! lowest common denominator across our targets — Android `@ColorInt`, UIKit
//! `UIColor` (via components), and any GPU backend can all consume it without
//! loss. Color *spaces* (sRGB vs. display-P3) are a styling concern and live in
//! `rax-style`; here we are deliberately just bytes.

/// An 8-bit-per-channel RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red channel, `0..=255`.
    pub r: u8,
    /// Green channel, `0..=255`.
    pub g: u8,
    /// Blue channel, `0..=255`.
    pub b: u8,
    /// Alpha channel, `0` fully transparent .. `255` fully opaque.
    pub a: u8,
}

impl Color {
    /// Fully transparent.
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    /// Opaque color from RGB channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    /// Color from RGBA channels.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Packs the color into a `0xAARRGGBB` integer (Android `@ColorInt` layout).
    pub const fn to_argb_u32(self) -> u32 {
        (self.a as u32) << 24 | (self.r as u32) << 16 | (self.g as u32) << 8 | (self.b as u32)
    }

    /// Returns this color with its alpha replaced.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Color { a, ..self }
    }
}

/// The system appearance, reported by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorScheme {
    /// Light mode (the default).
    #[default]
    Light,
    /// Dark mode.
    Dark,
}

impl ColorScheme {
    /// Whether this is the dark scheme.
    pub const fn is_dark(self) -> bool {
        matches!(self, ColorScheme::Dark)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_is_opaque() {
        assert_eq!(
            Color::rgb(1, 2, 3),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: 255
            }
        );
    }

    #[test]
    fn argb_packing_order() {
        // 0xAARRGGBB
        assert_eq!(
            Color::rgba(0x11, 0x22, 0x33, 0x44).to_argb_u32(),
            0x4411_2233
        );
        assert_eq!(Color::WHITE.to_argb_u32(), 0xFFFF_FFFF);
    }

    #[test]
    fn with_alpha_replaces_only_alpha() {
        assert_eq!(
            Color::BLACK.with_alpha(128),
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 128
            }
        );
    }
}
