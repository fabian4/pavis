/// Formats an IP address and port into a socket address string.
/// Handles IPv6 addresses by wrapping them in brackets if they don't already have them.
pub fn format_address(ip: &str, port: u16) -> String {
    if ip.contains(':') && !ip.starts_with('[') {
        format!("[{}]:{}", ip, port)
    } else {
        format!("{}:{}", ip, port)
    }
}
