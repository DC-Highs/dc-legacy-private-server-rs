//! Packet signing.
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

use crate::config::SECRET_KEY;
use crate::json::to_ascii_json;

type HmacSha256 = Hmac<Sha256>;

pub fn sign_text(json_payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(SECRET_KEY.as_bytes()).expect("HMAC accepts any key length");
    mac.update(json_payload.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    let mut signed = String::with_capacity(signature.len() + 1 + json_payload.len());
    signed.push_str(&signature);
    signed.push(';');
    signed.push_str(json_payload);
    signed
}

pub fn sign_packet<T: Serialize + ?Sized>(payload: &T) -> String {
    sign_text(&to_ascii_json(payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn signature_matches_the_payload() {
        let signed = sign_packet(&json!({"a": 1, "b": "é"}));
        let (signature, body) = signed.split_once(';').unwrap();
        assert_eq!(body, "{\"a\":1,\"b\":\"\\u00e9\"}");
        assert_eq!(signature.len(), 64);
        assert_eq!(sign_text(body), signed);
    }
}
