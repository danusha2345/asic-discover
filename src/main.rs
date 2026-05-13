use std::collections::{HashMap, HashSet, VecDeque};
use std::env;
use std::fmt;
use std::fmt::Write as FmtWrite;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write as IoWrite};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpStream, UdpSocket};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const VERSION: &str = "1.4.0";
const DEFAULT_PORTS: &[u16] = &[4028, 4029, 80, 443, 8080, 8081, 8888, 22, 23];
const CGMINER_PORTS: &[u16] = &[4028, 4029];
const HTTP_PORTS: &[u16] = &[80, 443, 8080, 8081, 8888];
const HTTP_PATHS_FAST: &[&str] = &["/"];
const HTTP_PATHS_DEEP: &[&str] = &[
    "/",
    "/cgi-bin/get_system_info.cgi",
    "/cgi-bin/minerStatus.cgi",
    "/cgi-bin/stats.cgi",
    "/cgi-bin/luci/",
    "/api/v1/status",
    "/api/v1/summary",
    "/status",
    "/miner_status",
];

const VENDOR_PATTERNS: &[(&str, &[&str])] = &[
    (
        "Bitmain Antminer",
        &["antminer", "bitmain", "bmminer", "bmsc"],
    ),
    ("MicroBT WhatsMiner", &["whatsminer", "microbt", "mminer"]),
    ("Canaan Avalon", &["avalon", "canaan"]),
    ("Braiins OS", &["braiins", "bosminer"]),
    ("Hiveon ASIC", &["hiveon asic", "hive os asic"]),
    ("VNish", &["vnish"]),
    ("Goldshell", &["goldshell"]),
    ("IceRiver", &["iceriver", "ice river"]),
    ("Innosilicon", &["innosilicon"]),
    ("Jasminer", &["jasminer"]),
    ("DragonMint", &["dragonmint", "halong"]),
    ("Baikal", &["baikal miner", "baikal"]),
    ("iBeLink", &["ibelink"]),
    ("StrongU", &["strongu"]),
    ("Ebang Ebit", &["ebang", "ebit"]),
    ("Dayun", &["dayun"]),
    ("BlackMiner", &["blackminer"]),
];

const MINING_PATTERNS: &[&str] = &[
    "cgminer",
    "bfgminer",
    "bmminer",
    "bosminer",
    "asic",
    "hashrate",
    "hash rate",
    "hash board",
    "miner status",
    "mining status",
    "pool1",
    "pool 1",
    "pool_url",
    "fan speed",
    "chain",
    "ghs",
    "mhs",
    "ths",
    "temperature",
];

const HASHRATE_LABELS: &[&str] = &[
    "ths rt",
    "ths avg",
    "ths av",
    "ghs 5s",
    "ghs av",
    "ghs avg",
    "ghs 1m",
    "mhs 5s",
    "mhs av",
    "mhs avg",
    "rate_5s",
    "rate_30m",
    "rate_avg",
    "hashrate_5s",
    "hashrate_avg",
    "hashrate",
    "hash rate",
];

const TEMPERATURE_LABELS: &[&str] = &[
    "temp_chip",
    "temp_pcb",
    "temp_max",
    "temp1",
    "temp2",
    "temp3",
    "temp4",
    "temp",
    "temperature",
    "chip_temp",
    "pcb_temp",
];

const FAN_LABELS: &[&str] = &[
    "fan_speed",
    "fan speed",
    "fan1",
    "fan2",
    "fan3",
    "fan4",
    "fan5",
    "fan6",
    "fan",
];

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct Ipv4Network {
    network: Ipv4Addr,
    prefix: u8,
}

impl Ipv4Network {
    fn new(ip: Ipv4Addr, prefix: u8) -> Result<Self, String> {
        if prefix > 32 {
            return Err(format!("invalid prefix length: {}", prefix));
        }
        let mask = prefix_mask(prefix);
        let network = Ipv4Addr::from(u32::from(ip) & mask);
        Ok(Self { network, prefix })
    }

    fn parse(value: &str) -> Result<Self, String> {
        let (ip_text, prefix_text) = value.split_once('/').ok_or_else(|| {
            format!(
                "network must be CIDR, for example 192.168.1.0/24: {}",
                value
            )
        })?;
        let ip: Ipv4Addr = ip_text
            .trim()
            .parse()
            .map_err(|_| format!("invalid IPv4 address: {}", ip_text))?;
        let prefix: u8 = prefix_text
            .trim()
            .parse()
            .map_err(|_| format!("invalid prefix length: {}", prefix_text))?;
        Self::new(ip, prefix)
    }

    fn from_ip_mask(ip: Ipv4Addr, mask: Option<Ipv4Addr>) -> Self {
        let prefix = mask.and_then(prefix_from_mask).unwrap_or(24);
        Self::new(ip, prefix).unwrap_or_else(|_| Self::new(ip, 24).expect("valid /24"))
    }

    fn host_count(&self) -> u64 {
        match self.prefix {
            32 => 1,
            31 => 2,
            prefix => (1u64 << (32 - prefix)) - 2,
        }
    }

    fn hosts(&self) -> Vec<Ipv4Addr> {
        let network = u32::from(self.network);
        let total = if self.prefix == 32 {
            1
        } else {
            1u64 << (32 - self.prefix)
        };
        let first = if self.prefix <= 30 {
            network + 1
        } else {
            network
        };
        let last = if self.prefix <= 30 {
            network + total as u32 - 2
        } else {
            network + total as u32 - 1
        };

        let mut hosts = Vec::with_capacity(self.host_count().min(1_000_000) as usize);
        for value in first..=last {
            hosts.push(Ipv4Addr::from(value));
        }
        hosts
    }
}

impl fmt::Display for Ipv4Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix)
    }
}

#[derive(Clone, Debug)]
struct DiscoveredNetwork {
    network: Ipv4Network,
    source_ip: Ipv4Addr,
    source: String,
}

#[derive(Clone, Debug)]
struct Config {
    manual_networks: Vec<String>,
    ports: Vec<u16>,
    timeout: Duration,
    threads: usize,
    max_hosts: u64,
    force: bool,
    deep: bool,
    include_low: bool,
    user: Option<String>,
    password: String,
    output: PathBuf,
    database: PathBuf,
    no_save: bool,
    no_db: bool,
    quiet: bool,
    list_networks: bool,
    watch: bool,
    interval: Duration,
}

#[derive(Clone, Debug)]
struct HostResult {
    ip: Ipv4Addr,
    score: u32,
    confidence: Confidence,
    vendor: String,
    model: String,
    open_ports: Vec<u16>,
    services: Vec<(u16, String)>,
    titles: Vec<(String, String)>,
    telemetry: Telemetry,
    reasons: Vec<String>,
    api_summary: String,
}

#[derive(Clone, Debug, Default)]
struct Telemetry {
    hashrate_ths: Option<f64>,
    hashrate_source: String,
    temperatures_c: Vec<f64>,
    fan_rpm: Vec<u32>,
}

