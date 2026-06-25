# Lists & Scrolling

Match RN `FlatList`/`SectionList`/`VirtualizedList` and Flutter
`ListView`/`CustomScrollView`/slivers. Performance-critical. ✅ · 🟡 · ⬜.

## Scrolling
- ✅ `ScrollView` (vertical, basic)
- ✅ horizontal `ScrollView`; ⬜ bidirectional
- ⬜ momentum / deceleration matching platform physics
- ⬜ overscroll / bounce (iOS) / stretch (Android)
- ⬜ paging / snap-to-interval / snap points
- ⬜ scroll events (offset, velocity, begin/end, momentum)
- ⬜ programmatic scroll (to offset / to item / to top)
- ⬜ scroll indicators (style, hide)
- ⬜ nested scrolling + scroll coordination
- ⬜ keyboard-dismiss-on-drag
- ⬜ zoomable scroll (pinch)
- ⬜ scroll-to-on-focus (forms)

## Virtualized lists (recycling)
- ⬜ `LazyColumn`/`LazyRow` (windowed rendering)
- ⬜ view **recycling / pooling** (bounded memory for huge lists)
- 🟡 `List` with keyed items + minimal reconciliation
- ⬜ `SectionList` (sticky section headers/footers)
- ⬜ variable / dynamic item heights, measured & cached
- ⬜ horizontal + grid virtualization (`LazyGrid`)
- ✅ separators (`item_separator(color, inset)` — 1pt horizontal rule with leading inset)
- ⬜ header / footer / empty-state components
- ⬜ initialScrollIndex, maintainVisibleContentPosition
- ⬜ estimated item size hints
- ⬜ windowing tuning (overscan)

## Interactions & affordances
- ✅ pull-to-refresh (`RefreshControl`)
- ⬜ infinite scroll / onEndReached / pagination
- ⬜ swipe actions (swipe-to-delete, leading/trailing)
- ⬜ drag-to-reorder
- ⬜ multi-select
- ⬜ sticky headers, floating headers, collapsing toolbars

## Advanced (slivers-equivalent)
- ⬜ composable scroll effects (parallax, collapsing, pinned)
- ⬜ scroll-linked animations
- ⬜ `CustomScrollView`-style composition of scrollable regions
- ⬜ virtualized 2D grids / tables

## Performance
- ⬜ off-main-thread layout for list items
- ⬜ recycle + diff so only changed items mutate
- ⬜ jank instrumentation for scroll
