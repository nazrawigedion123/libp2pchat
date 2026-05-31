use std::os::raw::c_char;

#[link(name = "govpn", kind = "static")]
unsafe extern "C" {
    pub fn StartDirectVPNTunnel(
        local_port: i32,
        public_listen_port: i32,
        remote_addr: *const c_char,
    );
}
