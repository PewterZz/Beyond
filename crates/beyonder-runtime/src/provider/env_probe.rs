//! One-shot environment detection — OS, shell, toolchain availability.
//! Results are embedded in the agent's system prompt so it knows which
//! CLI dialect to generate (BSD vs GNU, pbcopy vs xclip, etc.).

use std::process::Command;
use std::sync::{mpsc, OnceLock};
use std::time::Instant;

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
    let targets = ["1.1.1.1:443", "8.8.8.8:443", "cloudflare.com:443"];
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
            Err(_) => {
                saw_dns_or_connect_failure = true;
            }
        }
    }
    if saw_dns_or_connect_failure {
        InternetStatus::Offline
    } else {
        InternetStatus::Unknown
    }
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
            "linux" => {
                "- `sed -i 's/a/b/g' file` (GNU sed — no empty '' arg; that's a macOS/BSD quirk)."
            }
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

/// Run `have()` for every listed command in parallel via `thread::scope`.
/// Sequential probing was ~10ms/spawn × ~45 tools = 400ms+ on macOS, which
/// blocked the agent task that called `build_system_prompt`. Each probe is
/// an independent fork+exec so fanning them across OS threads is close to
/// a linear speedup until the process-creation limit — in practice the
/// whole set lands in 20–60ms.
fn have_many(cmds: &[&'static str]) -> std::collections::HashMap<&'static str, bool> {
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|scope| {
        for &cmd in cmds {
            let tx = tx.clone();
            scope.spawn(move || {
                let _ = tx.send((cmd, have(cmd)));
            });
        }
    });
    drop(tx);
    rx.into_iter().collect()
}

fn detect_os_name() -> String {
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = Command::new("sw_vers").arg("-productVersion").output() {
            if out.status.success() {
                let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !v.is_empty() {
                    return format!("macOS {v}");
                }
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
    {
        "Windows".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        std::env::consts::OS.to_string()
    }
}

fn detect_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|p| p.rsplit('/').next().map(|s| s.to_string()))
        .unwrap_or_else(|| "sh".to_string())
}

/// Probe once; subsequent calls are free. Called by `build_system_prompt`.
///
/// Kick this off early (e.g. at app startup via a background thread) so the
/// first agent spawn doesn't pay the ~tens-of-ms probe cost on its critical
/// path. The internal OnceLock guarantees the underlying work runs exactly
/// once per process.
pub fn probe_environment() -> EnvProbe {
    static CACHE: OnceLock<EnvProbe> = OnceLock::new();
    CACHE.get_or_init(probe_environment_uncached).clone()
}

fn probe_environment_uncached() -> EnvProbe {
    let t0 = Instant::now();

    // Fan subprocess probes + the internet check across threads. The internet
    // probe is the slowest single item (up to ~3×1200ms when offline) so we
    // run it in parallel with all the `have()` calls instead of serialising.
    let net_handle = std::thread::spawn(probe_internet);

    // Every command we might query — listed once so we dispatch a single
    // batch of parallel probes instead of chains of `have() || have()`.
    const COMMANDS: &[&str] = &[
        "rg",
        "fd",
        "fdfind",
        "jq",
        "git",
        "gh",
        "python3",
        "python",
        "node",
        "cargo",
        "gsed",
        "curl",
        "wget",
        "dig",
        "nslookup",
        "nc",
        "ncat",
        "nmap",
        "ss",
        "netstat",
        "lsof",
        "tcpdump",
        "tshark",
        "wireshark",
        "mtr",
        "traceroute",
        "tracert",
        "htop",
        "btop",
        "btm",
        "iostat",
        "dtrace",
        "strace",
        "journalctl",
        "systemctl",
        "launchctl",
        "nvidia-smi",
        "rocm-smi",
        "openssl",
        "gpg",
        "gpg2",
        "ssh",
        "security",
        "secret-tool",
        "http",
        "httpie",
        "curl_chrome110",
        "curl_chrome116",
        "curl_chrome120",
        "curl-impersonate",
        "curl-impersonate-chrome",
        "aria2c",
        "yt-dlp",
        "youtube-dl",
        "pandoc",
        "lynx",
        "w3m",
        "chromium",
        "google-chrome",
        "chrome",
        "playwright",
        "puppeteer",
        "readability",
        "readable",
        "trafilatura",
    ];
    let probe_start = Instant::now();
    let tools = have_many(COMMANDS);
    let tools_elapsed = probe_start.elapsed();
    let h = |name: &str| tools.get(name).copied().unwrap_or(false);

    let internet = net_handle.join().unwrap_or(InternetStatus::Unknown);

    let probe = EnvProbe {
        os_name: detect_os_name(),
        os_family: std::env::consts::FAMILY.to_string().replace(
            "unix",
            match std::env::consts::OS {
                "macos" => "macos",
                "linux" => "linux",
                other => other,
            },
        ),
        arch: std::env::consts::ARCH.to_string(),
        shell: detect_shell(),
        has_rg: h("rg"),
        has_fd: h("fd") || h("fdfind"),
        has_jq: h("jq"),
        has_git: h("git"),
        has_gh: h("gh"),
        has_python: h("python3") || h("python"),
        has_node: h("node"),
        has_cargo: h("cargo"),
        has_gsed: h("gsed"),
        has_curl: h("curl"),
        has_wget: h("wget"),
        has_dig: h("dig"),
        has_nslookup: h("nslookup"),
        has_netcat: h("nc") || h("ncat"),
        has_nmap: h("nmap"),
        has_ss: h("ss"),
        has_netstat: h("netstat"),
        has_lsof: h("lsof"),
        has_tcpdump: h("tcpdump"),
        has_tshark: h("tshark") || h("wireshark"),
        has_mtr: h("mtr"),
        has_traceroute: h("traceroute") || h("tracert"),
        has_htop: h("htop"),
        has_btop: h("btop") || h("btm"),
        has_iostat: h("iostat"),
        has_dtrace: h("dtrace"),
        has_strace: h("strace"),
        has_journalctl: h("journalctl"),
        has_systemctl: h("systemctl"),
        has_launchctl: h("launchctl"),
        has_nvidia_smi: h("nvidia-smi"),
        has_rocm_smi: h("rocm-smi"),
        has_openssl: h("openssl"),
        has_gpg: h("gpg") || h("gpg2"),
        has_ssh: h("ssh"),
        has_keychain_cli: h("security"),
        has_secret_tool: h("secret-tool"),
        has_httpie: h("http") || h("httpie"),
        has_curl_impersonate: h("curl_chrome110")
            || h("curl_chrome116")
            || h("curl_chrome120")
            || h("curl-impersonate")
            || h("curl-impersonate-chrome"),
        has_aria2: h("aria2c"),
        has_yt_dlp: h("yt-dlp") || h("youtube-dl"),
        has_pandoc: h("pandoc"),
        has_lynx: h("lynx"),
        has_w3m: h("w3m"),
        has_chromium: h("chromium") || h("google-chrome") || h("chrome"),
        has_playwright: h("playwright"),
        has_puppeteer: h("puppeteer"),
        has_readability: h("readability") || h("readable"),
        has_trafilatura: h("trafilatura"),
        internet,
    };

    let detected: usize = tools.values().filter(|&&v| v).count();
    tracing::info!(
        total_ms = t0.elapsed().as_millis() as u64,
        tools_ms = tools_elapsed.as_millis() as u64,
        tools_probed = COMMANDS.len(),
        tools_found = detected,
        internet = ?probe.internet,
        "env_probe complete"
    );
    probe
}
