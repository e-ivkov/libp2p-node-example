use rand::distributions::Standard;
use rand::{thread_rng, Rng};
use std::time::SystemTime;

pub fn gen_random_bytes(n_bytes: usize) -> Vec<u8> {
    let rng = thread_rng();
    rng.sample_iter(Standard).take(n_bytes).collect()
}

pub fn current_time_millis() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Failed to get duration since UNIX_EPOCH.")
        .as_millis()
}
