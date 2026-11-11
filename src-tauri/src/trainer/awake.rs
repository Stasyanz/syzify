//! Keeping the Mac awake through a ride: macOS's own `caffeinate`, held
//! for as long as the ride runs. `-w` ties it to this process, so a crash
//! never leaves the Mac unable to sleep. No new dependency; elsewhere the
//! feature is hidden and this does nothing.

/// Held while riding; dropping it lets the Mac sleep again.
pub struct KeepAwake {
    #[cfg(target_os = "macos")]
    child: std::process::Child,
}

impl KeepAwake {
    /// None when it could not be had (not macOS, or caffeinate failed to
    /// start): the ride goes on, and a sleeping Mac is still saved by the
    /// journal.
    pub fn start() -> Option<KeepAwake> {
        #[cfg(target_os = "macos")]
        {
            // -i: no idle sleep; -d: the display stays on (the workout
            // screen is being watched).
            std::process::Command::new("/usr/bin/caffeinate")
                .args(["-i", "-d", "-w", &std::process::id().to_string()])
                .spawn()
                .ok()
                .map(|child| KeepAwake { child })
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for KeepAwake {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_holds_on_macos_and_lets_go_when_dropped() {
        let awake = KeepAwake::start();
        assert_eq!(awake.is_some(), cfg!(target_os = "macos"));
        #[cfg(target_os = "macos")]
        {
            let mut a = awake.expect("caffeinate");
            assert!(matches!(a.child.try_wait(), Ok(None)), "running");
            let pid = a.child.id();
            drop(a);
            // Gone: signal 0 to the pid fails once the child was reaped.
            let alive = std::process::Command::new("/bin/kill").args(["-0", &pid.to_string()]).status().map(|s| s.success());
            assert_eq!(alive.ok(), Some(false));
        }
    }
}
