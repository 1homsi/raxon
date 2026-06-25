# Accessibility

First-class, never optional. Map to VoiceOver/TalkBack (and ARIA on web). A
release that fails the a11y gate does not ship. ⬜ planned.

## Semantics
- 🟡 roles (button/header/image/link/adjustable/search/…)
- 🟡 label / hint / value / description
- ⬜ state (selected/checked/disabled/expanded/busy)
- ⬜ traits/properties, live regions (announcements)
- ⬜ grouping / merging semantics, hidden-from-a11y
- ⬜ custom actions (rotor / context actions)
- ⬜ heading levels, landmarks

## Screen readers
- ⬜ VoiceOver (iOS), TalkBack (Android), Narrator/NVDA (desktop), ARIA (web)
- ⬜ focus order + programmatic focus
- ⬜ reading order independent of visual order
- ⬜ announcements / live updates
- ⬜ accessible names computed correctly for composed widgets

## Vision & motor
- ⬜ Dynamic Type / font scaling honored everywhere
- ⬜ high-contrast / increased-contrast modes
- ⬜ color-blind-safe defaults, contrast checks in CI
- ⬜ reduced-motion / reduce-transparency respected by animation
- ⬜ large touch targets, hit-slop, focus-visible rings
- ⬜ switch control / keyboard-only operation
- ⬜ voice control compatibility

## Tooling & process
- ⬜ accessibility inspector in devtools
- ⬜ automated a11y audits in CI (missing labels, contrast, target size)
- ⬜ a11y **release gate** (conformance suite includes a11y checks)
- ⬜ per-platform a11y conformance (mobile/desktop/web)
- ⬜ docs + lints guiding accessible component authoring
