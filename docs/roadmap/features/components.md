# Components / Widget Library

The standard widget set. Goal: the **union** of React Native's core + essential
community components and Flutter's material/cupertino/widgets — every one
styleable, with a headless core, and replaceable via the theme registry
(see [customizability](../03-customizability.md)). ✅ shipped · 🟡 wip · ⬜ planned.

## Primitives & layout containers
- ✅ `View` (box / container)
- ✅ `Text`
- ✅ `Button`
- ✅ `Column` / `Row` (flex containers)
- ✅ `Spacer`
- ✅ `Dynamic` (reactive subtree)
- ⬜ `Stack` / `ZStack` (overlapping, z-ordered children)
- ⬜ `Wrap` (flow layout)
- ⬜ `Grid` / `LazyGrid`
- ⬜ `Expanded` / `Flexible` helpers
- ⬜ `AspectRatio`, `Center`, `Align`, `Positioned` (absolute)
- ⬜ `SafeArea`
- ⬜ `ScrollView` (🟡 basic), `LazyColumn`/`LazyRow`
- ⬜ `Fragment` / keyed `For` list helper

## Text & display
- 🟡 `Text` with font family/weight/size/color/line-height/align/truncation
- ⬜ Rich text / spans (inline styles, links, inline images)
- ✅ `Icon` (vector icon set + custom)
- 🟡 `Image` (source + tint; network/placeholder/fade-in/resize modes later)
- ✅ `Avatar` (composed from public API)
- ✅ `Badge` (composed from public API)
- ✅ `Divider` / `Separator`
- ✅ `Card` primitive (composed from public API)
- ✅ `Chip` / `Tag` (composed from public API)
- ⬜ `Tooltip`
- ⬜ `Skeleton` / shimmer placeholder

## Input & controls
- 🟡 `TextInput` / `TextField` (single + multi-line) — see [text-input](text-input-and-forms.md)
- ✅ `Switch` / `Toggle`
- ✅ `Checkbox` (composed from public API — no engine support needed)
- ✅ `Radio` / `RadioGroup` (composed from public API)
- ✅ `Slider` (single + range)
- ✅ `Stepper`
- ✅ `SegmentedControl`
- ⬜ `Picker` / `Select` / `Dropdown`
- ⬜ `DatePicker` / `TimePicker` / `DateTimePicker`
- ⬜ `Pressable` / `Touchable` (with pressed/hover/focus states)
- ⬜ `RatingBar`
- ⬜ `SearchBar`
- ⬜ `ColorPicker`

## Feedback & status
- ✅ `ActivityIndicator` / `Spinner`
- ✅ `ProgressBar` (linear) / `ProgressRing` (circular)
- ⬜ `Toast` / `Snackbar`
- ⬜ `Alert` / `Dialog`
- ⬜ `ActionSheet`
- ⬜ `Banner` / inline alert
- ⬜ `RefreshControl` (pull-to-refresh)
- ⬜ `StatusBar` control (style/color/visibility)

## Overlays & surfaces
- ⬜ `Modal`
- ⬜ `BottomSheet` (draggable, snap points)
- ⬜ `Popover`
- ⬜ `Menu` / `ContextMenu`
- ⬜ `Drawer` / `SideMenu`
- ⬜ `Backdrop` / scrim

## Navigation surfaces
- ⬜ `AppBar` / `NavigationBar` / `Toolbar`
- ⬜ `TabBar` / `TabView` / `BottomNavigation`
- ⬜ `Breadcrumbs`
- ⬜ `SegmentedTabs`

## Containers & disclosure
- ⬜ `Accordion` / `Disclosure` / `ExpansionPanel`
- ⬜ `Collapsible`
- ⬜ `Carousel` / `PageView` (paged horizontal)
- ⬜ `SwipeActions` (swipe-to-delete etc.)
- ⬜ `Pull-to-refresh`, `infinite scroll` helpers
- ⬜ `KeyboardAvoidingView`
- ⬜ `Resizable` / `SplitView` (desktop/tablet)

## Data display
- ⬜ `List` / `SectionList` / `VirtualizedList` (recycled) — see [lists](lists-and-scrolling.md)
- ⬜ `Table` / `DataGrid`
- ⬜ `Tree` view
- ⬜ Charts primitives (line/bar/pie) — custom-drawn on the GPU renderer

## Media
- ⬜ `Image`, `AnimatedImage` (GIF/WebP), `SVG`
- ⬜ `Video` player
- ⬜ `Camera` preview view
- ⬜ `Map` view
- ⬜ `WebView` (escape hatch, not the rendering model)

## Cross-cutting requirements for every component
- Styleable inline + via theme tokens + per-type variants.
- Headless core (state/a11y/gestures) separable from presentation.
- Replaceable app-wide via the component registry.
- Accessible by default (role/label/state) — see [accessibility](accessibility.md).
- RTL-correct and locale-aware where text is involved.
- Works under both the native-widget backends and the GPU renderer.
