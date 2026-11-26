//! Where an AI chat step may be sent (#1945): the endpoint URL, the
//! loopback test, and the two laws checked before any socket opens -
//! offline-only reaches loopback hosts alone, and nothing leaves this
//! machine over plain `http://` (a key or a prompt would travel in clear).
//!
//! Provenance: our own policy; loopback per RFC 6761 ("localhost") and
//! RFC 5735 / RFC 4291 (127.0.0.0/8, ::1). URLs parse with the WHATWG
//! `url` crate, the same parser the HTTP client uses, so the host checked
//! here is the host it connects to.

use std::net::{IpAddr, SocketAddr};

/// The URL scheme of an endpoint. Nothing else is accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

/// A parsed provider base URL, e.g. `https://api.openai.com/v1` or
/// `http://localhost:11434/v1`. No user info, query or fragment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub scheme: Scheme,
    /// The host without IPv6 brackets, lower-cased.
    pub host: String,
    pub port: u16,
    /// The base path without a trailing slash (`""` for the root).
    pub base_path: String,
}

impl Endpoint {
    /// Parse a base URL, refusing anything a provider base has no use
    /// for and anything that could hide the real host.
    pub fn parse(text: &str) -> Result<Self, String> {
        let url = url::Url::parse(text.trim()).map_err(|e| format!("endpoint '{text}' is not a URL ({e})"))?;
        let scheme = match url.scheme() {
            "http" => Scheme::Http,
            "https" => Scheme::Https,
            other => return Err(format!("endpoint scheme '{other}' is not http or https")),
        };
        if !url.username().is_empty() || url.password().is_some() {
            return Err("endpoint must not carry a user name or password".into());
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("endpoint must not carry a query or fragment".into());
        }
        let host = match url.host() {
            Some(url::Host::Domain(d)) => d.to_ascii_lowercase(),
            Some(url::Host::Ipv4(ip)) => ip.to_string(),
            Some(url::Host::Ipv6(ip)) => ip.to_string(),
            None => return Err(format!("endpoint '{text}' has no host")),
        };
        let port = url.port_or_known_default().ok_or_else(|| format!("endpoint '{text}' has no port"))?;
        let base_path = url.path().trim_end_matches('/').to_owned();
        Ok(Self { scheme, host, port, base_path })
    }

    /// True for `localhost`, 127.0.0.0/8 and `::1` (also IPv4-mapped).
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        is_loopback_host(&self.host)
    }

    /// The full URL of `path` (which starts with `/`) under this base.
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        let scheme = match self.scheme {
            Scheme::Http => "http",
            Scheme::Https => "https",
        };
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        format!("{scheme}://{host}:{}{}{path}", self.port, self.base_path)
    }

    /// The same host and port with another base path (the Ollama probe
    /// lives at `/api/show`, beside its `/v1` chat base).
    #[must_use]
    pub fn with_base_path(&self, base_path: &str) -> Self {
        Self { base_path: base_path.trim_end_matches('/').to_owned(), ..self.clone() }
    }
}

/// True when `host` names this machine's loopback interface. Only the
/// literal name `localhost` counts by name; any other name is remote
/// until resolved (and the client re-checks resolved addresses).
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    h.parse::<IpAddr>().is_ok_and(is_loopback_ip)
}

/// Loopback test on an address, IPv4-mapped IPv6 included.
#[must_use]
pub fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback()),
    }
}

/// The two laws, checked before the client resolves or connects:
/// offline-only admits loopback hosts alone, and a host off this machine
/// is reached over `https://` only.
pub fn admit(endpoint: &Endpoint, offline_only: bool) -> Result<(), String> {
    let local = endpoint.is_loopback();
    if offline_only && !local {
        return Err(format!(
            "offline-only mode blocks '{}': only localhost, 127.0.0.1 and [::1] are reachable",
            endpoint.host
        ));
    }
    if !local && endpoint.scheme == Scheme::Http {
        return Err(format!(
            "plain http:// to '{}' is refused: off this machine a provider is reached over https:// only",
            endpoint.host
        ));
    }
    Ok(())
}

/// The resolver's half of the law: when only loopback is allowed, every
/// resolved address must be loopback (a hosts-file `localhost` pointing
/// elsewhere is refused, not followed).
pub fn admit_addrs(addrs: Vec<SocketAddr>, loopback_only: bool) -> Result<Vec<SocketAddr>, String> {
    if addrs.is_empty() {
        return Err("the host resolved to no address".into());
    }
    if loopback_only && let Some(bad) = addrs.iter().find(|a| !is_loopback_ip(a.ip())) {
        return Err(format!("refused: the host resolved to {} which is not loopback", bad.ip()));
    }
    Ok(addrs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_names_and_addresses() {
        for h in ["localhost", "LOCALHOST", "127.0.0.1", "127.8.9.1", "::1", "[::1]", "::ffff:127.0.0.1"] {
            assert!(is_loopback_host(h), "{h}");
        }
        for h in ["api.openai.com", "10.0.0.1", "192.168.1.5", "localhost.evil.com", "0.0.0.0", "::"] {
            assert!(!is_loopback_host(h), "{h}");
        }
    }

    #[test]
    fn parse_refuses_hidden_hosts_and_odd_schemes() {
        assert!(Endpoint::parse("http://127.0.0.1@evil.example/v1").is_err());
        assert!(Endpoint::parse("ftp://localhost/").is_err());
        assert!(Endpoint::parse("https://api.openai.com/v1?x=1").is_err());
        let e = Endpoint::parse("http://[::1]:1234/v1/").expect("v6 loopback");
        assert_eq!(e.url("/chat/completions"), "http://[::1]:1234/v1/chat/completions");
        assert!(e.is_loopback());
    }
}
