//! Media session model for the Global Media Hub.
//!
//! A `MediaSession` is one audio or video element producing output in a tab.
//! The engine (Servo) is the source of truth for these sessions; this module
//! only holds the browser-side view and the actions the user can take on them.

use crate::container::ContainerId;
use crate::tab::TabId;
use serde::{Deserialize, Serialize};

/// Unique id of a media session within the browser process.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MediaSessionId(pub String);

impl MediaSessionId {
    pub fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self(format!("media-{:x}", nanos))
    }
}

impl Default for MediaSessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Playback state reported by the media element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaPlaybackState {
    #[default]
    Paused,
    Playing,
    Buffering,
    Ended,
    Errored,
}

/// Whether the session is audio-only or carries video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MediaKind {
    #[default]
    Audio,
    Video,
}

/// One media element producing output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaSession {
    pub id: MediaSessionId,
    pub tab_id: TabId,
    pub container_id: ContainerId,
    pub title: String,
    pub origin: String,
    pub kind: MediaKind,
    pub state: MediaPlaybackState,
    /// Current playback position in milliseconds.
    pub position_ms: u64,
    /// Total duration in milliseconds; `None` for live streams.
    pub duration_ms: Option<u64>,
    /// Volume in the range 0.0..=1.0.
    pub volume: f32,
    pub muted: bool,
    pub playback_rate: f32,
    pub picture_in_picture: bool,
}

impl MediaSession {
    pub fn new(
        id: MediaSessionId,
        tab_id: TabId,
        container_id: ContainerId,
        title: impl Into<String>,
        origin: impl Into<String>,
        kind: MediaKind,
    ) -> Self {
        Self {
            id,
            tab_id,
            container_id,
            title: title.into(),
            origin: origin.into(),
            kind,
            state: MediaPlaybackState::Paused,
            position_ms: 0,
            duration_ms: None,
            volume: 1.0,
            muted: false,
            playback_rate: 1.0,
            picture_in_picture: false,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.state == MediaPlaybackState::Playing
    }

    pub fn is_live(&self) -> bool {
        self.duration_ms.is_none()
    }

    pub fn display_title(&self) -> String {
        if self.title.is_empty() {
            self.origin.clone()
        } else {
            self.title.clone()
        }
    }
}

/// Events reported by the engine for a media session.
#[derive(Debug, Clone)]
pub enum MediaSessionEvent {
    Created(MediaSession),
    Removed(MediaSessionId),
    PlaybackStateChanged {
        id: MediaSessionId,
        state: MediaPlaybackState,
    },
    PositionChanged {
        id: MediaSessionId,
        position_ms: u64,
    },
    DurationChanged {
        id: MediaSessionId,
        duration_ms: Option<u64>,
    },
    VolumeChanged {
        id: MediaSessionId,
        volume: f32,
        muted: bool,
    },
    RateChanged {
        id: MediaSessionId,
        rate: f32,
    },
    TitleChanged {
        id: MediaSessionId,
        title: String,
    },
    PipChanged {
        id: MediaSessionId,
        active: bool,
    },
}

/// Actions the user (or a shortcut) can request on a session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaAction {
    Play,
    Pause,
    TogglePlayPause,
    Stop,
    SeekRelative(i64),
    SeekTo(u64),
    SkipForward(u64),
    SkipBackward(u64),
    SetVolume(f32),
    SetMuted(bool),
    ToggleMute,
    SetRate(f32),
    EnterPip,
    ExitPip,
    NextTrack,
    PreviousTrack,
}

/// Browser-side store of all known media sessions.
#[derive(Debug, Default)]
pub struct MediaHub {
    pub sessions: Vec<MediaSession>,
    pub pip_sessions: Vec<MediaSessionId>,
}

