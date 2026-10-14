//! Threads in the background whose panic is theirs alone: a cover, a
//! container's facts, a logo, the font list, the update check. Their
//! libraries read other people's files and pictures from the network
//! (lofty, image), and a panic there is no reason to end the player; the
//! panic hook (`main::install_panic_hook`) only logs one on such a thread,
//! where on any other it shows it and ends the program, as before.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread::JoinHandle;

/// The start of these threads' names, by which the hook knows them.
const PREFIX: &str = "worker: ";

/// Start `f` on a background thread named `name`.
pub fn spawn<T: Send + 'static>(
    name: &str,
    f: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<JoinHandle<T>> {
    std::thread::Builder::new()
        .name(format!("{PREFIX}{name}"))
        .spawn(f)
}

/// Whether the current thread is one of these.
pub fn current() -> bool {
    std::thread::current()
        .name()
        .is_some_and(|name| name.starts_with(PREFIX))
}

/// `f`'s result, or `None` if it panicked (logged by the hook): for a
/// thread that goes on with its next job.
pub fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_ends_the_job_not_the_program() {
        let handle = spawn("test", || {
            assert!(current());
            let lost = guarded(|| -> u32 { panic!("a broken picture") });
            let next = guarded(|| 7);
            (lost, next)
        })
        .unwrap();
        assert_eq!(handle.join().unwrap(), (None, Some(7)));
        assert!(!current());
        // A panic left uncaught ends the thread alone.
        assert!(
            spawn("test", || panic!("uncaught"))
                .unwrap()
                .join()
                .is_err()
        );
    }
}
