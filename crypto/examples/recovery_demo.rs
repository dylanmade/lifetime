//! End-to-end demo of the vault + recovery flow.
//!
//! Run with: `cargo run -p lifetime-crypto --example recovery_demo`

use lifetime_crypto::{MasterKey, RecoveryFile, vault};

fn main() {
    println!("== Onboarding ==");
    let mk = MasterKey::generate();
    println!("Generated master key. Fingerprint: {}", mk.fingerprint());

    let passphrase = "correct horse battery staple";
    let sealed = vault::seal(&mk, passphrase).unwrap();
    println!(
        "\nSealed vault (this is what lives on disk):\n{}",
        serde_json::to_string_pretty(&sealed).unwrap()
    );

    let recovery = RecoveryFile::new(mk);
    let recovery_text = recovery.to_text();
    println!("\nRecovery file (user stores this somewhere safe):\n{recovery_text}");

    println!("\n== Daily use: unlock with passphrase ==");
    let unlocked = vault::unlock_with_passphrase(&sealed, passphrase).unwrap();
    println!("✓ Vault unlocked. Fingerprint: {}", unlocked.fingerprint());

    println!("\n== Disaster: forgotten passphrase, recovery file used ==");
    let parsed = RecoveryFile::from_text(&recovery_text).unwrap();
    let recovered = parsed.into_master_key();
    println!("✓ Recovered master key. Fingerprint: {}", recovered.fingerprint());

    let new_sealed = vault::seal(&recovered, "new stronger passphrase").unwrap();
    let re_unlocked = vault::unlock_with_passphrase(&new_sealed, "new stronger passphrase").unwrap();
    println!(
        "✓ Resealed with new passphrase, unlocked again. Fingerprint: {}",
        re_unlocked.fingerprint()
    );
}
