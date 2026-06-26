# Internationalization (i18n) & Localization (l10n)

Match Flutter `intl` / RN i18n ecosystems. ⬜ planned.

## Messages & translation
- ✅ message catalogs (`I18n::add_locale(locale, &[(key, template)])` — runtime hashmap catalog)
- ✅ interpolation (`{var}` replacement in templates via `i18n.t(key, &[("var","val")])`)
- ✅ pluralization (`singular|plural` split; `i18n.t_plural(key, count, args)`)
- ✅ runtime locale switching (`i18n.set_locale(code)` — `Signal<String>`, no rebuild; reactive reads re-derive)
- ✅ fallback locale chain (falls back to "en" if key missing in active locale)
- ⬜ ICU MessageFormat (select, gender, complex plurals)
- ⬜ extraction tooling (scan source → catalog)
- ⬜ translation file formats (ARB / PO / JSON / XLIFF) import/export

## Formatting
- ✅ locale-aware number formatting (`format_number(i18n, f64, decimals)` — period vs comma by locale, thousands grouping)
- ✅ currency (`format_currency(i18n, amount, code)` — 16 ISO codes, prefix vs suffix by locale)
- ✅ relative time (`format_relative_time(i18n, seconds)` — "3 minutes ago" / "in 1 hour"; fr/de/es/ar variants)
- ✅ list formatting (`format_list(i18n, items)` — locale conjunctions: and/et/und/y/و)
- ⬜ dates, times, durations (calendar-aware)
- ⬜ units, measurements
- ✅ collation / locale-aware sorting & search (`collate_sort`, `collate_search` — case-insensitive, ICU extension point)
- ⬜ calendars (Gregorian + non-Gregorian)

## Layout & text direction
- ⬜ RTL layout mirroring (logical start/end)
- ⬜ bidi text handling
- ⬜ per-locale typography / font selection
- ⬜ locale-aware casing

## Integration
- ✅ RTL detection (`i18n.is_rtl()` — checks locale against known RTL language tags)
- ✅ system locale detection (`system_locale()` — parses `LANG` env var, falls back to "en")
- ⬜ system locale detection (iOS/Android CFLocale / Resources.getConfiguration)
- ⬜ override UI
- ✅ pseudolocalization (`pseudolocalize(s)` — ASCII → accented chars + bracket wrapping for i18n completeness testing)
- ⬜ i18n lints (hard-coded strings, missing translations)
- ⬜ region-specific assets (images/audio per locale)
