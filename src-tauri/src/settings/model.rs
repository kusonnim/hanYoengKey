use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum HotkeyMode {
    HangulEnglishKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProviderPreference {
    Automatic,
    UiAutomationOnly,
    ClipboardOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Settings {
    pub(crate) launch_at_startup: bool,
    pub(crate) enable_conversion: bool,
    pub(crate) hotkey_mode: HotkeyMode,
    pub(crate) selection_provider: ProviderPreference,
    pub(crate) replacement_provider: ProviderPreference,
    pub(crate) debug_logging: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_at_startup: false,
            enable_conversion: true,
            hotkey_mode: HotkeyMode::HangulEnglishKey,
            selection_provider: ProviderPreference::Automatic,
            replacement_provider: ProviderPreference::Automatic,
            debug_logging: false,
        }
    }
}

impl Settings {
    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        // Enum deserialization rejects unsupported hotkeys and providers.
        // Keep an explicit validation boundary for future constrained fields.
        match self.hotkey_mode {
            HotkeyMode::HangulEnglishKey => Ok(()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("settings validation failed")]
pub(crate) struct ValidationError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_product_requirements() {
        let settings = Settings::default();
        assert!(!settings.launch_at_startup);
        assert!(settings.enable_conversion);
        assert_eq!(settings.hotkey_mode, HotkeyMode::HangulEnglishKey);
        assert_eq!(settings.selection_provider, ProviderPreference::Automatic);
        assert_eq!(settings.replacement_provider, ProviderPreference::Automatic);
        assert!(!settings.debug_logging);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn invalid_enum_value_is_rejected() {
        let json = serde_json::json!({
            "launchAtStartup": false,
            "enableConversion": true,
            "hotkeyMode": "unsupported",
            "selectionProvider": "automatic",
            "replacementProvider": "automatic",
            "debugLogging": false
        });
        assert!(serde_json::from_value::<Settings>(json).is_err());
    }

    #[test]
    fn every_provider_preference_round_trips() {
        for preference in [
            ProviderPreference::Automatic,
            ProviderPreference::UiAutomationOnly,
            ProviderPreference::ClipboardOnly,
        ] {
            let settings = Settings {
                selection_provider: preference,
                replacement_provider: preference,
                ..Settings::default()
            };
            let json = serde_json::to_vec(&settings).unwrap();
            assert_eq!(serde_json::from_slice::<Settings>(&json).unwrap(), settings);
        }
    }
}
