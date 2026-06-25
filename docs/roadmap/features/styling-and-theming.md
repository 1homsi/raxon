# Styling & Theming

Typed, predictable styling with a runtime-switchable theme system. The core of
[super-customizability](../03-customizability.md). ✅ · 🟡 · ⬜.

## Paint properties
- ✅ background color, corner radius
- ✅ text color, font size
- 🟡 borders (per-edge width, color, style, per-corner radius)
- 🟡 shadows (box + text), elevation
- ✅ linear gradient (vertical/horizontal/custom points); 🟡 radial/sweep + multiple backgrounds pending
- ✅ custom font family (`font_family()`)
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
- ✅ conditional styles (disabled/visible/hidden — `.disabled_opacity()`, `.visible_when()`, `.hidden_when()` reactive opacity helpers)
- ⬜ responsive styles (by breakpoint / size-class / orientation / platform)
- ⬜ style composition / merge / extend

## Design tokens (typed)
- ✅ color palette + semantic roles (`ColorTokens` — primary/surface/onSurface/error/success/warning/info/outline + light/dark Material-3 palettes)
- ✅ spacing scale, radius scale (`SpacingTokens{xs/sm/md/lg/xl/xxl}`, `RadiusTokens{xs/sm/md/lg/xl/full}`)
- ✅ typography scale (`TypographyTokens` — display/headline/title/body/label at all sizes)
- ✅ shadow/elevation tokens (`ShadowTokens{sm/md/lg/xl}` in `Theme`; `ShadowToken{color,offset_x,offset_y,blur}`)
- ✅ motion tokens (`MotionTokens` — duration_short/medium/long + easing names)
- ⬜ z-index, opacity, breakpoints tokens
- ⬜ custom/user-defined tokens (extend the type-safe theme)

## Theming
- ✅ `Theme` context (scoped/nested themes)
- ✅ runtime theme switching (no rebuild) via signals — only affected props update
- 🟡 light / dark / high-contrast modes; system-driven + manual override
  - ✅ reactive system color-scheme signal (`use_color_scheme`) — content auto-adapts to OS light/dark
  - ✅ safe-area backdrop: fixed color or `System { light, dark }` auto-following appearance
  - ⬜ high-contrast; manual app-level override of the system scheme
- ⬜ brand theme packages (publishable, composable)
- ✅ component registry (`register_component(name, factory)` → thread-local `HashMap<String, Factory>`; `resolve_component`, `unregister_component`, `ComponentProps` builder)
- ⬜ per-platform theme overrides (native-feel iOS vs Android vs your own)
- ⬜ dynamic color (Material You / system accent) integration

## Tooling
- ⬜ theme editor / preview in devtools
- ⬜ contrast & a11y linting of token combos
- ⬜ export/import design tokens (Style Dictionary / Figma interop)
