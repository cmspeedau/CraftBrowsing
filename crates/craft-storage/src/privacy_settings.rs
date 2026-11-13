//! Persistent Firefox-style privacy preferences and browsing-data cleanup.
use rusqlite::{OptionalExtension, Result as SqlResult, params};
use serde::{Deserialize, Serialize};

use crate::Storage;

/// Effective privacy and security preferences. Missing database keys use these defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub tracking_protection_enabled: bool,
    pub tracking_protection_strict: bool,
    pub do_not_track: bool,
    pub clear_on_exit: bool,
    pub clear_cookies_on_exit: bool,
    pub clear_cache_on_exit: bool,
    pub clear_history_on_exit: bool,
    pub block_third_party_cookies: bool,
    pub block_third_party_storage: bool,
    pub save_browsing_history: bool,
    pub remember_download_history: bool,
    pub safe_browsing_enabled: bool,
    pub https_only_mode: bool,
    pub private_browsing_default: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            tracking_protection_enabled: true,
            tracking_protection_strict: false,
            do_not_track: false,
            clear_on_exit: false,
            clear_cookies_on_exit: false,
            clear_cache_on_exit: false,
            clear_history_on_exit: false,
            block_third_party_cookies: true,
            block_third_party_storage: true,
            save_browsing_history: true,
            remember_download_history: true,
            safe_browsing_enabled: true,
            https_only_mode: false,
            private_browsing_default: false,
        }
    }
}

/// Selects data categories and an optional container partition to clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClearDataOptions {
    pub cookies: bool,
    pub cache: bool,
    pub history: bool,
    /// `None` clears all containers, including the default partition.
    pub container_id: Option<i64>,
}

/// Counts of rows removed by a clear operation; `cleared_at_epoch_ms` is the supplied `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClearDataResult {
    pub cookies: usize,
    pub cache: usize,
    pub history: usize,
    pub cleared_at_epoch_ms: i64,
}

impl Storage {
    /// Return all known privacy preferences, supplying defaults for unset keys.
    pub fn get_privacy_settings(&self) -> SqlResult<PrivacySettings> {
        let mut settings = PrivacySettings::default();
        macro_rules! read {
            ($field:ident) => {{
                if let Some(value) = self.privacy_setting_value(stringify!($field))? {
                    settings.$field = parse_bool(stringify!($field), &value)?;
                }
            }};
        }
        read!(tracking_protection_enabled);
        read!(tracking_protection_strict);
        read!(do_not_track);
        read!(clear_on_exit);
        read!(clear_cookies_on_exit);
        read!(clear_cache_on_exit);
        read!(clear_history_on_exit);
        read!(block_third_party_cookies);
        read!(block_third_party_storage);
        read!(save_browsing_history);
        read!(remember_download_history);
        read!(safe_browsing_enabled);
        read!(https_only_mode);
        read!(private_browsing_default);
        Ok(settings)
    }

    /// Persist or replace a privacy setting. Values are stored verbatim as text.
    pub fn set_privacy_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO privacy_settings(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Remove persisted overrides, restoring effective settings to their defaults.
    pub fn reset_to_defaults(&self) -> SqlResult<()> {
        self.conn.execute("DELETE FROM privacy_settings", [])?;
        Ok(())
    }

    /// Clear selected categories in one optional container. `now` is recorded for the caller;
    /// the current storage API clears all matching history rather than applying a time range.
    pub fn clear_browsing_data(
        &self,
        now: i64,
        options: ClearDataOptions,
    ) -> SqlResult<ClearDataResult> {
        Ok(ClearDataResult {
            cookies: if options.cookies {
                self.clear_cookies(options.container_id)?
            } else {
                0
            },
            cache: if options.cache {
                self.clear_cache(options.container_id)?
            } else {
                0
            },
            history: if options.history {
                self.clear_history(options.container_id)?
            } else {
                0
            },
            cleared_at_epoch_ms: now,
        })
    }

    /// Apply configured clear-on-exit selections; no-op if that preference is disabled.
    pub fn apply_shutdown_cleanup(&self) -> SqlResult<ClearDataResult> {
        let settings = self.get_privacy_settings()?;
        if !settings.clear_on_exit {
            return Ok(ClearDataResult {
                cleared_at_epoch_ms: Self::now_ms(),
                ..ClearDataResult::default()
            });
        }
        self.clear_browsing_data(
            Self::now_ms(),
            ClearDataOptions {
                cookies: settings.clear_cookies_on_exit,
                cache: settings.clear_cache_on_exit,
                history: settings.clear_history_on_exit,
                container_id: None,
            },
        )
    }

    fn privacy_setting_value(&self, key: &str) -> SqlResult<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM privacy_settings WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
    }
}

