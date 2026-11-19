//! SSH helpers built on russh (pure Rust, supports modern algorithms such as
//! curve25519 / ed25519 that current Raspberry Pi OS requires).
//! The public functions are blocking: call them from a background thread.

use russh::keys::{load_secret_key, HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{client, ChannelMsg, Disconnect};
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Host-key checking, "trust on first use": the fingerprint seen during setup
/// is stored with the Pi, and every later connection must present the same one.
struct Client {
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

fn fingerprint(key: &PublicKeyOrCertificate) -> String {
    match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => key.fingerprint(HashAlg::Sha256).to_string(),
        PublicKeyOrCertificate::Certificate(cert) => cert.public_key().fingerprint(HashAlg::Sha256).to_string(),
    }
}

impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let fp = fingerprint(server_public_key);
        let ok = self.expected.as_deref().is_none_or(|exp| exp == fp);
        *self.seen.lock().unwrap() = Some(fp);
        Ok(ok)
    }
}

type Session = client::Handle<Client>;

enum Auth<'a> {
    Password(&'a str),
    Key(&'a Path),
}

fn e<E: std::fmt::Display>(err: E) -> String {
    err.to_string()
}

fn block_on<F: Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(fut)
}

/// Connects and logs in. `host_key` is the fingerprint we expect (None = first
/// contact, accept and report it). Returns the session and the Pi's fingerprint.
async fn connect(
    host: &str,
    port: u16,
    user: &str,
    auth: Auth<'_>,
    host_key: Option<&str>,
) -> Result<(Session, String), String> {
    let config = Arc::new(client::Config::default());
    let seen = Arc::new(Mutex::new(None));
    let handler = Client { expected: host_key.map(str::to_owned), seen: seen.clone() };
    let result = tokio::time::timeout(Duration::from_secs(10), client::connect(config, (host, port), handler))
        .await
        .map_err(|_| format!("timed out connecting to {host}:{port}"))?;
    let fp = seen.lock().unwrap().clone();
    if let (Some(exp), Some(got)) = (host_key, fp.as_deref()) {
        if exp != got {
            return Err(format!(
                "HOST KEY CHANGED — refusing to connect. Expected {exp}, got {got}. \
                 If you reinstalled this Pi, remove it and add it again."
            ));
        }
    }
    let mut sess = result.map_err(|err| format!("can't connect to {host}:{port}: {err}"))?;
    let fp = fp.unwrap_or_default();

    let res = match auth {
        Auth::Password(pw) => sess.authenticate_password(user, pw).await,
        Auth::Key(path) => {
            let key = load_secret_key(path, None).map_err(|err| format!("can't load key: {err}"))?;
            let hash = sess.best_supported_rsa_hash().await.map_err(e)?.flatten();
            sess.authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
        }
    }
    .map_err(|err| format!("login failed: {err}"))?;

    if !res.success() {
        return Err(match auth {
            Auth::Password(_) => "login failed: wrong user or password".into(),
            Auth::Key(_) => "login with the homelab key was refused".into(),
        });
    }
    Ok((sess, fp))
}

async fn exec(sess: &Session, cmd: &str, stdin: Option<&str>) -> Result<(u32, String), String> {
    let mut ch = sess.channel_open_session().await.map_err(e)?;
    ch.exec(true, cmd).await.map_err(e)?;
    if let Some(input) = stdin {
        ch.data(input.as_bytes()).await.map_err(e)?;
    }
    ch.eof().await.map_err(e)?;

    let mut out = Vec::new();
    let mut code = None;
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { ref data } | ChannelMsg::ExtendedData { ref data, .. } => {
                out.extend_from_slice(data)
            }
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
            _ => {}
        }
    }
    Ok((code.unwrap_or(255), String::from_utf8_lossy(&out).into_owned()))
}

async fn close(sess: Session) {
    let _ = sess.disconnect(Disconnect::ByApplication, "", "English").await;
}

