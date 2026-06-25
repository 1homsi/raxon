# Gestures & Input

Match RN Gesture Handler and Flutter's gesture system. A gesture arena resolves
competing recognizers. ✅ · 🟡 · ⬜.

## Pointer / touch
- ✅ tap (via event seam)
- 🟡 multi-touch pointers (down/move/up/cancel) with ids
- ⬜ pressed/hover/focus states (Pressable)
- ⬜ hit-testing control (pointer-events, hitSlop, z-order aware)

## Recognizers
- ✅ tap, double-tap, multi-tap
- ✅ long-press (with duration, movement tolerance)
- ⬜ pan / drag (with thresholds, axis lock)
- ⬜ pinch / zoom
- ⬜ rotation
- ⬜ fling / swipe (directional)
- ⬜ force/3D-touch / pressure
- ⬜ edge / screen-edge gestures

## Composition & resolution
- ⬜ gesture arena (declare relationships: simultaneous/exclusive/require-fail)
- ⬜ gesture priority & cancellation
- ⬜ native recognizer bridging (cooperate with platform scroll/back gestures)
- ⬜ nested/overlapping gesture coordination
- ⬜ gesture-driven animations (drag follows finger)

## Desktop / hardware input
- ⬜ mouse (click/right-click/middle, wheel/trackpad scroll, hover, cursor styles)
- ⬜ keyboard events, shortcuts, modifiers, focus traversal (tab/arrows)
- ⬜ drag-and-drop (in-app + OS-level)
- ⬜ stylus / Apple Pencil (pressure/tilt)
- ⬜ context menus / right-click menus

## Accessibility & feedback
- ⬜ accessible activation (works with screen readers / switch control)
- ⬜ haptic feedback on gesture milestones
- ⬜ focus + keyboard equivalents for every gesture action