impl Telemetry {
    fn merge(&mut self, other: Telemetry) {
        if self.hashrate_ths.is_none() && other.hashrate_ths.is_some() {
            self.hashrate_ths = other.hashrate_ths;
            self.hashrate_source = other.hashrate_source;
        }
        for value in other.temperatures_c {
            push_unique_f64(&mut self.temperatures_c, value, 0.25, 16);
        }
        for value in other.fan_rpm {
            push_unique_u32(&mut self.fan_rpm, value, 16);
        }
    }

    fn has_any(&self) -> bool {
        self.hashrate_ths.is_some() || !self.temperatures_c.is_empty() || !self.fan_rpm.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Confidence {
    Low,
    Medium,
    High,
}

impl Confidence {
    fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Default)]
struct Fingerprint {
    score: u32,
    vendor: String,
    model: String,
    reasons: Vec<String>,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {}", err);
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32, String> {
    let config = parse_args()?;
    let auto_mode = config.manual_networks.is_empty();
    let mut networks = if auto_mode {
        discover_local_networks()
    } else {
        parse_manual_networks(&config.manual_networks)?
    };

    if networks.is_empty() {
        return Err("no local IPv4 networks found; use --network 192.168.1.0/24".to_string());
    }

    networks = normalize_networks(networks, auto_mode, config.max_hosts, config.force)?;

    if config.list_networks {
        for item in &networks {
            println!(
                "{}  source_ip={}  source={}",
                item.network, item.source_ip, item.source
            );
        }
        return Ok(0);
    }

    let targets = build_targets(&networks);
    if targets.is_empty() {
        return Err("no target hosts in selected networks".to_string());
    }

    if config.watch {
        run_watch(&config, &networks, targets)?;
        return Ok(0);
    }

    let started_epoch = unix_timestamp();
    let started = Instant::now();

    if !config.quiet {
        let nets = networks
            .iter()
            .map(|item| item.network.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("asic-discover {}", VERSION);
        println!("scanning {} hosts in: {}", targets.len(), nets);
        println!("ports: {}", join_ports(&config.ports));
    }

    let results = scan_all(&config, targets);
    let elapsed = started.elapsed().as_secs_f64();

    print_results(&results);

    if !config.no_save {
        let (json_path, csv_path) =
            save_reports(&config, &networks, &results, started_epoch, elapsed)?;
        println!("Reports written:");
        println!("  JSON: {}", json_path.display());
        println!("  CSV : {}", csv_path.display());
        if !config.no_db {
            let latest_path = save_database(&config, &results, started_epoch)?;
            println!("  DB  : {}", config.database.display());
            println!("  LAST: {}", latest_path.display());
        }
    }

    Ok(0)
}

fn parse_args() -> Result<Config, String> {
    let mut config = Config {
        manual_networks: Vec::new(),
        ports: DEFAULT_PORTS.to_vec(),
        timeout: Duration::from_millis(550),
        threads: 512,
        max_hosts: 4096,
        force: false,
        deep: false,
        include_low: false,
        user: None,
        password: String::new(),
        output: PathBuf::from("reports"),
        database: PathBuf::from("database/asic_inventory.jsonl"),
        no_save: false,
        no_db: false,
        quiet: false,
        list_networks: false,
        watch: false,
        interval: Duration::from_secs(30),
    };

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--network=") {
            config.manual_networks.push(value.to_string());
            continue;
        }
        if let Some(value) = arg.strip_prefix("--ports=") {
            config.ports = parse_ports(value)?;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--timeout=") {
            config.timeout = parse_timeout(value)?;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--threads=") {
            config.threads = parse_usize(value, "--threads")?;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--concurrency=") {
            config.threads = parse_usize(value, "--concurrency")?;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--max-hosts=") {
            config.max_hosts = parse_u64(value, "--max-hosts")?;
            continue;
        }
        if let Some(value) = arg.strip_prefix("--output=") {
            config.output = PathBuf::from(value);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--database=") {
            config.database = PathBuf::from(value);
            continue;
        }
        if let Some(value) = arg.strip_prefix("--interval=") {
            config.interval = parse_interval(value)?;
            continue;
        }

        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "--version" => {
                println!("asic-discover {}", VERSION);
                std::process::exit(0);
            }
            "-n" | "--network" => config.manual_networks.push(next_value(&mut args, &arg)?),
            "--ports" => config.ports = parse_ports(&next_value(&mut args, &arg)?)?,
            "--timeout" => config.timeout = parse_timeout(&next_value(&mut args, &arg)?)?,
            "--threads" | "--concurrency" => {
                config.threads = parse_usize(&next_value(&mut args, &arg)?, &arg)?
            }
            "--max-hosts" => config.max_hosts = parse_u64(&next_value(&mut args, &arg)?, &arg)?,
            "--force" => config.force = true,
            "--deep" => config.deep = true,
            "--include-low" => config.include_low = true,
            "--user" => config.user = Some(next_value(&mut args, &arg)?),
            "--password" => config.password = next_value(&mut args, &arg)?,
            "--output" => config.output = PathBuf::from(next_value(&mut args, &arg)?),
            "--database" => config.database = PathBuf::from(next_value(&mut args, &arg)?),
            "--no-save" => config.no_save = true,
            "--no-db" => config.no_db = true,
            "--quiet" => config.quiet = true,
            "--list-networks" => config.list_networks = true,
            "--watch" => config.watch = true,
            "--interval" => config.interval = parse_interval(&next_value(&mut args, &arg)?)?,
            other => return Err(format!("unknown argument: {}", other)),
        }
    }

    if config.ports.is_empty() {
        return Err("at least one port must be selected".to_string());
    }
    if config.threads == 0 {
        return Err("--threads must be at least 1".to_string());
    }
    if config.interval.as_secs() == 0 && config.interval.subsec_millis() == 0 {
        return Err("--interval must be positive".to_string());
    }
    Ok(config)
}

fn next_value<I>(args: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("{} requires a value", flag))
}

fn parse_timeout(value: &str) -> Result<Duration, String> {
    let seconds: f64 = value
        .parse()
        .map_err(|_| format!("invalid timeout value: {}", value))?;
    if seconds <= 0.0 {
        return Err("--timeout must be positive".to_string());
    }
    Ok(Duration::from_millis((seconds * 1000.0).round() as u64))
}

fn parse_interval(value: &str) -> Result<Duration, String> {
    let seconds: f64 = value
        .parse()
        .map_err(|_| format!("invalid interval value: {}", value))?;
    if seconds <= 0.0 {
        return Err("--interval must be positive".to_string());
    }
    Ok(Duration::from_millis((seconds * 1000.0).round() as u64))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("invalid value for {}: {}", flag, value))
}

fn parse_u64(value: &str, flag: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("invalid value for {}: {}", flag, value))
}

