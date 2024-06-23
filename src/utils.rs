#![cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

pub fn poor_mans_random() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos()
}
