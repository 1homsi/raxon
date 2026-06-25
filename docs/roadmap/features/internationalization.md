# Internationalization (i18n) & Localization (l10n)

Match Flutter `intl` / RN i18n ecosystems. ⬜ planned.

## Messages & translation
- 🟡 message catalogs (typed keys, compile-checked)
- ⬜ ICU MessageFormat (interpolation, select, plurals, gender)
- ⬜ pluralization rules per locale
- ⬜ runtime locale switching (no rebuild)
- ⬜ fallback locale chains
- ⬜ extraction tooling (scan source → catalog)
- ⬜ translation file formats (ARB / PO / JSON / XLIFF) import/export

## Formatting
- ⬜ numbers, currency, percent (locale-aware)
- ⬜ dates, times, relative time, durations
- ⬜ lists, units, measurements
- ⬜ collation / locale-aware sorting & search
- ⬜ calendars (Gregorian + non-Gregorian)

## Layout & text direction
- ⬜ RTL layout mirroring (logical start/end)
- ⬜ bidi text handling
- ⬜ per-locale typography / font selection
- ⬜ locale-aware casing

## Integration
- ⬜ system locale detection + override
- ⬜ pseudolocalization for testing
- ⬜ i18n lints (hard-coded strings, missing translations)
- ⬜ region-specific assets (images/audio per locale)