/// First-time setup: log in with the password, install our public key,
/// run the install script with sudo, then check that key login works.
/// The password is only used here and is never stored.
pub fn setup(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    pubkey: &str,
    key: &Path,
    script: &str,
) -> Result<(String, String), String> {
    block_on(async {
        let mut log = String::new();
        let (sess, fp) = connect(host, port, user, Auth::Password(password), None).await?;
        log.push_str(&format!("✔ logged in with password (host key {fp})\n"));

        let pubkey = pubkey.trim();
        let add_key = format!(
            "mkdir -p ~/.ssh && chmod 700 ~/.ssh && \
             (grep -qxF '{pubkey}' ~/.ssh/authorized_keys 2>/dev/null || echo '{pubkey}' >> ~/.ssh/authorized_keys) && \
             chmod 600 ~/.ssh/authorized_keys"
        );
        let (code, out) = exec(&sess, &add_key, None).await?;
        if code != 0 {
            return Err(format!("{log}✘ couldn't install SSH key:\n{out}"));
        }
        log.push_str("✔ installed homelab SSH key\n");

        if !script.trim().is_empty() {
            log.push_str("… running install script (can take a while)\n");
            // The script travels base64-encoded inside the command; the password goes
            // over stdin to `sudo -S`, so it never shows up in the process list and
            // can never be executed as part of the script.
            let cmd = format!(
                "sudo -S -p '' bash -c 'echo {} | base64 -d | bash' 2>&1",
                base64(script.replace("\r\n", "\n").as_bytes())
            );
            let (code, out) = exec(&sess, &cmd, Some(&format!("{password}\n"))).await?;
            log.push_str(&out);
            if code != 0 {
                return Err(format!("{log}\n✘ install script exited with code {code}"));
            }
            log.push_str("✔ install script finished\n");
        }
        close(sess).await;

        let (check, _) = connect(host, port, user, Auth::Key(key), Some(&fp))
            .await
            .map_err(|err| format!("{log}✘ key login check failed: {err}"))?;
        close(check).await;
        log.push_str("✔ key login works — password no longer needed\n");
        Ok((log, fp))
    })
}

const STATS_SCRIPT: &str = r#"
echo "hostname=$(hostname)"
echo "model=$(tr -d '\0' < /proc/device-tree/model 2>/dev/null)"
echo "os=$(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME")"
echo "uptime=$(cut -d' ' -f1 /proc/uptime)"
echo "load=$(cut -d' ' -f1-3 /proc/loadavg)"
echo "cpus=$(nproc)"
echo "mem_total=$(awk '/MemTotal/{print $2}' /proc/meminfo)"
echo "mem_avail=$(awk '/MemAvailable/{print $2}' /proc/meminfo)"
echo "disk=$(df -B1 / | awk 'NR==2{print $2" "$3}')"
echo "temp=$(cat /sys/class/thermal/thermal_zone0/temp 2>/dev/null)"
"#;

