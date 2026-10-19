//! Single instance: a second launch asks the running one to show its window.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::time::Duration;

const PORT: u16 = 48820;
const OPEN: &[u8] = b"ak820-open";

/// Become the primary instance, calling `on_open` whenever another launch
/// asks to show the window. Returns `false` if another instance is already
/// running (it has been asked to show itself).
pub fn acquire(on_open: impl Fn() + Send + 'static) -> bool {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, PORT)) {
        Ok(listener) => {
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut buf = [0u8; 16];
                    let mut s = stream;
                    let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
                    if let Ok(n) = s.read(&mut buf) {
                        if &buf[..n] == OPEN {
                            on_open();
                        }
                    }
                }
            });
            true
        }
        // Port taken: if it is us, wake it up; if it is something else, just run.
        Err(_) => match TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, PORT).into(), Duration::from_millis(500)) {
            Ok(mut s) => s.write_all(OPEN).is_err(),
            Err(_) => true,
        },
    }
}
