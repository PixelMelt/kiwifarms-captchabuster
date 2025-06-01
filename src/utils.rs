use rand::Rng;

/// Generates an initial random float for the attempt nonce,
/// similar to `Math.random() * 4503599627370496` in JavaScript.
pub fn generate_initial_attempt_nonce_seed() -> f64 {
    let mut rng = rand::thread_rng();
    rng.gen::<f64>() * 4503599627370496.0
}