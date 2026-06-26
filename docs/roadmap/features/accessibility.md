# Accessibility

First-class, never optional. Map to VoiceOver/TalkBack (and ARIA on web). A
release that fails the a11y gate does not ship. ⬜ planned.

## Semantics
- 🟡 roles (button/header/image/link/adjustable/search/…)
- 🟡 label / hint / value / description
- ✅ state (selected/checked/disabled/expanded/busy) — `accessibility_selected/disabled/expanded/busy()` modifiers; map to UIAccessibilityTraits
- ✅ live regions / announcements (`announce_accessibility(msg)` on Tree → `UIAccessibilityPostNotification(1008, nsString)`)
- ✅ grouping / hidden-from-a11y (`accessibility_group(bool)` → `setIsAccessibilityElement:`; `accessibility_hidden(bool)` → `setAccessibilityElementsHidden:`)
- ✅ custom actions (`accessibility_actions(Vec<&str>)` → `Attribute::AccessibilityActions` stub; UIAccessibilityCustomAction wiring TODO)
- ✅ heading levels (`accessibility_heading(level: u8)` → UIAccessibilityTraitHeader bitmask)

## Screen readers
- ⬜ VoiceOver (iOS), TalkBack (Android), Narrator/NVDA (desktop), ARIA (web)
- ✅ programmatic focus (`request_focus(id)` on Tree → `UIAccessibilityPostNotification(1000, view)`; moves VoiceOver cursor)
- ⬜ reading order independent of visual order
- ⬜ announcements / live updates (see live regions ✅ above)
- ⬜ accessible names computed correctly for composed widgets

## Vision & motor
- ✅ Dynamic Type / font scaling (`dynamic_type(bool)` → `Attribute::DynamicType` → `setAdjustsFontForContentSizeCategory:`)
- ⬜ high-contrast / increased-contrast modes
- ⬜ color-blind-safe defaults, contrast checks in CI
- ✅ reduced-motion respected by animation (`use_reduced_motion()` signal + `animate_unless_reduced()` in rax-anim; platform sets via `set_reduced_motion`)
- 🟡 large touch targets, hit-slop (`hit_slop(top,right,bottom,left)` modifier — stored, custom hit-test pending)
- ⬜ switch control / keyboard-only operation
- ⬜ voice control compatibility

## Tooling & process
- ⬜ accessibility inspector in devtools
- ⬜ automated a11y audits in CI (missing labels, contrast, target size)
- ⬜ a11y **release gate** (conformance suite includes a11y checks)
- ⬜ per-platform a11y conformance (mobile/desktop/web)
- ⬜ docs + lints guiding accessible component authoring
