//! One-shot environment detection — OS, shell, toolchain availability.
//! Results are embedded in the agent's system prompt so it knows which
//! CLI dialect to generate (BSD vs GNU, pbcopy vs xclip, etc.).

use std::process::Command;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct EnvProbe {
    pub os_name: String,
    pub os_family: String,
    pub arch: String,
    pub shell: String,
    pub has_rg: bool,
    pub has_fd: bool,
    pub has_jq: bool,
    pub has_git: bool,
    pub has_gh: bool,
    pub has_python: bool,
    pub has_node: bool,
    pub has_cargo: bool,
    pub has_gsed: bool,
    // Network
    pub has_curl: bool,
    pub has_wget: bool,
    pub has_dig: bool,
    pub has_nslookup: bool,
    pub has_netcat: bool,
    pub has_nmap: bool,
    pub has_ss: bool,
    pub has_netstat: bool,
    pub has_lsof: bool,
    pub has_tcpdump: bool,
    pub has_tshark: bool,
    pub has_mtr: bool,
    pub has_traceroute: bool,
    // System / observability
    pub has_htop: bool,
    pub has_btop: bool,
    pub has_iostat: bool,
    pub has_dtrace: bool,
    pub has_strace: bool,
    pub has_journalctl: bool,
    pub has_systemctl: bool,
    pub has_launchctl: bool,
    // GPU / ML
    pub has_nvidia_smi: bool,
    pub has_rocm_smi: bool,
    // Security
    pub has_openssl: bool,
    pub has_gpg: bool,
    pub has_ssh: bool,
    pub has_keychain_cli: bool,
    pub has_secret_tool: bool,
    // Web fetching / scraping
    pub has_httpie: bool,
    pub has_curl_impersonate: bool,
    pub has_aria2: bool,
    pub has_yt_dlp: bool,
    pub has_pandoc: bool,
    pub has_lynx: bool,
    pub has_w3m: bool,
    pub has_chromium: bool,
    pub has_playwright: bool,
    pub has_puppeteer: bool,
    pub has_readability: bool,
    pub has_trafilatura: bool,
    pub internet: InternetStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternetStatus {
    Online,
    Offline,
    Unknown,
}

impl InternetStatus {
    pub fn label(self) -> &'static str {
        match self {
            InternetStatus::Online => "ONLINE",
            InternetStatus::Offline => "OFFLINE (no external reachability detected)",
            InternetStatus::Unknown => "UNKNOWN (probe skipped/failed)",
        }
    }
}

fn probe_internet() -> InternetStatus {
    use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let targets = [
        "1.1.1.1:443",
        "8.8.8.8:443",
        "cloudflare.com:443",
    ];
    let mut saw_dns_or_connect_failure = false;
    for t in targets {
        match t.to_socket_addrs() {
            Ok(mut addrs) => {
                if let Some(addr) = addrs.next() {
                    let sa: SocketAddr = addr;
                    if TcpStream::connect_timeout(&sa, Duration::from_millis(1200)).is_ok() {
                        return InternetStatus::Online;
                    }
                    saw_dns_or_connect_failure = true;
                }
            }
            Err(_) => { saw_dns_or_connect_failure = true; }
        }
    }
    if saw_dns_or_connect_failure { InternetStatus::Offline } else { InternetStatus::Unknown }
}

impl EnvProbe {
    /// The sed-in-place line tailored to the user's OS + what's installed.
    pub fn sed_inplace_rule(&self) -> &'static str {
        match self.os_family.as_str() {
            "macos" => {
                if self.has_gsed {
                    "- `sed -i '' 's/a/b/g' file` (BSD sed on macOS — the empty '' arg is required). \
Or use `gsed -i 's/a/b/g' file` (GNU sed via brew) when you need GNU extensions like `\\+`, `\\|`."
                } else {
                    "- `sed -i '' 's/a/b/g' file` (BSD sed on macOS — the empty '' arg is required, unlike GNU sed). \
BSD sed lacks `\\+` and `\\|` in BRE — use `-E` for ERE, or install `gsed` via brew for GNU behavior."
                }
            }
            "linux" => "- `sed -i 's/a/b/g' file` (GNU sed — no empty '' arg; that's a macOS/BSD quirk).",
            _ => "- `sed -i 's/a/b/g' file` on GNU; `sed -i '' 's/a/b/g' file` on BSD/macOS.",
        }
    }
}

