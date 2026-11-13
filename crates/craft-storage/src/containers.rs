//! # Multi-Account Containers (UserContextId) storage partitioning
//!
//! Firefox-style container isolation at the storage layer: every cookie jar,
//! origin-storage (localStorage) namespace, and cache entry is partitioned by
//! `container_id` (`UserContextId`). Data written inside one container is
//! invisible to every other container and to the default (`None`) jar — the
//! same isolation model Firefox applies to cookies, cache, and site data.
//!
//! Deleting a container purges every partitioned row that belonged to it,
//! mirroring Firefox's "remove container and its data" semantics.

use rusqlite::{OptionalExtension, Result as SqlResult, params};

use crate::Storage;

/// A user-facing container identity ("Personal", "Work", "Banking", ...).
///
/// `id` is the `UserContextId` used to partition cookie jars, cache, and
/// origin storage. `None` container ids across the storage layer mean the
/// default (uncontained) jar.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Container {
    pub id: i64,
    pub name: String,
    /// UI badge color, e.g. `#4a90d9`.
    pub color_hex: String,
    /// Icon identifier for the container badge.
    pub icon: String,
    pub created_at_epoch_ms: i64,
}

/// One cookie inside a container-partitioned jar.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CookieEntry {
    pub id: i64,
    /// Owning jar; `None` = default jar.
    pub container_id: Option<i64>,
    pub domain: String,
    pub name: String,
    pub value: String,
    pub created_at_epoch_ms: i64,
}

/// One key/value pair of container-partitioned origin storage
/// (e.g. `localStorage`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OriginEntry {
    pub id: i64,
    pub container_id: Option<i64>,
    pub origin: String,
    pub key: String,
    pub value: String,
}

/// One cache entry attributed to a container partition.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    pub id: i64,
    pub container_id: Option<i64>,
    pub url: String,
    pub size_bytes: i64,
    pub cached_at_epoch_ms: i64,
}

impl Storage {
    // -- Container identities ------------------------------------------------

