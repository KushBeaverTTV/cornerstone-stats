//! TCP traffic per socket from the kernel's sock_diag netlink interface.
//!
//! Each TCP socket carries tcp_info with bytes_acked (sent and confirmed)
//! and bytes_received. UDP sockets have no byte counters there, so per
//! process traffic is TCP only.

const NETLINK_SOCK_DIAG: i32 = 4;
const SOCK_DIAG_BY_FAMILY: u16 = 20;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
const INET_DIAG_INFO: u16 = 2;
const TCP_ESTABLISHED: u8 = 1;
const TCP_LISTEN: u8 = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TcpSocket {
    pub inode: u64,
    pub sent: u64,
    pub received: u64,
    pub established: bool,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_ne_bytes([b[at], b[at + 1]])
}
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_ne_bytes(b[at..at + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    u64::from_ne_bytes(b[at..at + 8].try_into().unwrap())
}

/// Parse one received datagram of netlink messages. Returns false at the
/// end of the dump.
pub fn parse_messages(buf: &[u8], out: &mut Vec<TcpSocket>) -> bool {
    let mut at = 0;
    while at + 16 <= buf.len() {
        let len = u32_at(buf, at) as usize;
        let kind = u16_at(buf, at + 4);
        if len < 16 || at + len > buf.len() {
            return false;
        }
        if kind == NLMSG_DONE || kind == NLMSG_ERROR {
            return false;
        }
        let msg = &buf[at + 16..at + len];
        if kind == SOCK_DIAG_BY_FAMILY && msg.len() >= 72 {
            let state = msg[1];
            let inode = u32_at(msg, 68) as u64;
            let mut sock = TcpSocket { inode, sent: 0, received: 0, established: state == TCP_ESTABLISHED };
            let mut a = 72;
            while a + 4 <= msg.len() {
                let rlen = u16_at(msg, a) as usize;
                let rtype = u16_at(msg, a + 2);
                if rlen < 4 || a + rlen > msg.len() {
                    break;
                }
                if rtype == INET_DIAG_INFO && rlen >= 4 + 136 {
                    let info = &msg[a + 4..a + rlen];
                    sock.sent = u64_at(info, 120);
                    sock.received = u64_at(info, 128);
                }
                a += (rlen + 3) & !3;
            }
            if inode != 0 && state != TCP_LISTEN {
                out.push(sock);
            }
        }
        at += (len + 3) & !3;
    }
    true
}

fn request(family: u8) -> [u8; 72] {
    let mut r = [0u8; 72];
    r[0..4].copy_from_slice(&72u32.to_ne_bytes());
    r[4..6].copy_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
    let flags: u16 = 0x1 | 0x300; // NLM_F_REQUEST | NLM_F_DUMP
    r[6..8].copy_from_slice(&flags.to_ne_bytes());
    r[8..12].copy_from_slice(&1u32.to_ne_bytes());
    // inet_diag_req_v2
    r[16] = family;
    r[17] = libc::IPPROTO_TCP as u8;
    r[18] = 1 << (INET_DIAG_INFO - 1);
    r[20..24].copy_from_slice(&u32::MAX.to_ne_bytes()); // every state
    r
}

/// Every TCP socket of the machine with its byte counters, or None when
/// sock_diag is unavailable.
pub fn tcp_sockets() -> Option<Vec<TcpSocket>> {
    let fd = unsafe { libc::socket(libc::AF_NETLINK, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, NETLINK_SOCK_DIAG) };
    if fd < 0 {
        return None;
    }
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut ok = true;
    for family in [libc::AF_INET as u8, libc::AF_INET6 as u8] {
        let req = request(family);
        let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
        addr.nl_family = libc::AF_NETLINK as u16;
        let sent = unsafe {
            libc::sendto(fd, req.as_ptr() as *const _, req.len(), 0, &addr as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_nl>() as u32)
        };
        if sent < 0 {
            ok = false;
            break;
        }
        loop {
            let n = unsafe { libc::recv(fd, buf.as_mut_ptr() as *mut _, buf.len(), 0) };
            if n <= 0 {
                break;
            }
            if !parse_messages(&buf[..n as usize], &mut out) {
                break;
            }
        }
    }
    unsafe { libc::close(fd) };
    ok.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_socket_with_tcp_info() {
        let mut msg = vec![0u8; 72];
        msg[1] = TCP_ESTABLISHED;
        msg[68..72].copy_from_slice(&4242u32.to_ne_bytes());
        let mut info = vec![0u8; 140];
        info[120..128].copy_from_slice(&1000u64.to_ne_bytes());
        info[128..136].copy_from_slice(&5000u64.to_ne_bytes());
        let rlen = 4 + info.len();
        msg.extend_from_slice(&(rlen as u16).to_ne_bytes());
        msg.extend_from_slice(&INET_DIAG_INFO.to_ne_bytes());
        msg.extend_from_slice(&info);
        let mut buf = Vec::new();
        buf.extend_from_slice(&((16 + msg.len()) as u32).to_ne_bytes());
        buf.extend_from_slice(&SOCK_DIAG_BY_FAMILY.to_ne_bytes());
        buf.extend_from_slice(&[0u8; 10]);
        buf.extend_from_slice(&msg);
        let mut done = vec![0u8; 16];
        done[0..4].copy_from_slice(&16u32.to_ne_bytes());
        done[4..6].copy_from_slice(&NLMSG_DONE.to_ne_bytes());
        buf.extend_from_slice(&done);
        let mut out = Vec::new();
        assert!(!parse_messages(&buf, &mut out));
        assert_eq!(out, vec![TcpSocket { inode: 4242, sent: 1000, received: 5000, established: true }]);
    }

    #[test]
    fn live_dump_works() {
        // sock_diag is available to unprivileged users on Linux.
        assert!(tcp_sockets().is_some());
    }
}
