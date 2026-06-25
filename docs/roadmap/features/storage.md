# Storage & Persistence

Match RN AsyncStorage/MMKV/SQLite/MMKV/SecureStore and Flutter
shared_preferences/sqflite/hive/secure_storage. ⬜ planned.

## Key-value
- ✅ simple async KV store (prefs) — typed
- ⬜ fast synchronous KV (MMKV-style)
- ⬜ namespaced / scoped stores
- ✅ reactive storage (persisted signals)

## Structured / database
- ⬜ SQLite (`rusqlite`) with a typed query layer
- ⬜ migrations (versioned, automatic)
- ⬜ an ORM/query-builder option
- ⬜ embedded document/KV DB option (sled/redb)
- ⬜ full-text search
- ⬜ reactive queries (live results as signals)

## Files & blobs
- ⬜ file system access (app dirs, cache, documents, temp)
- ⬜ streaming read/write, large files
- ⬜ blob/asset storage + image cache integration

## Secure & sensitive
- ⬜ secure storage (Keychain / Keystore)
- ⬜ encryption at rest
- ⬜ biometric-gated storage

## Sync & lifecycle
- ⬜ state restoration (navigation + app state)
- ⬜ hydration for web/SSR
- ⬜ offline-first sync + conflict resolution
- ⬜ backup/restore, export/import
- ⬜ pluggable storage backends
- ⬜ storage inspector in devtools
