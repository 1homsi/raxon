# Layout

Flexbox-first (taffy), matching Yoga (RN) and Flutter's box model, plus grid.
✅ shipped · 🟡 wip · ⬜ planned.

## Flexbox
- ✅ flex-direction (row/column), gap, padding, align-items
- ✅ flex-grow
- ✅ align (start/center/end/stretch)
- ✅ justify-content (start/center/end/space-between/around/evenly)
- ✅ flex-wrap + align-content
- ✅ flex-shrink, flex-basis
- ✅ align-self (per-child override)
- ✅ row/column gap (independent)
- ⬜ order

## Sizing
- 🟡 width/height: auto, points, percent, `fr`
- ✅ min/max width & height
- ✅ aspect-ratio
- ⬜ intrinsic sizing from real text/content metrics
- ⬜ `Expanded`/`Flexible`/`Spacer` helpers
- ⬜ fit modes (contain/cover/fill) for media

## Positioning
- 🟡 position: relative / absolute / sticky
- 🟡 inset (top/right/bottom/left), z-index / z-order
- ⬜ `Stack`/overlay layout
- ⬜ transforms (translate/scale/rotate/skew, matrix), transform-origin

## Box model & spacing
- ✅ margin (incl. auto-margins for centering)
- ✅ padding
- ⬜ border width per-edge (paint side: color/style/radius)
- ⬜ safe-area insets + display cutout / notch handling
- ⬜ keyboard insets (avoidance)

## Grid
- ⬜ CSS-grid: template rows/cols, areas, auto-flow, gaps
- ⬜ `LazyGrid` (virtualized)

## Direction & adaptivity
- ⬜ RTL-aware layout (logical start/end vs left/right)
- ⬜ writing modes
- ⬜ responsive layout by size-class / breakpoints / orientation
- ⬜ container queries
- ⬜ adaptive split-view (tablet/desktop)

## Custom layout
- ⬜ `Layout` trait — author bespoke layout algorithms
- ⬜ measure/arrange callbacks for custom widgets
- ⬜ baseline alignment

## Performance
- ⬜ dirty-subtree relayout (don't relayout the world)
- ⬜ layout result caching + measure memoization
- ⬜ off-main-thread layout
