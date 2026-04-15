//! Append-only, signed, hash-chained JSONL session log writer.
//!
//! Each line is a [`SessionEvent`] serialized as JSON. Before writing:
//! 1. `prevhash` is set to `SHA-256` of the previous record's canonical bytes.
//! 2. The event is JCS-canonicalized (RFC 8785) with `signature`/`pubkey` = `None`.
//! 3. The canonical bytes are Ed25519-signed.
//! 4. `signature` and `pubkey` are set, and the full event is appended.
//!
//! [`SessionEvent`]: lesearch_protocol::session::SessionEvent

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use lesearch_protocol::session::SessionEvent;

use crate::keyring::Keyring;

/// Initial `prevhash` value for the first record in a session log (64 hex zeros).
const GENESIS_HASH: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// Append-only session log writer.
pub struct SessionLogWriter {
    writer: BufWriter<File>,
    keyring: Keyring,
    last_hash: String,
    path: PathBuf,
}

impl std::fmt::Debug for SessionLogWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionLogWriter")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl SessionLogWriter {
    /// Create a new session log writer.
    ///
    /// Opens (or creates) `path` in append mode. If the file already has
    /// content, the hash chain is resumed from the last line.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError`] on I/O or parse failures.
    pub fn open(path: &Path, keyring: Keyring) -> Result<Self, crate::StorageError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Determine the last hash from existing content
        let last_hash = if path.exists() {
            last_hash_from_file(path)?
        } else {
            GENESIS_HASH.to_owned()
        };

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        Ok(Self {
            writer: BufWriter::new(file),
            keyring,
            last_hash,
            path: path.to_owned(),
        })
    }

    /// Append a session event, signing and hash-chaining it in place.
    ///
    /// The event is modified: `prevhash`, `signature`, and `pubkey` fields
    /// are set before writing.
    ///
    /// # Errors
    ///
    /// Returns [`crate::StorageError`] on canonicalization, signing, or I/O errors.
    pub fn append(&mut self, event: &mut SessionEvent) -> Result<(), crate::StorageError> {
        // 1. Link to chain
        event.prevhash = Some(self.last_hash.clone());

        // 2. Clear signing fields for canonicalization
        event.signature = None;
        event.pubkey = None;

        // 3. JCS-canonicalize
        let canonical = serde_json_canonicalizer::to_vec(event)
            .map_err(|e| crate::StorageError::Signing(format!("JCS canonicalization: {e}")))?;

        // 4. Sign the canonical bytes
        event.signature = Some(self.keyring.sign(&canonical));
        event.pubkey = Some(self.keyring.public_key_hex());

        // 5. Update hash chain for next record
        self.last_hash = Keyring::hash(&canonical);

        // 6. Serialize the FULL event (with sig + pubkey) and append
        let line = serde_json::to_string(event)?;
        writeln!(self.writer, "{line}")?;
        self.writer.flush()?;

        Ok(())
    }

    /// Path to the underlying JSONL file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Verify all signatures and hash chain in a session log file.
///
/// Returns `Ok(event_count)` if every record passes, or an error describing
/// the first failure with its line number.
///
/// # Errors
///
/// Returns [`crate::StorageError::VerificationFailed`] on the first tampered
/// or invalid record.
pub fn verify_session_log(path: &Path) -> Result<usize, crate::StorageError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut expected_hash = GENESIS_HASH.to_owned();
    let mut count = 0;

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result?;
        if line.trim().is_empty() {
            continue;
        }

        let event: SessionEvent = serde_json::from_str(&line).map_err(|e| {
            crate::StorageError::VerificationFailed(format!("line {}: parse error: {e}", line_num + 1))
        })?;

        // Check prevhash
        let prevhash = event.prevhash.as_deref().unwrap_or("");
        if prevhash != expected_hash {
            return Err(crate::StorageError::VerificationFailed(format!(
                "line {}: prevhash mismatch: expected {expected_hash}, got {prevhash}",
                line_num + 1
            )));
        }

        // Extract and strip signing fields
        let sig = event.signature.clone().ok_or_else(|| {
            crate::StorageError::VerificationFailed(format!("line {}: missing signature", line_num + 1))
        })?;
        let pubkey = event.pubkey.clone().ok_or_else(|| {
            crate::StorageError::VerificationFailed(format!("line {}: missing pubkey", line_num + 1))
        })?;

        let mut stripped = event;
        stripped.signature = None;
        stripped.pubkey = None;

        // Canonicalize
        let canonical = serde_json_canonicalizer::to_vec(&stripped)
            .map_err(|e| crate::StorageError::Signing(format!("JCS: {e}")))?;

        // Verify signature
        Keyring::verify(&canonical, &sig, &pubkey).map_err(|e| {
            crate::StorageError::VerificationFailed(format!("line {}: {e}", line_num + 1))
        })?;

        // Update expected hash for next record
        expected_hash = Keyring::hash(&canonical);
        count += 1;
    }

    Ok(count)
}

