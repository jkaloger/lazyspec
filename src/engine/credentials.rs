//! Global ClickUp credential storage.
//!
//! A ClickUp personal token is a bearer credential: full read/write as the
//! token owner, no scope limit, no expiry. So it is wrapped in a redacting
//! [`Token`] newtype whose `Debug`/`Display` print a fixed mask -- an accidental
//! `{:?}` or a `--json` dump can never leak the secret. The raw value is read
//! only through the explicit [`Token::expose`] accessor.
//!
//! Storage lives behind the [`CredentialStore`] seam so callers (and tests) can
//! inject where the token lands. Per RFC-056 §Auth the storage is
//! keychain-first: [`LayeredCredentialStore`] tries the OS keychain (via the
//! `keyring` crate, behind the [`Keychain`] seam) and only falls back --
//! loudly, never silently -- to the plaintext [`FileCredentialStore`] when no
//! keychain backend is reachable (headless/CI). The file is a global,
//! never-committed `~/.lazyspec/credentials.toml` (`[clickup] api_token`) with
//! dir mode `0700` and file mode `0600`, enforced on write and repaired on
//! read.
//!
//! The `keyring` interaction is isolated in [`KeyringKeychain`] behind the
//! [`Keychain`] trait so tests inject a fake and never touch the real OS
//! keychain (project principle 4: I/O boundaries behind traits).

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};

/// keyring "service" namespace for lazyspec-owned secrets.
const KEYCHAIN_SERVICE: &str = "lazyspec";
/// keyring "account"/user key under which the ClickUp token is stored.
const KEYCHAIN_ACCOUNT: &str = "clickup";

const CLICKUP_TABLE: &str = "clickup";
const API_TOKEN_KEY: &str = "api_token";

/// A redacted bearer credential. `Debug`/`Display` print a fixed mask; the raw
/// value is reachable only via [`Token::expose`], so the token never leaks into
/// logs, error messages, or `--json` output by accident. Deliberately not
/// `Serialize`/`Deserialize` -- serialization goes through [`Token::expose`] at
/// the one place that writes the credential file.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn new(raw: impl Into<String>) -> Self {
        Token(raw.into())
    }

    /// The raw secret. The only way out of the newtype -- call sites are the
    /// audit surface for where the token travels.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(***)")
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

/// Where a stored credential landed, for the caller's (loud) reporting. Carries
/// no secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialLocation {
    /// The OS keychain -- the default, encrypted-at-rest path.
    Keychain,
    /// The plaintext fallback file, used only when no keychain is reachable.
    File(PathBuf),
}

impl std::fmt::Display for CredentialLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialLocation::Keychain => f.write_str("the OS keychain"),
            CredentialLocation::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// Read/write seam for the global ClickUp credential. Global, never per-repo:
/// implementations must not read from the current working directory or a repo.
pub trait CredentialStore {
    /// Returns the stored ClickUp token, or `None` if none is stored.
    fn load_clickup_token(&self) -> Result<Option<Token>>;

    /// Persists `token`, returning where it landed. Overwrites any existing
    /// ClickUp token while preserving unrelated content.
    fn store_clickup_token(&self, token: &Token) -> Result<CredentialLocation>;
}

/// Plaintext-file credential store at a fixed path. The global constructor
/// resolves `~/.lazyspec/credentials.toml`; [`FileCredentialStore::at_path`] is
/// the injection seam tests use so they never touch the real home dir.
pub struct FileCredentialStore {
    path: PathBuf,
}