fn parse_kv(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Logs in with the app's key and collects stats as key=value pairs.
/// Also returns the Pi's host-key fingerprint (to store it the first time).
pub fn poll(
    host: &str,
    port: u16,
    user: &str,
    key: &Path,
    host_key: Option<&str>,
) -> Result<(HashMap<String, String>, String), String> {
    block_on(async {
        let (sess, fp) = connect(host, port, user, Auth::Key(key), host_key).await?;
        let (_, out) = exec(&sess, STATS_SCRIPT, None).await?;
        close(sess).await;
        Ok((parse_kv(&out), fp))
    })
}

const DIAG_SCRIPT: &str = r#"
avg() { awk -F/ '/^rtt|^round-trip/{print $5}'; }
echo "ip=$(hostname -I 2>/dev/null | awk '{print $1}')"
gw=$(ip route 2>/dev/null | awk '/^default/{print $3; exit}')
echo "gateway=$gw"
[ -n "$gw" ] && echo "gw_ping=$(ping -c 3 -q -W 1 "$gw" 2>/dev/null | avg)"
echo "net_ping=$(ping -c 3 -q -W 2 1.1.1.1 2>/dev/null | avg)"
s=$(date +%s%N)
if getent hosts raspberrypi.com >/dev/null 2>&1; then echo "dns_ms=$(( ($(date +%s%N) - s) / 1000000 ))"; else echo "dns_ms=fail"; fi
echo "wifi=$(awk 'NR==3{gsub(/\./,"",$4); print $4}' /proc/net/wireless 2>/dev/null)"
echo "throttled=$(vcgencmd get_throttled 2>/dev/null | cut -d= -f2)"
echo "failed=$(systemctl --failed --no-legend --plain 2>/dev/null | awk '{print $1}' | paste -sd, -)"
echo "updates=$(apt list --upgradable 2>/dev/null | grep -c upgradable)"
echo "errors=$(journalctl -p 3 -b --no-pager -q 2>/dev/null | wc -l)"
echo "swap=$(free -m 2>/dev/null | awk '/^Swap/{print $3" / "$2" MB"}')"
echo "ntp=$(timedatectl show -p NTPSynchronized --value 2>/dev/null)"
echo "kernel=$(uname -r)"
# Second `top` frame = real current CPU use (the first frame is an average since start).
echo "top=$(top -bn2 -d 0.5 -o %CPU 2>/dev/null | awk '/^top -/{n++} n==2 && /^ *[0-9]+ /{if (c++ < 3) printf "%s %s%%  ", $12, $9}')"
"#;

/// Health and network checks, plus the SSH round-trip time from this PC.
pub fn diagnostics(
    host: &str,
    port: u16,
    user: &str,
    key: &Path,
    host_key: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    block_on(async {
        let (sess, _) = connect(host, port, user, Auth::Key(key), host_key).await?;
        let mut total = Duration::ZERO;
        for _ in 0..3 {
            let t = Instant::now();
            exec(&sess, "true", None).await?;
            total += t.elapsed();
        }
        let (_, out) = exec(&sess, DIAG_SCRIPT, None).await?;
        close(sess).await;
        let mut map = parse_kv(&out);
        map.insert("ssh_ms".into(), format!("{:.1}", total.as_secs_f64() * 1000.0 / 3.0));
        Ok(map)
    })
}

const INET_SCRIPT: &str = r#"
command -v curl >/dev/null || { echo "error=curl is not installed on the Pi"; exit 0; }
echo "ping=$(ping -c 4 -q -W 2 1.1.1.1 2>/dev/null | awk -F/ '/^rtt|^round-trip/{print $5}')"
# Cloudflare refuses >25 MB per request, so fetch 4 x 25 MB in parallel and time the total.
set -f
url='https://speed.cloudflare.com/__down?bytes=25000000'
s=$(date +%s%N)
b=$(curl -s -Z -m 15 -w '%{size_download}\n' -o /dev/null "$url" -o /dev/null "$url" -o /dev/null "$url" -o /dev/null "$url" | awk '{t+=$1} END{print t+0}')
e=$(date +%s%N)
echo "down=$(awk -v b="$b" -v s="$s" -v e="$e" 'BEGIN{printf "%.0f", b / ((e - s) / 1e9)}')"
echo "up=$(head -c 25000000 /dev/zero | curl -s -o /dev/null -m 15 -w '%{speed_upload}' --data-binary @- https://speed.cloudflare.com/__up)"
"#;

fn mbps(bytes: usize, d: Duration) -> String {
    format!("{:.1}", bytes as f64 * 8.0 / d.as_secs_f64().max(1e-6) / 1e6)
}

/// LAN speed between this PC and the Pi (over SSH), then the Pi's internet speed.
/// Keys: lan_down, lan_up (Mbit/s), inet_ping (ms), inet_down, inet_up (bytes/s), inet_error.
pub fn speedtest(
    host: &str,
    port: u16,
    user: &str,
    key: &Path,
    host_key: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    block_on(async {
        let (sess, _) = connect(host, port, user, Auth::Key(key), host_key).await?;
        let mut map = HashMap::new();

        // Pi → PC
        let mut ch = sess.channel_open_session().await.map_err(e)?;
        let t = Instant::now();
        ch.exec(true, "head -c 33554432 /dev/zero").await.map_err(e)?;
        let mut n = 0usize;
        while let Some(msg) = ch.wait().await {
            if let ChannelMsg::Data { ref data } = msg {
                n += data.len();
            }
        }
        map.insert("lan_down".into(), mbps(n, t.elapsed()));

        // PC → Pi
        let mut ch = sess.channel_open_session().await.map_err(e)?;
        ch.exec(true, "cat > /dev/null").await.map_err(e)?;
        let buf = vec![0u8; 16 << 20];
        let t = Instant::now();
        ch.data(&buf[..]).await.map_err(e)?;
        ch.eof().await.map_err(e)?;
        while ch.wait().await.is_some() {}
        map.insert("lan_up".into(), mbps(buf.len(), t.elapsed()));

        // Pi → internet
        let (_, out) = exec(&sess, INET_SCRIPT, None).await?;
        for (k, v) in parse_kv(&out) {
            map.insert(format!("inet_{k}"), v);
        }
        close(sess).await;
        Ok(map)
    })
}

fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_works() {
        assert_eq!(super::base64(b"hi"), "aGk=");
        assert_eq!(super::base64(b"abc"), "YWJj");
        assert_eq!(super::base64(b"abcd"), "YWJjZA==");
    }

    /// Network test: the handshake with a modern OpenSSH-style server must succeed
    /// (login itself is expected to fail). Run with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn handshake_with_modern_server() {
        let err = super::block_on(super::connect("github.com", 22, "git", super::Auth::Password("x"), None))
            .err()
            .expect("login should fail");
        assert!(err.starts_with("login failed"), "handshake failed: {err}");
    }
}
