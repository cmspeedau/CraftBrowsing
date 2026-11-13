//! Per-origin site permissions with Prompt as the implicit default.

use rusqlite::{OptionalExtension, Result as SqlResult, params};
use serde::{Deserialize, Serialize};

use crate::Storage;

/// Browser capabilities that can be granted, blocked, or prompted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionType {
    Camera,
    Microphone,
    Location,
    Geolocation,
    Notifications,
    Clipboard,
    Popup,
    Autoplay,
    Midi,
    Usb,
    Serial,
}

impl PermissionType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::Location => "location",
            Self::Geolocation => "geolocation",
            Self::Notifications => "notifications",
            Self::Clipboard => "clipboard",
            Self::Popup => "popup",
            Self::Autoplay => "autoplay",
            Self::Midi => "midi",
            Self::Usb => "usb",
            Self::Serial => "serial",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "camera" => Some(Self::Camera),
            "microphone" => Some(Self::Microphone),
            "location" => Some(Self::Location),
            "geolocation" => Some(Self::Geolocation),
            "notifications" => Some(Self::Notifications),
            "clipboard" => Some(Self::Clipboard),
            "popup" => Some(Self::Popup),
            "autoplay" => Some(Self::Autoplay),
            "midi" => Some(Self::Midi),
            "usb" => Some(Self::Usb),
            "serial" => Some(Self::Serial),
            _ => None,
        }
    }
}

/// Persisted/effective decision for a site permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionValue {
    Allow,
    Block,
    Prompt,
}

impl PermissionValue {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Block => "block",
            Self::Prompt => "prompt",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Self::Allow),
            "block" => Some(Self::Block),
            "prompt" => Some(Self::Prompt),
            _ => None,
        }
    }
}

/// One explicit origin/permission setting, suitable for a settings UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SitePermission {
    pub origin: String,
    pub permission: PermissionType,
    pub value: PermissionValue,
}

