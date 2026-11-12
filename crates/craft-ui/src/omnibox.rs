//! Omnibox and address bar URL resolution, search shortcuts, and container badge state.

use craft_core::container::ContainerId;

/// An omnibox navigation target resolved from user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmniboxTarget {
    /// Direct URL navigation (e.g. "https://crates.io" or "craft://settings").
    Url(String),
    /// Search engine query with resolved target URL (e.g. "!g rustlang" -> Google search URL).
    Search {
        engine: String,
        query: String,
        resolved_url: String,
    },
    /// Internal Craft browser schema (e.g. "craft://welcome", "craft://containers").
    Internal(String),
}

/// Address bar UI state for the active tab view.
#[derive(Debug, Clone)]
pub struct OmniboxState {
    pub input_text: String,
    pub is_focused: bool,
    pub active_container: ContainerId,
    pub is_secure: bool,
    pub is_loading: bool,
}

impl OmniboxState {
    pub fn new(initial_url: &str, container_id: ContainerId) -> Self {
        let is_secure = initial_url.starts_with("https://") || initial_url.starts_with("craft://");
        Self {
            input_text: initial_url.to_string(),
            is_focused: false,
            active_container: container_id,
            is_secure,
            is_loading: false,
        }
    }

    /// Resolve an arbitrary user string into a valid navigation target.
    pub fn resolve_input(input: &str) -> OmniboxTarget {
        let trimmed = input.trim();

        // 1. Check for search engine bang shortcuts (e.g. !g, !ddg, !w, !gh)
        if let Some(target) = Self::parse_search_bang(trimmed) {
            return target;
        }

        // 2. Check for craft internal protocol URLs
        if trimmed.starts_with("craft://") || trimmed.starts_with("about:") {
            return OmniboxTarget::Internal(trimmed.to_string());
        }

        // 3. Check for explicit web schemes
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("file://") {
            return OmniboxTarget::Url(trimmed.to_string());
        }

        // 4. Check for typical domain patterns (e.g. "crates.io", "localhost:8080", "sub.domain.org/path")
        if Self::looks_like_url(trimmed) {
            return OmniboxTarget::Url(format!("https://{}", trimmed));
        }

        // 5. Default fallback: DuckDuckGo search query
        let encoded_query = urlencoding_simple(trimmed);
        OmniboxTarget::Search {
            engine: "DuckDuckGo".into(),
            query: trimmed.into(),
            resolved_url: format!("https://duckduckgo.com/?q={}", encoded_query),
        }
    }

    fn parse_search_bang(input: &str) -> Option<OmniboxTarget> {
        let mut parts = input.splitn(2, ' ');
        let bang = parts.next()?;
        let query = parts.next()?.trim();

        if query.is_empty() {
            return None;
        }

        let encoded = urlencoding_simple(query);
        match bang {
            "!g" => Some(OmniboxTarget::Search {
                engine: "Google".into(),
                query: query.into(),
                resolved_url: format!("https://www.google.com/search?q={}", encoded),
            }),
            "!ddg" => Some(OmniboxTarget::Search {
                engine: "DuckDuckGo".into(),
                query: query.into(),
                resolved_url: format!("https://duckduckgo.com/?q={}", encoded),
            }),
            "!w" | "!wiki" => Some(OmniboxTarget::Search {
                engine: "Wikipedia".into(),
                query: query.into(),
                resolved_url: format!("https://en.wikipedia.org/wiki/Special:Search?search={}", encoded),
            }),
            "!gh" => Some(OmniboxTarget::Search {
                engine: "GitHub".into(),
                query: query.into(),
                resolved_url: format!("https://github.com/search?q={}", encoded),
            }),
            "!crates" => Some(OmniboxTarget::Search {
                engine: "Crates.io".into(),
                query: query.into(),
                resolved_url: format!("https://crates.io/search?q={}", encoded),
            }),
            _ => None,
        }
    }

    fn looks_like_url(input: &str) -> bool {
        if input.contains(' ') {
            return false;
        }
        if input.starts_with("localhost") {
            return true;
        }
        if let Some((domain, _)) = input.split_once('/') {
            return domain.contains('.');
        }
        input.contains('.') && !input.ends_with('.')
    }
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_explicit_url() {
        let target = OmniboxState::resolve_input("https://github.com/cmspeedau/CraftBrowsing");
        assert_eq!(
            target,
            OmniboxTarget::Url("https://github.com/cmspeedau/CraftBrowsing".into())
        );
    }

    #[test]
    fn test_resolve_domain_fallback_https() {
        let target = OmniboxState::resolve_input("crates.io");
        assert_eq!(target, OmniboxTarget::Url("https://crates.io".into()));
    }

    #[test]
    fn test_resolve_search_bangs() {
        let target = OmniboxState::resolve_input("!gh servo");
        assert_eq!(
            target,
            OmniboxTarget::Search {
                engine: "GitHub".into(),
                query: "servo".into(),
                resolved_url: "https://github.com/search?q=servo".into(),
            }
        );
    }

    #[test]
    fn test_resolve_default_search() {
        let target = OmniboxState::resolve_input("rust web browser");
        assert_eq!(
            target,
            OmniboxTarget::Search {
                engine: "DuckDuckGo".into(),
                query: "rust web browser".into(),
                resolved_url: "https://duckduckgo.com/?q=rust+web+browser".into(),
            }
        );
    }
}
