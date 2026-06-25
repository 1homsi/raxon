# Text Input, Keyboard & Forms

The hardest parity area (controlled input + IME). Goal: match RN `TextInput` and
Flutter `TextField`/`Form` completely. ⬜ planned unless noted.

## TextInput / TextField
- 🟡 controlled value (value ↔ signal, two-way, race-free)
- ⬜ uncontrolled / defaultValue
- ⬜ single-line + multi-line (auto-grow, max lines)
- ⬜ placeholder, prefix/suffix, clear button
- ⬜ selection + caret control (programmatic get/set)
- ⬜ keyboard types (default/email/number/phone/url/decimal/search)
- ⬜ return key types + onSubmit
- ⬜ autocapitalize, autocorrect, spellcheck, autocomplete/contentType
- ⬜ secure entry (password), reveal toggle
- ⬜ max length, input masks / formatters
- ⬜ editable/read-only/disabled
- ⬜ onFocus/onBlur/onChange/onKeyPress/onSelectionChange
- ⬜ focus management: focus()/blur(), focus order, focus traversal

## IME / composition (the hard part)
- ⬜ composition (marked text) for CJK/dictation without clobbering
- ⬜ autocorrect/suggestion bar integration
- ⬜ predictive text, inline completion
- ⬜ emoji & dictation input

## Keyboard
- ⬜ keyboard avoidance (content moves with keyboard)
- ⬜ keyboard show/hide events + frame
- ⬜ input accessory view / toolbar (done button, custom)
- ⬜ hardware keyboard + shortcuts (desktop/tablet), key events, modifiers
- ⬜ custom in-app keyboards

## Forms
- ⬜ form state management (values, touched, dirty)
- ⬜ validation (sync + async), error messages, schema validation
- ⬜ field-level + form-level validation
- ⬜ submit handling, reset
- ⬜ accessible labels/errors wiring
- ⬜ multi-step / wizard helpers
- ⬜ controlled groups (radio/checkbox/select)

## Customizability
- ⬜ headless input core (state + IME + a11y) with bring-your-own presentation
- ⬜ fully custom-rendered text editing on the GPU path (advanced)