    /// Create a new container identity and return its `UserContextId`.
    pub fn create_container(name: &str, color_hex: &str, icon: &str) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO containers (name, color_hex, icon, created_at_epoch_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![name, color_hex, icon, Self::now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// All container identities, in creation order.
    pub fn list_containers(&self) -> SqlResult<Vec<Container>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color_hex, icon, created_at_epoch_ms
             FROM containers ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Container {
                id: row.get(0)?,
                name: row.get(1)?,
                color_hex: row.get(2)?,
                icon: row.get(3)?,
                created_at_epoch_ms: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Look up a container by `UserContextId`.
    pub fn get_container(&self, id: i64) -> SqlResult<Option<Container>> {
        self.conn
            .query_row(
                "SELECT id, name, color_hex, icon, created_at_epoch_ms
                 FROM containers WHERE id = ?1",
                [id],
                |row| {
                    Ok(Container {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        color_hex: row.get(2)?,
                        icon: row.get(3)?,
                        created_at_epoch_ms: row.get(4)?,
                    })
                },
            )
            .optional()
    }

    /// Update the display fields of a container; returns whether it existed.
    pub fn update_container(
        &self,
        id: i64,
        name: &str,
        color_hex: &str,
        icon: &str,
    ) -> SqlResult<bool> {
        Ok(self.conn.execute(
            "UPDATE containers SET name = ?2, color_hex = ?3, icon = ?4 WHERE id = ?1",
            params![id, name, color_hex, icon],
        )? > 0)
    }

    /// Delete a container and every partitioned cookie, origin-storage entry,
    /// and cache entry that belonged to it. Returns whether it existed.
    pub fn delete_container(&self, id: i64) -> SqlResult<bool> {
        self.conn.execute_batch("BEGIN;")?;
        let res = (|| {
            self.conn
                .execute("DELETE FROM cookies WHERE container_id = ?1", [id])?;
            self.conn
                .execute("DELETE FROM origin_storage WHERE container_id = ?1", [id])?;
            self.conn
                .execute("DELETE FROM cache_entries WHERE container_id = ?1", [id])?;
            Ok(self
                .conn
                .execute("DELETE FROM containers WHERE id = ?1", [id])?
                > 0)
        })();
        match res {
            Ok(deleted) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(deleted)
            }
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    // -- Cookie jar partitioning ----------------------------------------------

    /// Set (or overwrite) a cookie inside the jar of `container_id`
    /// (`None` = default jar). Cookies in one jar never leak into another.
    pub fn set_cookie(
        &self,
        container_id: Option<i64>,
        domain: &str,
        name: &str,
        value: &str,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO cookies (container_id, domain, name, value, created_at_epoch_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(container_id, domain, name)
             DO UPDATE SET value = excluded.value,
                           created_at_epoch_ms = excluded.created_at_epoch_ms",
            params![container_id, domain, name, value, Self::now_ms()],
        )?;
        Ok(())
    }

    /// All cookies in one jar, keyed by container.
    pub fn cookies_for(&self, container_id: Option<i64>) -> SqlResult<Vec<CookieEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, container_id, domain, name, value, created_at_epoch_ms
             FROM cookies WHERE container_id IS ?1 ORDER BY domain, name",
        )?;
        let rows = stmt.query_map([container_id], |row| {
            Ok(CookieEntry {
                id: row.get(0)?,
                container_id: row.get(1)?,
                domain: row.get(2)?,
                name: row.get(3)?,
                value: row.get(4)?,
                created_at_epoch_ms: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    /// Clear cookies of one jar (`Some(id)`) or every jar (`None`).
    pub fn clear_cookies(&self, container_id: Option<i64>) -> SqlResult<usize> {
        self.conn.execute(
            "DELETE FROM cookies WHERE (?1 IS NULL OR container_id = ?1)",
            params![container_id],
        )
    }

    // -- Origin storage partitioning (localStorage) ----------------------------

    /// Set a key in the origin-storage partition of `container_id`.
    pub fn set_origin_storage(
        &self,
        container_id: Option<i64>,
        origin: &str,
        key: &str,
        value: &str,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO origin_storage (container_id, origin, key, value)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(container_id, origin, key)
             DO UPDATE SET value = excluded.value",
            params![container_id, origin, key, value],
        )?;
        Ok(())
    }

    /// Read one key from the origin-storage partition of `container_id`.
    pub fn origin_storage_get(
        &self,
        container_id: Option<i64>,
        origin: &str,
        key: &str,
    ) -> SqlResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM origin_storage
                 WHERE container_id IS ?1 AND origin = ?2 AND key = ?3",
                params![container_id, origin, key],
                |row| row.get(0),
            )
            .optional()
    }

    /// All key/value pairs for one origin inside one container partition.
    pub fn origin_storage_for(
        &self,
        container_id: Option<i64>,
        origin: &str,
    ) -> SqlResult<Vec<OriginEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, container_id, origin, key, value
             FROM origin_storage WHERE container_id IS ?1 AND origin = ?2 ORDER BY key",
        )?;
        let rows = stmt.query_map(params![container_id, origin], |row| {
            Ok(OriginEntry {
                id: row.get(0)?,
                container_id: row.get(1)?,
                origin: row.get(2)?,
                key: row.get(3)?,
                value: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    // -- Cache partitioning ----------------------------------------------------

    /// Record a cache entry attributed to a container partition.
    pub fn record_cache_entry(
        &self,
        container_id: Option<i64>,
        url: &str,
        size_bytes: i64,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO cache_entries (container_id, url, size_bytes, cached_at_epoch_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![container_id, url, size_bytes, Self::now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// All cache entries in one container partition.
    pub fn cache_entries_for(&self, container_id: Option<i64>) -> SqlResult<Vec<CacheEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, container_id, url, size_bytes, cached_at_epoch_ms
             FROM cache_entries WHERE container_id IS ?1 ORDER BY cached_at_epoch_ms DESC",
        )?;
        let rows = stmt.query_map([container_id], |row| {
            Ok(CacheEntry {
                id: row.get(0)?,
                container_id: row.get(1)?,
                url: row.get(2)?,
                size_bytes: row.get(3)?,
                cached_at_epoch_ms: row.get(4)?,
            })
        })?;
        rows.collect()
    }

    /// Clear cache entries of one partition (`Some(id)`) or all partitions (`None`).
    pub fn clear_cache(&self, container_id: Option<i64>) -> SqlResult<usize> {
        self.conn.execute(
            "DELETE FROM cache_entries WHERE (?1 IS NULL OR container_id = ?1)",
            params![container_id],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_crud_and_lookup() {
        let s = Storage::open_in_memory().unwrap();
        let personal = s
            .create_container("Personal", "#4a90d9", "fingerprint")
            .unwrap();
        let work = s.create_container("Work", "#7bc86c", "briefcase").unwrap();
        assert_ne!(personal, work);

        let c = s.get_container(work).unwrap().unwrap();
        assert_eq!(c.name, "Work");
        assert_eq!(c.color_hex, "#7bc86c");

        assert!(
            s.update_container(work, "Employer", "#e0704a", "building")
                .unwrap()
        );
        assert_eq!(s.get_container(work).unwrap().unwrap().name, "Employer");
        assert_eq!(s.list_containers().unwrap().len(), 2);
        assert!(s.get_container(999).unwrap().is_none());
    }

    #[test]
    fn cookie_jars_are_isolated_per_container() {
        let s = Storage::open_in_memory().unwrap();
        let work = s.create_container("Work", "#7bc86c", "briefcase").unwrap();

        // Same (domain, name) cookie in three different jars.
        s.set_cookie(None, "https://shop.example", "session", "default-jar")
            .unwrap();
        s.set_cookie(Some(work), "https://shop.example", "session", "work-jar")
            .unwrap();
        assert!(
            s.cookies_for(None)
                .unwrap()
                .iter()
                .all(|c| c.value != "work-jar")
        );
        assert!(
            s.cookies_for(Some(work))
                .unwrap()
                .iter()
                .all(|c| c.value != "default-jar")
        );

        // Overwrite within one jar only.
        s.set_cookie(Some(work), "https://shop.example", "session", "work-jar-2")
            .unwrap();
        let work_jar = s.cookies_for(Some(work)).unwrap();
        assert_eq!(work_jar.len(), 1);
        assert_eq!(work_jar[0].value, "work-jar-2");

        // Clearing one jar leaves the others intact.
        assert_eq!(s.clear_cookies(Some(work)).unwrap(), 1);
        assert_eq!(s.cookies_for(Some(work)).unwrap().len(), 0);
        assert_eq!(s.cookies_for(None).unwrap().len(), 1);
    }

    #[test]
    fn origin_storage_and_cache_are_partitioned() {
        let s = Storage::open_in_memory().unwrap();
        let banking = s.create_container("Banking", "#8f44ad", "dollar").unwrap();

        // Same origin + key, different partitions.
        s.set_origin_storage(None, "https://portal.example", "theme", "light")
            .unwrap();
        s.set_origin_storage(Some(banking), "https://portal.example", "theme", "dark")
            .unwrap();
        assert_eq!(
            s.origin_storage_get(None, "https://portal.example", "theme")
                .unwrap(),
            Some("light".into())
        );
        assert_eq!(
            s.origin_storage_get(Some(banking), "https://portal.example", "theme")
                .unwrap(),
            Some("dark".into())
        );
        assert_eq!(
            s.origin_storage_for(Some(banking), "https://portal.example")
                .unwrap()
                .len(),
            1
        );

        // Cache entries stay partitioned too.
        s.record_cache_entry(None, "https://cdn.example/app.js", 1024)
            .unwrap();
        s.record_cache_entry(Some(banking), "https://cdn.example/app.js", 4096)
            .unwrap();
        assert_eq!(s.cache_entries_for(None).unwrap()[0].size_bytes, 1024);
        assert_eq!(
            s.cache_entries_for(Some(banking)).unwrap()[0].size_bytes,
            4096
        );
        assert_eq!(s.clear_cache(Some(banking)).unwrap(), 1);
        assert!(s.cache_entries_for(Some(banking)).unwrap().is_empty());
        assert_eq!(s.cache_entries_for(None).unwrap().len(), 1);
    }

    #[test]
    fn deleting_a_container_purges_its_partitioned_data() {
        let s = Storage::open_in_memory().unwrap();
        let shopping = s.create_container("Shopping", "#e0704a", "cart").unwrap();

        s.set_cookie(Some(shopping), "https://store.example", "cart", "abc")
            .unwrap();
        s.set_origin_storage(Some(shopping), "https://store.example", "prefs", "x")
            .unwrap();
        s.record_cache_entry(Some(shopping), "https://store.example/a.png", 2048)
            .unwrap();
        // Data in other partitions must survive.
        s.set_cookie(None, "https://store.example", "cart", "default")
            .unwrap();

        assert!(s.delete_container(shopping).unwrap());
        assert!(s.get_container(shopping).unwrap().is_none());
        assert!(s.cookies_for(Some(shopping)).unwrap().is_empty());
        assert_eq!(
            s.origin_storage_get(Some(shopping), "https://store.example", "prefs")
                .unwrap(),
            None
        );
        assert!(s.cache_entries_for(Some(shopping)).unwrap().is_empty());
        // Default jar untouched.
        assert_eq!(s.cookies_for(None).unwrap().len(), 1);
        assert!(!s.delete_container(shopping).unwrap());
    }
}
