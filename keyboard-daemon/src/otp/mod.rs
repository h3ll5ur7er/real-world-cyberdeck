//! OTP (One-Time Password) generation module.
//!
//! Implements HOTP (RFC 4226) and TOTP (RFC 6238).
//!
//! This is a **Phase 1 standalone implementation**.  In Phase 2 the daemon
//! will retrieve OTP codes from Vaultwarden via its REST API, and this module
//! will serve as an offline fallback only.

pub mod hotp;
pub mod totp;
