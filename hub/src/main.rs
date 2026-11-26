//! RustDo Hub: syncs RustDo between devices, on a server you run yourself.
//!
//! - **Sync my devices:** one person's lists, tasks, sticky notes and settings, kept the
//!   same on all their devices. An account is just a random sync key (like
//!   `ABCDE-FGHJK-MNPQR-STUVW`); there are no user names or passwords.
//! - **Family sharing:** sharing a list gives it a code (like `K7QM-2XPD`); anyone who has
//!   the code and can reach the hub can read and change that list.
//! - No cloud.
//! - Everything lives in one JSON file in the data folder.
//! - Conflicts: each task (and the list's name) is one item; the most recent edit wins.
//!
//! Settings (command line or environment):
//!   --port <n>      RUSTDO_HUB_PORT   default 7979
//!   --data <dir>    RUSTDO_HUB_DATA   default: %APPDATA%\RustDo Hub (Windows), ./data (others)

mod store;

use std::io::Read;
use std::path::PathBuf;
use store::{AccountSyncRequest, Hub, SyncRequest};
use tiny_http::{Header, Method, Request, Response, Server};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Requests bigger than this are refused (a list with thousands of tasks is well under it).
const MAX_BODY: u64 = 16 * 1024 * 1024;

fn main() {
    let mut port: u16 = std::env::var("RUSTDO_HUB_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7979);
    let mut data: Option<PathBuf> = std::env::var_os("RUSTDO_HUB_DATA").map(PathBuf::from);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" | "-p" => match args.next().and_then(|p| p.parse().ok()) {
                Some(p) => port = p,
                None => fail("--port needs a number, e.g. --port 7979"),
            },
            "--data" | "-d" => match args.next() {
                Some(d) => data = Some(PathBuf::from(d)),
                None => fail("--data needs a folder"),
            },
            "--version" | "-V" => {
                println!("rustdo-hub {VERSION}");
                return;
            }
            "--help" | "-h" => {
                println!("{}", HELP.trim());
                return;
            }
            other => fail(&format!("unknown option {other} (try --help)")),
        }
    }
    let data = data.unwrap_or_else(default_data_dir);
    let mut hub = match Hub::open(&data) {
        Ok(h) => h,
        Err(e) => fail(&format!("can't use data folder {}: {e}", data.display())),
    };

    let server = match Server::http(("0.0.0.0", port)) {
        Ok(s) => s,
        Err(e) => fail(&format!("can't listen on port {port}: {e} (is another hub already running?)")),
    };
    println!("RustDo Hub {VERSION}");
    println!("Data: {}", hub.path().display());
    println!("Daily backups (last {} days): {}", store::KEEP_BACKUPS, hub.backup_dir().display());
    println!("Shared lists: {} · synced accounts: {}", hub.list_count(), hub.account_count());
    println!();
    println!("In RustDo, use this hub address:");
    // Inside Docker the hub only sees the container's private address, which is no use.
    let in_docker = std::path::Path::new("/.dockerenv").exists();
    match lan_ip().filter(|_| !in_docker) {
        Some(ip) => println!("    http://{ip}:{port}"),
        None if in_docker => println!("    http://<IP address of the machine running Docker>:<published port>"),
        None => println!("    http://<this computer's IP address>:{port}"),
    }
    println!();
    println!("Leave this running. Press Ctrl+C to stop.");

    for request in server.incoming_requests() {
        handle(&mut hub, request);
    }
}

const HELP: &str = r#"
RustDo Hub: share RustDo lists with family and friends on your own network.

Usage: rustdo-hub [--port 7979] [--data <folder>]

  -p, --port <n>     Port to listen on (env RUSTDO_HUB_PORT, default 7979)
  -d, --data <dir>   Where shared lists are stored (env RUSTDO_HUB_DATA)
  -V, --version      Print the version
"#;

fn fail(msg: &str) -> ! {
    eprintln!("rustdo-hub: {msg}");
    // Double-clicked on Windows: keep the window open long enough to read the error.
    if cfg!(windows) {
        eprintln!("Press Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    std::process::exit(1);
}

fn default_data_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("RustDo Hub");
        }
    }
    PathBuf::from("data")
}