impl FileCredentialStore {
    /// The global credential file at `~/.lazyspec/credentials.toml`.
    pub fn global() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let path = PathBuf::from(home)
            .join(".lazyspec")
            .join("credentials.toml");
        FileCredentialStore { path }
    }

    /// A store rooted at an explicit file path.
    pub fn at_path(path: impl Into<PathBuf>) -> Self {
        FileCredentialStore { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl CredentialStore for FileCredentialStore {
    fn load_clickup_token(&self) -> Result<Option<Token>> {
        if !self.path.exists() {
            return Ok(None);
        }
        enforce_read_perms(&self.path)?;

        let src = fs::read_to_string(&self.path)
            .with_context(|| format!("reading credential file {}", self.path.display()))?;
        let doc: DocumentMut = src.parse().with_context(|| {
            format!("credential file {} is not valid TOML", self.path.display())
        })?;

        let token = doc
            .get(CLICKUP_TABLE)
            .and_then(|t| t.get(API_TOKEN_KEY))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(Token::new);
        Ok(token)
    }

    fn store_clickup_token(&self, token: &Token) -> Result<CredentialLocation> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating credential dir {}", parent.display()))?;
            set_dir_perms(parent)?;
        }

        // Merge into any existing file so unrelated tables/keys survive.
        let mut doc = if self.path.exists() {
            enforce_read_perms(&self.path)?;
            let src = fs::read_to_string(&self.path)
                .with_context(|| format!("reading credential file {}", self.path.display()))?;
            src.parse::<DocumentMut>().with_context(|| {
                format!("credential file {} is not valid TOML", self.path.display())
            })?
        } else {
            DocumentMut::new()
        };

        if !doc.contains_table(CLICKUP_TABLE) {
            doc[CLICKUP_TABLE] = Item::Table(Table::new());
        }
        doc[CLICKUP_TABLE][API_TOKEN_KEY] = value(token.expose());

        write_secret_file(&self.path, doc.to_string().as_bytes())?;
        Ok(CredentialLocation::File(self.path.clone()))
    }
}

/// Failure from a [`Keychain`] probe, split so a caller can tell "no backend
/// reachable" (which licenses the plaintext-file fallback) from any other
/// failure (which does not -- surfacing it beats silently writing plaintext).
#[derive(Debug)]
pub enum KeychainError {
    /// No OS keychain backend is reachable (headless/CI, locked store, or no
    /// default store). This is the only condition that licenses the file
    /// fallback.
    Unavailable(String),
    /// The keychain is reachable but the operation failed for another reason.
    Other(String),
}

impl std::fmt::Display for KeychainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeychainError::Unavailable(msg) => write!(f, "keychain unavailable: {}", msg),
            KeychainError::Other(msg) => write!(f, "keychain error: {}", msg),
        }
    }
}

/// Seam over an OS keychain backend. [`KeyringKeychain`] is the real
/// `keyring`-crate impl; tests inject a fake at this boundary so they never
/// touch the real OS keychain.
pub trait Keychain {
    /// `Ok(Some)` = token present; `Ok(None)` = reachable but no entry stored;
    /// `Err(Unavailable)` = no keychain backend reachable.
    fn get_token(&self) -> std::result::Result<Option<Token>, KeychainError>;

    /// Persists `token` into the keychain. `Err(Unavailable)` signals the caller
    /// to fall back to the plaintext file.
    fn set_token(&self, token: &Token) -> std::result::Result<(), KeychainError>;
}

/// Real [`Keychain`] backed by the `keyring` crate (macOS Keychain, Windows
/// Credential Manager, Linux Secret Service). Constructed only in production
/// wiring; never in tests.
pub struct KeyringKeychain {
    service: String,
    account: String,
}

impl KeyringKeychain {
    pub fn new() -> Self {
        KeyringKeychain {
            service: KEYCHAIN_SERVICE.to_string(),
            account: KEYCHAIN_ACCOUNT.to_string(),
        }
    }

    fn entry(&self) -> std::result::Result<keyring::Entry, KeychainError> {
        keyring::Entry::new(&self.service, &self.account).map_err(map_keyring_err)
    }
}

