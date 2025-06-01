use sha2::{Digest, Sha256};
use std::time::Instant;
use rayon::prelude::*;
use log::{debug, error, info};
use std::sync::mpsc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Solves the SSSG Proof-of-Work challenge.
///
/// # Arguments
/// * `salt_str` - The salt string provided by the server.
/// * `difficulty` - The required number of leading zero bits.
/// * `initial_attempt_base` - A base value for starting attempt nonces. Each thread will start from this base + its thread index.
/// * `num_threads` - The number of threads to use for solving.
///
/// # Returns
/// An `Option` containing a tuple of `(successful_attempt_string, hex_encoded_hash_solution)` if a solution is found,
/// otherwise `None` (though in the current parallel implementation, it will block until a solution is found by one thread).
pub fn solve_challenge(salt_str: &str, difficulty: u32, initial_attempt_base: f64, num_threads: usize) -> Option<(String, String)> {
    // The debug! macro was already here from a previous attempt, which is good.
    // Ensuring the function signature is correct and suppress_logging is removed.
    debug!("[PoW Solver] Received Salt: \"{}\", Difficulty: {}", salt_str, difficulty);
    let start_time = Instant::now();
    let (tx, rx) = mpsc::channel();
    let solution_found_flag = Arc::new(AtomicBool::new(false));

    (0..num_threads).into_par_iter().for_each_with(tx, |tx_clone, thread_idx| {
        // Each thread starts its attempt numbers from a slightly different base to reduce overlap,
        // and then increments by the total number of threads to ensure unique attempt spaces.
        let mut current_attempt_val = initial_attempt_base + thread_idx as f64;
        let mut iteration_count: u64 = 0; // Iteration counter for yield logic
        
        loop {
            if solution_found_flag.load(Ordering::Relaxed) {
                return; // Another thread found the solution
            }

            // 1. Convert attempt number to string EXACTLY as JavaScript does.
            //    JS `String(float)` or `${float}`. For `1419766378392277.5`, it's "1419766378392277.5".
            //    Rust's default `format!("{}", float)` for non-fractional floats (e.g., 1.0) might produce "1" instead of "1.0".
            //    However, for large floats with fractional parts, it's generally okay.
            //    If precision issues arise, a more specific formatting might be needed,
            //    or ensuring the number always has a fractional part if JS behaves that way.
            //    For now, standard formatting is used.
            let attempt_str = format!("{:.1}", current_attempt_val);

            // 2. Create a SHA-256 hasher and process the data (as UTF-8 bytes).
            //    Update hasher with salt and attempt string separately to avoid intermediate allocation.
            let mut hasher = Sha256::new();
            hasher.update(salt_str.as_bytes());
            hasher.update(attempt_str.as_bytes());
            let hash_result = hasher.finalize(); // This is GenericArray<u8, U32>

            // 4. Extract the first 32 bits (4 bytes) of the hash.
            //    SHA-256 output is big-endian.
            let first_word_bytes: [u8; 4] = [hash_result[0], hash_result[1], hash_result[2], hash_result[3]];
            let first_word_u32 = u32::from_be_bytes(first_word_bytes);

            // 5. Count leading zeros.
            let leading_zeros = first_word_u32.leading_zeros();

            // 6. Check against difficulty.
            if leading_zeros >= difficulty {
                if !solution_found_flag.swap(true, Ordering::Relaxed) { // Atomically set flag and check previous value
                    let solution_hex = hex::encode(hash_result);
                    // Send the successful attempt string and the hex solution
                    tx_clone.send(Some((attempt_str, solution_hex))).unwrap_or_else(|e| {
                        error!("Solver: Error sending solution: {}",e);
                    });
                }
                return; // Solution found by this thread
            }
            
            // Increment attempt value for the next iteration for this thread.
            // Each thread increments by `num_threads` to ensure they are checking different numbers.
            current_attempt_val += num_threads as f64;

            // Basic yield to prevent a single thread from hogging CPU completely if running on a system
            // where Rayon's work-stealing isn't perfectly balancing very tight loops.
            // Consider removing if performance is impacted and not needed.
            iteration_count += 1;
            if iteration_count % 10000 == 0 { // Periodically yield, e.g., every 10000 iterations
                 std::thread::yield_now();
            }
        }
    });

    // Wait for the first solution from any thread.
    // `rx.recv()` will block until a message is sent or the channel is closed.
    // If all sender threads exit without sending (e.g., if solution_found_flag was set by an external mechanism not shown here),
    // recv() would error. In this setup, one thread is expected to send.
    match rx.recv() {
        Ok(solution_opt) => {
            if solution_opt.is_some() {
                let duration = start_time.elapsed();
                info!("[TIMING] PoW solve_challenge took {:.2?}", duration);
            }
            solution_opt
        }
        Err(e) => {
            error!("Solver: Error receiving solution from worker threads: {}. This might happen if all threads exited prematurely.", e);
            None
        }
    }
}