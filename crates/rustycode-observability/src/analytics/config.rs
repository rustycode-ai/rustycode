use serde::{Deserialize, Serialize};

/// Default GA4 Measurement Protocol endpoint.
const GA4_ENDPOINT: &str = "https://www.google-analytics.com/mp/collect";

/// Default GA4 debug endpoint (validates payload without recording).
const GA4_DEBUG_ENDPOINT: &str = "https://www.google-analytics.com/debug/mp/collect";

/// Default Measurement ID baked into the binary.
/// Users can override via config.
pub const DEFAULT_MEASUREMENT_ID: &str = "G-PLACEHOLDER";

/// Analytics configuration for GA4 integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsConfig {
    /// Enable analytics (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// GA4 Measurement ID (e.g., "G-XXXXXXXXXX").
    #[serde(default = "default_measurement_id")]
    pub measurement_id: String,

    /// GA4 API Secret for Measurement Protocol.
    #[serde(default)]
    pub api_secret: String,

    /// Custom endpoint override (for testing).
    #[serde(default = "default_endpoint")]
    pub endpoint: String,

    /// Use GA4 debug endpoint (validates but doesn't record).
    #[serde(default)]
    pub debug: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_measurement_id() -> String {
    DEFAULT_MEASUREMENT_ID.to_string()
}

fn default_endpoint() -> String {
    GA4_ENDPOINT.to_string()
}

impl Default for AnalyticsConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            measurement_id: default_measurement_id(),
            api_secret: String::new(),
            endpoint: default_endpoint(),
            debug: false,
        }
    }
}

impl AnalyticsConfig {
    /// Return the active endpoint (production or debug).
    pub fn active_endpoint(&self) -> &str {
        if self.debug {
            GA4_DEBUG_ENDPOINT
        } else {
            &self.endpoint
        }
    }

    /// Whether analytics can actually send (needs measurement_id and api_secret).
    pub fn can_send(&self) -> bool {
        self.enabled && !self.measurement_id.is_empty() && !self.api_secret.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let config = AnalyticsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.measurement_id, DEFAULT_MEASUREMENT_ID);
        assert!(config.api_secret.is_empty());
        assert_eq!(config.endpoint, GA4_ENDPOINT);
        assert!(!config.debug);
    }

    #[test]
    fn debug_endpoint() {
        let config = AnalyticsConfig {
            debug: true,
            ..Default::default()
        };
        assert_eq!(config.active_endpoint(), GA4_DEBUG_ENDPOINT);
    }

    #[test]
    fn can_send_requires_id_and_secret() {
        let no_secret = AnalyticsConfig::default();
        assert!(!no_secret.can_send());

        let with_both = AnalyticsConfig {
            api_secret: "secret".into(),
            ..Default::default()
        };
        assert!(with_both.can_send());

        let disabled = AnalyticsConfig {
            enabled: false,
            api_secret: "secret".into(),
            ..Default::default()
        };
        assert!(!disabled.can_send());
    }

    #[test]
    fn deserialize_minimal() {
        let json = r"{}";
        let config: AnalyticsConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
    }

    #[test]
    fn deserialize_disabled() {
        let json = r#"{"enabled": false}"#;
        let config: AnalyticsConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
    }
}