impl Default for KeyringKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain for KeyringKeychain {
    fn get_token(&self) -> std::result::Result<Option<Token>, KeychainError> {
        let entry = self.entry()?;
        match entry.get_password() {
            Ok(pw) if pw.is_empty() => Ok(None),
            Ok(pw) => Ok(Some(Token::new(pw))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn set_token(&self, token: &Token) -> std::result::Result<(), KeychainError> {
        let entry = self.entry()?;
        entry.set_password(token.expose()).map_err(map_keyring_err)
    }
}

/// Classifies a `keyring` error as "no backend reachable" vs anything else. The
/// unreachable set (no default store, no storage access, platform failure,
/// unsupported) is what triggers the file fallback; everything else is a hard
/// error rather than a silent plaintext write.
fn map_keyring_err(e: keyring::Error) -> KeychainError {
    match e {
        keyring::Error::NoDefaultStore
        | keyring::Error::NoStorageAccess(_)
        | keyring::Error::PlatformFailure(_)
        | keyring::Error::NotSupportedByStore(_) => KeychainError::Unavailable(e.to_string()),
        other => KeychainError::Other(other.to_string()),
    }
}

/// Keychain-first [`CredentialStore`]: reads/writes the OS keychain and falls
/// back to a plaintext [`FileCredentialStore`] only when no keychain backend is
/// reachable. The fallback is loudly logged (RFC-056 §Auth: never a silent
/// default).
pub struct LayeredCredentialStore<K: Keychain> {
    keychain: K,
    file: FileCredentialStore,
}

impl<K: Keychain> LayeredCredentialStore<K> {
    pub fn new(keychain: K, file: FileCredentialStore) -> Self {
        LayeredCredentialStore { keychain, file }
    }
}

impl LayeredCredentialStore<KeyringKeychain> {
    /// Production wiring: the real OS keychain in front of the global
    /// `~/.lazyspec/credentials.toml` fallback.
    pub fn global() -> Self {
        LayeredCredentialStore::new(KeyringKeychain::new(), FileCredentialStore::global())
    }
}

impl<K: Keychain> CredentialStore for LayeredCredentialStore<K> {
    fn load_clickup_token(&self) -> Result<Option<Token>> {
        match self.keychain.get_token() {
            Ok(Some(token)) => Ok(Some(token)),
            // Keychain reachable but empty, or no backend at all: consult the
            // global file. Never the repo/cwd.
            Ok(None) | Err(KeychainError::Unavailable(_)) => self.file.load_clickup_token(),
            Err(KeychainError::Other(msg)) => Err(anyhow::anyhow!(
                "reading ClickUp token from the OS keychain: {}",
                msg
            )),
        }
    }

    fn store_clickup_token(&self, token: &Token) -> Result<CredentialLocation> {
        match self.keychain.set_token(token) {
            Ok(()) => Ok(CredentialLocation::Keychain),
            Err(KeychainError::Unavailable(reason)) => {
                eprintln!(
                    "warning: no OS keychain backend reachable ({}); falling back to the plaintext credential file. This is an explicit fallback (RFC-056 §Auth), not the default -- protect the file.",
                    reason
                );
                self.file.store_clickup_token(token)
            }
            Err(KeychainError::Other(msg)) => Err(anyhow::anyhow!(
                "storing ClickUp token in the OS keychain: {}",
                msg
            )),
        }
    }
}

/// In-memory [`Keychain`] for tests: available (empty or seeded) or
/// unreachable. Never touches the real OS keychain.
#[cfg(any(test, feature = "test-support"))]
pub struct FakeKeychain {
    available: bool,
    stored: std::sync::Mutex<Option<Token>>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeKeychain {
    /// A reachable, empty keychain.
    pub fn available() -> Self {
        FakeKeychain {
            available: true,
            stored: std::sync::Mutex::new(None),
        }
    }

    /// A reachable keychain already holding `token`.
    pub fn with_token(token: Token) -> Self {
        FakeKeychain {
            available: true,
            stored: std::sync::Mutex::new(Some(token)),
        }
    }

    /// An unreachable keychain (headless/CI) -- every op reports `Unavailable`.
    pub fn unavailable() -> Self {
        FakeKeychain {
            available: false,
            stored: std::sync::Mutex::new(None),
        }
    }

    /// The token currently held, for assertions.
    pub fn stored(&self) -> Option<Token> {
        self.stored.lock().unwrap().clone()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl Keychain for FakeKeychain {
    fn get_token(&self) -> std::result::Result<Option<Token>, KeychainError> {
        if !self.available {
            return Err(KeychainError::Unavailable("fake keychain".to_string()));
        }
        Ok(self.stored.lock().unwrap().clone())
    }

    fn set_token(&self, token: &Token) -> std::result::Result<(), KeychainError> {
        if !self.available {
            return Err(KeychainError::Unavailable("fake keychain".to_string()));
        }
        *self.stored.lock().unwrap() = Some(token.clone());
        Ok(())
    }
}

/// Writes `bytes` to `path` with owner-only (`0600`) permissions enforced.
fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes)
        .with_context(|| format!("writing credential file {}", path.display()))?;
    set_file_perms(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_dir_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting 0700 on {}", path.display()))
}

#[cfg(not(unix))]
fn set_dir_perms(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("setting 0600 on {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_perms(_path: &Path) -> Result<()> {
    Ok(())
}

/// On read, a credential file whose perms are looser than `0600` (any
/// group/other bit set) is loudly warned about and repaired to `0600` rather
/// than silently trusted.
#[cfg(unix)]
fn enforce_read_perms(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)
        .with_context(|| format!("reading permissions of {}", path.display()))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        eprintln!(
            "warning: credential file {} had insecure permissions {:o}; tightening to 0600",
            path.display(),
            mode & 0o777
        );
        set_file_perms(path)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn enforce_read_perms(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_the_secret() {
        let token = Token::new("pk_secret_value_123");
        assert_eq!(format!("{:?}", token), "Token(***)");
        assert!(!format!("{:?}", token).contains("pk_secret"));
    }

    #[test]
    fn display_masks_the_secret() {
        let token = Token::new("pk_secret_value_123");
        assert_eq!(token.to_string(), "***");
        assert!(!token.to_string().contains("pk_secret"));
    }

    #[test]
    fn expose_returns_raw_secret() {
        let token = Token::new("pk_secret_value_123");
        assert_eq!(token.expose(), "pk_secret_value_123");
    }

    #[test]
    fn load_returns_none_when_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::at_path(dir.path().join("credentials.toml"));
        assert_eq!(store.load_clickup_token().unwrap(), None);
    }

    #[test]
    fn store_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::at_path(dir.path().join(".lazyspec/credentials.toml"));

        let location = store
            .store_clickup_token(&Token::new("pk_roundtrip"))
            .unwrap();
        assert!(matches!(location, CredentialLocation::File(_)));

        let loaded = store.load_clickup_token().unwrap().unwrap();
        assert_eq!(loaded.expose(), "pk_roundtrip");
    }

    #[test]
    fn credential_file_contains_clickup_api_token_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        let store = FileCredentialStore::at_path(&path);
        store.store_clickup_token(&Token::new("pk_xyz")).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[clickup]"));
        assert!(contents.contains("api_token = \"pk_xyz\""));
    }

    #[test]
    fn store_overwrites_existing_token() {
        let dir = tempfile::tempdir().unwrap();
        let store = FileCredentialStore::at_path(dir.path().join("credentials.toml"));
        store.store_clickup_token(&Token::new("pk_old")).unwrap();
        store.store_clickup_token(&Token::new("pk_new")).unwrap();

        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_new"
        );
    }

    #[test]
    fn store_preserves_unrelated_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        fs::write(&path, "[other]\nkeep = \"me\"\n").unwrap();

        let store = FileCredentialStore::at_path(&path);
        store.store_clickup_token(&Token::new("pk_merge")).unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("keep = \"me\""));
        assert!(contents.contains("api_token = \"pk_merge\""));
    }

