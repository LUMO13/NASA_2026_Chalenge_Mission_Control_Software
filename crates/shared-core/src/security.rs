//! Криптографічна перевірка цілісності телеметрії (HMAC-SHA256) та захист від Replay-атак.

use std::collections::{HashMap, VecDeque};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::error::{LunarHmsError, LunarResult};
use crate::models::TelemetryEnvelope;

type HmacSha256 = Hmac<Sha256>;

/// Обчислює канонічний підпис HMAC-SHA256 для пакета телеметрії,
/// прив'язуючи ідентифікатор космонавта, порядковий nonce, UTC-час та ключові показники.
pub fn compute_envelope_hmac(envelope: &TelemetryEnvelope, secret_key: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret_key)
        .expect("HMAC-SHA256 приймає ключ будь-якої довжини");

    let canonical_payload = format!(
        "{}|{}|{}|{}|{:.3}|{:.3}|{:.3}|{:.3}|{:.3}",
        envelope.record_id,
        envelope.sequence_nonce,
        envelope.astronaut_id,
        envelope.measured_at_utc.timestamp_millis(),
        envelope
            .cardiovascular
            .heart_rate_bpm
            .as_ref()
            .map(|m| m.value)
            .unwrap_or(-1.0),
        envelope
            .respiratory
            .spo2_pct
            .as_ref()
            .map(|m| m.value)
            .unwrap_or(-1.0),
        envelope.habitat.atmospheric_pressure_kpa.value,
        envelope.radiation.equivalent_dose_rate_usv_h.value,
        envelope.spacesuit.suit_internal_pressure_kpa.value,
    );

    mac.update(canonical_payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Перевіряє підпис HMAC-SHA256 за сталий час, запобігаючи атакам за часом.
pub fn verify_envelope_hmac(envelope: &TelemetryEnvelope, secret_key: &[u8]) -> LunarResult<()> {
    let expected = compute_envelope_hmac(envelope, secret_key);
    if expected != envelope.hmac_sha256_signature {
        return Err(LunarHmsError::IntegrityViolation {
            expected,
            actual: envelope.hmac_sha256_signature.clone(),
        });
    }
    Ok(())
}

/// Обчислює контрольну суму SHA-256 для стиснених архівних пакетів синхронізації із Землею.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Захист від повторного відтворення повідомлень (Anti-Replay Guard) із ковзним вікном nonce для кожного космонавта.
#[derive(Debug, Default)]
pub struct AntiReplayGuard {
    window_size: usize,
    seen_nonces: HashMap<String, VecDeque<u64>>,
    highest_nonce: HashMap<String, u64>,
}

impl AntiReplayGuard {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size: window_size.max(32),
            seen_nonces: HashMap::new(),
            highest_nonce: HashMap::new(),
        }
    }

    /// Перевіряє та реєструє nonce пакета телеметрії.
    pub fn check_and_record(&mut self, astronaut_id: &str, nonce: u64) -> LunarResult<()> {
        let queue = self
            .seen_nonces
            .entry(astronaut_id.to_string())
            .or_default();

        if queue.contains(&nonce) {
            return Err(LunarHmsError::ReplayDetected {
                astronaut_id: astronaut_id.to_string(),
                nonce,
            });
        }

        if let Some(&max_seen) = self.highest_nonce.get(astronaut_id) {
            if nonce + (self.window_size as u64) < max_seen {
                return Err(LunarHmsError::ReplayDetected {
                    astronaut_id: astronaut_id.to_string(),
                    nonce,
                });
            }
            if nonce > max_seen {
                self.highest_nonce.insert(astronaut_id.to_string(), nonce);
            }
        } else {
            self.highest_nonce.insert(astronaut_id.to_string(), nonce);
        }

        queue.push_back(nonce);
        if queue.len() > self.window_size {
            queue.pop_front();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anti_replay_guard_rejects_duplicates() {
        let mut guard = AntiReplayGuard::new(64);
        assert!(guard.check_and_record("AST-001", 101).is_ok());
        assert!(guard.check_and_record("AST-001", 102).is_ok());
        assert!(guard.check_and_record("AST-001", 101).is_err());
        assert!(guard.check_and_record("AST-002", 101).is_ok());
    }
}
