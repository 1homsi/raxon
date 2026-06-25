# Images, Media & Graphics

Match RN Image/`expo-av`/`react-native-svg` and Flutter's image/canvas/painting.
⬜ planned.

## Images
- 🟡 sources: network, bundled asset, local file, data-URI, memory
- ⬜ async decode off the UI thread
- ⬜ in-memory + disk cache, cache control, prefetch
- ⬜ resize modes (cover/contain/stretch/center/repeat)
- ⬜ placeholder + fade-in, blurhash/thumbhash, progressive
- ⬜ priority, cancellation, retry
- 🟡 tint/recolor, rounded corners, borders
- ⬜ animated formats (GIF/APNG/animated WebP)
- ⬜ HDR / wide-gamut, downsampling, density-aware `@2x/@3x`
- ⬜ error/loading callbacks

## Vector & drawing
- ⬜ SVG rendering
- ⬜ vector icon system
- ⬜ `Canvas` / custom painting API (paths, fills, strokes, gradients, clips)
- ⬜ shapes (rect/rrect/circle/path/arc), shadows
- ⬜ blend modes, masking, clipping
- ⬜ shaders / effects (on the GPU renderer)
- ⬜ charts/graphs built on the canvas

## Video & audio
- ⬜ video player (controls, fullscreen, PiP, HLS/DASH, captions)
- ⬜ audio playback + recording
- ⬜ background audio, lock-screen controls, AirPlay/Cast
- ⬜ camera preview + capture (see device-apis)
- ⬜ media metadata, thumbnails

## Camera & capture
- ⬜ camera preview view, photo/video capture
- ⬜ barcode/QR scanning, face/ML hooks
- ⬜ image/video picker from library

## Graphics infrastructure
- ⬜ GPU renderer (`rax-vello`, Vello/wgpu) as the custom-drawing path
- ⬜ offscreen rendering / snapshots / view-to-image
- ⬜ Lottie / animated vector support
- ⬜ color management / color spaces