fn parse_ports(value: &str) -> Result<Vec<u16>, String> {
    let mut ports = HashSet::new();
    for raw in value.split(',') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start_text, end_text)) = part.split_once('-') {
            let start = parse_port(start_text.trim())?;
            let end = parse_port(end_text.trim())?;
            if start > end {
                return Err(format!("invalid port range: {}", part));
            }
            for port in start..=end {
                ports.insert(port);
            }
        } else {
            ports.insert(parse_port(part)?);
        }
    }
    let mut values = ports.into_iter().collect::<Vec<_>>();
    values.sort_unstable();
    Ok(values)
}

fn parse_port(value: &str) -> Result<u16, String> {
    let port: u16 = value
        .parse()
        .map_err(|_| format!("invalid port: {}", value))?;
    if port == 0 {
        return Err("port 0 is invalid".to_string());
    }
    Ok(port)
}

fn print_help() {
    println!(
        "asic-discover {VERSION}

Find ASIC miners on local IPv4 networks.

Usage:
  asic-discover [options]

Options:
  -n, --network <CIDR>      Network to scan, for example 192.168.1.0/24. Can be repeated.
      --ports <LIST>        Ports and ranges, default: 4028,4029,80,443,8080,8081,8888,22,23
      --timeout <SECONDS>   TCP timeout, default: 0.55
      --threads <N>         Worker threads for TCP probes and fingerprinting, default: 512
      --max-hosts <N>       Safety limit unless --force is used, default: 4096
      --force               Allow large ranges above --max-hosts
      --deep                Probe extra HTTP API/status paths
      --include-low         Show low-confidence candidates
      --user <USER>         Optional HTTP Basic Auth user
      --password <PASS>     Optional HTTP Basic Auth password
      --output <DIR>        Report directory, default: reports
      --database <FILE>     Append-only JSONL database, default: database/asic_inventory.jsonl
      --no-save             Do not write JSON/CSV reports
      --no-db               Do not append scan rows to the local database
      --watch               Keep running and rescan on an interval
      --interval <SECONDS>  Watch-mode scan interval, default: 30
      --quiet               Print only final output
      --list-networks       List auto-detected networks and exit
  -h, --help                Show help
      --version             Show version"
    );
}

fn prefix_mask(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    }
}

fn prefix_from_mask(mask: Ipv4Addr) -> Option<u8> {
    let value = u32::from(mask);
    let mut prefix = 0u8;
    let mut seen_zero = false;
    for bit in 0..32 {
        let is_one = (value & (1 << (31 - bit))) != 0;
        if is_one {
            if seen_zero {
                return None;
            }
            prefix += 1;
        } else {
            seen_zero = true;
        }
    }
    Some(prefix)
}

fn run_command(program: &str, args: &[&str]) -> String {
    match Command::new(program).args(args).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).to_string(),
        Err(_) => String::new(),
    }
}

fn discover_local_networks() -> Vec<DiscoveredNetwork> {
    let mut networks = if cfg!(windows) {
        discover_windows_networks()
    } else {
        discover_unix_networks()
    };
    if networks.is_empty() {
        networks = discover_socket_fallback();
    }
    dedupe_networks(networks)
}

fn discover_windows_networks() -> Vec<DiscoveredNetwork> {
    let script = "Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '169.254*' -and $_.IPAddress -ne '127.0.0.1' -and $_.PrefixLength -le 32 } | ForEach-Object { \"$($_.IPAddress)/$($_.PrefixLength)\" }";
    let output = run_command(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-Command", script],
    );
    let mut networks = Vec::new();
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(item) = discovered_from_interface_cidr(line, "powershell") {
            networks.push(item);
        }
    }

    if !networks.is_empty() {
        return networks;
    }

    let output = run_command("ipconfig", &["/all"]);
    let mut current_ip: Option<Ipv4Addr> = None;
    let mut current_mask: Option<Ipv4Addr> = None;

    fn flush(
        networks: &mut Vec<DiscoveredNetwork>,
        current_ip: &mut Option<Ipv4Addr>,
        current_mask: &mut Option<Ipv4Addr>,
    ) {
        if let Some(ip) = *current_ip {
            if is_usable_ipv4(ip) {
                networks.push(DiscoveredNetwork {
                    network: Ipv4Network::from_ip_mask(ip, *current_mask),
                    source_ip: ip,
                    source: "ipconfig".to_string(),
                });
            }
        }
        *current_ip = None;
        *current_mask = None;
    }

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            flush(&mut networks, &mut current_ip, &mut current_mask);
            continue;
        }
        let lowered = line.to_ascii_lowercase();
        if lowered.contains("ipv4") {
            current_ip = first_ipv4(line);
        } else if lowered.contains("subnet mask") || lowered.contains("mask") {
            current_mask = first_ipv4(line);
        }
    }
    flush(&mut networks, &mut current_ip, &mut current_mask);
    networks
}

fn discover_unix_networks() -> Vec<DiscoveredNetwork> {
    let output = run_command("ip", &["-o", "-4", "addr", "show", "scope", "global"]);
    let mut networks = Vec::new();
    for line in output.lines() {
        let mut last = "";
        for token in line.split_whitespace() {
            if last == "inet" {
                if let Some(item) = discovered_from_interface_cidr(token, "ip") {
                    networks.push(item);
                }
            }
            last = token;
        }
    }
    if !networks.is_empty() {
        return networks;
    }

    let output = run_command("ifconfig", &[]);
    let mut current_ip: Option<Ipv4Addr> = None;
    let mut current_mask: Option<Ipv4Addr> = None;
    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            if let Some(ip) = current_ip {
                if is_usable_ipv4(ip) {
                    networks.push(DiscoveredNetwork {
                        network: Ipv4Network::from_ip_mask(ip, current_mask),
                        source_ip: ip,
                        source: "ifconfig".to_string(),
                    });
                }
            }
            current_ip = None;
            current_mask = None;
            continue;
        }
        if line.contains("inet ") {
            current_ip = first_ipv4(line);
            if let Some(index) = line.find("netmask") {
                current_mask = first_ipv4(&line[index..]);
            }
        }
    }
    networks
}

fn discover_socket_fallback() -> Vec<DiscoveredNetwork> {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(socket) => socket,
        Err(_) => return Vec::new(),
    };
    if socket.connect("8.8.8.8:80").is_err() {
        return Vec::new();
    }
    match socket.local_addr() {
        Ok(SocketAddr::V4(addr)) if is_usable_ipv4(*addr.ip()) => vec![DiscoveredNetwork {
            network: Ipv4Network::new(*addr.ip(), 24).expect("valid /24"),
            source_ip: *addr.ip(),
            source: "socket-fallback".to_string(),
        }],
        _ => Vec::new(),
    }
}

fn discovered_from_interface_cidr(value: &str, source: &str) -> Option<DiscoveredNetwork> {
    let (ip_text, prefix_text) = value.trim().split_once('/')?;
    let ip = ip_text.parse::<Ipv4Addr>().ok()?;
    if !is_usable_ipv4(ip) {
        return None;
    }
    let prefix = prefix_text.parse::<u8>().ok()?;
    let network = Ipv4Network::new(ip, prefix).ok()?;
    Some(DiscoveredNetwork {
        network,
        source_ip: ip,
        source: source.to_string(),
    })
}

