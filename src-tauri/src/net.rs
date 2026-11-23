//! One LAN address for this machine, chosen the way the routing table would
//! choose it: a UDP socket "connected" to a public address names the interface
//! the default route leaves from, without sending a packet.

use std::net::{IpAddr, Ipv4Addr, UdpSocket};

pub fn primary_interface() -> Ipv4Addr {
    // A phone with mobile data on has a default route out of the cellular
    // interface whenever Wi-Fi has no internet, so asking the routing table
    // answers "10.x from rmnet0" — an address nobody on the Wi-Fi can reach.
    // Advertising or showing that address makes the phone look offline.
    #[cfg(target_os = "android")]
    if let Some(ip) = wifi_interface() {
        return ip;
    }

    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80))?;
            socket.local_addr()
        })
        .ok()
        .and_then(|addr| match addr.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() => Some(ip),
            _ => None,
        })
        .unwrap_or(Ipv4Addr::UNSPECIFIED)
}

/// The Wi-Fi (or hotspot) address, ignoring the mobile-data interfaces.
#[cfg(target_os = "android")]
fn wifi_interface() -> Option<Ipv4Addr> {
    let mut fallback = None;
    for interface in if_addrs::get_if_addrs().ok()? {
        let name = interface.name.to_ascii_lowercase();
        if ["rmnet", "ccmni", "pdp", "clat", "tun", "dummy", "ip6tnl", "v4-"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let IpAddr::V4(ip) = interface.addr.ip() else {
            continue;
        };
        if ip.is_loopback() || ip.is_link_local() {
            continue;
        }
        if name.starts_with("wlan") || name.starts_with("ap") || name.starts_with("eth") {
            return Some(ip);
        }
        fallback.get_or_insert(ip);
    }
    fallback
}

/// The address a phone on the same LAN has to type. Falls back to the first
/// adapter when there is no default route (a machine with only a local switch).
pub fn lan_ip() -> String {
    let primary = primary_interface();
    if !primary.is_unspecified() {
        return primary.to_string();
    }
    if let Ok(list) = if_addrs::get_if_addrs() {
        for interface in list {
            if let IpAddr::V4(ip) = interface.addr.ip() {
                if !ip.is_loopback() && !ip.is_link_local() {
                    return ip.to_string();
                }
            }
        }
    }
    "127.0.0.1".to_string()
}
