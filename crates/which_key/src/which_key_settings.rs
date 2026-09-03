use settings::{
    RegisterSetting, Settings, SettingsContent, WhichKeyLayout, WhichKeyPosition,
    WhichKeySettingsContent,
};

#[derive(Debug, Clone, Copy, RegisterSetting)]
pub struct WhichKeySettings {
    pub enabled: bool,
    pub delay_ms: u64,
    pub position: WhichKeyPosition,
    pub layout: WhichKeyLayout,
    pub persistent: bool,
}

impl Settings for WhichKeySettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let which_key: &WhichKeySettingsContent = content.which_key.as_ref().unwrap();

        Self {
            enabled: which_key.enabled.unwrap(),
            delay_ms: which_key.delay_ms.unwrap(),
            position: which_key.position.unwrap(),
            layout: which_key.layout.unwrap(),
            persistent: which_key.persistent.unwrap(),
        }
    }
}
