//! Settings the backend needs (custom AI providers, external MCP access), kept in one JSON file
//! in the app config dir. UI-only preferences (language, theme, density) live in the webview.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub providers: Vec<Provider>,
    /// let agents outside traym (Claude Desktop, Cursor…) use the MCP server with `mcp_token`
    pub mcp_external: bool,
    pub mcp_token: String,
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

/// Written owner-only: it holds API keys and the MCP token.
pub fn save(path: &Path, s: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(s).unwrap()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// 32 random bytes as hex.
pub fn token() -> String {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("no OS randomness");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_private() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cfg/settings.json");
        assert_eq!(load(&p), Settings::default());
        let s = Settings {
            providers: vec![Provider { id: "o".into(), name: "Ollama".into(), base_url: "http://localhost:11434/v1".into(), api_key: "".into() }],
            mcp_external: true,
            mcp_token: token(),
        };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), s);
        assert_eq!(s.mcp_token.len(), 64);
        assert_ne!(token(), token());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
