//! Генерация fake-TLS и WEB-ссылок на прокси telemt.

use crate::config::{WebLinkScheme, WebSecretMode};
use crate::telemt_cfg::TelemtLinkParams;
use base64::Engine;
use rand::RngCore;
use std::fmt::Write;

/// Генерирует 32 hex-символа (16 байт) для секрета пользователя.
pub fn generate_user_secret() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Формирует fake-TLS секрет: ee + user_secret (32 hex) + hex(tls_domain).
pub fn build_fake_tls_secret(user_secret: &str, tls_domain: &str) -> String {
    let domain_hex = hex::encode(tls_domain.as_bytes());
    let mut s = String::with_capacity(2 + 32 + domain_hex.len());
    s.push_str("ee");
    s.push_str(user_secret);
    s.push_str(&domain_hex);
    s
}

/// Формирует tg://proxy ссылку.
pub fn build_proxy_link(
    params: &TelemtLinkParams,
    user_secret: &str,
) -> Result<String, std::fmt::Error> {
    let secret = build_fake_tls_secret(user_secret, &params.tls_domain);
    let mut url = String::new();
    write!(
        url,
        "tg://proxy?server={}&port={}&secret={}",
        params.host, params.port, secret
    )?;
    Ok(url)
}

/// Извлекает 32-символьный секрет пользователя из ссылки `tg://proxy` (форматы `ee`, `dd`, classic).
pub fn extract_user_secret(proxy_link: &str) -> Option<String> {
    let (_, query) = proxy_link.split_once('?')?;
    let value = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("secret="))?
        .to_ascii_lowercase();
    if !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    // classic — 32 hex; `dd` — ровно 34; `ee` — 34 + hex(tls_domain).
    let secret = match value.len() {
        32 => value.as_str(),
        34 if value.starts_with("dd") => &value[2..34],
        len if len > 34 && value.starts_with("ee") => &value[2..34],
        _ => return None,
    };
    Some(secret.to_string())
}

/// Маркер path-секрета WEB-ссылки telemt (`0x70 || client_secret`).
const WEB_PATH_SECRET_MARKER: u8 = 0x70;

/// Формирует WEB-ссылку для WEB-прокси telemt (порт всегда 443).
///
/// Без `base_path` — root-ссылка с hex-секретом (`dd` + secret или secret). С `base_path` —
/// path-ссылка telemt: `server` = percent-encoded `HOST/BASE`, `secret` = base64url без padding
/// от `0x70 || client_secret`, где `client_secret` = `0xdd || secret` (`dd`) или `secret` (`plain`).
/// `None`, если секрет не hex (для path-ссылки нужны байты секрета).
pub fn build_web_proxy_link(
    host: &str,
    base_path: Option<&str>,
    user_secret: &str,
    mode: WebSecretMode,
    scheme: WebLinkScheme,
) -> Option<String> {
    let prefix = match scheme {
        WebLinkScheme::Tg => "tg://webproxy",
        WebLinkScheme::Https => "https://t.me/webproxy",
    };
    let Some(base_path) = base_path else {
        let secret_prefix = match mode {
            WebSecretMode::Dd => "dd",
            WebSecretMode::Plain => "",
        };
        return Some(format!(
            "{prefix}?server={host}&secret={secret_prefix}{user_secret}"
        ));
    };
    let decoded = hex::decode(user_secret).ok()?;
    let mut marked = Vec::with_capacity(decoded.len() + 2);
    marked.push(WEB_PATH_SECRET_MARKER);
    if mode == WebSecretMode::Dd {
        marked.push(0xdd);
    }
    marked.extend_from_slice(&decoded);
    let server: String =
        form_urlencoded::byte_serialize(format!("{host}/{base_path}").as_bytes()).collect();
    let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(marked);
    Some(format!("{prefix}?server={server}&secret={secret}"))
}

#[cfg(test)]
mod tests {
    use super::{
        build_fake_tls_secret, build_proxy_link, build_web_proxy_link, extract_user_secret,
        generate_user_secret,
    };
    use crate::config::{WebLinkScheme, WebSecretMode};
    use crate::telemt_cfg::TelemtLinkParams;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn extract_user_secret_reads_fake_tls_link() {
        let link = "tg://proxy?server=proxy.example.com&port=443&secret=ee0123456789abcdef0123456789abcdef6578616d706c652e636f6d";

        assert_eq!(extract_user_secret(link).as_deref(), Some(SECRET));
    }

    #[test]
    fn extract_user_secret_reads_dd_and_classic_links() {
        let dd = format!("tg://proxy?server=p.example&port=443&secret=dd{SECRET}");
        let classic = format!("tg://proxy?secret={SECRET}&server=p.example&port=443");

        assert_eq!(extract_user_secret(&dd).as_deref(), Some(SECRET));
        assert_eq!(extract_user_secret(&classic).as_deref(), Some(SECRET));
    }

    #[test]
    fn extract_user_secret_normalizes_case() {
        let link =
            "tg://proxy?server=p.example&port=443&secret=EE0123456789ABCDEF0123456789ABCDEF00";

        assert_eq!(extract_user_secret(link).as_deref(), Some(SECRET));
    }