/// Read the last line of a file and compute its canonical hash.
fn last_hash_from_file(path: &Path) -> Result<String, crate::StorageError> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut last_line = None;

    for line in reader.lines() {
        let l = line?;
        if !l.trim().is_empty() {
            last_line = Some(l);
        }
    }

    let Some(line) = last_line else {
        return Ok(GENESIS_HASH.to_owned());
    };

    let event: SessionEvent = serde_json::from_str(&line)?;
    let mut stripped = event;
    stripped.signature = None;
    stripped.pubkey = None;
    let canonical = serde_json_canonicalizer::to_vec(&stripped)
        .map_err(|e| crate::StorageError::Signing(format!("JCS: {e}")))?;
    Ok(Keyring::hash(&canonical))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lesearch_protocol::session::event_type;

    fn temp_log_path() -> PathBuf {
        let dir = std::env::temp_dir().join("lesearch-session-log-test");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(format!("{}.jsonl", uuid::Uuid::now_v7()))
    }

    #[test]
    fn write_and_verify_session_log() {
        let path = temp_log_path();
        let keyring = Keyring::generate();
        let mut writer = SessionLogWriter::open(&path, keyring).unwrap();

        // Append 3 events
        for i in 0..3 {
            let mut event = SessionEvent::new(
                "lesearch://test",
                event_type::STREAM_CHUNK,
                "agent:test",
                serde_json::json!({"content": format!("chunk {i}")}),
            );
            writer.append(&mut event).unwrap();

            // Event should now have signing fields
            assert!(event.signature.is_some());
            assert!(event.pubkey.is_some());
            assert!(event.prevhash.is_some());
        }

        // Verify
        let count = verify_session_log(&path).unwrap();
        assert_eq!(count, 3);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tampered_file_fails_verification() {
        let path = temp_log_path();
        let keyring = Keyring::generate();
        let mut writer = SessionLogWriter::open(&path, keyring).unwrap();

        let mut event = SessionEvent::new(
            "lesearch://test",
            event_type::SESSION_STARTED,
            "agent:test",
            serde_json::json!({"provider": "claude", "prompt": "hi", "cwd": "/tmp"}),
        );
        writer.append(&mut event).unwrap();
        drop(writer);

        // Tamper with the file
        let content = std::fs::read_to_string(&path).unwrap();
        let tampered = content.replace("claude", "TAMPERED");
        std::fs::write(&path, tampered).unwrap();

        let result = verify_session_log(&path);
        assert!(result.is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resume_existing_log() {
        let path = temp_log_path();

        // Write one event, close writer
        {
            let keyring = Keyring::generate();
            let mut writer = SessionLogWriter::open(&path, keyring).unwrap();
            let mut event = SessionEvent::new(
                "src",
                event_type::SESSION_STARTED,
                "sub",
                serde_json::json!({}),
            );
            writer.append(&mut event).unwrap();
        }

        // Reopen with new keyring (simulating daemon restart)
        // Note: verification will still work because each record embeds its pubkey
        {
            let keyring = Keyring::generate();
            let mut writer = SessionLogWriter::open(&path, keyring).unwrap();
            let mut event = SessionEvent::new(
                "src",
                event_type::SESSION_STOPPED,
                "sub",
                serde_json::json!({"exit_code": 0}),
            );
            writer.append(&mut event).unwrap();
        }

        // Both events should be in the file
        let file = File::open(&path).unwrap();
        assert_eq!(BufReader::new(file).lines().count(), 2);

        let _ = std::fs::remove_file(&path);
    }
}
