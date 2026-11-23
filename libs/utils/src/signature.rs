use std::io::Cursor;

use base64::Engine;
use minisign::{PublicKeyBox, SecretKeyBox, SignatureBox};

/// Sign data with minisign
pub fn sign_minisign(
    data: &[u8],
    secret_key_base64: &str,
    password: String,
) -> Result<String, String> {
    let secret_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(secret_key_base64)
        .map_err(|e| format!("Failed to decode secret key: {}", e))?;

    let secret_key_str = String::from_utf8(secret_key_bytes)
        .map_err(|e| format!("Secret key is not valid UTF-8: {}", e))?;

    let sk_box = SecretKeyBox::from_string(&secret_key_str)
        .map_err(|e| format!("Invalid secret key format: {}", e))?;

    let secret_key = sk_box
        .into_secret_key(Some(password))
        .map_err(|e| format!("Failed to decrypt secret key: {}", e))?;

    let data_reader = Cursor::new(data);
    let signature_box = minisign::sign(None, &secret_key, data_reader, None, None)
        .map_err(|e| format!("Failed to sign data: {}", e))?;

    Ok(signature_box.to_string())
}

/// Verify minisign signature
pub fn verify_minisign(
    data: &[u8],
    signature_content: &str,
    public_key_base64: &str,
) -> Result<(), String> {
    let public_key_bytes = base64::engine::general_purpose::STANDARD
        .decode(public_key_base64)
        .map_err(|e| format!("Failed to decode public key: {}", e))?;

    let public_key_str = String::from_utf8(public_key_bytes)
        .map_err(|e| format!("Public key is not valid UTF-8: {}", e))?;

    let public_key = PublicKeyBox::from_string(&public_key_str)
        .map_err(|e| format!("Invalid public key format: {}", e))?
        .into_public_key()
        .map_err(|e| format!("Failed to parse public key: {}", e))?;

    let signature = SignatureBox::from_string(signature_content)
        .map_err(|e| format!("Invalid signature format: {}", e))?;

    let data_reader = Cursor::new(data);
    minisign::verify(&public_key, &signature, data_reader, true, false, false)
        .map_err(|_| "Signature verification failed".to_string())?;

    Ok(())
}

/// Signs data with a freshly generated (unencrypted) minisign key pair, for builds made without
/// the official signing key. Returns the signature and the matching public key in the same
/// base64 format that `verify_minisign` takes, so the build can embed it.
pub fn sign_minisign_with_new_key(data: &[u8]) -> Result<(String, String), String> {
    let key_pair = minisign::KeyPair::generate_unencrypted_keypair()
        .map_err(|e| format!("Failed to generate key pair: {}", e))?;

    let signature_box = minisign::sign(None, &key_pair.sk, Cursor::new(data), None, None)
        .map_err(|e| format!("Failed to sign data: {}", e))?;

    let public_key_box = key_pair
        .pk
        .to_box()
        .map_err(|e| format!("Failed to encode public key: {}", e))?;
    let public_key_base64 =
        base64::engine::general_purpose::STANDARD.encode(public_key_box.to_string());

    Ok((signature_box.to_string(), public_key_base64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_key_signature_verifies() {
        let data = b"SHA256SUMS content";
        let (signature, public_key) = sign_minisign_with_new_key(data).unwrap();
        assert!(verify_minisign(data, &signature, &public_key).is_ok());
        assert!(verify_minisign(b"tampered", &signature, &public_key).is_err());
    }
}
