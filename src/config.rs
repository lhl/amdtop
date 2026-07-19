//! Persisted UI state (section collapse flags). Stored as JSON under
//! `$XDG_CONFIG_HOME/amdtop/state.json`, or `~/.config/amdtop/state.json` when
//! `XDG_CONFIG_HOME` is unset or invalid.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct CollapseState {
    pub cpu: bool,
    pub gpu: bool,
    pub npu: bool,
    pub processes: bool,
    /// Persisted theme name (empty => use default). #[serde(default)] keeps old
    /// state files loadable.
    #[serde(default)]
    pub theme: String,
    /// Persisted gauge block-style index.
    #[serde(default)]
    pub block_style: usize,
}

fn state_path() -> Option<PathBuf> {
    let xdg_config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    state_path_from(xdg_config.as_deref(), home.as_deref())
}

fn state_path_from(
    xdg_config: Option<&std::path::Path>,
    home: Option<&std::path::Path>,
) -> Option<PathBuf> {
    if let Some(xdg_config) = xdg_config.filter(|path| path.is_absolute()) {
        return Some(xdg_config.join("amdtop/state.json"));
    }

    home.map(|home| home.join(".config/amdtop/state.json"))
}

impl CollapseState {
    pub fn load() -> Self {
        state_path()
            .and_then(|path| Self::load_from(&path).ok())
            .unwrap_or_default()
    }

    fn load_from(path: &std::path::Path) -> io::Result<Self> {
        let json = fs::read(path)?;
        serde_json::from_slice(&json).map_err(io::Error::other)
    }

    pub fn save(&self) -> io::Result<()> {
        match state_path() {
            Some(path) => self.save_to(&path),
            None => Ok(()),
        }
    }

    fn save_to(&self, path: &std::path::Path) -> io::Result<()> {
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory)?;
        }

        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        let temporary_path = path.with_extension("json.tmp");
        fs::write(&temporary_path, json)?;
        if let Err(error) = fs::rename(&temporary_path, path) {
            let _ = fs::remove_file(temporary_path);
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::state_path_from;

    #[test]
    fn state_path_prefers_absolute_xdg_config_home() {
        let path = state_path_from(Some(Path::new("/tmp/xdg")), Some(Path::new("/home/test")));
        assert_eq!(path, Some("/tmp/xdg/amdtop/state.json".into()));
    }

    #[test]
    fn state_path_falls_back_to_home_for_relative_xdg_path() {
        let path = state_path_from(Some(Path::new("relative")), Some(Path::new("/home/test")));
        assert_eq!(path, Some("/home/test/.config/amdtop/state.json".into()));
    }

    #[test]
    fn state_path_requires_xdg_or_home() {
        assert_eq!(state_path_from(None, None), None);
    }

    #[test]
    fn missing_newer_fields_use_defaults() {
        let state: super::CollapseState =
            serde_json::from_str(r#"{"cpu":true,"gpu":false,"npu":true,"processes":false}"#)
                .expect("valid legacy state JSON");

        assert!(state.cpu);
        assert_eq!(state.theme, "");
        assert_eq!(state.block_style, 0);
    }

    #[test]
    fn state_json_round_trips() {
        let state = super::CollapseState {
            cpu: true,
            gpu: false,
            npu: true,
            processes: false,
            theme: "onedark".into(),
            block_style: 2,
        };

        let json = serde_json::to_string(&state).expect("serialize state");
        let restored = serde_json::from_str(&json).expect("deserialize state");
        assert_eq!(state, restored);
    }

    #[test]
    fn state_file_is_saved_and_loaded_without_a_leftover_temporary_file() {
        let directory =
            std::env::temp_dir().join(format!("amdtop-config-test-{}", std::process::id()));
        let path = directory.join("state.json");
        let state = super::CollapseState {
            cpu: true,
            theme: "onedark".into(),
            ..super::CollapseState::default()
        };

        state.save_to(&path).expect("save state file");
        assert_eq!(
            super::CollapseState::load_from(&path).expect("load state file"),
            state
        );
        assert!(!path.with_extension("json.tmp").exists());

        std::fs::remove_dir_all(directory).expect("remove test directory");
    }
}
