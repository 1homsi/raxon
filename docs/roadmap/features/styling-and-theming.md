# Styling & Theming

Typed, predictable styling with a runtime-switchable theme system. The core of
[super-customizability](../03-customizability.md). ✅ · 🟡 · ⬜.

## Paint properties
- ✅ background color, corner radius
- ✅ text color, font size
- 🟡 borders (per-edge width, color, style, per-corner radius)
- 🟡 shadows (box + text), elevation
- ⬜ gradients (linear/radial/sweep), multiple backgrounds
- 🟡 opacity, blend modes
- ⬜ background images / nine-patch
- ⬜ blur / backdrop-filter (frosted glass)
- ⬜ clip / mask / overflow (visible/hidden/scroll)
- ⬜ filters (brightness/contrast/saturate), tint

## Style application model (resolution order, explicit)
- ✅ inline style (per instance)
- ⬜ per-type variants (e.g. Button "primary"/"ghost"/custom)
- ✅ theme defaults (tokens)
- ⬜ documented precedence: inline > variant > theme > default (no magic cascade)
- ⬜ conditional styles (pressed/hover/focus/disabled/selected)
- ⬜ responsive styles (by breakpoint / size-class / orientation / platform)
- ⬜ style composition / merge / extend

## Design tokens (typed)
- ⬜ color palette + semantic roles (primary/surface/onSurface/error/…)
- ⬜ spacing scale, radius scale, border widths
- ⬜ typography scale (families, sizes, weights, line-heights, letter-spacing)
- ⬜ shadow/elevation tokens
- ⬜ motion tokens (durations, easing curves)
- ⬜ z-index, opacity, breakpoints tokens
- ⬜ custom/user-defined tokens (extend the type-safe theme)

## Theming
- ✅ `Theme` context (scoped/nested themes)
- ✅ runtime theme switching (no rebuild) via signals — only affected props update
- 🟡 light / dark / high-contrast modes; system-driven + manual override
- ⬜ brand theme packages (publishable, composable)
- ⬜ component registry: override any built-in widget app-wide
- ⬜ per-platform theme overrides (native-feel iOS vs Android vs your own)
- ⬜ dynamic color (Material You / system accent) integration

## Tooling
- ⬜ theme editor / preview in devtools
- ⬜ contrast & a11y linting of token combos
- ⬜ export/import design tokens (Style Dictionary / Figma interop)