impl Storage {
    /// Return the site's configured decision, or Prompt if no setting exists.
    pub fn query_permission(
        &self,
        origin: &str,
        kind: PermissionType,
    ) -> SqlResult<PermissionValue> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM site_permissions WHERE origin = ?1 AND permission = ?2",
                params![origin, kind.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match raw {
            None => Ok(PermissionValue::Prompt),
            Some(value) => PermissionValue::parse(&value).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    format!("invalid stored permission value: {value}").into(),
                )
            }),
        }
    }

    /// Set (or replace) an explicit decision for an origin and permission.
    pub fn set_permission(
        &self,
        origin: &str,
        kind: PermissionType,
        value: PermissionValue,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO site_permissions (origin, permission, value) VALUES (?1, ?2, ?3)
             ON CONFLICT(origin, permission) DO UPDATE SET value = excluded.value",
            params![origin, kind.as_str(), value.as_str()],
        )?;
        Ok(())
    }

    /// Remove every explicit permission decision for an origin.
    pub fn reset_for_origin(&self, origin: &str) -> SqlResult<usize> {
        self.conn.execute(
            "DELETE FROM site_permissions WHERE origin = ?1",
            params![origin],
        )
    }

    /// Remove all explicit site permission decisions.
    pub fn clear_all_permissions(&self) -> SqlResult<usize> {
        self.conn.execute("DELETE FROM site_permissions", [])
    }

    /// List explicit decisions in stable origin/permission order.
    pub fn list_permissions(&self) -> SqlResult<Vec<SitePermission>> {
        let mut stmt = self.conn.prepare(
            "SELECT origin, permission, value FROM site_permissions
             ORDER BY origin COLLATE NOCASE, permission",
        )?;
        let rows = stmt.query_map([], |row| {
            let permission: String = row.get(1)?;
            let value: String = row.get(2)?;
            let permission = PermissionType::parse(&permission).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    format!("invalid stored permission type: {permission}").into(),
                )
            })?;
            let value = PermissionValue::parse(&value).ok_or_else(|| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    format!("invalid stored permission value: {value}").into(),
                )
            })?;
            Ok(SitePermission {
                origin: row.get(0)?,
                permission,
                value,
            })
        })?;
        rows.collect()
    }

    /// True only when the effective policy is Prompt.
    pub fn should_show_permission_prompt(
        &self,
        origin: &str,
        kind: PermissionType,
    ) -> SqlResult<bool> {
        Ok(self.query_permission(origin, kind)? == PermissionValue::Prompt)
    }

    /// Record a prompt response. If `remember` is false, the response is
    /// returned for this request only and the stored policy is left unchanged.
    pub fn record_prompt_answer(
        &self,
        origin: &str,
        kind: PermissionType,
        answer: PermissionValue,
        remember: bool,
    ) -> SqlResult<PermissionValue> {
        if answer == PermissionValue::Prompt {
            return Err(rusqlite::Error::InvalidParameterName(
                "a prompt answer must be Allow or Block".into(),
            ));
        }
        if remember {
            self.set_permission(origin, kind, answer)?;
        }
        Ok(answer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    #[test]
    fn missing_permission_defaults_to_prompt_and_requests_prompt() {
        let s = storage();
        assert_eq!(
            s.query_permission("https://example.com", PermissionType::Camera)
                .unwrap(),
            PermissionValue::Prompt
        );
        assert!(
            s.should_show_permission_prompt("https://example.com", PermissionType::Camera)
                .unwrap()
        );
    }

    #[test]
    fn set_query_and_upsert_permission() {
        let s = storage();
        s.set_permission(
            "https://example.com",
            PermissionType::Camera,
            PermissionValue::Allow,
        )
        .unwrap();
        assert_eq!(
            s.query_permission("https://example.com", PermissionType::Camera)
                .unwrap(),
            PermissionValue::Allow
        );
        s.set_permission(
            "https://example.com",
            PermissionType::Camera,
            PermissionValue::Block,
        )
        .unwrap();
        assert_eq!(
            s.query_permission("https://example.com", PermissionType::Camera)
                .unwrap(),
            PermissionValue::Block
        );
        assert!(
            !s.should_show_permission_prompt("https://example.com", PermissionType::Camera)
                .unwrap()
        );
    }

    #[test]
    fn permission_settings_are_isolated_by_origin_and_kind() {
        let s = storage();
        s.set_permission(
            "https://one.test",
            PermissionType::Location,
            PermissionValue::Allow,
        )
        .unwrap();
        assert_eq!(
            s.query_permission("https://two.test", PermissionType::Location)
                .unwrap(),
            PermissionValue::Prompt
        );
        assert_eq!(
            s.query_permission("https://one.test", PermissionType::Microphone)
                .unwrap(),
            PermissionValue::Prompt
        );
    }

    #[test]
    fn list_reset_origin_and_clear_all_permissions() {
        let s = storage();
        s.set_permission(
            "https://one.test",
            PermissionType::Camera,
            PermissionValue::Allow,
        )
        .unwrap();
        s.set_permission(
            "https://one.test",
            PermissionType::Popup,
            PermissionValue::Block,
        )
        .unwrap();
        s.set_permission(
            "https://two.test",
            PermissionType::Camera,
            PermissionValue::Block,
        )
        .unwrap();
        assert_eq!(s.list_permissions().unwrap().len(), 3);
        assert_eq!(s.reset_for_origin("https://one.test").unwrap(), 2);
        assert_eq!(
            s.query_permission("https://one.test", PermissionType::Camera)
                .unwrap(),
            PermissionValue::Prompt
        );
        assert_eq!(s.clear_all_permissions().unwrap(), 1);
        assert!(s.list_permissions().unwrap().is_empty());
    }

    #[test]
    fn prompt_answers_are_remembered_only_when_requested() {
        let s = storage();
        assert_eq!(
            s.record_prompt_answer(
                "https://example.com",
                PermissionType::Notifications,
                PermissionValue::Block,
                false
            )
            .unwrap(),
            PermissionValue::Block
        );
        assert_eq!(
            s.query_permission("https://example.com", PermissionType::Notifications)
                .unwrap(),
            PermissionValue::Prompt
        );
        s.record_prompt_answer(
            "https://example.com",
            PermissionType::Notifications,
            PermissionValue::Allow,
            true,
        )
        .unwrap();
        assert_eq!(
            s.query_permission("https://example.com", PermissionType::Notifications)
                .unwrap(),
            PermissionValue::Allow
        );
        assert!(
            s.record_prompt_answer(
                "https://example.com",
                PermissionType::Notifications,
                PermissionValue::Prompt,
                true
            )
            .is_err()
        );
    }
}
