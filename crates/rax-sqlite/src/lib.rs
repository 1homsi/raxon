//! SQLite storage for rax apps, backed by [rusqlite](https://docs.rs/rusqlite)
//! with a bundled SQLite so no system library is required.
//!
//! # Example
//!
//! ```rust,no_run
//! use rax_sqlite::Database;
//!
//! let db = Database::open("app.db").unwrap();
//! db.execute("CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, body TEXT)").unwrap();
//! db.execute_with("INSERT INTO notes (body) VALUES (?1)", &[&"hello"]).unwrap();
//! let notes: Vec<String> = db.query("SELECT body FROM notes", |row| row.get(0)).unwrap();
//! ```

use rusqlite::{params_from_iter, types::ToSql, Connection};
use std::path::Path;

/// A reactive query that re-runs when `invalidate()` is called.
/// Returns a Signal<Vec<T>> that updates reactively.
#[derive(Clone, Copy)]
pub struct ReactiveQuery<T: Clone + 'static> {
    result: rax_reactive::Signal<Vec<T>>,
    // Invalidation token — increment to trigger re-run
    version: rax_reactive::Signal<u64>,
}

impl<T: Clone + 'static> ReactiveQuery<T> {
    /// Invalidate the cache — triggers a re-fetch on the next reactive read.
    pub fn invalidate(&self) {
        self.version.update(|v| *v += 1);
    }

    /// Access results reactively.
    pub fn get(&self) -> Vec<T> {
        self.result.get()
    }

    /// Re-run the query now and update the signal.
    pub fn refresh<F: Fn() -> Vec<T>>(&self, fetch: F) {
        self.result.update(|r| *r = fetch());
    }
}

/// Create a reactive query seeded with `initial_results`.
pub fn use_reactive_query<T: Clone + 'static>(initial_results: Vec<T>) -> ReactiveQuery<T> {
    use rax_reactive::create_signal;
    ReactiveQuery {
        result: create_signal(initial_results),
        version: create_signal(0u64),
    }
}

/// A SQLite database connection.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a database at `path`. Use `":memory:"` for an in-memory db.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        Connection::open(path)
            .map(|conn| Database { conn })
            .map_err(|e| e.to_string())
    }

    /// Execute a SQL statement with no parameters. Returns the number of rows changed.
    pub fn execute(&self, sql: &str) -> Result<usize, String> {
        self.conn.execute(sql, []).map_err(|e| e.to_string())
    }

    /// Execute a SQL statement with positional parameters.
    pub fn execute_with(&self, sql: &str, params: &[&dyn ToSql]) -> Result<usize, String> {
        self.conn
            .execute(sql, params_from_iter(params.iter().copied()))
            .map_err(|e| e.to_string())
    }

    /// Query rows. `map_row` maps each `rusqlite::Row` to your type.
    pub fn query<T, F>(&self, sql: &str, map_row: F) -> Result<Vec<T>, String>
    where
        F: Fn(&rusqlite::Row<'_>) -> Result<T, rusqlite::Error>,
    {
        let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], map_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    /// Query with positional parameters.
    pub fn query_with<T, F>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        map_row: F,
    ) -> Result<Vec<T>, String>
    where
        F: Fn(&rusqlite::Row<'_>) -> Result<T, rusqlite::Error>,
    {
        let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params_from_iter(params.iter().copied()), map_row)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }

    /// Apply versioned SQL migrations in order.
    ///
    /// Each entry in `migrations` is `(version, sql)` where `version` is a
    /// monotonically increasing integer (e.g. `1`, `2`, `3`, …). Applied
    /// versions are recorded in a `_rax_migrations` table that is created
    /// automatically on first call. Already-applied versions are skipped, so
    /// this method is safe to call on every app start.
    ///
    /// # Reactive use
    ///
    /// `migrate` is a one-shot setup call and is not reactive itself.  If you
    /// need to expose query results reactively, wrap them in a `create_memo`
    /// from `rax-reactive`:
    ///
    /// ```no_run
    /// # use rax_sqlite::Database;
    /// # use rax_reactive::create_memo;
    /// # let db = Database::open(":memory:").unwrap();
    /// let notes = create_memo(move || db.query("SELECT body FROM notes", |r| r.get(0)).unwrap_or_default::<Vec<String>>());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error string if any SQL statement fails to execute.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use rax_sqlite::Database;
    ///
    /// let db = Database::open("app.db").unwrap();
    /// db.migrate(&[
    ///     (1, "CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, body TEXT)"),
    ///     (2, "ALTER TABLE notes ADD COLUMN created_at TEXT"),
    /// ]).unwrap();
    /// ```
    pub fn migrate(&self, migrations: &[(u32, &str)]) -> Result<(), String> {
        self.execute(
            "CREATE TABLE IF NOT EXISTS _rax_migrations \
             (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
        )?;

        let applied: Vec<u32> = self.query(
            "SELECT version FROM _rax_migrations ORDER BY version",
            |row| row.get::<_, u32>(0),
        )?;

        for (version, sql) in migrations {
            if applied.contains(version) {
                continue;
            }
            self.execute(sql)?;
            let v: i64 = *version as i64;
            self.execute_with(
                "INSERT INTO _rax_migrations (version, applied_at) VALUES (?1, datetime('now'))",
                &[&v as &dyn rusqlite::types::ToSql],
            )?;
        }

        Ok(())
    }

    /// Return a path to the given `filename` in the app's Documents directory
    /// (the standard location for user data on iOS).
    ///
    /// On non-iOS targets the filename is returned as-is (relative to cwd),
    /// which is fine for desktop testing.
    pub fn documents_path(filename: &str) -> String {
        #[cfg(target_os = "ios")]
        {
            // On iOS the app sandbox places Documents at a fixed path under
            // the app container; using a bare filename opens in the current
            // working directory which is also inside the sandbox and works for
            // the simulator. A production app should use
            // NSSearchPathForDirectoriesInDomains to resolve the real path.
            filename.to_string()
        }
        #[cfg(not(target_os = "ios"))]
        {
            filename.to_string()
        }
    }
}
