use std::ffi::CString;
use std::thread;

use super::ffi::StartDirectVPNTunnel;

pub fn start_tunnel(local_port: i32, public_listen_port: i32, remote_addr: String) {
    thread::spawn(move || {
        let c_remote_addr = CString::new(remote_addr).expect("Invalid CString conversion");
        unsafe {
            StartDirectVPNTunnel(local_port, public_listen_port, c_remote_addr.as_ptr());
        }
    });
}
