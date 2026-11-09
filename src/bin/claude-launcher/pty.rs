//! fzf runs in a small terminal of its own whose output is copied to Konsole. Talking to
//! Konsole directly, fzf's "synchronized update" sequences sometimes freeze the window
//! (blank until a key is pressed), so they are dropped on the way.

use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const SYNC_ON: &[u8] = b"\x1b[?2026h";
const SYNC_OFF: &[u8] = b"\x1b[?2026l";

#[derive(Default)]
struct SyncFilter {
    carry: Vec<u8>,
}

impl SyncFilter {
    /// Keeps a sequence cut at the end of a chunk to glue it to the next one.
    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        let mut buf = std::mem::take(&mut self.carry);
        buf.extend_from_slice(data);
        let mut out = Vec::with_capacity(buf.len());
        let mut rest = &buf[..];
        loop {
            let Some(at) = rest.iter().position(|&b| b == 0x1b) else {
                out.extend_from_slice(rest);
                break;
            };
            out.extend_from_slice(&rest[..at]);
            rest = &rest[at..];
            if rest.starts_with(SYNC_ON) || rest.starts_with(SYNC_OFF) {
                rest = &rest[SYNC_ON.len()..];
            } else if rest.len() < SYNC_ON.len() && (SYNC_ON.starts_with(rest) || SYNC_OFF.starts_with(rest)) {
                self.carry = rest.to_vec();
                break;
            } else {
                out.push(0x1b);
                rest = &rest[1..];
            }
        }
        out
    }
}

static MASTER: AtomicI32 = AtomicI32::new(-1);

fn copy_size(to: RawFd) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe {
        if libc::ioctl(0, libc::TIOCGWINSZ, &mut ws) == 0 {
            libc::ioctl(to, libc::TIOCSWINSZ, &ws);
        }
    }
}

extern "C" fn on_resize(_: libc::c_int) {
    copy_size(MASTER.load(Ordering::Relaxed));
}

fn interrupted() -> bool {
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR)
}

fn write_all(fd: RawFd, mut data: &[u8]) {
    while !data.is_empty() {
        let n = unsafe { libc::write(fd, data.as_ptr().cast(), data.len()) };
        if n < 0 {
            if interrupted() {
                continue;
            }
            return;
        }
        data = &data[n as usize..];
    }
}

fn relay(master: RawFd) {
    let mut filter = SyncFilter::default();
    let mut buf = vec![0u8; 65536];
    let mut fds = [
        libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 },
        libc::pollfd { fd: master, events: libc::POLLIN, revents: 0 },
    ];
    loop {
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } < 0 {
            if interrupted() {
                continue;
            }
            break;
        }
        if fds[1].revents != 0 {
            let n = unsafe { libc::read(master, buf.as_mut_ptr().cast(), buf.len()) };
            if n < 0 && interrupted() {
                continue;
            }
            if n <= 0 {
                break; // fzf is gone
            }
            write_all(1, &filter.feed(&buf[..n as usize]));
        }
        if fds[0].revents != 0 {
            let n = unsafe { libc::read(0, buf.as_mut_ptr().cast(), 4096) };
            if n > 0 {
                write_all(master, &buf[..n as usize]);
            } else if n == 0 || !interrupted() {
                fds[0].fd = -1;
            }
        }
    }
}

/// Runs fzf with `input` as its list; returns its exit code and what it printed.
pub fn run_fzf(args: &[String], input: String) -> (i32, String) {
    let (mut master, mut slave) = (-1, -1);
    if unsafe { libc::openpty(&mut master, &mut slave, null_mut(), null(), null()) } != 0 {
        return (2, String::new());
    }
    unsafe {
        libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(slave, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    copy_size(slave);
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    let spawned = {
        let mut cmd = Command::new("fzf");
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::from(slave));
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                libc::ioctl(2, libc::TIOCSCTTY, 0);
                Ok(())
            });
        }
        cmd.spawn()
    }; // our copy of the slave closes here, so the relay sees fzf exit
    let Ok(mut child) = spawned else {
        unsafe { libc::close(master) };
        return (2, String::new());
    };
    if let Some(mut stdin) = child.stdin.take() {
        thread::spawn(move || {
            let _ = stdin.write_all(input.as_bytes());
        });
    }
    let (tx, rx) = mpsc::channel();
    if let Some(mut stdout) = child.stdout.take() {
        thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.read_to_end(&mut out);
            let _ = tx.send(out);
        });
    }

    MASTER.store(master, Ordering::Relaxed);
    let mut old_winch: libc::sigaction = unsafe { std::mem::zeroed() };
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    let raw_mode = unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = on_resize as *const () as usize;
        libc::sigemptyset(&mut sa.sa_mask);
        libc::sigaction(libc::SIGWINCH, &sa, &mut old_winch);
        let ok = libc::tcgetattr(0, &mut saved) == 0;
        if ok {
            let mut raw = saved;
            libc::cfmakeraw(&mut raw);
            libc::tcsetattr(0, libc::TCSANOW, &raw);
        }
        ok
    };

    relay(master);

    unsafe {
        if raw_mode {
            libc::tcsetattr(0, libc::TCSADRAIN, &saved);
        }
        libc::sigaction(libc::SIGWINCH, &old_winch, null_mut());
        MASTER.store(-1, Ordering::Relaxed);
        libc::close(master);
    }
    let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
    let out = rx.recv_timeout(Duration::from_millis(500)).unwrap_or_default();
    (code, String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::SyncFilter;

    #[test]
    fn drops_sync_even_when_split() {
        let mut f = SyncFilter::default();
        let mut out = f.feed(b"a\x1b[?2026hb\x1b[?20");
        out.extend(f.feed(b"26lc\x1b[Ad\x1b"));
        out.extend(f.feed(b"[0m"));
        assert_eq!(out, b"abc\x1b[Ad\x1b[0m");
    }
}
