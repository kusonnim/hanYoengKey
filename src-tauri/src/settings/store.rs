use std::{
    fs::{self, File},
    io::Write,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use windows::{
    core::PCWSTR,
    Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
};

use super::{
    model::{Settings, ValidationError},
    startup::{
        StartupError, StartupRegistration, UnavailableStartupRegistration,
        WindowsStartupRegistration,
    },
};

#[derive(Clone)]
pub(crate) struct SettingsRuntime {
    current: Arc<RwLock<Settings>>,
}

impl SettingsRuntime {
    pub(crate) fn current(&self) -> Settings {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[derive(Clone)]
pub(crate) struct SettingsStore {
    inner: Arc<StoreInner>,
}

struct StoreInner {
    path: PathBuf,
    current: Arc<RwLock<Settings>>,
    subscribers: RwLock<Vec<mpsc::Sender<Settings>>>,
    startup: Arc<dyn StartupRegistration>,
    update_lock: Mutex<()>,
}

impl SettingsStore {
    pub(crate) fn load(path: PathBuf) -> Result<Self, SettingsError> {
        let startup: Arc<dyn StartupRegistration> =
            match WindowsStartupRegistration::current_executable() {
                Ok(startup) => Arc::new(startup),
                Err(error) => {
                    eprintln!("[settings] startup-initialization-failed error={error}");
                    Arc::new(UnavailableStartupRegistration)
                }
            };
        Self::load_with(path, startup)
    }

    fn load_with(
        path: PathBuf,
        startup: Arc<dyn StartupRegistration>,
    ) -> Result<Self, SettingsError> {
        let mut settings = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(&bytes)
                .map_err(SettingsError::Deserialize)
                .and_then(|settings| {
                    settings.validate().map_err(SettingsError::Validation)?;
                    Ok(settings)
                }) {
                Ok(settings) => settings,
                Err(error) => {
                    eprintln!("[settings] load-failed error={error}; using defaults");
                    Settings::default()
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(error) => {
                eprintln!("[settings] read-failed error={error}; using defaults");
                Settings::default()
            }
        };

        if let Err(error) = startup.set_enabled(settings.launch_at_startup) {
            eprintln!("[settings] startup-registration-failed error={error}");
            if settings.launch_at_startup {
                settings.launch_at_startup = false;
            }
        }
        if let Err(error) = atomic_save(&path, &settings) {
            eprintln!("[settings] initial-save-failed error={error}; continuing in memory");
        }

        Ok(Self {
            inner: Arc::new(StoreInner {
                path,
                current: Arc::new(RwLock::new(settings)),
                subscribers: RwLock::new(Vec::new()),
                startup,
                update_lock: Mutex::new(()),
            }),
        })
    }

    pub(crate) fn runtime(&self) -> SettingsRuntime {
        SettingsRuntime {
            current: Arc::clone(&self.inner.current),
        }
    }

    pub(crate) fn subscribe(&self) -> mpsc::Receiver<Settings> {
        let (sender, receiver) = mpsc::channel();
        self.inner
            .subscribers
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(sender);
        receiver
    }

    pub(crate) fn current(&self) -> Settings {
        self.runtime().current()
    }

    pub(crate) fn update(&self, next: Settings) -> Result<Settings, SettingsError> {
        let _update_guard = self
            .inner
            .update_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        next.validate().map_err(SettingsError::Validation)?;
        let previous = self.current();

        if previous.launch_at_startup != next.launch_at_startup {
            self.inner
                .startup
                .set_enabled(next.launch_at_startup)
                .map_err(SettingsError::Startup)?;
        }

        if let Err(error) = atomic_save(&self.inner.path, &next) {
            if previous.launch_at_startup != next.launch_at_startup {
                if let Err(rollback) = self.inner.startup.set_enabled(previous.launch_at_startup) {
                    eprintln!("[settings] startup-rollback-failed error={rollback}");
                }
            }
            return Err(error);
        }

        *self
            .inner
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = next.clone();
        self.notify(&next);
        Ok(next)
    }

    pub(crate) fn reset_defaults(&self) -> Result<Settings, SettingsError> {
        self.update(Settings::default())
    }

    fn notify(&self, settings: &Settings) {
        self.inner
            .subscribers
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .retain(|subscriber| subscriber.send(settings.clone()).is_ok());
    }
}

fn atomic_save(path: &Path, settings: &Settings) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(SettingsError::Io)?;
    }
    let json = serde_json::to_vec_pretty(settings).map_err(SettingsError::Serialize)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("json.{nonce}.tmp"));

    let result = (|| {
        let mut file = File::create(&temporary).map_err(SettingsError::Io)?;
        file.write_all(&json).map_err(SettingsError::Io)?;
        file.write_all(b"\n").map_err(SettingsError::Io)?;
        file.sync_all().map_err(SettingsError::Io)?;
        atomic_replace(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn atomic_replace(source: &Path, destination: &Path) -> Result<(), SettingsError> {
    let source = wide_path(source);
    let destination = wide_path(destination);
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(destination.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(SettingsError::AtomicReplace)
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SettingsError {
    #[error("settings I/O failed: {0}")]
    Io(#[source] std::io::Error),
    #[error("settings serialization failed: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("settings deserialization failed: {0}")]
    Deserialize(#[source] serde_json::Error),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Startup(#[from] StartupError),
    #[error("atomic settings replacement failed: {0}")]
    AtomicReplace(#[source] windows::core::Error),
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    };

    use super::*;
    use crate::settings::{
        model::ProviderPreference,
        startup::{StartupError, StartupRegistration},
    };

    static TEMP_ID: AtomicU64 = AtomicU64::new(1);

    #[derive(Default)]
    struct MockStartup {
        calls: Mutex<Vec<bool>>,
        fail: AtomicBool,
    }

    impl StartupRegistration for MockStartup {
        fn set_enabled(&self, enabled: bool) -> Result<(), StartupError> {
            self.calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(enabled);
            if self.fail.load(Ordering::Relaxed) {
                Err(StartupError::Mock)
            } else {
                Ok(())
            }
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "hanyeongkey-settings-test-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn settings_path(&self) -> PathBuf {
            self.0.join("settings.json")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn store(directory: &TestDirectory, startup: Arc<MockStartup>) -> SettingsStore {
        SettingsStore::load_with(directory.settings_path(), startup).unwrap()
    }

    #[test]
    fn creates_default_configuration_when_missing() {
        let directory = TestDirectory::new();
        let store = store(&directory, Arc::new(MockStartup::default()));
        assert_eq!(store.current(), Settings::default());
        assert!(directory.settings_path().is_file());
    }

    #[test]
    fn loads_existing_configuration() {
        let directory = TestDirectory::new();
        let expected = Settings {
            enable_conversion: false,
            selection_provider: ProviderPreference::ClipboardOnly,
            ..Settings::default()
        };
        atomic_save(&directory.settings_path(), &expected).unwrap();
        let store = store(&directory, Arc::new(MockStartup::default()));
        assert_eq!(store.current(), expected);
    }

    #[test]
    fn corrupted_configuration_recovers_to_defaults() {
        let directory = TestDirectory::new();
        fs::write(directory.settings_path(), b"{not-json").unwrap();
        let store = store(&directory, Arc::new(MockStartup::default()));
        assert_eq!(store.current(), Settings::default());
        let persisted: Settings =
            serde_json::from_slice(&fs::read(directory.settings_path()).unwrap()).unwrap();
        assert_eq!(persisted, Settings::default());
    }

    #[test]
    fn atomic_save_replaces_file_without_leaving_temporary_files() {
        let directory = TestDirectory::new();
        let path = directory.settings_path();
        atomic_save(&path, &Settings::default()).unwrap();
        let changed = Settings {
            debug_logging: true,
            ..Settings::default()
        };
        atomic_save(&path, &changed).unwrap();
        let entries = fs::read_dir(&directory.0).unwrap().count();
        assert_eq!(entries, 1);
        assert_eq!(
            serde_json::from_slice::<Settings>(&fs::read(path).unwrap()).unwrap(),
            changed
        );
    }

    #[test]
    fn startup_registration_enables_and_disables() {
        let directory = TestDirectory::new();
        let startup = Arc::new(MockStartup::default());
        let store = store(&directory, Arc::clone(&startup));

        store
            .update(Settings {
                launch_at_startup: true,
                ..Settings::default()
            })
            .unwrap();
        store.update(Settings::default()).unwrap();

        assert_eq!(
            *startup
                .calls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![false, true, false]
        );
    }

    #[test]
    fn startup_failure_keeps_settings_consistent() {
        let directory = TestDirectory::new();
        let startup = Arc::new(MockStartup::default());
        let store = store(&directory, Arc::clone(&startup));
        startup.fail.store(true, Ordering::Relaxed);

        let result = store.update(Settings {
            launch_at_startup: true,
            ..Settings::default()
        });

        assert!(result.is_err());
        assert!(!store.current().launch_at_startup);
    }

    #[test]
    fn reset_restores_defaults_and_runtime_update_is_published() {
        let directory = TestDirectory::new();
        let store = store(&directory, Arc::new(MockStartup::default()));
        let updates = store.subscribe();
        let runtime = store.runtime();
        let changed = Settings {
            enable_conversion: false,
            replacement_provider: ProviderPreference::UiAutomationOnly,
            ..Settings::default()
        };

        store.update(changed.clone()).unwrap();
        assert_eq!(runtime.current(), changed);
        assert_eq!(updates.recv().unwrap(), changed);

        let reset = store.reset_defaults().unwrap();
        assert_eq!(reset, Settings::default());
        assert_eq!(runtime.current(), Settings::default());
    }
}
