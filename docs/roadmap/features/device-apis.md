# Device & Platform APIs (Native Modules)

The capability surface apps reach for — delivered as first-party **plugins** over
a stable plugin ABI (see [extensibility](extensibility-and-plugins.md)). Goal:
the union of RN community modules + Flutter plugins + Expo SDK. ⬜ planned.

## Sensors & hardware
- ✅ accelerometer, gyroscope (`use_accelerometer/gyroscope() -> Signal<Option<AccelerometerData/GyroscopeData>>`; `update_accelerometer/gyroscope(data)` platform hooks); ⬜ magnetometer, barometer
- ⬜ device motion / orientation, pedometer
- ⬜ proximity, ambient light
- ✅ haptics (`haptic(HapticStyle)` — UIImpactFeedbackGenerator / UINotificationFeedbackGenerator / UISelectionFeedbackGenerator)
- ✅ battery status (`use_battery_level() -> Signal<f32>`, `use_battery_charging() -> Signal<bool>` — UIDevice, polled every 60 ticks)
- ✅ flashlight / torch (`set_torch(on: bool)` → `Mutation::SetTorch` → AVCaptureDevice stub)

## Location & maps
- ✅ GPS location (`use_location() -> Signal<Option<GeoLocation>>`; `GeoLocation{lat,lon,alt,accuracy,speed}`; `request_location/stop_location_updates()` → CLLocationManager stub)
- ⬜ geofencing, background location
- ⬜ geocoding / reverse geocoding
- ⬜ Map view (markers, polylines, regions, clustering)

## Connectivity
- ⬜ Bluetooth / BLE (central + peripheral)
- ⬜ NFC (read/write/HCE)
- 🟡 network reachability (`use_network_status() -> Signal<NetworkStatus>`; `update_network_status` platform hook; SCNetworkReachability integration pending)
- ⬜ nearby / multipeer

## Camera, media & files
- ✅ camera capture + QR scanner (AVFoundation-backed); ⬜ image/video picker, media library
- ⬜ file system (read/write/stream), document picker
- ✅ share sheet (`share_text(text)` → `UIActivityViewController`); ✅ clipboard (`set_clipboard(text)` → `UIPasteboard`)
- ⬜ downloads / uploads (background)
- ⬜ printing, PDF generation

## Notifications & background
- ✅ local notifications (`schedule_notification` / `cancel_notification` — UNUserNotificationCenter, time-interval trigger)
- ✅ push notification token (`register_for_push()` → `UIApplication.registerForRemoteNotifications` stub; `use_push_token() -> Signal<Option<String>>`; `update_push_token/clear_push_token`)
- ⬜ push notifications (APNs/FCM payload handling), rich/silent push
- ⬜ background tasks / background fetch / headless tasks
- ✅ app badge (`set_app_badge(count)` → `setApplicationIconBadgeNumber:` via pending queue in tick); ⬜ live activities, widgets

## Identity & security
- ✅ biometrics (`authenticate_biometric(reason)` — LAContext, Face/Touch ID → `Event::BiometricResult`)
- ✅ secure storage / keychain (`rax-keychain`: `set/get/delete_secret` — Security.framework on iOS)
- ⬜ auth helpers (OAuth, sign-in-with-Apple/Google), deep-link auth
- ⬜ app attest / integrity, encryption primitives

## Commerce & platform services
- ⬜ in-app purchases / subscriptions (StoreKit / Play Billing)
- ⬜ contacts, calendar, reminders
- ⬜ health / fitness data (HealthKit / Google Fit)
- ⬜ speech-to-text / text-to-speech
- ⬜ on-device ML / vision hooks (Core ML / ML Kit)

## App & system integration
- ⬜ app lifecycle (foreground/background/terminate) — partially via event seam
- ⬜ permissions framework (request/check, rationale, settings deep-link)
- ⬜ deep links / universal links / app shortcuts / quick actions
- ⬜ system theme / appearance, locale, accessibility settings
- ⬜ device info, app info/version, environment
- ⬜ keyboard, status bar, orientation lock, screen brightness/keep-awake
- ⬜ App Clips / Instant Apps, handoff/continuity

> Each capability ships as a plugin with: a typed Rust API, generated
> platform glue, permission handling, graceful unsupported-platform fallbacks,
> and plugin-conformance tests.
