//! TOTP – Time-Based One-Time Password (RFC 6238).
//!
//! Builds on HOTP by deriving the counter from the current Unix time.

use super::hotp;
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha512};

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

/// Generate a TOTP code per RFC 6238.
///
/// # Arguments
/// * `secret_b32` – base-32 encoded secret (no padding).
/// * `time`       – current Unix timestamp in seconds.
/// * `period`     – time step in seconds (commonly 30).
/// * `digits`     – number of digits (6 or 8).
/// * `algo`       – hash algorithm: `"SHA1"`, `"SHA256"`, or `"SHA512"`.
pub fn generate_totp(
    secret_b32: &str,
    time: u64,
    period: u64,
    digits: u32,
    algo: &str,
) -> Result<String, String> {
    if period == 0 {
        return Err("TOTP period must be > 0".to_string());
    }
    let counter = time / period;

    let secret = BASE32_NOPAD
        .decode(secret_b32.as_bytes())
        .map_err(|e| format!("Invalid base-32 secret: {}", e))?;

    let code = match algo.to_uppercase().as_str() {
        "SHA1" => hotp::hotp_raw(&secret, counter, digits),
        "SHA256" => totp_hmac_sha256(&secret, counter, digits),
        "SHA512" => totp_hmac_sha512(&secret, counter, digits),
        other => return Err(format!("Unsupported algorithm: {}", other)),
    };

    Ok(format!("{:0>width$}", code, width = digits as usize))
}

fn totp_hmac_sha256(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    dynamic_truncate(&result, digits)
}

fn totp_hmac_sha512(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac = HmacSha512::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();
    dynamic_truncate(&result, digits)
}

/// RFC 4226 dynamic truncation, generalized to any hash length ≥ 20.
fn dynamic_truncate(hmac_result: &[u8], digits: u32) -> u32 {
    let last = hmac_result.len() - 1;
    let offset = (hmac_result[last] & 0x0F) as usize;
    let bin_code = u32::from_be_bytes([
        hmac_result[offset] & 0x7F,
        hmac_result[offset + 1],
        hmac_result[offset + 2],
        hmac_result[offset + 3],
    ]);
    bin_code % 10u32.pow(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6238 test secret for SHA-1 (ASCII "12345678901234567890").
    const SECRET_SHA1_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn test_totp_sha1_known_time() {
        // RFC 6238 Appendix B, T = 59 → counter = 1 (period=30)
        // Expected TOTP with SHA-1, 8 digits: 94287082
        let code = generate_totp(SECRET_SHA1_B32, 59, 30, 8, "SHA1").unwrap();
        assert_eq!(code, "94287082");
    }

    #[test]
    fn test_totp_sha1_t1111111109() {
        // T = 1111111109 → counter = 37037036
        // Expected: 07081804
        let code = generate_totp(SECRET_SHA1_B32, 1111111109, 30, 8, "SHA1").unwrap();
        assert_eq!(code, "07081804");
    }

    #[test]
    fn test_totp_6_digits() {
        let code = generate_totp(SECRET_SHA1_B32, 59, 30, 6, "SHA1").unwrap();
        assert_eq!(code.len(), 6);
    }

    #[test]
    fn test_totp_bad_algo() {
        let result = generate_totp(SECRET_SHA1_B32, 59, 30, 6, "MD5");
        assert!(result.is_err());
    }

    #[test]
    fn test_totp_bad_secret() {
        let result = generate_totp("!!!invalid!!!", 59, 30, 6, "SHA1");
        assert!(result.is_err());
    }

    #[test]
    fn test_totp_zero_period() {
        let result = generate_totp(SECRET_SHA1_B32, 59, 0, 6, "SHA1");
        assert!(result.is_err());
    }
}