fn parse_manual_networks(values: &[String]) -> Result<Vec<DiscoveredNetwork>, String> {
    let mut networks = Vec::new();
    for value in values {
        let network = Ipv4Network::parse(value)?;
        networks.push(DiscoveredNetwork {
            source_ip: network.network,
            network,
            source: "manual".to_string(),
        });
    }
    Ok(dedupe_networks(networks))
}

fn normalize_networks(
    networks: Vec<DiscoveredNetwork>,
    auto_mode: bool,
    max_hosts: u64,
    force: bool,
) -> Result<Vec<DiscoveredNetwork>, String> {
    let mut normalized = Vec::new();
    for item in networks {
        let host_count = item.network.host_count();
        if host_count <= max_hosts || force {
            normalized.push(item);
            continue;
        }

        if auto_mode {
            let narrowed = Ipv4Network::new(item.source_ip, 24)?;
            eprintln!(
                "Auto network {} is large ({} hosts); scanning {}. Use --network {} --force for the full range.",
                item.network, host_count, narrowed, item.network
            );
            normalized.push(DiscoveredNetwork {
                network: narrowed,
                source_ip: item.source_ip,
                source: format!("{}-narrowed", item.source),
            });
            continue;
        }

        return Err(format!(
            "network {} has {} hosts; use --force or increase --max-hosts",
            item.network, host_count
        ));
    }
    Ok(dedupe_networks(normalized))
}

fn dedupe_networks(items: Vec<DiscoveredNetwork>) -> Vec<DiscoveredNetwork> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        let key = item.network.to_string();
        if seen.insert(key) {
            result.push(item);
        }
    }
    result
}

fn build_targets(networks: &[DiscoveredNetwork]) -> Vec<Ipv4Addr> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for item in networks {
        for ip in item.network.hosts() {
            if seen.insert(ip) {
                targets.push(ip);
            }
        }
    }
    targets
}

fn is_usable_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    if octets == [0, 0, 0, 0] || octets[0] == 127 {
        return false;
    }
    if octets[0] == 169 && octets[1] == 254 {
        return false;
    }
    if octets[0] >= 224 {
        return false;
    }
    true
}

fn first_ipv4(text: &str) -> Option<Ipv4Addr> {
    for token in text.split(|ch: char| !(ch.is_ascii_digit() || ch == '.')) {
        if token.matches('.').count() == 3 {
            if let Ok(ip) = token.parse::<Ipv4Addr>() {
                return Some(ip);
            }
        }
    }
    None
}

fn scan_all(config: &Config, targets: Vec<Ipv4Addr>) -> Vec<HostResult> {
    let open_hosts = probe_open_ports(config, &targets);
    if open_hosts.is_empty() {
        if !config.quiet {
            eprintln!();
        }
        return Vec::new();
    }

    let total = open_hosts.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(open_hosts)));
    let config = Arc::new(config.clone());
    let completed = Arc::new(AtomicUsize::new(0));
    let print_lock = Arc::new(Mutex::new(()));
    let (tx, rx) = mpsc::channel();

    let worker_count = config.threads.min(total.max(1));
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let config = Arc::clone(&config);
        let completed = Arc::clone(&completed);
        let print_lock = Arc::clone(&print_lock);
        let tx = tx.clone();
        let handle = thread::spawn(move || loop {
            let item = {
                let mut guard = queue.lock().expect("queue lock poisoned");
                guard.pop_front()
            };
            let Some((ip, open_ports)) = item else {
                break;
            };

            if let Some(result) = scan_host(ip, open_ports, &config) {
                if config.include_low || result.confidence != Confidence::Low {
                    let _ = tx.send(result);
                }
            }

            let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
            if !config.quiet && (done == total || done % 10 == 0) {
                let _guard = print_lock.lock().expect("print lock poisoned");
                eprint!("\rfingerprinted {}/{} open hosts", done, total);
                let _ = std::io::stderr().flush();
            }
        });
        handles.push(handle);
    }
    drop(tx);

    let mut results = Vec::new();
    for result in rx {
        results.push(result);
    }

    for handle in handles {
        let _ = handle.join();
    }
    if !config.quiet {
        eprintln!();
    }

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| u32::from(left.ip).cmp(&u32::from(right.ip)))
    });
    results
}

fn probe_open_ports(config: &Config, targets: &[Ipv4Addr]) -> Vec<(Ipv4Addr, Vec<u16>)> {
    let mut checks = VecDeque::new();
    for &ip in targets {
        for &port in &config.ports {
            checks.push_back((ip, port));
        }
    }

    let total = checks.len();
    if total == 0 {
        return Vec::new();
    }

    let queue = Arc::new(Mutex::new(checks));
    let completed = Arc::new(AtomicUsize::new(0));
    let print_lock = Arc::new(Mutex::new(()));
    let (tx, rx) = mpsc::channel();

    let worker_count = config.threads.min(total.max(1));
    let mut handles = Vec::new();
    for _ in 0..worker_count {
        let queue = Arc::clone(&queue);
        let completed = Arc::clone(&completed);
        let print_lock = Arc::clone(&print_lock);
        let tx = tx.clone();
        let timeout = config.timeout;
        let quiet = config.quiet;
        let handle = thread::spawn(move || loop {
            let item = {
                let mut guard = queue.lock().expect("probe queue lock poisoned");
                guard.pop_front()
            };
            let Some((ip, port)) = item else {
                break;
            };

            if tcp_open(ip, port, timeout) {
                let _ = tx.send((ip, port));
            }

            let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
            if !quiet && (done == total || done % 250 == 0) {
                let _guard = print_lock.lock().expect("probe print lock poisoned");
                eprint!("\rprobed {}/{} TCP ports", done, total);
                let _ = std::io::stderr().flush();
            }
        });
        handles.push(handle);
    }
    drop(tx);

    let mut by_host: HashMap<Ipv4Addr, Vec<u16>> = HashMap::new();
    for (ip, port) in rx {
        by_host.entry(ip).or_default().push(port);
    }

    for handle in handles {
        let _ = handle.join();
    }

    let mut open_hosts = by_host.into_iter().collect::<Vec<_>>();
    for (_, ports) in &mut open_hosts {
        ports.sort_unstable();
    }
    open_hosts.sort_by_key(|(ip, _)| u32::from(*ip));
    open_hosts
}

