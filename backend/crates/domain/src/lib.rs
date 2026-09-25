//! Pure refund domain: types, policy rules, the deterministic engine, the prose
//! renderer and the heuristic pre-scan. No I/O and no async live here, so every
//! decision the system makes can be tested without a database or an LLM.

pub mod policy;