/// This machine's address on the local network: the address the OS would use to reach
/// the internet. A UDP "connect" only picks a route; no packet is sent.
fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified()).then_some(ip)
}

fn handle(hub: &mut Hub, mut request: Request) {
    let path = request.url().split('?').next().unwrap_or("").to_string();
    let response = match (request.method(), path.as_str()) {
        (Method::Get, "/") => html(status_page(hub)),
        (Method::Get, "/api/hello") => json(
            200,
            serde_json::json!({ "app": "rustdo-hub", "version": VERSION, "protocol": store::PROTOCOL, "accounts": true }),
        ),
        (Method::Post, "/api/account/new") => match hub.new_account() {
            Ok(key) => json(200, serde_json::json!({ "key": key })),
            Err(e) => error(500, &e),
        },
        (Method::Post, "/api/account/sync") => match read_json::<AccountSyncRequest>(&mut request) {
            Ok(req) => match hub.account_sync(req) {
                Ok(Some(resp)) => json(200, serde_json::to_value(resp).unwrap_or_default()),
                Ok(None) => error(404, "This hub doesn't know that sync key. Check the key, or turn on sync again with a new one."),
                Err(e) => error(500, &e),
            },
            Err(e) => error(400, &e),
        },
        (Method::Post, "/api/share") => match read_json::<store::ShareRequest>(&mut request) {
            Ok(req) => match hub.share(req) {
                Ok(code) => json(200, serde_json::json!({ "code": code })),
                Err(e) => error(500, &e),
            },
            Err(e) => error(400, &e),
        },
        (Method::Post, "/api/sync") => match read_json::<SyncRequest>(&mut request) {
            Ok(req) => match hub.sync(req) {
                Ok(Some(resp)) => json(200, serde_json::to_value(resp).unwrap_or_default()),
                Ok(None) => error(404, "No shared list has that code. Check the code, or ask for a new one."),
                Err(e) => error(500, &e),
            },
            Err(e) => error(400, &e),
        },
        (Method::Options, _) => Response::from_data(Vec::new()).with_status_code(204),
        _ => error(404, "not found"),
    };
    // The web app (rustdo.app) calls the hub from another origin; the sync key or share
    // code in the body is what grants access, so any origin may ask.
    let response = response
        .with_header(header("Access-Control-Allow-Origin", "*"))
        .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"))
        .with_header(header("Access-Control-Allow-Headers", "Content-Type"))
        .with_header(header("Access-Control-Max-Age", "86400"));
    let _ = request.respond(response);
}

fn read_json<T: serde::de::DeserializeOwned>(request: &mut Request) -> Result<T, String> {
    if request.body_length().is_some_and(|n| n as u64 > MAX_BODY) {
        return Err("request too large".into());
    }
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_BODY {
        return Err("request too large".into());
    }
    serde_json::from_slice(&body).map_err(|e| format!("bad request: {e}"))
}

type Resp = Response<std::io::Cursor<Vec<u8>>>;

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn json(status: u16, value: serde_json::Value) -> Resp {
    Response::from_data(value.to_string().into_bytes())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json"))
}

fn error(status: u16, msg: &str) -> Resp {
    json(status, serde_json::json!({ "error": msg }))
}

fn html(body: String) -> Resp {
    Response::from_data(body.into_bytes()).with_header(header("Content-Type", "text/html; charset=utf-8"))
}

/// A tiny page so you can check the hub is reachable from a phone's browser.
/// It shows counts only: never list names, tasks or share codes.
fn status_page(hub: &Hub) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>RustDo Hub</title><style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:3rem auto;padding:0 1rem;line-height:1.5}}</style></head>
<body><h1>RustDo Hub is running ✔</h1>
<p>Version {VERSION} · {} shared list{} · {} synced account{}</p>
<p>In RustDo, open ⚙ → <b>Sync my devices</b> or <b>Family sharing</b> and enter this page's address (for example <code>http://192.168.1.20:7979</code>, or your own <code>https://</code> subdomain).</p>
</body></html>"#,
        hub.list_count(),
        if hub.list_count() == 1 { "" } else { "s" },
        hub.account_count(),
        if hub.account_count() == 1 { "" } else { "s" }
    )
}
