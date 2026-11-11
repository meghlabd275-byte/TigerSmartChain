//! Guard against server-side request forgery when fetching a metadata URI.
//!
//! A token's `tokenURI` is set by whoever deployed or minted the token, so the
//! value reaching `fetch_from_uri` is attacker-controlled. Without a check, a
//! hostile collection can point its metadata at `http://169.254.169.254/...`
//! (cloud instance metadata), `http://127.0.0.1:8545` (the node's own RPC port)
//! or any host on the internal network, and the indexer will fetch it and hand
//! the body back as "metadata". That turns the indexer into a proxy for the
//! cluster it runs in.
//!
//! The rule here is to resolve the host and refuse anything that is not a
//! globally routable unicast address. Resolving first matters: a hostname like
//! `metadata.internal` can point at a private address, so checking the literal
//! string is not enough.
//!
//! This deliberately does not try to be a complete egress policy. It blocks the
//! private, loopback, link-local, shared and reserved ranges, which is what
//! separates "reach the public internet" from "reach this machine and its
//! neighbours".

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::indexer::NFTMetadataError;

/// Whether an address is outside the globally routable unicast space.
///
/// Everything rejected here is an address that only has meaning inside a
/// private network, on the local machine, or as a special-purpose reservation.
pub fn is_disallowed_address(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => is_disallowed_v4(v4),
        IpAddr::V6(v6) => is_disallowed_v6(v6),
    }
}

fn is_disallowed_v4(addr: Ipv4Addr) -> bool {
    let [a, b, c, _] = addr.octets();

    // This host, and the whole 127/8 loopback block.
    addr.is_loopback()
    // The "this network" block (0/8) and its broadcast address.
    || a == 0
    // Private-use: 10/8, 172.16/12, 192.168/16.
    || addr.is_private()
    // Link-local, which is where cloud instance metadata lives (169.254/16).
    || addr.is_link_local()
    // Carrier-grade NAT, 100.64/10.
    || (a == 100 && (64..=127).contains(&b))
    // IETF protocol assignments, 192.0.0/24.
    || (a == 192 && b == 0 && c == 0)
    // TEST-NET-1/2/3, which must never appear as a real destination.
    || (a == 192 && b == 0 && c == 2)
    || (a == 198 && b == 51 && c == 100)
    || (a == 203 && b == 0 && c == 113)
    // Benchmarking, 198.18/15.
    || (a == 198 && (18..=19).contains(&b))
    // Multicast and the reserved 240/4 block.
    || addr.is_multicast()
    || a >= 240
    // The limited broadcast address.
    || addr.is_broadcast()
}

fn is_disallowed_v6(addr: Ipv6Addr) -> bool {
    // Unspecified (::), loopback (::1) and IPv4-mapped forms such as
    // ::ffff:127.0.0.1, which must be checked as their IPv4 equivalent.
    if addr.is_unspecified() || addr.is_loopback() {
        return true;
    }
    if let Some(v4) = addr.to_ipv4_mapped() {
        return is_disallowed_v4(v4);
    }

    let segments = addr.segments();
    // Unique local addresses, fc00::/7.
    (segments[0] & 0xfe00) == 0xfc00
    // Link-local, fe80::/10.
    || (segments[0] & 0xffc0) == 0xfe80
    // Multicast, ff00::/8.
    || (segments[0] & 0xff00) == 0xff00
    // IPv4-compatible and NAT64 forms embed an IPv4 address that may be private.
    || is_disallowed_embedded_v4(segments)
}

/// Check the IPv4 address embedded in the 6to4 (`2002::/16`) and
/// NAT64 (`64:ff9b::/96`) forms.
fn is_disallowed_embedded_v4(segments: [u16; 8]) -> bool {
    if segments[0] == 0x2002 {
        let v4 = Ipv4Addr::new(
            (segments[1] >> 8) as u8,
            segments[1] as u8,
            (segments[2] >> 8) as u8,
            segments[2] as u8,
        );
        return is_disallowed_v4(v4);
    }
    if segments[0] == 0x0064 && segments[1] == 0xff9b {
        let v4 = Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            segments[6] as u8,
            (segments[7] >> 8) as u8,
            segments[7] as u8,
        );
        return is_disallowed_v4(v4);
    }
    false
}

