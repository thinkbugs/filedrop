//! App-to-app file transfer discovery.
//!
//! Every FileDrop with file transfer switched on registers a `_filedrop._tcp`
//! service whose TXT record carries its display name and an instance id.
//! Another instance browses the same type and gets a list of peers it can POST
//! a file to — no subnet sweep, no typing an address.
//!
//! The id is what keeps two instances on one machine apart: the display name
//! may be the same, the instance id never is.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;

pub const SERVICE_TYPE: &str = "_filedrop._tcp.local.";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
}

pub struct FileDiscovery {
    daemon: ServiceDaemon,
    peers: Arc<Mutex<HashMap<String, Peer>>>,
    instance_id: String,
}

impl FileDiscovery {
    pub fn start(instance_id: &str, name: &str, port: u16) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|error| format!("无法启动 mDNS：{error}"))?;

        let address = crate::net::primary_interface();
        let ip = if address.is_unspecified() {
            "127.0.0.1".to_string()
        } else {
            address.to_string()
        };
        let host = format!("{}.local.", host_label(name, instance_id));
        let properties = [("id", instance_id), ("name", name), ("v", "1")];
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            instance_id,
            &host,
            ip.as_str(),
            port,
            &properties[..],
        )
        .map_err(|error| format!("无法构造文件服务：{error}"))?
        .enable_addr_auto();
        daemon
            .register(service)
            .map_err(|error| format!("无法广播文件服务：{error}"))?;

        let peers: Arc<Mutex<HashMap<String, Peer>>> = Arc::new(Mutex::new(HashMap::new()));
        let sink = Arc::clone(&peers);
        let browser = daemon
            .browse(SERVICE_TYPE)
            .map_err(|error| format!("无法浏览文件服务：{error}"))?;
        let mine = instance_id.to_string();
        // Prefer a peer address on the room network: a machine with Hyper-V or a
        // VPN registers those adapters too, and they are not reachable from the
        // other side of the LAN.
        let local = crate::net::primary_interface().octets();
        thread::Builder::new()
            .name("filedrop-peers".into())
            .spawn(move || {
                while let Ok(event) = browser.recv() {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let id = info
                                .get_property_val_str("id")
                                .unwrap_or_default()
                                .to_string();
                            if id.is_empty() || id == mine {
                                continue;
                            }
                            // IPv4 first: the upload client connects with a plain host:port.
                            let addresses = info.get_addresses();
                            let same_lan = |address: &std::net::IpAddr| match address {
                                std::net::IpAddr::V4(v4) => v4.octets()[..2] == local[..2],
                                std::net::IpAddr::V6(_) => false,
                            };
                            // A link-local IPv6 address is unreachable without a zone
                            // id, and mDNS answers sometimes carry only AAAA. Letting
                            // such an answer replace a working IPv4 host is what makes
                            // a peer look offline for a while and then online again.
                            let routable = |address: &&std::net::IpAddr| match **address {
                                std::net::IpAddr::V4(_) => true,
                                std::net::IpAddr::V6(v6) => {
                                    !v6.is_unicast_link_local() && !v6.is_loopback()
                                }
                            };
                            let picked = addresses
                                .iter()
                                .find(|address| same_lan(address))
                                .or_else(|| {
                                    addresses
                                        .iter()
                                        .find(|address| address.is_ipv4())
                                })
                                .or_else(|| addresses.iter().find(routable))
                                // Last resort: an address that may not connect is
                                // still better than dropping the peer from the
                                // list, which the send path would read as offline.
                                .or_else(|| addresses.iter().next())
                                .cloned();
                            let host = match picked {
                                Some(address) => address.to_string(),
                                // Nothing usable here: keep the address the peer
                                // already had instead of forgetting it.
                                None => match sink
                                    .lock()
                                    .ok()
                                    .and_then(|peers| peers.get(&id).map(|peer| peer.host.clone()))
                                {
                                    Some(host) => host,
                                    None => continue,
                                },
                            };
                            let peer = Peer {
                                name: info
                                    .get_property_val_str("name")
                                    .unwrap_or(info.get_fullname())
                                    .to_string(),
                                id,
                                host,
                                port: info.get_port(),
                            };
                            if let Ok(mut peers) = sink.lock() {
                                peers.insert(peer.id.clone(), peer);
                            }
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            if let Ok(mut peers) = sink.lock() {
                                peers.retain(|_, peer| !fullname.contains(&peer.id));
                            }
                        }
                        _ => {}
                    }
                }
            })
            .ok();

        Ok(Self {
            daemon,
            peers,
            instance_id: instance_id.to_string(),
        })
    }

    pub fn peers(&self) -> Vec<Peer> {
        let mut list: Vec<Peer> = self
            .peers
            .lock()
            .map(|peers| peers.values().cloned().collect())
            .unwrap_or_default();
        list.retain(|peer| peer.id != self.instance_id);
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

impl Drop for FileDiscovery {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.instance_id);
        let _ = self.daemon.shutdown();
    }
}

/// mDNS instance labels may not contain dots or spaces.
fn host_label(name: &str, id: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let clean = clean.trim_matches('-');
    if clean.is_empty() {
        format!("filedrop-{}", &id[..id.len().min(8)])
    } else {
        format!("{clean}-{}", &id[..id.len().min(6)])
    }
}