    #[test]
    #[cfg(unix)]
    fn store_writes_file_0600_and_dir_0700() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let cred_dir = dir.path().join(".lazyspec");
        let path = cred_dir.join("credentials.toml");
        let store = FileCredentialStore::at_path(&path);
        store.store_clickup_token(&Token::new("pk_perms")).unwrap();

        let file_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        let dir_mode = fs::metadata(&cred_dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn load_repairs_loose_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        fs::write(&path, "[clickup]\napi_token = \"pk_loose\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let store = FileCredentialStore::at_path(&path);
        let loaded = store.load_clickup_token().unwrap().unwrap();
        assert_eq!(loaded.expose(), "pk_loose");

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn load_returns_none_when_token_key_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        fs::write(&path, "[other]\nk = \"v\"\n").unwrap();
        let store = FileCredentialStore::at_path(&path);
        assert_eq!(store.load_clickup_token().unwrap(), None);
    }

    // --- layered (keychain-first) store ---

    fn layered(keychain: FakeKeychain, file_path: PathBuf) -> LayeredCredentialStore<FakeKeychain> {
        LayeredCredentialStore::new(keychain, FileCredentialStore::at_path(file_path))
    }

    #[test]
    fn write_uses_keychain_when_available_and_skips_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("credentials.toml");
        let store = layered(FakeKeychain::available(), file_path.clone());

        let location = store.store_clickup_token(&Token::new("pk_kc")).unwrap();

        assert_eq!(location, CredentialLocation::Keychain);
        assert!(
            !file_path.exists(),
            "keychain success must not write the file"
        );
    }