fn run_watch(
    config: &Config,
    networks: &[DiscoveredNetwork],
    targets: Vec<Ipv4Addr>,
) -> Result<(), String> {
    let mut scan_config = config.clone();
    scan_config.quiet = true;

    let mut last_signature = String::new();
    let network_label = networks
        .iter()
        .map(|item| item.network.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    loop {
        let started_epoch = unix_timestamp();
        let started = Instant::now();
        let results = scan_all(&scan_config, targets.clone());
        let elapsed = started.elapsed().as_secs_f64();
        let signature = watch_signature(&results);

        if signature != last_signature {
            clear_terminal();
            println!("asic-discover {} watch mode", VERSION);
            println!("networks: {}", network_label);
            println!(
                "targets: {}  ports: {}  interval: {}",
                targets.len(),
                join_ports(&config.ports),
                format_duration(config.interval)
            );
            println!(
                "changed_at_epoch: {}  scan_elapsed: {:.2}s  stop: Ctrl+C",
                started_epoch, elapsed
            );
            println!();
            print_results(&results);

            if !config.no_save {
                let (json_path, csv_path) =
                    save_reports(config, networks, &results, started_epoch, elapsed)?;
                println!("Reports updated:");
                println!("  JSON: {}", json_path.display());
                println!("  CSV : {}", csv_path.display());
                if !config.no_db {
                    let latest_path = save_database(config, &results, started_epoch)?;
                    println!("  DB  : {}", config.database.display());
                    println!("  LAST: {}", latest_path.display());
                }
            }

            std::io::stdout()
                .flush()
                .map_err(|err| format!("failed to flush stdout: {}", err))?;
            last_signature = signature;
        }

        thread::sleep(config.interval);
    }
}

fn watch_signature(results: &[HostResult]) -> String {
    let mut signature = String::new();
    for item in results {
        let _ = writeln!(
            signature,
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            item.ip,
            item.confidence.as_str(),
            item.score,
            item.vendor,
            item.model,
            format_hashrate(&item.telemetry),
            format_temperatures(&item.telemetry),
            format_fans(&item.telemetry),
            join_ports(&item.open_ports)
        );
    }
    signature
}

fn clear_terminal() {
    print!("\x1b[2J\x1b[H");
}

fn format_duration(duration: Duration) -> String {
    if duration.as_millis() % 1000 == 0 {
        format!("{}s", duration.as_secs())
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

fn scan_host(ip: Ipv4Addr, open_ports: Vec<u16>, config: &Config) -> Option<HostResult> {
    if open_ports.is_empty() {
        return None;
    }

    let mut result = HostResult {
        ip,
        score: 0,
        confidence: Confidence::Low,
        vendor: String::new(),
        model: String::new(),
        open_ports,
        services: Vec::new(),
        titles: Vec::new(),
        telemetry: Telemetry::default(),
        reasons: Vec::new(),
        api_summary: String::new(),
    };

    if result.open_ports.contains(&22) {
        set_service(&mut result, 22, "ssh");
    }
    if result.open_ports.contains(&23) {
        set_service(&mut result, 23, "telnet");
    }

    let open_ports = result.open_ports.clone();
    for port in open_ports.iter().copied() {
        if CGMINER_PORTS.contains(&port) {
            result.score += 10;
            add_reason(
                &mut result,
                format!("common ASIC API port {} is open", port),
            );
            let api_text = query_cgminer(ip, port, config.timeout);
            if !api_text.is_empty() {
                set_service(&mut result, port, "cgminer-api");
                result.api_summary = trim_snippet(&api_text, 600);
                let fp = fingerprint_text(&api_text);
                result.score += fp.score + 20;
                merge_fingerprint(&mut result, fp);
                let telemetry = extract_telemetry(&api_text);
                if telemetry.has_any() {
                    result.score += 10;
                    result.telemetry.merge(telemetry);
                    add_reason(&mut result, "telemetry from API".to_string());
                }
                add_reason(
                    &mut result,
                    format!("CGMiner-compatible API answered on {}", port),
                );
            }
        }
    }

    let paths = if config.deep {
        HTTP_PATHS_DEEP
    } else {
        HTTP_PATHS_FAST
    };
    let auth_header = basic_auth_header(config.user.as_deref(), &config.password);

    for port in open_ports.iter().copied() {
        if !HTTP_PORTS.contains(&port) {
            continue;
        }
        if port == 443 {
            set_service(&mut result, port, "tls-or-http");
        } else {
            set_service(&mut result, port, "http");
        }
        for path in paths {
            let http_text = fetch_http(ip, port, path, config.timeout, &auth_header);
            if http_text.is_empty() {
                continue;
            }
            let title = extract_title(&http_text);
            if !title.is_empty() {
                result
                    .titles
                    .push((format!("{}{}", port, path), title.clone()));
            }
            let server = header_value(&http_text, "Server");
            let auth_realm = header_value(&http_text, "WWW-Authenticate");
            let status = status_line(&http_text);
            let sample = [
                status.as_str(),
                server.as_str(),
                auth_realm.as_str(),
                title.as_str(),
                http_text.get(..http_text.len().min(20_000)).unwrap_or(""),
            ]
            .join("\n");
            let fp = fingerprint_text(&sample);
            if fp.score > 0 {
                result.score += fp.score;
                add_reason(&mut result, format!("HTTP fingerprint on {}{}", port, path));
                merge_fingerprint(&mut result, fp);
                let telemetry = extract_telemetry(&sample);
                if telemetry.has_any() {
                    result.score += 5;
                    result.telemetry.merge(telemetry);
                    add_reason(&mut result, format!("telemetry from HTTP {}{}", port, path));
                }
                if !config.deep {
                    break;
                }
            }
        }
    }

    if result.score == 0 {
        return None;
    }
    result.score = result.score.min(100);
    result.confidence = score_to_confidence(result.score);
    Some(result)
}

fn tcp_open(ip: Ipv4Addr, port: u16, timeout: Duration) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    TcpStream::connect_timeout(&addr, timeout).is_ok()
}

fn connect(ip: Ipv4Addr, port: u16, timeout: Duration) -> Option<TcpStream> {
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    match TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(timeout));
            let _ = stream.set_write_timeout(Some(timeout));
            Some(stream)
        }
        Err(_) => None,
    }
}

fn query_cgminer(ip: Ipv4Addr, port: u16, timeout: Duration) -> String {
    let commands = ["version", "summary", "stats", "devs"];
    let mut texts = Vec::new();
    for command in commands {
        let Some(mut stream) = connect(ip, port, timeout) else {
            continue;
        };
        let payload = format!("{{\"command\":\"{}\"}}\n", command);
        if stream.write_all(payload.as_bytes()).is_err() {
            continue;
        }
        let _ = stream.shutdown(Shutdown::Write);
        let data = read_limited(&mut stream, 65_536);
        let text = decode_bytes(&data);
        if text.trim().is_empty() {
            continue;
        }
        let lowered = text.to_ascii_lowercase();
        texts.push(text);
        if lowered.contains("status")
            || lowered.contains("summary")
            || lowered.contains("stats")
            || lowered.contains("devs")
        {
            break;
        }
    }
    texts.join("\n")
}

