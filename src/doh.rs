// SPDX-License-Identifier: GPL-3.0-or-later
//! When the computer's own DNS can't find GitHub, ask a public DNS service over
//! HTTPS instead (Cloudflare 1.1.1.1, then Google 8.8.8.8). Only the address
//! lookup changes: the connection to GitHub is still checked with its normal
//! certificate. Used by the updater only.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

/// Public DNS-over-HTTPS services, by address so they need no lookup themselves.
const SERVERS: [&str; 2] = ["https://1.1.1.1/dns-query", "https://8.8.8.8/resolve"];

type Lookup = fn(&str) -> Result<Vec<IpAddr>, String>;

#[derive(Debug)]
pub struct FallbackResolver {
    system: DefaultResolver,
    lookup: Lookup,
}

impl Default for FallbackResolver {
    fn default() -> Self {
        Self {
            system: DefaultResolver::default(),
            lookup: lookup_public,
        }
    }
}

impl Resolver for FallbackResolver {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        match self.system.resolve(uri, config, timeout) {
            Ok(a) => Ok(a),
            Err(e) => {
                let host = uri.host().unwrap_or("");
                // Addresses and names like "localhost" are never sent anywhere.
                if host.parse::<IpAddr>().is_ok() || !host.contains('.') {
                    return Err(e);
                }
                let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
                    Some("http") => 80,
                    _ => 443,
                });
                let ips = (self.lookup)(host).unwrap_or_default();
                if ips.is_empty() {
                    return Err(e);
                }
                log::info!("The system's DNS didn't answer; found GitHub through public DNS");
                // empty() starts with no addresses in use.
                let mut out = self.empty();
                for ip in ips.into_iter().take(4) {
                    out.push(SocketAddr::new(ip, port));
                }
                Ok(out)
            }
        }
    }
}

/// An agent that falls back to public DNS when the system's DNS fails.
pub fn agent(config: ureq::config::Config) -> ureq::Agent {
    ureq::Agent::with_parts(
        config,
        DefaultConnector::default(),
        FallbackResolver::default(),
    )
}

/// Asks each public service in turn for the host's IPv4 addresses.
fn lookup_public(host: &str) -> Result<Vec<IpAddr>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into();
    let mut last = String::new();
    for server in SERVERS {
        let res = agent
            .get(server)
            .query("name", host)
            .query("type", "A")
            .header("Accept", "application/dns-json")
            .call();
        match res {
            Ok(mut r) => match r.body_mut().read_to_string() {
                Ok(body) => {
                    let ips = parse_answer(&body);
                    if !ips.is_empty() {
                        return Ok(ips);
                    }
                }
                Err(e) => last = e.to_string(),
            },
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

/// IPv4 addresses in a DNS-over-HTTPS JSON answer (type 1 records).
pub fn parse_answer(json: &str) -> Vec<IpAddr> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    v["Answer"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|a| a["type"].as_u64() == Some(1))
        .filter_map(|a| a["data"].as_str()?.parse::<IpAddr>().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn reads_doh_answers() {
        let j = r#"{"Status":0,"Answer":[{"name":"api.github.com","type":5,"data":"x.github.com."},{"name":"x","type":1,"data":"140.82.112.6"}]}"#;
        assert_eq!(
            parse_answer(j),
            vec!["140.82.112.6".parse::<IpAddr>().unwrap()]
        );
        assert!(parse_answer("{\"Status\":3}").is_empty());
        assert!(parse_answer("not json").is_empty());
    }

    #[test]
    fn falls_back_when_system_dns_fails() {
        // A name the system can't find, answered by the fallback with this PC's address.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = s.read(&mut buf);
                let _ = s.write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok",
                );
            }
        });
        let resolver = FallbackResolver {
            system: DefaultResolver::default(),
            lookup: |_| Ok(vec!["127.0.0.1".parse().unwrap()]),
        };
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(5)))
            .proxy(None)
            .build();
        let agent = ureq::Agent::with_parts(config, DefaultConnector::default(), resolver);
        let body = agent
            .get(format!("http://preflight-test.invalid:{port}/"))
            .call()
            .unwrap()
            .body_mut()
            .read_to_string()
            .unwrap();
        assert_eq!(body, "ok");
    }
}