    #[test]
    fn write_falls_back_to_file_only_when_keychain_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join(".lazyspec/credentials.toml");
        let store = layered(FakeKeychain::unavailable(), file_path.clone());

        let location = store
            .store_clickup_token(&Token::new("pk_fallback"))
            .unwrap();

        assert_eq!(location, CredentialLocation::File(file_path.clone()));
        assert!(file_path.exists());
        assert!(fs::read_to_string(&file_path)
            .unwrap()
            .contains("api_token = \"pk_fallback\""));
    }

    #[test]
    fn read_prefers_keychain_over_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("credentials.toml");
        FileCredentialStore::at_path(&file_path)
            .store_clickup_token(&Token::new("pk_file"))
            .unwrap();
        let store = layered(
            FakeKeychain::with_token(Token::new("pk_keychain")),
            file_path,
        );

        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_keychain"
        );
    }

    #[test]
    fn read_falls_back_to_file_when_keychain_empty() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("credentials.toml");
        FileCredentialStore::at_path(&file_path)
            .store_clickup_token(&Token::new("pk_file"))
            .unwrap();
        let store = layered(FakeKeychain::available(), file_path);

        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_file"
        );
    }

    #[test]
    fn read_falls_back_to_file_when_keychain_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("credentials.toml");
        FileCredentialStore::at_path(&file_path)
            .store_clickup_token(&Token::new("pk_file"))
            .unwrap();
        let store = layered(FakeKeychain::unavailable(), file_path);

        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_file"
        );
    }

    /// Verification bullet 3: with a token stored globally, the reader returns
    /// it while "inside a repo" that has its own credential file -- the store
    /// reads only its configured (global, absolute) path, never the repo.
    #[test]
    fn read_uses_global_file_and_ignores_repo_local_credentials() {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();

        // Global credential (what the store is configured with).
        let global_path = home.path().join(".lazyspec/credentials.toml");
        // A decoy credential file living inside the repo working tree.
        let repo_path = repo.path().join(".lazyspec/credentials.toml");
        fs::create_dir_all(repo_path.parent().unwrap()).unwrap();
        fs::write(&repo_path, "[clickup]\napi_token = \"pk_repo_decoy\"\n").unwrap();

        let store = layered(FakeKeychain::unavailable(), global_path.clone());
        store.store_clickup_token(&Token::new("pk_global")).unwrap();

        assert_eq!(
            store.load_clickup_token().unwrap().unwrap().expose(),
            "pk_global"
        );
        // The repo-local decoy is untouched and never consulted.
        assert!(fs::read_to_string(&repo_path)
            .unwrap()
            .contains("pk_repo_decoy"));
    }
}
