use crate::domain::{Preferences, Snapshot};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Store {
    pub directory: PathBuf,
}
impl Store {
    pub fn new(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory)
            .map_err(|e| format!("Не удалось создать каталог настроек: {e}"))?;
        Ok(Self { directory })
    }
    pub fn default_directory() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("AI Picker")
    }
    pub fn load_preferences(&self) -> Result<Preferences, String> {
        let p = self.directory.join("preferences.json");
        if !p.exists() {
            return Ok(Preferences::default());
        }
        let mut preferences: Preferences = read_json(&p)?;
        if !preferences.input_share.is_finite() || !(0.0..=1.0).contains(&preferences.input_share) {
            preferences.input_share = 0.75;
        }
        if !preferences.reasoning_tolerance.is_finite()
            || !(0.0..=20.0).contains(&preferences.reasoning_tolerance)
        {
            preferences.reasoning_tolerance = 2.0;
        }
        if !preferences.reasoning_savings.is_finite()
            || !(0.01..=1.0).contains(&preferences.reasoning_savings)
        {
            preferences.reasoning_savings = 0.20;
        }
        if !preferences.quality_weight.is_finite()
            || !(0.0..=1.0).contains(&preferences.quality_weight)
        {
            preferences.quality_weight = 0.65;
        }
        Ok(preferences)
    }
    pub fn save_preferences(&self, preferences: &Preferences) -> Result<(), String> {
        write_json(&self.directory.join("preferences.json"), preferences)
    }
    pub fn load_snapshot(&self) -> Result<Option<Snapshot>, String> {
        let path = self.directory.join("benchmarks.json");
        if !path.exists() {
            return Ok(None);
        }
        let snapshot: Snapshot = read_json(&path)?;
        snapshot.validate()?;
        if snapshot.demo {
            return Err("Демонстрационные данные не могут быть живым кешем".into());
        }
        Ok(Some(snapshot))
    }
    pub fn save_snapshot(&self, snapshot: &Snapshot) -> Result<(), String> {
        snapshot.validate()?;
        if snapshot.demo {
            return Err("Демонстрационные данные не сохраняются в кеш API".into());
        }
        write_json(&self.directory.join("benchmarks.json"), snapshot)
    }
    pub fn load_key(&self) -> Result<Option<String>, String> {
        let path = self.directory.join("credential.dpapi");
        if !path.exists() {
            return Ok(None);
        }
        let encrypted =
            fs::read(path).map_err(|_| "Не удалось прочитать сохранённый ключ".to_string())?;
        let plain = crypt(&encrypted, false)?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|_| "Повреждён сохранённый ключ".into())
    }
    pub fn save_key(&self, key: &str) -> Result<(), String> {
        let path = self.directory.join("credential.dpapi");
        if key.trim().is_empty() {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err("Не удалось удалить сохранённый ключ".into()),
            }
        } else {
            atomic_write(&path, &crypt(key.trim().as_bytes(), true)?)
        }
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|e| format!("Не удалось прочитать {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|_| {
        format!(
            "Повреждён файл {}. Он оставлен без изменений.",
            path.display()
        )
    })
}
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| "Не удалось сериализовать данные".to_string())?;
    atomic_write(path, &bytes)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let result = (|| -> std::io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| std::io::Error::other("no parent"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|e| e.error)?;
        Ok(())
    })();
    result.map_err(|e| format!("Не удалось сохранить {}: {e}", path.display()))
}

#[cfg(windows)]
fn crypt(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    };
    if bytes.len() > u32::MAX as usize {
        return Err("Ключ слишком длинный".into());
    }
    let input = CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI borrows input for this synchronous call and allocates output with LocalAlloc.
    // UI is forbidden; the encrypted blob is bound to the current Windows user.
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if ok == 0 {
            return Err("Windows не смог защитить/прочитать ключ. Введите ключ заново под своей учётной записью.".into());
        }
        let result = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData.cast());
        Ok(result)
    }
}

#[cfg(not(windows))]
fn crypt(_: &[u8], _: bool) -> Result<Vec<u8>, String> {
    Err("Хранение ключа доступно только через Windows DPAPI".into())
}
