use std::collections::BTreeMap;

use aes::Aes128;
use base64::Engine as _;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use cbc::Decryptor;
use hmac::{Hmac, Mac as _};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use sha2::Sha256;

use super::InverterError;

const APP_ID: &str = "rBrTRfAPXz";
const APP_SECRET_ENC: &str = "I4D0KRr2339z3pQ/at91V9BpFAOe54DaTafwSm6suIQ=";

type HmacSha256 = Hmac<Sha256>;
type Aes128Cbc = Decryptor<Aes128>;

pub fn md5_hex(data: &[u8]) -> String {
    use md5::Digest as _;
    hex::encode(md5::Md5::digest(data))
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest as _;
    hex::encode(Sha256::digest(data))
}

/// Decrypts the obfuscated app secret using the same scheme as the Node
/// `appSecret()` in `solar-server.mjs`: md5(APP_ID) hex bytes split into a
/// 16-byte key and 16-byte IV for AES-128-CBC (no padding), then trailing NULs
/// are stripped.
pub fn app_secret() -> Result<String, InverterError> {
    let digest = md5_hex(APP_ID.as_bytes());
    let key = digest.as_bytes()[..16].to_vec();
    let iv = digest.as_bytes()[16..].to_vec();
    let encrypted = base64::engine::general_purpose::STANDARD
        .decode(APP_SECRET_ENC)
        .map_err(|_| InverterError::Crypto("invalid base64 app secret".into()))?;
    let mut buffer = encrypted;
    let cipher = Aes128Cbc::new_from_slices(&key, &iv)
        .map_err(|_| InverterError::Crypto("invalid aes key/iv".into()))?;
    let plaintext = cipher
        .decrypt_padded_mut::<NoPadding>(&mut buffer)
        .map_err(|_| InverterError::Crypto("aes decryption failed".into()))?;
    Ok(String::from_utf8_lossy(plaintext)
        .trim_end_matches('\0')
        .to_string())
}

pub fn generate_nonce() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Port of `signedHeaders()` from `solar-server.mjs` with a caller-supplied
/// nonce so the output is reproducible for tests. The body hash, base64 query
/// and signature all match the Node implementation for the same nonce.
pub fn signed_headers_with_nonce(body: &str, nonce: &str) -> Result<BTreeMap<String, String>, InverterError> {
    let body_hash = sha256_hex(body.as_bytes());
    let mut values = BTreeMap::new();
    values.insert("IOT-Open-AppID".to_string(), APP_ID.to_string());
    values.insert("IOT-Open-Body-Hash".to_string(), body_hash.clone());
    values.insert("IOT-Open-Nonce".to_string(), nonce.to_string());
    let query = values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let encoded = base64::engine::general_purpose::STANDARD.encode(query.as_bytes());
    let secret = app_secret()?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| InverterError::Crypto("invalid hmac key".into()))?;
    mac.update(encoded.as_bytes());
    let hmac_bytes = mac.finalize().into_bytes();
    let sign = md5_hex(&hmac_bytes);

    let mut headers = BTreeMap::new();
    headers.insert("Accept".to_string(), "application/json".to_string());
    headers.insert(
        "Content-Type".to_string(),
        "application/json; charset=utf-8".to_string(),
    );
    headers.insert("Origin".to_string(), "https://solar.siseli.com".to_string());
    headers.insert("Referer".to_string(), "https://solar.siseli.com/".to_string());
    headers.insert("IOT-Open-AppID".to_string(), APP_ID.to_string());
    headers.insert("IOT-Open-Nonce".to_string(), nonce.to_string());
    headers.insert("IOT-Open-Body-Hash".to_string(), body_hash);
    headers.insert("IOT-Open-Sign".to_string(), sign);
    Ok(headers)
}

pub fn signed_headers(body: &str) -> Result<BTreeMap<String, String>, InverterError> {
    signed_headers_with_nonce(body, &generate_nonce())
}

/// Thrown-away container to convert a `BTreeMap<String, String>` into a
/// `reqwest::HeaderMap` without pulling HTTP concerns into the pure crypto API.
pub struct AppIdHeaderMap;

impl AppIdHeaderMap {
    pub fn from_map(headers: BTreeMap<String, String>) -> HeaderMap {
        headers
            .into_iter()
            .filter_map(|(key, value)| {
                let name = key.parse::<HeaderName>().ok()?;
                let value = HeaderValue::from_str(&value).ok()?;
                Some((name, value))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Golden values captured from the Node implementation in
    // bms-bluetooth/solar-server.mjs with a fixed nonce.
    const NONCE: &str = "0123456789abcdef0123456789abcdef";
    const BODY: &str = "{\"account\":\"test\",\"password\":\"abc\"}";

    #[test]
    fn app_secret_matches_node() {
        assert_eq!(app_secret().unwrap(), "CJbrtLtqFES62bJ3ZW7c");
    }

    #[test]
    fn signed_headers_match_node_vectors() {
        let headers = signed_headers_with_nonce(BODY, NONCE).unwrap();
        assert_eq!(
            headers["IOT-Open-Body-Hash"],
            "b0cd154064b9f007318febaec5808b59604869f1fe1ee485fa76b65ae9ea006a"
        );
        assert_eq!(
            headers["IOT-Open-Sign"],
            "e5647b27e4681e94dc39b4a7e16014be"
        );
    }

    #[test]
    fn encoded_query_matches_node() {
        let _ = signed_headers_with_nonce(BODY, NONCE).unwrap();
        let body_hash = sha256_hex(BODY.as_bytes());
        let query = format!(
            "IOT-Open-AppID={APP_ID}&IOT-Open-Body-Hash={body_hash}&IOT-Open-Nonce={NONCE}"
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(query.as_bytes());
        assert_eq!(
            encoded,
            "SU9ULU9wZW4tQXBwSUQ9ckJyVFJmQVBYeiZJT1QtT3Blbi1Cb2R5LUhhc2g9YjBjZDE1NDA2NGI5ZjAwNzMxOGZlYmFlYzU4MDhiNTk2MDQ4NjlmMWZlMWVlNDg1ZmE3NmI2NWFlOWVhMDA2YSZJT1QtT3Blbi1Ob25jZT0wMTIzNDU2Nzg5YWJjZGVmMDEyMzQ1Njc4OWFiY2RlZg=="
        );
    }

    #[test]
    fn password_md5_matches_node() {
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn history_body_vectors_match_node() {
        let body = "{\"deviceId\":\"25882565467115827574\",\"count\":2000,\"page\":1,\"fromTime\":\"2026-09-23T00:00:00+05:00\",\"toTime\":\"2026-09-23T01:00:00+05:00\",\"orderByTimeAsc\":true,\"keys\":[\"pvInputPower\"]}";
        let headers = signed_headers_with_nonce(body, NONCE).unwrap();
        assert_eq!(
            headers["IOT-Open-Body-Hash"],
            "c50334c08b705ad72d09caf846a89d8f6421b68f7bf399ac916497340b97b6f5"
        );
        assert_eq!(
            headers["IOT-Open-Sign"],
            "d82d42ea4684948902460180e1005391"
        );
    }
}