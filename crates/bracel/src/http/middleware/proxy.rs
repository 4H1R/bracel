use axum::http::HeaderMap;
use std::net::IpAddr;
/// Forwarding information is considered only when the actual socket peer is trusted.
pub fn client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &[ipnet::IpNet]) -> IpAddr {
    if !trusted.iter().any(|network| network.contains(&peer)) {
        return peer;
    }
    let Some(raw) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.len() <= 1024)
    else {
        return peer;
    };
    let addresses = raw
        .split(',')
        .map(|v| v.trim().parse::<IpAddr>())
        .collect::<Result<Vec<_>, _>>();
    let Ok(addresses) = addresses else {
        return peer;
    };
    if addresses.len() > 16 {
        return peer;
    }
    addresses
        .into_iter()
        .rev()
        .find(|address| !trusted.iter().any(|network| network.contains(address)))
        .unwrap_or(peer)
}