fn have(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
}

fn detect_os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = Command::new("sw_vers").arg("-productVersion").output() {
            if out.status.success() {
                let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !v.is_empty() { return format!("macOS {v}"); }
            }
        }
        "macOS".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/etc/os-release") {
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                    return rest.trim().trim_matches('"').to_string();
                }
            }
        }
        "Linux".to_string()
    }
    #[cfg(target_os = "windows")]
    { "Windows".to_string() }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { std::env::consts::OS.to_string() }
}

fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|p| p.rsplit('/').next().map(|s| s.to_string()))
        .unwrap_or_else(|| "sh".to_string())
}

/// Probe once; subsequent calls are free. Called by `build_system_prompt`.
pub fn probe_environment() -> EnvProbe {
    static CACHE: OnceLock<EnvProbe> = OnceLock::new();
    CACHE
        .get_or_init(|| EnvProbe {
            os_name: detect_os_name(),
            os_family: std::env::consts::FAMILY.to_string().replace("unix", match std::env::consts::OS {
                "macos" => "macos",
                "linux" => "linux",
                other => other,
            }),
            arch: std::env::consts::ARCH.to_string(),
            shell: detect_shell(),
            has_rg: have("rg"),
            has_fd: have("fd") || have("fdfind"),
            has_jq: have("jq"),
            has_git: have("git"),
            has_gh: have("gh"),
            has_python: have("python3") || have("python"),
            has_node: have("node"),
            has_cargo: have("cargo"),
            has_gsed: have("gsed"),
            has_curl: have("curl"),
            has_wget: have("wget"),
            has_dig: have("dig"),
            has_nslookup: have("nslookup"),
            has_netcat: have("nc") || have("ncat"),
            has_nmap: have("nmap"),
            has_ss: have("ss"),
            has_netstat: have("netstat"),
            has_lsof: have("lsof"),
            has_tcpdump: have("tcpdump"),
            has_tshark: have("tshark") || have("wireshark"),
            has_mtr: have("mtr"),
            has_traceroute: have("traceroute") || have("tracert"),
            has_htop: have("htop"),
            has_btop: have("btop") || have("btm"),
            has_iostat: have("iostat"),
            has_dtrace: have("dtrace"),
            has_strace: have("strace"),
            has_journalctl: have("journalctl"),
            has_systemctl: have("systemctl"),
            has_launchctl: have("launchctl"),
            has_nvidia_smi: have("nvidia-smi"),
            has_rocm_smi: have("rocm-smi"),
            has_openssl: have("openssl"),
            has_gpg: have("gpg") || have("gpg2"),
            has_ssh: have("ssh"),
            has_keychain_cli: have("security"),
            has_secret_tool: have("secret-tool"),
            has_httpie: have("http") || have("httpie"),
            has_curl_impersonate: have("curl_chrome110")
                || have("curl_chrome116")
                || have("curl_chrome120")
                || have("curl-impersonate")
                || have("curl-impersonate-chrome"),
            has_aria2: have("aria2c"),
            has_yt_dlp: have("yt-dlp") || have("youtube-dl"),
            has_pandoc: have("pandoc"),
            has_lynx: have("lynx"),
            has_w3m: have("w3m"),
            has_chromium: have("chromium") || have("google-chrome") || have("chrome"),
            has_playwright: have("playwright"),
            has_puppeteer: have("puppeteer"),
            has_readability: have("readability") || have("readable"),
            has_trafilatura: have("trafilatura"),
            internet: probe_internet(),
        })
        .clone()
}
