//! Design-token theme system for rax.

use rax_core::Color;
use rax_reactive::{create_signal, provide_context, use_context, Signal};

// ---------------------------------------------------------------------------
// Token structs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ColorTokens {
    // Brand
    pub primary: Color,
    pub primary_variant: Color,
    pub on_primary: Color,
    // Surface
    pub surface: Color,
    pub surface_variant: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    // Background
    pub background: Color,
    pub on_background: Color,
    // Status
    pub error: Color,
    pub on_error: Color,
    pub success: Color,
    pub warning: Color,
    pub info: Color,
    // Outline/divider
    pub outline: Color,
    pub outline_variant: Color,
}

impl ColorTokens {
    pub fn light() -> Self {
        Self {
            primary: Color::hex(0x6750A4FF),
            primary_variant: Color::hex(0x7965AFff),
            on_primary: Color::WHITE,
            surface: Color::hex(0xFFFBFEff),
            surface_variant: Color::hex(0xE7E0ECff),
            on_surface: Color::hex(0x1C1B1Fff),
            on_surface_variant: Color::hex(0x49454Fff),
            background: Color::hex(0xFFFBFEff),
            on_background: Color::hex(0x1C1B1Fff),
            error: Color::hex(0xB3261Eff),
            on_error: Color::WHITE,
            success: Color::hex(0x146C2Eff),
            warning: Color::hex(0xE65100ff),
            info: Color::hex(0x0277BDff),
            outline: Color::hex(0x79747Eff),
            outline_variant: Color::hex(0xCAC4D0ff),
        }
    }

