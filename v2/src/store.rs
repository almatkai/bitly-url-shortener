use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};
use tezbar_extension_sdk::ExtensionContext;
const LIMIT: u64 = 128 * 1024;
const MAX_ENTRIES: usize = 64;
static LOCK: Mutex<()> = Mutex::new(());
const ERROR: &str = "Recovery storage is unavailable, unsafe, full or corrupt. No request was automatically repeated.";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
pub enum Recovery {
    Uncertain,
    Prepared { link: String, copied: bool },
}
pub fn key(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for p in parts {
        hash.update((p.len() as u64).to_be_bytes());
        hash.update(p.as_bytes());
    }
    format!("{:x}", hash.finalize())
}
fn safe(path: &Path, file: bool) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m)
            if !m.file_type().is_symlink()
                && if file {
                    m.is_file() && m.len() <= LIMIT
                } else {
                    m.is_dir()
                } =>
        {
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && file => Ok(()),
        _ => Err(ERROR.into()),
    }
}
fn read(
    ctx: &ExtensionContext,
) -> Result<(std::path::PathBuf, BTreeMap<String, Recovery>), String> {
    safe(&ctx.paths.data_dir, false)?;
    let dir = ctx.paths.data_dir.join("bitly-v2");
    match fs::create_dir(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(ERROR.into()),
    }
    safe(&dir, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|_| ERROR)?;
    }
    let path = dir.join("recovery.json");
    safe(&path, true)?;
    let entries: BTreeMap<String, Recovery> = match fs::File::open(&path).and_then(|f| {
        let mut bytes = Vec::new();
        f.take(LIMIT + 1).read_to_end(&mut bytes)?;
        Ok(bytes)
    }) {
        Ok(b) if b.len() <= LIMIT as usize => serde_json::from_slice(&b).map_err(|_| ERROR)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        _ => return Err(ERROR.into()),
    };
    if entries.len() > MAX_ENTRIES || entries.iter().any(|(k, v)| {
        k.len() != 64
            || !k
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || matches!(v, Recovery::Prepared { link, .. } if crate::http::safe_web(link).is_err())
    }) {
        return Err(ERROR.into());
    }
    Ok((path, entries))
}
pub fn get(ctx: &ExtensionContext, key: &str) -> Result<Option<Recovery>, String> {
    let _lock = LOCK.lock().map_err(|_| ERROR)?;
    let (_, entries) = read(ctx)?;
    Ok(entries.get(key).cloned())
}
pub fn put(ctx: &ExtensionContext, key: &str, recovery: Recovery) -> Result<(), String> {
    let _lock = LOCK.lock().map_err(|_| ERROR)?;
    let (path, mut entries) = read(ctx)?;
    if entries.len() >= MAX_ENTRIES && !entries.contains_key(key) {
        // Never discard unknown creation or an unacknowledged copy to make room.
        let acknowledged = entries.iter().find_map(|(key, value)| {
            matches!(value, Recovery::Prepared { copied: true, .. }).then(|| key.clone())
        });
        if let Some(key) = acknowledged {
            entries.remove(&key);
        } else {
            return Err(ERROR.into());
        }
    }
    entries.insert(key.into(), recovery);
    let bytes = serde_json::to_vec(&entries).map_err(|_| ERROR)?;
    if bytes.len() > LIMIT as usize {
        return Err(ERROR.into());
    }
    let parent = path.parent().ok_or(ERROR)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| ERROR)?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| ERROR)?;
    // The SDK private data directory is host owned. Recheck before atomic replacement.
    safe(parent, false)?;
    safe(&path, true)?;
    temp.persist(&path).map_err(|_| ERROR)?;
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| ERROR)?;
    Ok(())
}