    #[test]
    fn extract_user_secret_rejects_malformed_links() {
        assert_eq!(
            extract_user_secret("tg://proxy?server=p.example&port=443"),
            None
        );
        assert_eq!(extract_user_secret("tg://proxy?secret=ee0123"), None);
        assert_eq!(
            extract_user_secret("tg://proxy?secret=eezz23456789abcdef0123456789abcdef00"),
            None
        );
        assert_eq!(
            extract_user_secret(&format!("tg://proxy?secret=ab{SECRET}")),
            None
        );
        assert_eq!(
            extract_user_secret(&format!("tg://proxy?secret=dd{SECRET}00")),
            None
        );
        assert_eq!(
            extract_user_secret(&format!("tg://proxy?secret=ee{SECRET}zz")),
            None
        );
        assert_eq!(
            extract_user_secret(&format!("tg://proxy?secret=ee{SECRET}")),
            None
        );
    }

    #[test]
    fn build_web_proxy_link_uses_dd_secret_by_default_mode() {
        assert_eq!(
            build_web_proxy_link(
                "lk.example.com",
                None,
                SECRET,
                WebSecretMode::Dd,
                WebLinkScheme::Tg
            )
            .as_deref(),
            Some(format!("tg://webproxy?server=lk.example.com&secret=dd{SECRET}").as_str())
        );
    }

    #[test]
    fn build_web_proxy_link_supports_plain_secret() {
        assert_eq!(
            build_web_proxy_link(
                "lk.example.com",
                None,
                SECRET,
                WebSecretMode::Plain,
                WebLinkScheme::Tg
            )
            .as_deref(),
            Some(format!("tg://webproxy?server=lk.example.com&secret={SECRET}").as_str())
        );
    }

    // Векторы из telemt (`src/maestro/helpers/runtime.rs`, docs/WEB/WEB_PROXY.md).
    const TELEMT_SECRET: &str = "000102030405060708090a0b0c0d0e0f";

    #[test]
    fn build_web_proxy_link_with_base_path_matches_telemt_dd_vector() {
        assert_eq!(
            build_web_proxy_link(
                "proxy.example.com",
                Some("dobry-cola/super_app"),
                TELEMT_SECRET,
                WebSecretMode::Dd,
                WebLinkScheme::Tg
            )
            .as_deref(),
            Some(
                "tg://webproxy?server=proxy.example.com%2Fdobry-cola%2Fsuper_app&secret=cN0AAQIDBAUGBwgJCgsMDQ4P"
            )
        );
    }

    #[test]
    fn build_web_proxy_link_with_base_path_matches_telemt_plain_vector() {
        assert_eq!(
            build_web_proxy_link(
                "proxy.example.com",
                Some("telegram/web"),
                TELEMT_SECRET,
                WebSecretMode::Plain,
                WebLinkScheme::Tg
            )
            .as_deref(),
            Some(
                "tg://webproxy?server=proxy.example.com%2Ftelegram%2Fweb&secret=cAABAgMEBQYHCAkKCwwNDg8"
            )
        );
    }

    #[test]
    fn build_web_proxy_link_supports_https_scheme() {
        assert_eq!(
            build_web_proxy_link(
                "joy.example.com",
                Some("chill"),
                SECRET,
                WebSecretMode::Dd,
                WebLinkScheme::Https
            )
            .as_deref(),
            Some(
                "https://t.me/webproxy?server=joy.example.com%2Fchill&secret=cN0BI0VniavN7wEjRWeJq83v"
            )
        );
        assert_eq!(
            build_web_proxy_link(
                "lk.example.com",
                None,
                SECRET,
                WebSecretMode::Dd,
                WebLinkScheme::Https
            )
            .as_deref(),
            Some(format!("https://t.me/webproxy?server=lk.example.com&secret=dd{SECRET}").as_str())
        );
    }

    #[test]
    fn build_web_proxy_link_with_base_path_rejects_non_hex_secret() {
        assert_eq!(
            build_web_proxy_link(
                "lk.example.com",
                Some("chill"),
                "zz",
                WebSecretMode::Dd,
                WebLinkScheme::Tg
            ),
            None
        );
    }

    #[test]
    fn generate_user_secret_returns_32_hex_chars() {
        let secret = generate_user_secret();

        assert_eq!(secret.len(), 32);
        assert!(secret.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn build_fake_tls_secret_prefixes_domain_hex() {
        let secret = build_fake_tls_secret("0123456789abcdef0123456789abcdef", "example.com");

        assert_eq!(
            secret,
            "ee0123456789abcdef0123456789abcdef6578616d706c652e636f6d"
        );
    }

    #[test]
    fn build_proxy_link_uses_fake_tls_secret() {
        let params = TelemtLinkParams {
            host: "proxy.example.com".to_string(),
            port: 443,
            tls_domain: "example.com".to_string(),
        };

        let link = build_proxy_link(&params, "0123456789abcdef0123456789abcdef").unwrap();

        assert_eq!(
            link,
            "tg://proxy?server=proxy.example.com&port=443&secret=ee0123456789abcdef0123456789abcdef6578616d706c652e636f6d"
        );
    }
}
