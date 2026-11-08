use kadr_core::plugin::{PlaybackDescriptor, PlaybackMode};
use std::net::IpAddr;

/// True for loopback, private, link-local, unspecified, CGNAT and unique-local addresses.
pub fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64)
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            v6.is_loopback()
                || v6.is_unspecified()
                || (seg0 & 0xfe00) == 0xfc00
                || (seg0 & 0xffc0) == 0xfe80
                || v6
                    .to_ipv4()
                    .map(|v4| is_private_ip(IpAddr::V4(v4)))
                    .unwrap_or(false)
        }
    }
}

fn header_text_ok(s: &str) -> bool {
    !s.chars().any(|c| c == '\r' || c == '\n' || c == '\0')
}

async fn ensure_public_host(url: &reqwest::Url) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "url has no host".to_string())?;
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<IpAddr>() {
        return if is_private_ip(ip) {
            Err(format!("host {bare} is a private or loopback address"))
        } else {
            Ok(())
        };
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((bare, port))
        .await
        .map_err(|e| format!("cannot resolve host {bare}: {e}"))?;
    let mut seen = false;
    for addr in addrs {
        seen = true;
        if is_private_ip(addr.ip()) {
            return Err(format!(
                "host {bare} resolves to a private or loopback address"
            ));
        }
    }
    if seen {
        Ok(())
    } else {
        Err(format!("host {bare} did not resolve"))
    }
}

/// Validates a plugin-supplied playback descriptor.
///
/// Known limitation: DNS is re-resolved when the server (proxy) or ffmpeg (transcode)
/// connects, and ffmpeg follows redirects internally. Operators who need stronger guarantees
/// should keep `allow_private_proxy = false` and run plugins they trust.
pub async fn validate_descriptor(
    desc: &PlaybackDescriptor,
    allow_private: bool,
) -> Result<(), String> {
    let url = reqwest::Url::parse(&desc.url).map_err(|e| format!("invalid url: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("unsupported url scheme '{}'", url.scheme()));
    }
    for (name, value) in &desc.headers {
        if name.is_empty() || name.contains(':') || !header_text_ok(name) || !header_text_ok(value)
        {
            return Err(format!("invalid header '{name}'"));
        }
    }
    match desc.mode {
        PlaybackMode::Direct => {
            if !desc.headers.is_empty() {
                return Err(
                    "direct descriptors cannot carry headers; use proxy or transcode".to_string(),
                );
            }
            Ok(())
        }
        PlaybackMode::Proxy | PlaybackMode::Transcode => {
            if allow_private {
                Ok(())
            } else {
                ensure_public_host(&url).await
            }
        }
    }
}
