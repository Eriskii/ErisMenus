//! `erismenus`: installs the root daemon that loads ErisMenus into every app as it starts, and
//! is that daemon.

use std::ffi::c_int;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use eris_inject::daemon::{self, Client};
use eris_inject::procs::{self, App, Kind};
use eris_inject::inject::{self, Payload};
use eris_inject::launchd::Service;

const PAYLOAD: &str = "liberismenus.dylib";
/// The name the payload gives `payload!`.
const NAME: &str = "erismenus";

const SERVICE: Service = Service {
    label: "io.github.eriskii.erismenus",
    dir: "/Library/Application Support/ErisMenus",
    log: "/Library/Logs/erismenus.log",
    files: &[("erismenus", 0o755), (inject::THREAD_HELPER, 0o755), (PAYLOAD, 0o644)],
    arguments: &["daemon"],
    link: Some("/usr/local/bin/erismenus"),
};

#[derive(Parser)]
#[command(version, about = "Menus drawn by ErisMenus instead of Liquid Glass, in every app")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install the daemon that loads into every app (run with sudo from the build directory).
    Install,
    /// Remove the daemon and its files; running apps keep our menus until they quit.
    Uninstall,
    /// Show whether the daemon is running.
    Status,
    /// List the running processes the daemon loads into (or would).
    Targets,
    /// Load into one running app right now (needs root).
    Inject {
        pid: i32,
        #[arg(long)]
        payload: Option<PathBuf>,
    },
    #[command(hide = true)]
    Daemon,
    /// Map the payload into this process and start it, without root.
    #[command(hide = true)]
    SelfTest { payload: PathBuf },
}

fn payload(path: &Path) -> Payload<'_> {
    Payload { path, name: NAME, patches: &[] }
}

/// Never loaded into: the login window and the password prompt hold secrets, and the Dock is
/// yabai's to load into.
const SKIPPED: &[&str] = &["com.apple.loginwindow", "com.apple.SecurityAgent", "com.apple.dock"];

/// Everything that can show an AppKit menu: regular apps, agents (menu bar extras, login items,
/// ErisDock), system UI, app extensions, and the XPC services that run AppKit's app loop (open and
/// save panels, System Settings panes). Not background-only processes, nor the helper apps
/// browsers and Electron keep in their Frameworks folders, which never show AppKit menus. In a
/// process that never makes an NSApplication, the payload hooks nothing.
fn shows_menus(app: &App) -> bool {
    match app.kind {
        Kind::Background => false,
        Kind::Service => true,
        Kind::Regular | Kind::Agent | Kind::Extension => {
            !(app.nested && app.in_frameworks && app.kind != Kind::Extension) && !SKIPPED.contains(&app.id.as_str())
        }
    }
}

struct Menus {
    payload: PathBuf,
}

impl Client for Menus {
    fn wants(&mut self, app: &App, _home: &Path) -> bool {
        shows_menus(app)
    }

    fn load(&mut self, pid: i32, _bundle_id: &str) -> Result<(), inject::Error> {
        inject::inject(pid, &payload(&self.payload))
    }
}

unsafe extern "C" {
    fn dlopen(path: *const std::ffi::c_char, mode: c_int) -> *mut std::ffi::c_void;
}

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Install => {
            SERVICE.install()?;
            println!("installed; the daemon is loading into your apps as they start");
            println!("apps already running get ErisMenus once they're relaunched, or with `sudo erismenus inject <pid>`");
            Ok(())
        }
        Command::Uninstall => {
            SERVICE.uninstall()?;
            println!("uninstalled; apps go back to normal as they're relaunched");
            Ok(())
        }
        Command::Status => {
            match (SERVICE.installed(), SERVICE.pid()) {
                (_, Some(pid)) => println!("daemon: running (pid {pid})"),
                (true, None) => println!("daemon: installed but not running; see {}", SERVICE.log),
                (false, None) => println!("daemon: not installed (sudo erismenus install)"),
            }
            Ok(())
        }
        Command::Targets => {
            let me = unsafe { libc::getuid() };
            let mut rows: Vec<(String, i32, &str)> = Vec::new();
            for pid in procs::all_pids() {
                let Some(app) = procs::owner(pid).filter(|&uid| uid == me).and(procs::executable(pid)).and_then(|exe| procs::app(&exe))
                else {
                    continue;
                };
                let verdict = if shows_menus(&app) { "yes" } else { "no" };
                rows.push((app.id, pid, verdict));
            }
            rows.sort();
            for (id, pid, verdict) in rows {
                println!("{verdict:4}{pid:>7}  {id}");
            }
            Ok(())
        }
        Command::Inject { pid, payload: path } => {
            let path = path.unwrap_or_else(|| SERVICE.file(PAYLOAD));
            inject::inject(pid, &payload(&path)).map_err(|e| format!("loading into {pid} failed: {e}"))
        }
        Command::Daemon => {
            eris_inject::log!("started; payload {}", SERVICE.file(PAYLOAD).display());
            daemon::run(&mut Menus { payload: SERVICE.file(PAYLOAD) })
        }
        Command::SelfTest { payload: path } => {
            // With AppKit here the payload installs its hooks for real, as it would in an app.
            const RTLD_NOW: c_int = 2;
            let appkit = c"/System/Library/Frameworks/AppKit.framework/AppKit";
            if unsafe { dlopen(appkit.as_ptr(), RTLD_NOW) }.is_null() {
                return Err("can't load AppKit".into());
            }
            inject::self_test(&payload(&path)).map_err(|e| format!("self-test failed: {e}"))?;
            println!("mapped and started; see `log show --last 1m --predicate 'eventMessage BEGINSWITH \"[ErisMenus]\"'`");
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erismenus: {e}");
            ExitCode::FAILURE
        }
    }
}