fn parse_bool(key: &str, value: &str) -> SqlResult<bool> {
    match value {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(rusqlite::Error::InvalidParameterName(format!(
            "invalid boolean for privacy setting {key}: {value}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    #[test]
    fn defaults_round_trip() {
        let s = storage();
        assert_eq!(
            s.get_privacy_settings().unwrap(),
            PrivacySettings::default()
        );
        let settings = s.get_privacy_settings().unwrap();
        s.set_privacy_setting("tracking_protection_enabled", "false")
            .unwrap();
        s.set_privacy_setting("https_only_mode", "true").unwrap();
        let updated = s.get_privacy_settings().unwrap();
        assert!(!updated.tracking_protection_enabled);
        assert!(updated.https_only_mode);
        s.set_privacy_setting("tracking_protection_enabled", "true")
            .unwrap();
        assert!(
            s.get_privacy_settings()
                .unwrap()
                .tracking_protection_enabled
        );
        assert!(settings.block_third_party_cookies);
    }

    #[test]
    fn individual_setting_persists_and_unknown_keys_do_not_affect_defaults() {
        let s = storage();
        s.set_privacy_setting("do_not_track", "true").unwrap();
        s.set_privacy_setting("future_option", "opaque").unwrap();
        assert!(s.get_privacy_settings().unwrap().do_not_track);
    }

    #[test]
    fn reset_to_defaults_removes_overrides() {
        let s = storage();
        s.set_privacy_setting("do_not_track", "true").unwrap();
        s.set_privacy_setting("https_only_mode", "true").unwrap();
        s.reset_to_defaults().unwrap();
        assert_eq!(
            s.get_privacy_settings().unwrap(),
            PrivacySettings::default()
        );
    }

    #[test]
    fn clear_browsing_data_wipes_history_cookies_and_cache() {
        let s = storage();
        s.record_history_visit("https://example.test", "Example", None)
            .unwrap();
        s.set_cookie(None, "example.test", "session", "secret")
            .unwrap();
        s.record_cache_entry(None, "https://example.test/a", 10)
            .unwrap();
        let result = s
            .clear_browsing_data(
                1234,
                ClearDataOptions {
                    cookies: true,
                    cache: true,
                    history: true,
                    container_id: None,
                },
            )
            .unwrap();
        assert_eq!(
            result,
            ClearDataResult {
                cookies: 1,
                cache: 1,
                history: 1,
                cleared_at_epoch_ms: 1234
            }
        );
        assert!(s.recent_history(10, None).unwrap().is_empty());
        assert!(s.cookies_for(None).unwrap().is_empty());
        assert!(s.cache_entries_for(None).unwrap().is_empty());
    }

    #[test]
    fn shutdown_cleanup_only_clears_selected_categories() {
        let s = storage();
        s.record_history_visit("https://example.test", "Example", None)
            .unwrap();
        s.set_cookie(None, "example.test", "session", "secret")
            .unwrap();
        s.record_cache_entry(None, "https://example.test/a", 10)
            .unwrap();
        s.set_privacy_setting("clear_on_exit", "true").unwrap();
        s.set_privacy_setting("clear_cookies_on_exit", "true")
            .unwrap();
        s.set_privacy_setting("clear_cache_on_exit", "false")
            .unwrap();
        s.set_privacy_setting("clear_history_on_exit", "true")
            .unwrap();
        let result = s.apply_shutdown_cleanup().unwrap();
        assert_eq!((result.cookies, result.cache, result.history), (1, 0, 1));
        assert!(s.cookies_for(None).unwrap().is_empty());
        assert_eq!(s.cache_entries_for(None).unwrap().len(), 1);
        assert!(s.recent_history(10, None).unwrap().is_empty());
    }
}
