//! HOTP – HMAC-Based One-Time Password (RFC 4226).
//!
//! Generates a one-time password from a shared secret and a monotonically
//! increasing counter value.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Generate an HOTP code per RFC 4226.
///
/// # Arguments
/// * `secret_b32` – base-32 encoded secret (no padding).
/// * `counter`    – 8-byte counter value.
/// * `digits`     – number of digits in the output (typically 6 or 8).
///
/// # Returns
/// The OTP code as a zero-padded decimal string.
pub fn generate_hotp(secret_b32: &str, counter: u64, digits: u32) -> Result<String, String> {
    if !(1..=9).contains(&digits) {
        return Err(format!(
            "digits must be between 1 and 9 (got {}); RFC 4226 recommends 6 or 8",
            digits
        ));
    }

    let secret = BASE32_NOPAD
        .decode(secret_b32.as_bytes())
        .map_err(|e| format!("Invalid base-32 secret: {}", e))?;

    let code = hotp_raw(&secret, counter, digits);
    Ok(format!("{:0>width$}", code, width = digits as usize))
}

/// Core HOTP computation (RFC 4226 §5.3).
pub(crate) fn hotp_raw(secret: &[u8], counter: u64, digits: u32) -> u32 {
    let mut mac =
        HmacSha1::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    // Dynamic truncation
    let offset = (result[19] & 0x0F) as usize;
    let bin_code = u32::from_be_bytes([
        result[offset] & 0x7F,
        result[offset + 1],
        result[offset + 2],
        result[offset + 3],
    ]);

    bin_code % 10u32.pow(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4226 Appendix D test vectors (secret = "12345678901234567890").
    /// The base-32 encoding of that ASCII string is:
    const SECRET_B32: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn test_rfc4226_vectors() {
        // Expected OTP values from RFC 4226 Appendix D.
        let expected = [
            "755224", "287082", "359152", "969429", "338314", "254676", "287922",
            "162583", "399871", "520489",
        ];

        for (counter, &exp) in expected.iter().enumerate() {
            let got = generate_hotp(SECRET_B32, counter as u64, 6).unwrap();
            assert_eq!(got, exp, "counter={}", counter);
        }
    }

    #[test]
    fn test_8_digit() {
        let code = generate_hotp(SECRET_B32, 0, 8).unwrap();
        assert_eq!(code.len(), 8);
    }

    #[test]
    fn test_invalid_secret() {
        let result = generate_hotp("!!!invalid!!!", 0, 6);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_digits_zero() {
        let result = generate_hotp(SECRET_B32, 0, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_digits_too_large() {
        let result = generate_hotp(SECRET_B32, 0, 10);
        assert!(result.is_err());
    }
}
