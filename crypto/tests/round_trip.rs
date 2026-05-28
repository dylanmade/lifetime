use lifetime_crypto::{CryptoError, MasterKey, RecoveryFile, vault};

#[test]
fn vault_seal_unlock_round_trip() {
    let mk = MasterKey::generate();
    let original_fp = mk.fingerprint();

    let sealed = vault::seal(&mk, "correct horse battery staple").unwrap();
    let unlocked = vault::unlock_with_passphrase(&sealed, "correct horse battery staple").unwrap();
    assert_eq!(unlocked.fingerprint(), original_fp);
}

#[test]
fn vault_wrong_passphrase_fails() {
    let mk = MasterKey::generate();
    let sealed = vault::seal(&mk, "correct").unwrap();
    let result = vault::unlock_with_passphrase(&sealed, "wrong");
    assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
}

#[test]
fn vault_tampered_ciphertext_fails() {
    let mk = MasterKey::generate();
    let mut sealed = vault::seal(&mk, "pass").unwrap();
    sealed.ciphertext = "AAAA".to_string() + &sealed.ciphertext[4..];
    let result = vault::unlock_with_passphrase(&sealed, "pass");
    assert!(result.is_err());
}

#[test]
fn recovery_file_round_trip() {
    let mk = MasterKey::generate();
    let original_fp = mk.fingerprint();

    let text = RecoveryFile::new(mk).to_text();
    let parsed = RecoveryFile::from_text(&text).unwrap();
    assert_eq!(parsed.fingerprint(), original_fp);
    assert_eq!(parsed.into_master_key().fingerprint(), original_fp);
}

#[test]
fn recovery_file_corrupted_payload_fails() {
    let mk = MasterKey::generate();
    let text = RecoveryFile::new(mk).to_text();
    let mut chars: Vec<char> = text.chars().collect();
    let last = chars.len() - 5;
    chars[last] = if chars[last] == 'A' { 'B' } else { 'A' };
    let corrupted: String = chars.into_iter().collect();
    let result = RecoveryFile::from_text(&corrupted);
    assert!(result.is_err());
}

#[test]
fn recovery_file_garbage_text_fails() {
    let result = RecoveryFile::from_text("not a recovery file");
    assert!(matches!(result, Err(CryptoError::InvalidRecoveryFormat)));
}

#[test]
fn recovery_then_reseal_with_new_passphrase() {
    // Onboarding: user creates key, seals with original passphrase, exports recovery.
    let mk = MasterKey::generate();
    let original_fp = mk.fingerprint();
    let original_sealed = vault::seal(&mk, "old passphrase").unwrap();
    let recovery_text = RecoveryFile::new(mk).to_text();

    // ... time passes, user forgets passphrase ...

    // Recovery flow: parse recovery file, get MK back, reseal with new passphrase.
    let recovered_mk = RecoveryFile::from_text(&recovery_text)
        .unwrap()
        .into_master_key();
    assert_eq!(recovered_mk.fingerprint(), original_fp);

    let new_sealed = vault::seal(&recovered_mk, "new passphrase").unwrap();
    let unlocked_with_new = vault::unlock_with_passphrase(&new_sealed, "new passphrase").unwrap();
    assert_eq!(unlocked_with_new.fingerprint(), original_fp);

    // Old vault still unlocks with old passphrase (until app overwrites the file).
    let unlocked_with_old = vault::unlock_with_passphrase(&original_sealed, "old passphrase").unwrap();
    assert_eq!(unlocked_with_old.fingerprint(), original_fp);
}

#[test]
fn derived_subkeys_are_purpose_separated() {
    let mk = MasterKey::generate();
    let sqlcipher = mk.derive_subkey(b"lifetime/sqlcipher/v1", 32);
    let sync = mk.derive_subkey(b"lifetime/sync-envelope/v1", 32);
    assert_eq!(sqlcipher.len(), 32);
    assert_eq!(sync.len(), 32);
    assert_ne!(sqlcipher, sync);
}

#[test]
fn fingerprint_is_deterministic() {
    let bytes = [7u8; 32];
    let a = MasterKey::from_bytes(bytes);
    let b = MasterKey::from_bytes(bytes);
    assert_eq!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.fingerprint().len(), 14); // ABCD-EFGH-IJKL
}