    pub fn dark() -> Self {
        Self {
            primary: Color::hex(0xD0BCFFff),
            primary_variant: Color::hex(0xB4A2E0ff),
            on_primary: Color::hex(0x381E72ff),
            surface: Color::hex(0x1C1B1Fff),
            surface_variant: Color::hex(0x49454Fff),
            on_surface: Color::hex(0xE6E1E5ff),
            on_surface_variant: Color::hex(0xCAC4D0ff),
            background: Color::hex(0x1C1B1Fff),
            on_background: Color::hex(0xE6E1E5ff),
            error: Color::hex(0xF2B8B5ff),
            on_error: Color::hex(0x601410ff),
            success: Color::hex(0x6DD58Cff),
            warning: Color::hex(0xFFB74Dff),
            info: Color::hex(0x4FC3F7ff),
            outline: Color::hex(0x938F99ff),
            outline_variant: Color::hex(0x49454Fff),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpacingTokens {
    pub xs: f32,  // 4
    pub sm: f32,  // 8
    pub md: f32,  // 16
    pub lg: f32,  // 24
    pub xl: f32,  // 32
    pub xxl: f32, // 48
}

impl Default for SpacingTokens {
    fn default() -> Self {
        Self { xs: 4.0, sm: 8.0, md: 16.0, lg: 24.0, xl: 32.0, xxl: 48.0 }
    }
}

#[derive(Clone, Debug)]
pub struct TypographyTokens {
    pub display_large: f32,   // 57
    pub display_medium: f32,  // 45
    pub display_small: f32,   // 36
    pub headline_large: f32,  // 32
    pub headline_medium: f32, // 28
    pub headline_small: f32,  // 24
    pub title_large: f32,     // 22
    pub title_medium: f32,    // 16
    pub title_small: f32,     // 14
    pub body_large: f32,      // 16
    pub body_medium: f32,     // 14
    pub body_small: f32,      // 12
    pub label_large: f32,     // 14
    pub label_medium: f32,    // 12
    pub label_small: f32,     // 11
}

impl Default for TypographyTokens {
    fn default() -> Self {
        Self {
            display_large: 57.0,
            display_medium: 45.0,
            display_small: 36.0,
            headline_large: 32.0,
            headline_medium: 28.0,
            headline_small: 24.0,
            title_large: 22.0,
            title_medium: 16.0,
            title_small: 14.0,
            body_large: 16.0,
            body_medium: 14.0,
            body_small: 12.0,
            label_large: 14.0,
            label_medium: 12.0,
            label_small: 11.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RadiusTokens {
    pub xs: f32,   // 4
    pub sm: f32,   // 8
    pub md: f32,   // 12
    pub lg: f32,   // 16
    pub xl: f32,   // 28
    pub full: f32, // 9999
}

impl Default for RadiusTokens {
    fn default() -> Self {
        Self { xs: 4.0, sm: 8.0, md: 12.0, lg: 16.0, xl: 28.0, full: 9999.0 }
    }
}

#[derive(Clone, Debug)]
pub struct MotionTokens {
    // Durations in ms
    pub duration_short: u64,  // 200
    pub duration_medium: u64, // 350
    pub duration_long: u64,   // 500
    // Easing names (for reference / anim crate)
    pub easing_standard: &'static str,   // "ease-in-out"
    pub easing_emphasize: &'static str,  // "cubic-bezier(0.2, 0, 0, 1)"
    pub easing_decelerate: &'static str, // "ease-out"
    pub easing_accelerate: &'static str, // "ease-in"
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            duration_short: 200,
            duration_medium: 350,
            duration_long: 500,
            easing_standard: "ease-in-out",
            easing_emphasize: "cubic-bezier(0.2, 0, 0, 1)",
            easing_decelerate: "ease-out",
            easing_accelerate: "ease-in",
        }
    }
}

/// A single shadow level with an RGBA color, XY offset, and blur radius.
#[derive(Clone, Debug)]
pub struct ShadowToken {
    /// Shadow color (typically semi-transparent black).
    pub color: Color,
    /// Horizontal offset in points (positive = right).
    pub offset_x: f32,
    /// Vertical offset in points (positive = down).
    pub offset_y: f32,
    /// Gaussian blur radius in points.
    pub blur: f32,
}

/// Four elevation levels — `sm`, `md`, `lg`, `xl` — expressed as shadow tokens.
///
/// The defaults follow Material Design 3 elevation ramp values using
/// 20 % black (`#00000033`) as the shadow color.
///
/// # Example
/// ```no_run
/// let token = theme.shadows.md;
/// card.shadow(token.color, token.blur, token.offset_x, token.offset_y)
/// ```
#[derive(Clone, Debug)]
pub struct ShadowTokens {
    /// Elevation 1 — subtle card lift.
    pub sm: ShadowToken,
    /// Elevation 3 — raised panels.
    pub md: ShadowToken,
    /// Elevation 6 — floating menus / FABs.
    pub lg: ShadowToken,
    /// Elevation 12 — modals / bottom sheets.
    pub xl: ShadowToken,
}

impl Default for ShadowTokens {
    fn default() -> Self {
        // 20 % opaque black — perceptually neutral on both light and dark surfaces.
        let base = Color::hex(0x00000033);
        Self {
            sm: ShadowToken { color: base, offset_x: 0.0, offset_y: 1.0, blur: 2.0 },
            md: ShadowToken { color: base, offset_x: 0.0, offset_y: 2.0, blur: 6.0 },
            lg: ShadowToken { color: base, offset_x: 0.0, offset_y: 4.0, blur: 12.0 },
            xl: ShadowToken { color: base, offset_x: 0.0, offset_y: 8.0, blur: 24.0 },
        }
    }
}

// ---------------------------------------------------------------------------
// Theme aggregate
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Theme {
    pub colors: ColorTokens,
    pub spacing: SpacingTokens,
    pub typography: TypographyTokens,
    pub radius: RadiusTokens,
    pub motion: MotionTokens,
    pub shadows: ShadowTokens,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            colors: ColorTokens::light(),
            spacing: SpacingTokens::default(),
            typography: TypographyTokens::default(),
            radius: RadiusTokens::default(),
            motion: MotionTokens::default(),
            shadows: ShadowTokens::default(),
        }
    }

    pub fn dark() -> Self {
        Self {
            colors: ColorTokens::dark(),
            spacing: SpacingTokens::default(),
            typography: TypographyTokens::default(),
            radius: RadiusTokens::default(),
            motion: MotionTokens::default(),
            shadows: ShadowTokens::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Context API
// ---------------------------------------------------------------------------

/// Provide a reactive theme to the subtree. Call near the app root.
///
/// ```no_run
/// provide_theme(Theme::light());
/// ```
pub fn provide_theme(theme: Theme) -> Signal<Theme> {
    let sig = create_signal(theme);
    provide_context(sig);
    sig
}

/// Returns the nearest [`Signal<Theme>`] provided by [`provide_theme`].
/// Panics if none is provided.
pub fn use_theme() -> Signal<Theme> {
    use_context::<Signal<Theme>>()
        .expect("no theme provided — call provide_theme() near the app root")
}

/// Returns the nearest [`Signal<Theme>`] or `None` if not provided.
pub fn try_use_theme() -> Option<Signal<Theme>> {
    use_context::<Signal<Theme>>()
}
