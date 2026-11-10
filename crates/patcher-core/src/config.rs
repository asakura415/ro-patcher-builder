use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatcherConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub client: ClientConfig,
    #[serde(default)]
    pub patching: PatchingConfig,
    #[serde(default)]
    pub window: WindowConfig,
}

impl Default for PatcherConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            client: ClientConfig::default(),
            patching: PatchingConfig::default(),
            window: WindowConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default)]
    pub patch_servers: Vec<PatchServerInfo>,
    #[serde(default)]
    pub preferred_server: Option<String>,
    #[serde(default)]
    pub notice_url: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            patch_servers: Vec::new(),
            preferred_server: None,
            notice_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchServerInfo {
    pub name: String,
    pub plist_url: String,
    pub patch_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    #[serde(default = "default_client_executable")]
    pub executable: String,
    #[serde(default = "default_client_arguments")]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub setup_executable: Option<String>,
    #[serde(default = "default_grf_name")]
    pub default_grf_name: String,
}

fn default_client_executable() -> String {
    "ragexe.exe".to_string()
}

fn default_client_arguments() -> Vec<String> {
    vec!["-1rag1".to_string()]
}

fn default_grf_name() -> String {
    "data.grf".to_string()
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            executable: default_client_executable(),
            arguments: default_client_arguments(),
            setup_executable: Some("opensetup.exe".to_string()),
            default_grf_name: default_grf_name(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchingConfig {
    #[serde(default = "default_true")]
    pub in_place: bool,
    #[serde(default = "default_true")]
    pub create_grf: bool,
    #[serde(default = "default_true")]
    pub check_integrity: bool,
    #[serde(default = "default_concurrent_downloads")]
    pub concurrent_downloads: usize,
}

fn default_true() -> bool {
    true
}

fn default_concurrent_downloads() -> usize {
    16
}

impl Default for PatchingConfig {
    fn default() -> Self {
        Self {
            in_place: true,
            create_grf: true,
            check_integrity: true,
            concurrent_downloads: default_concurrent_downloads(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfig {
    #[serde(default = "default_window_title")]
    pub title: String,
    #[serde(default = "default_window_width")]
    pub width: u32,
    #[serde(default = "default_window_height")]
    pub height: u32,
    #[serde(default)]
    pub resizable: bool,
    #[serde(default)]
    pub decorations: bool,
    #[serde(default = "default_true")]
    pub transparent: bool,
}

fn default_window_title() -> String {
    "Ragnarok Online Patcher".to_string()
}

fn default_window_width() -> u32 {
    800
}

fn default_window_height() -> u32 {
    500
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: default_window_title(),
            width: default_window_width(),
            height: default_window_height(),
            resizable: false,
            decorations: false,
            transparent: true,
        }
    }
}

impl PatcherConfig {
    pub fn from_json_str(json: &str) -> anyhow::Result<Self> {
        let config: Self = serde_json::from_str(json)?;
        Ok(config)
    }

    pub fn to_json_string(&self) -> anyhow::Result<String> {
        let json = serde_json::to_string_pretty(self)?;
        Ok(json)
    }
}