impl MediaHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn upsert(&mut self, session: MediaSession) {
        if let Some(existing) = self.sessions.iter_mut().find(|s| s.id == session.id) {
            *existing = session;
        } else {
            self.sessions.push(session);
        }
    }

    pub fn remove(&mut self, id: &MediaSessionId) -> Option<MediaSession> {
        self.pip_sessions.retain(|p| p != id);
        let pos = self.sessions.iter().position(|s| &s.id == id)?;
        Some(self.sessions.remove(pos))
    }

    pub fn get(&self, id: &MediaSessionId) -> Option<&MediaSession> {
        self.sessions.iter().find(|s| &s.id == id)
    }

    pub fn get_mut(&mut self, id: &MediaSessionId) -> Option<&mut MediaSession> {
        self.sessions.iter_mut().find(|s| &s.id == id)
    }

    pub fn sessions_for_tab(&self, tab_id: &TabId) -> Vec<&MediaSession> {
        self.sessions
            .iter()
            .filter(|s| &s.tab_id == tab_id)
            .collect()
    }

    pub fn playing_sessions(&self) -> Vec<&MediaSession> {
        self.sessions.iter().filter(|s| s.is_playing()).collect()
    }

    /// The session that should receive global play/pause and seek commands:
    /// the most recently playing one, otherwise the first known session.
    pub fn active_session(&self) -> Option<&MediaSession> {
        self.sessions
            .iter()
            .rev()
            .find(|s| s.is_playing())
            .or_else(|| self.sessions.first())
    }

    pub fn active_session_id(&self) -> Option<MediaSessionId> {
        self.active_session().map(|s| s.id.clone())
    }

    pub fn apply_event(&mut self, event: MediaSessionEvent) {
        match event {
            MediaSessionEvent::Created(session) => self.upsert(session),
            MediaSessionEvent::Removed(id) => {
                self.remove(&id);
            }
            MediaSessionEvent::PlaybackStateChanged { id, state } => {
                if let Some(s) = self.get_mut(&id) {
                    s.state = state;
                }
            }
            MediaSessionEvent::PositionChanged { id, position_ms } => {
                if let Some(s) = self.get_mut(&id) {
                    s.position_ms = position_ms;
                }
            }
            MediaSessionEvent::DurationChanged { id, duration_ms } => {
                if let Some(s) = self.get_mut(&id) {
                    s.duration_ms = duration_ms;
                }
            }
            MediaSessionEvent::VolumeChanged { id, volume, muted } => {
                if let Some(s) = self.get_mut(&id) {
                    s.volume = volume.clamp(0.0, 1.0);
                    s.muted = muted;
                }
            }
            MediaSessionEvent::RateChanged { id, rate } => {
                if let Some(s) = self.get_mut(&id) {
                    s.playback_rate = rate;
                }
            }
            MediaSessionEvent::TitleChanged { id, title } => {
                if let Some(s) = self.get_mut(&id) {
                    s.title = title;
                }
            }
            MediaSessionEvent::PipChanged { id, active } => {
                if let Some(s) = self.get_mut(&id) {
                    s.picture_in_picture = active;
                }
                if active {
                    if !self.pip_sessions.contains(&id) {
                        self.pip_sessions.push(id);
                    }
                } else {
                    self.pip_sessions.retain(|p| p != &id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerId;
    use crate::tab::TabId;

    fn session(id: &str, playing: bool) -> MediaSession {
        let mut s = MediaSession::new(
            MediaSessionId(id.to_string()),
            TabId("tab-1".to_string()),
            ContainerId::default_container(),
            "Test Video",
            "example.com",
            MediaKind::Video,
        );
        if playing {
            s.state = MediaPlaybackState::Playing;
        }
        s
    }

    #[test]
    fn upsert_replaces_existing_session() {
        let mut hub = MediaHub::new();
        hub.upsert(session("a", false));
        let mut updated = session("a", true);
        updated.title = "Renamed".to_string();
        hub.upsert(updated);
        assert_eq!(hub.len(), 1);
        assert!(
            hub.get(&MediaSessionId("a".to_string()))
                .unwrap()
                .is_playing()
        );
        assert_eq!(
            hub.get(&MediaSessionId("a".to_string())).unwrap().title,
            "Renamed"
        );
    }

    #[test]
    fn remove_cleans_up_pip() {
        let mut hub = MediaHub::new();
        hub.upsert(session("a", true));
        hub.apply_event(MediaSessionEvent::PipChanged {
            id: MediaSessionId("a".to_string()),
            active: true,
        });
        assert_eq!(hub.pip_sessions.len(), 1);
        hub.remove(&MediaSessionId("a".to_string()));
        assert!(hub.is_empty());
        assert!(hub.pip_sessions.is_empty());
    }

    #[test]
    fn apply_event_updates_fields() {
        let mut hub = MediaHub::new();
        hub.upsert(session("a", false));
        hub.apply_event(MediaSessionEvent::PlaybackStateChanged {
            id: MediaSessionId("a".to_string()),
            state: MediaPlaybackState::Playing,
        });
        hub.apply_event(MediaSessionEvent::PositionChanged {
            id: MediaSessionId("a".to_string()),
            position_ms: 42_000,
        });
        hub.apply_event(MediaSessionEvent::DurationChanged {
            id: MediaSessionId("a".to_string()),
            duration_ms: Some(120_000),
        });
        hub.apply_event(MediaSessionEvent::VolumeChanged {
            id: MediaSessionId("a".to_string()),
            volume: 0.6,
            muted: true,
        });
        hub.apply_event(MediaSessionEvent::RateChanged {
            id: MediaSessionId("a".to_string()),
            rate: 1.5,
        });
        hub.apply_event(MediaSessionEvent::TitleChanged {
            id: MediaSessionId("a".to_string()),
            title: "New Title".to_string(),
        });
        let s = hub.get(&MediaSessionId("a".to_string())).unwrap();
        assert!(s.is_playing());
        assert_eq!(s.position_ms, 42_000);
        assert_eq!(s.duration_ms, Some(120_000));
        assert!((s.volume - 0.6).abs() < f32::EPSILON);
        assert!(s.muted);
        assert!((s.playback_rate - 1.5).abs() < f32::EPSILON);
        assert_eq!(s.title, "New Title");
    }

    #[test]
    fn active_session_prefers_playing() {
        let mut hub = MediaHub::new();
        hub.upsert(session("a", false));
        hub.upsert(session("b", true));
        assert_eq!(
            hub.active_session().unwrap().id,
            MediaSessionId("b".to_string())
        );
    }

    #[test]
    fn sessions_for_tab_filters() {
        let mut hub = MediaHub::new();
        let mut s2 = session("b", false);
        s2.tab_id = TabId("tab-2".to_string());
        hub.upsert(session("a", false));
        hub.upsert(s2);
        assert_eq!(hub.sessions_for_tab(&TabId("tab-1".to_string())).len(), 1);
        assert_eq!(hub.sessions_for_tab(&TabId("tab-2".to_string())).len(), 1);
    }

    #[test]
    fn volume_clamps() {
        let mut hub = MediaHub::new();
        hub.upsert(session("a", false));
        hub.apply_event(MediaSessionEvent::VolumeChanged {
            id: MediaSessionId("a".to_string()),
            volume: 2.0,
            muted: false,
        });
        assert!(
            (hub.get(&MediaSessionId("a".to_string())).unwrap().volume - 1.0).abs() < f32::EPSILON
        );
    }
}