/// Parse `uri` and confirm it is an `http`/`https` URL whose host resolves only
/// to globally routable addresses.
///
/// Returns the parsed URL on success. A URI with any other scheme is rejected,
/// as are hosts that fail to resolve.
pub async fn validate_uri(uri: &str) -> Result<url::Url, NFTMetadataError> {
    let parsed = url::Url::parse(uri)
        .map_err(|e| NFTMetadataError::UnsupportedURI(format!("{uri}: {e}")))?;

    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(NFTMetadataError::UnsupportedURI(format!(
                "{uri}: scheme {other} is not allowed for metadata"
            )));
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| NFTMetadataError::UnsupportedURI(format!("{uri}: no host")))?;

    // `host_str` keeps the brackets on an IPv6 literal, which `lookup_host`
    // does not accept; strip them before resolving.
    let host_for_lookup = host.trim_start_matches('[').trim_end_matches(']');
    let port = parsed.port_or_known_default().unwrap_or(443);

    let addrs: Vec<IpAddr> = match tokio::net::lookup_host((host_for_lookup, port)).await {
        Ok(resolved) => resolved.map(|socket| socket.ip()).collect(),
        Err(e) => {
            return Err(NFTMetadataError::RequestError(format!(
                "could not resolve {host}: {e}"
            )));
        }
    };

    if addrs.is_empty() {
        return Err(NFTMetadataError::RequestError(format!(
            "could not resolve {host}"
        )));
    }

    // Refuse the request if *any* resolved address is disallowed. A host that
    // resolves to both a public and a private address is a rebinding attempt,
    // and picking the public one would let the private one be reached on a
    // later request.
    for addr in &addrs {
        if is_disallowed_address(*addr) {
            return Err(NFTMetadataError::UnsupportedURI(format!(
                "{uri}: host {host} resolves to {addr}, which is not publicly routable"
            )));
        }
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> IpAddr {
        IpAddr::V4(s.parse().unwrap())
    }

    fn v6(s: &str) -> IpAddr {
        IpAddr::V6(s.parse().unwrap())
    }

    #[test]
    fn blocks_loopback_and_private_ranges() {
        for addr in [
            "127.0.0.1",
            "127.1.2.3",
            "0.0.0.0",
            "10.0.0.1",
            "172.16.5.5",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254", // cloud instance metadata
            "100.64.0.1",
            "192.0.0.1",
            "198.18.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            assert!(is_disallowed_address(v4(addr)), "{addr} should be blocked");
        }
    }

    #[test]
    fn allows_public_addresses() {
        for addr in ["1.1.1.1", "8.8.8.8", "93.184.216.34", "172.32.0.1", "172.15.255.255"] {
            assert!(!is_disallowed_address(v4(addr)), "{addr} should be allowed");
        }
    }

    #[test]
    fn blocks_internal_ipv6_and_embedded_ipv4() {
        for addr in [
            "::1",
            "::",
            "fe80::1",
            "fc00::1",
            "fd00::1",
            "ff02::1",
            "::ffff:127.0.0.1",  // IPv4-mapped loopback
            "::ffff:10.0.0.1",   // IPv4-mapped private
            "2002:7f00:0001::",  // 6to4 embedding 127.0.0.1
            "64:ff9b::a00:1",    // NAT64 embedding 10.0.0.1
        ] {
            assert!(is_disallowed_address(v6(addr)), "{addr} should be blocked");
        }
    }

    #[test]
    fn allows_public_ipv6() {
        for addr in ["2606:4700:4700::1111", "2001:4860:4860::8888"] {
            assert!(!is_disallowed_address(v6(addr)), "{addr} should be allowed");
        }
    }

    #[tokio::test]
    async fn rejects_non_http_schemes() {
        for uri in [
            "file:///etc/passwd",
            "ftp://example.com/x",
            "gopher://example.com",
            "data:text/plain,hello",
        ] {
            assert!(
                validate_uri(uri).await.is_err(),
                "{uri} should be rejected by scheme"
            );
        }
    }

    #[tokio::test]
    async fn rejects_literal_private_hosts_without_a_lookup() {
        for uri in [
            "http://127.0.0.1:8545",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/",
            "http://[::1]:8080/",
        ] {
            assert!(
                validate_uri(uri).await.is_err(),
                "{uri} should be rejected"
            );
        }
    }
}
