#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalImageProtocol {
    Sixel,
    KittyGraphics,
    Unsupported,
}

#[cfg(unix)]
extern "C" {
    fn fcntl(fd: i32, cmd: i32, ...) -> i32;
}

#[cfg(unix)]
const F_GETFL: i32 = 3;
#[cfg(unix)]
const F_SETFL: i32 = 4;
#[cfg(unix)]
const O_NONBLOCK: i32 = 0x0004;

pub fn probe() -> TerminalImageProtocol {
    use std::io::{Read, Write};
    use std::time::Instant;

    let query = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
    let mut stdout = std::io::stdout();
    if stdout.write_all(query).is_err() || stdout.flush().is_err() {
        return TerminalImageProtocol::Unsupported;
    }

    let deadline = Instant::now() + std::time::Duration::from_millis(200);
    let mut buf = Vec::new();
    let mut tmp = [0u8; 128];

    #[cfg(unix)]
    let old_flags = unsafe {
        let flags = fcntl(0, F_GETFL);
        fcntl(0, F_SETFL, flags | O_NONBLOCK);
        flags
    };

    loop {
        if Instant::now() >= deadline {
            break;
        }
        match std::io::stdin().read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.windows(2).any(|w| w == b"OK") {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(_) => break,
        }
    }

    #[cfg(unix)]
    unsafe {
        fcntl(0, F_SETFL, old_flags);
    }

    if buf.windows(2).any(|w| w == b"OK") {
        TerminalImageProtocol::KittyGraphics
    } else {
        TerminalImageProtocol::Unsupported
    }
}