fn fetch_http(ip: Ipv4Addr, port: u16, path: &str, timeout: Duration, auth_header: &str) -> String {
    let Some(mut stream) = connect(ip, port, timeout) else {
        return String::new();
    };
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: asic-discover/{}\r\nAccept: text/html,application/json,text/plain,*/*\r\nConnection: close\r\n{}\r\n",
        path, ip, VERSION, auth_header
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return String::new();
    }
    let _ = stream.shutdown(Shutdown::Write);
    decode_bytes(&read_limited(&mut stream, 131_072))
}

fn extract_telemetry(text: &str) -> Telemetry {
    let mut telemetry = Telemetry::default();

    if let Some((value, source)) = extract_hashrate(text) {
        telemetry.hashrate_ths = Some(value);
        telemetry.hashrate_source = source;
    }

    for value in extract_temperatures(text) {
        push_unique_f64(&mut telemetry.temperatures_c, value, 0.25, 16);
    }
    telemetry
        .temperatures_c
        .sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));

    for value in extract_fans(text) {
        push_unique_u32(&mut telemetry.fan_rpm, value, 16);
    }
    telemetry.fan_rpm.sort_unstable();

    telemetry
}

fn extract_hashrate(text: &str) -> Option<(f64, String)> {
    for label in HASHRATE_LABELS {
        for value in values_after_label(text, label, 80) {
            if !(value > 0.0 && value < 10_000_000_000.0) {
                continue;
            }
            let ths = hashrate_to_ths(label, value);
            if ths > 0.001 && ths < 1_000_000.0 {
                return Some((ths, (*label).to_string()));
            }
        }
    }
    None
}

fn hashrate_to_ths(label: &str, value: f64) -> f64 {
    let lowered = label.to_ascii_lowercase();
    if lowered.contains("mhs") {
        value / 1_000_000.0
    } else if lowered.contains("ghs") {
        value / 1_000.0
    } else if lowered.contains("ths") {
        value
    } else if value >= 1000.0 {
        value / 1000.0
    } else {
        value
    }
}

fn extract_temperatures(text: &str) -> Vec<f64> {
    let mut values = Vec::new();
    for label in TEMPERATURE_LABELS {
        for value in values_after_label(text, label, 80) {
            if (10.0..=125.0).contains(&value) {
                push_unique_f64(&mut values, value, 0.25, 16);
            }
        }
    }
    values
}

fn extract_fans(text: &str) -> Vec<u32> {
    let mut values = Vec::new();
    for label in FAN_LABELS {
        for value in values_after_label(text, label, 80) {
            if value.fract().abs() > 0.001 {
                continue;
            }
            if *label == "fan" && value < 100.0 {
                continue;
            }
            if (0.0..=25_000.0).contains(&value) {
                push_unique_u32(&mut values, value.round() as u32, 16);
            }
        }
    }
    values
}

fn values_after_label(text: &str, label: &str, window_len: usize) -> Vec<f64> {
    let lowered = text.to_ascii_lowercase();
    let label = label.to_ascii_lowercase();
    let mut values = Vec::new();
    let mut offset = 0;

    while let Some(relative) = lowered[offset..].find(&label) {
        let label_start = offset + relative;
        let value_start = label_start + label.len();
        let value_end = (value_start + window_len).min(text.len());
        if let Some(window) = text.get(value_start..value_end) {
            if let Some(value) = first_number(window) {
                values.push(value);
            }
        }
        offset = value_start;
        if offset >= lowered.len() {
            break;
        }
    }

    values
}

fn first_number(text: &str) -> Option<f64> {
    let mut current = String::new();
    let mut started = false;
    let mut has_digit = false;

    for ch in text.chars() {
        if ch == ',' && started {
            break;
        }
        if ch.is_ascii_digit() || ch == '.' || (ch == '-' && !started) {
            started = true;
            if ch.is_ascii_digit() {
                has_digit = true;
            }
            current.push(ch);
            continue;
        }

        if started {
            break;
        }
    }

    if !started || !has_digit {
        return None;
    }
    current.parse::<f64>().ok()
}

fn push_unique_f64(values: &mut Vec<f64>, value: f64, tolerance: f64, limit: usize) {
    if values.iter().any(|item| (*item - value).abs() <= tolerance) {
        return;
    }
    if values.len() < limit {
        values.push(value);
    }
}

fn push_unique_u32(values: &mut Vec<u32>, value: u32, limit: usize) {
    if values.contains(&value) {
        return;
    }
    if values.len() < limit {
        values.push(value);
    }
}

fn read_limited(stream: &mut TcpStream, limit: usize) -> Vec<u8> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    while data.len() < limit {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = limit - data.len();
                data.extend_from_slice(&buffer[..n.min(remaining)]);
            }
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => break,
        }
    }
    data
}

fn decode_bytes(data: &[u8]) -> String {
    String::from_utf8_lossy(data)
        .replace('\0', "")
        .trim()
        .to_string()
}

fn basic_auth_header(user: Option<&str>, password: &str) -> String {
    let Some(user) = user else {
        return String::new();
    };
    let token = base64_encode(format!("{}:{}", user, password).as_bytes());
    format!("Authorization: Basic {}\r\n", token)
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index < data.len() {
        let b0 = data[index];
        let b1 = if index + 1 < data.len() {
            data[index + 1]
        } else {
            0
        };
        let b2 = if index + 2 < data.len() {
            data[index + 2]
        } else {
            0
        };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0b0000_0011) << 4) | (b1 >> 4)) as usize] as char);
        if index + 1 < data.len() {
            out.push(TABLE[(((b1 & 0b0000_1111) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if index + 2 < data.len() {
            out.push(TABLE[(b2 & 0b0011_1111) as usize] as char);
        } else {
            out.push('=');
        }
        index += 3;
    }
    out
}

fn fingerprint_text(text: &str) -> Fingerprint {
    if text.is_empty() {
        return Fingerprint::default();
    }

    let lowered = text.to_ascii_lowercase();
    let mut fp = Fingerprint::default();

    for (vendor, patterns) in VENDOR_PATTERNS {
        if patterns.iter().any(|pattern| lowered.contains(pattern)) {
            fp.score += 40;
            fp.vendor = (*vendor).to_string();
            fp.reasons.push(format!("vendor fingerprint: {}", vendor));
            break;
        }
    }

    let mining_hits = MINING_PATTERNS
        .iter()
        .copied()
        .filter(|pattern| lowered.contains(pattern))
        .collect::<Vec<_>>();
    if !mining_hits.is_empty() {
        fp.score += (8 + mining_hits.len() as u32 * 4).min(30);
        fp.reasons.push(format!(
            "mining terms: {}",
            mining_hits
                .iter()
                .take(4)
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let has_status = ["status", "summary", "devs", "stats"]
        .iter()
        .any(|word| lowered.contains(word));
    let has_metrics = ["ghs", "mhs", "ths", "elapsed", "fan", "pool"]
        .iter()
        .any(|word| lowered.contains(word));
    if has_status && has_metrics {
        fp.score += 25;
        fp.reasons.push("CGMiner-like status fields".to_string());
    }

    if lowered.contains("401")
        && (lowered.contains("realm") || lowered.contains("www-authenticate"))
        && VENDOR_PATTERNS
            .iter()
            .flat_map(|(_, patterns)| patterns.iter())
            .any(|pattern| lowered.contains(pattern))
    {
        fp.score += 25;
        fp.reasons.push("authenticated miner web UI".to_string());
    }

    if let Some(model) = find_model(text) {
        fp.score += 15;
        fp.reasons.push(format!("model string: {}", model));
        fp.model = model;
    }

    if lowered.contains("miner configuration") || lowered.contains("miner status") {
        fp.score += 20;
        fp.reasons.push("miner management page".to_string());
    }

    fp
}

fn find_model(text: &str) -> Option<String> {
    let brands = [
        ("antminer", "Antminer"),
        ("whatsminer", "WhatsMiner"),
        ("avalonminer", "AvalonMiner"),
        ("goldshell", "Goldshell"),
        ("iceriver", "IceRiver"),
        ("jasminer", "Jasminer"),
        ("dragonmint", "DragonMint"),
        ("innosilicon", "Innosilicon"),
    ];
    let tokens = text
        .split(|ch: char| {
            !(ch.is_ascii_alphanumeric() || ch == '+' || ch == '-' || ch == '_' || ch == '.')
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for index in 0..tokens.len().saturating_sub(1) {
        for (needle, canonical) in brands {
            if tokens[index].eq_ignore_ascii_case(needle) {
                let model = tokens[index + 1].trim_matches('.');
                if model.len() <= 32 && model.chars().any(|ch| ch.is_ascii_digit()) {
                    return Some(format!("{} {}", canonical, model));
                }
            }
        }
    }
    None
}

fn merge_fingerprint(result: &mut HostResult, fp: Fingerprint) {
    if result.vendor.is_empty() && !fp.vendor.is_empty() {
        result.vendor = fp.vendor;
    }
    if result.model.is_empty() && !fp.model.is_empty() {
        result.model = fp.model;
    }
    for reason in fp.reasons {
        add_reason(result, reason);
    }
}

fn score_to_confidence(score: u32) -> Confidence {
    if score >= 70 {
        Confidence::High
    } else if score >= 40 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

fn add_reason(result: &mut HostResult, reason: String) {
    if !reason.is_empty() && !result.reasons.iter().any(|item| item == &reason) {
        result.reasons.push(reason);
    }
}

fn set_service(result: &mut HostResult, port: u16, service: &str) {
    if let Some((_, existing)) = result
        .services
        .iter_mut()
        .find(|(item_port, _)| *item_port == port)
    {
        *existing = service.to_string();
    } else {
        result.services.push((port, service.to_string()));
    }
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(left, right)| left.to_ascii_lowercase() == right.to_ascii_lowercase())
    })
}

fn extract_title(text: &str) -> String {
    let Some(title_start) = find_ascii_ci(text, "<title") else {
        return String::new();
    };
    let Some(tag_end_rel) = text[title_start..].find('>') else {
        return String::new();
    };
    let content_start = title_start + tag_end_rel + 1;
    let Some(content_end_rel) = find_ascii_ci(&text[content_start..], "</title>") else {
        return String::new();
    };
    let content = &text[content_start..content_start + content_end_rel];
    trim_snippet(&strip_tags(content), 120)
}

fn strip_tags(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result
}

fn header_value(text: &str, name: &str) -> String {
    for line in text.lines() {
        let Some((left, right)) = line.split_once(':') else {
            continue;
        };
        if left.trim().eq_ignore_ascii_case(name) {
            return trim_snippet(right, 160);
        }
    }
    String::new()
}

fn status_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(120)
        .collect()
}

fn trim_snippet(text: &str, limit: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(limit).collect()
}

fn format_hashrate(telemetry: &Telemetry) -> String {
    match telemetry.hashrate_ths {
        Some(value) if value >= 100.0 => format!("{:.1} TH/s", value),
        Some(value) if value >= 10.0 => format!("{:.2} TH/s", value),
        Some(value) => format!("{:.3} TH/s", value),
        None => "-".to_string(),
    }
}

fn format_temperatures(telemetry: &Telemetry) -> String {
    if telemetry.temperatures_c.is_empty() {
        return "-".to_string();
    }
    let values = telemetry
        .temperatures_c
        .iter()
        .take(6)
        .map(|value| format!("{:.0}", value))
        .collect::<Vec<_>>()
        .join(",");
    let max = telemetry
        .temperatures_c
        .iter()
        .copied()
        .fold(f64::MIN, f64::max);
    format!("max {:.0}; {}", max, values)
}

fn format_fans(telemetry: &Telemetry) -> String {
    if telemetry.fan_rpm.is_empty() {
        return "-".to_string();
    }
    telemetry
        .fan_rpm
        .iter()
        .take(6)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn print_results(results: &[HostResult]) {
    if results.is_empty() {
        println!("No ASIC miner candidates found.");
        return;
    }

    let headers = [
        "IP",
        "CONF",
        "SCORE",
        "VENDOR / MODEL",
        "HASHRATE",
        "TEMP C",
        "FAN RPM",
        "PORTS",
        "REASON",
    ];
    let mut rows = Vec::new();
    for item in results {
        let mut name = [item.vendor.as_str(), item.model.as_str()]
            .iter()
            .filter(|part| !part.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if name.is_empty() {
            name = "-".to_string();
        }
        rows.push(vec![
            item.ip.to_string(),
            item.confidence.as_str().to_string(),
            item.score.to_string(),
            name,
            format_hashrate(&item.telemetry),
            format_temperatures(&item.telemetry),
            format_fans(&item.telemetry),
            join_ports(&item.open_ports),
            item.reasons
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; "),
        ]);
    }

    let mut widths = headers
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();
    for row in &rows {
        for (index, value) in row.iter().enumerate() {
            let cap = match index {
                8 => 56,
                3 => 26,
                4 | 5 | 6 => 18,
                _ => 28,
            };
            widths[index] = widths[index].max(value.len()).min(cap);
        }
    }

    println!(
        "{}",
        headers
            .iter()
            .enumerate()
            .map(|(index, value)| pad(value, widths[index]))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("  ")
    );
    for row in rows {
        println!(
            "{}",
            row.iter()
                .enumerate()
                .map(|(index, value)| pad(&crop(value, widths[index]), widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
        );
    }
}

fn crop(value: &str, width: usize) -> String {
    if value.len() <= width {
        value.to_string()
    } else if width <= 3 {
        value.chars().take(width).collect()
    } else {
        format!("{}...", value.chars().take(width - 3).collect::<String>())
    }
}

fn pad(value: &str, width: usize) -> String {
    format!("{:<width$}", value, width = width)
}

fn join_ports(ports: &[u16]) -> String {
    ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn save_reports(
    config: &Config,
    networks: &[DiscoveredNetwork],
    results: &[HostResult],
    started_epoch: u64,
    elapsed_seconds: f64,
) -> Result<(PathBuf, PathBuf), String> {
    fs::create_dir_all(&config.output)
        .map_err(|err| format!("failed to create {}: {}", config.output.display(), err))?;
    let stamp = unix_timestamp();
    let json_path = config.output.join(format!("asic_scan_{}.json", stamp));
    let csv_path = config.output.join(format!("asic_scan_{}.csv", stamp));

    fs::write(
        &json_path,
        build_json_report(networks, results, started_epoch, elapsed_seconds),
    )
    .map_err(|err| format!("failed to write {}: {}", json_path.display(), err))?;

    fs::write(&csv_path, build_csv_report(results))
        .map_err(|err| format!("failed to write {}: {}", csv_path.display(), err))?;

    Ok((json_path, csv_path))
}

fn build_json_report(
    networks: &[DiscoveredNetwork],
    results: &[HostResult],
    started_epoch: u64,
    elapsed_seconds: f64,
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{{");
    let _ = writeln!(out, "  \"tool\": \"asic-discover\",");
    let _ = writeln!(out, "  \"version\": \"{}\",", json_escape(VERSION));
    let _ = writeln!(out, "  \"started_epoch\": {},", started_epoch);
    let _ = writeln!(out, "  \"elapsed_seconds\": {:.3},", elapsed_seconds);
    let _ = writeln!(out, "  \"networks\": [");
    for (index, item) in networks.iter().enumerate() {
        let comma = if index + 1 == networks.len() { "" } else { "," };
        let _ = writeln!(
            out,
            "    {{\"network\":\"{}\",\"source_ip\":\"{}\",\"source\":\"{}\"}}{}",
            json_escape(&item.network.to_string()),
            item.source_ip,
            json_escape(&item.source),
            comma
        );
    }
    let _ = writeln!(out, "  ],");
    let _ = writeln!(out, "  \"results\": [");
    for (index, item) in results.iter().enumerate() {
        let comma = if index + 1 == results.len() { "" } else { "," };
        let _ = writeln!(out, "    {{");
        let _ = writeln!(out, "      \"ip\": \"{}\",", item.ip);
        let _ = writeln!(
            out,
            "      \"confidence\": \"{}\",",
            item.confidence.as_str()
        );
        let _ = writeln!(out, "      \"score\": {},", item.score);
        let _ = writeln!(out, "      \"vendor\": \"{}\",", json_escape(&item.vendor));
        let _ = writeln!(out, "      \"model\": \"{}\",", json_escape(&item.model));
        let _ = writeln!(
            out,
            "      \"open_ports\": [{}],",
            join_ports(&item.open_ports)
        );
        let _ = writeln!(out, "      \"services\": {},", json_pairs(&item.services));
        let _ = writeln!(
            out,
            "      \"titles\": {},",
            json_string_pairs(&item.titles)
        );
        let _ = writeln!(
            out,
            "      \"telemetry\": {{\"hashrate_ths\": {}, \"hashrate_source\": \"{}\", \"temperatures_c\": {}, \"fan_rpm\": {}}},",
            json_optional_f64(item.telemetry.hashrate_ths),
            json_escape(&item.telemetry.hashrate_source),
            json_f64_array(&item.telemetry.temperatures_c),
            json_u32_array(&item.telemetry.fan_rpm)
        );
        let _ = writeln!(
            out,
            "      \"reasons\": {},",
            json_string_array(&item.reasons)
        );
        let _ = writeln!(
            out,
            "      \"api_summary\": \"{}\"",
            json_escape(&item.api_summary)
        );
        let _ = writeln!(out, "    }}{}", comma);
    }
    let _ = writeln!(out, "  ]");
    let _ = writeln!(out, "}}");
    out
}

fn build_csv_report(results: &[HostResult]) -> String {
    let mut out = String::from(
        "ip,confidence,score,vendor,model,hashrate_ths,temperatures_c,fan_rpm,open_ports,reasons\n",
    );
    for item in results {
        let row = [
            item.ip.to_string(),
            item.confidence.as_str().to_string(),
            item.score.to_string(),
            item.vendor.clone(),
            item.model.clone(),
            item.telemetry
                .hashrate_ths
                .map(|value| format!("{:.3}", value))
                .unwrap_or_default(),
            item.telemetry
                .temperatures_c
                .iter()
                .map(|value| format!("{:.1}", value))
                .collect::<Vec<_>>()
                .join("|"),
            item.telemetry
                .fan_rpm
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("|"),
            join_ports(&item.open_ports),
            item.reasons.join(" | "),
        ];
        out.push_str(
            &row.iter()
                .map(|value| csv_escape(value))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

fn save_database(
    config: &Config,
    results: &[HostResult],
    seen_epoch: u64,
) -> Result<PathBuf, String> {
    if let Some(parent) = config.database.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("failed to create {}: {}", parent.display(), err))?;
        }
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.database)
        .map_err(|err| format!("failed to open {}: {}", config.database.display(), err))?;

    for item in results {
        writeln!(file, "{}", build_database_line(item, seen_epoch))
            .map_err(|err| format!("failed to write {}: {}", config.database.display(), err))?;
    }

    let latest_path = config
        .database
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("latest_inventory.csv");
    fs::write(&latest_path, build_csv_report(results))
        .map_err(|err| format!("failed to write {}: {}", latest_path.display(), err))?;
    Ok(latest_path)
}

fn build_database_line(item: &HostResult, seen_epoch: u64) -> String {
    format!(
        "{{\"seen_epoch\":{},\"ip\":\"{}\",\"confidence\":\"{}\",\"score\":{},\"vendor\":\"{}\",\"model\":\"{}\",\"hashrate_ths\":{},\"hashrate_source\":\"{}\",\"temperatures_c\":{},\"fan_rpm\":{},\"open_ports\":[{}],\"reasons\":{}}}",
        seen_epoch,
        item.ip,
        item.confidence.as_str(),
        item.score,
        json_escape(&item.vendor),
        json_escape(&item.model),
        json_optional_f64(item.telemetry.hashrate_ths),
        json_escape(&item.telemetry.hashrate_source),
        json_f64_array(&item.telemetry.temperatures_c),
        json_u32_array(&item.telemetry.fan_rpm),
        join_ports(&item.open_ports),
        json_string_array(&item.reasons)
    )
}

fn json_optional_f64(value: Option<f64>) -> String {
    match value {
        Some(value) if value.is_finite() => format!("{:.3}", value),
        _ => "null".to_string(),
    }
}

fn json_f64_array(values: &[f64]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        if value.is_finite() {
            let _ = write!(out, "{:.1}", value);
        } else {
            out.push_str("null");
        }
    }
    out.push(']');
    out
}

fn json_u32_array(values: &[u32]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "{}", value);
    }
    out.push(']');
    out
}

fn json_pairs(values: &[(u16, String)]) -> String {
    let mut out = String::from("{");
    for (index, (port, value)) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":\"{}\"", port, json_escape(value));
    }
    out.push('}');
    out
}

fn json_string_pairs(values: &[(String, String)]) -> String {
    let mut out = String::from("{");
    for (index, (key, value)) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\":\"{}\"", json_escape(key), json_escape(value));
    }
    out.push('}');
    out
}

fn json_string_array(values: &[String]) -> String {
    let mut out = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(out, "\"{}\"", json_escape(value));
    }
    out.push(']');
    out
}

fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}
