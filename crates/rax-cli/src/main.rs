//! rax CLI — project scaffolding and development tool.
//!
//! Usage:
//!   rax new <project-name>                Create a new rax iOS project
//!   rax doctor                            Print environment diagnostic info
//!   rax build [--target <ios-sim|ios|android|macos>]
//!                                         Print the cargo build command to run
//!   rax run [--target <ios-sim|ios>]      Print the cargo build + Xcode run steps
//!   rax test [-- <args>]                  Run cargo test, forwarding extra args
//!   rax lint                              Run cargo clippy --all-targets
//!   rax fmt [--check]                     Run cargo fmt (or check formatting)
//!   rax add <crate-name>                  Print the cargo add command for a crate
//!   rax --version                         Print the rax version
//!   rax --help                            Print help

use std::env;
use std::fs;
use std::path::Path;
use std::process;
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("new") => {
            let name = match args.get(2) {
                Some(n) => n.clone(),
                None => {
                    eprintln!("Usage: rax new <project-name>");
                    process::exit(1);
                }
            };
            create_project(&name);
        }
        Some("doctor") => {
            run_doctor();
        }
        Some("build") => {
            let target = parse_target_flag(&args, "ios-sim");
            run_build(&target);
        }
        Some("run") => {
            let target = parse_target_flag(&args, "ios-sim");
            run_run(&target);
        }
        Some("test") => {
            // Collect everything after an optional "--" separator, or any
            // trailing args that don't look like rax flags.
            let extra: Vec<String> = {
                let mut after_sep = false;
                let mut out = Vec::new();
                for arg in args.iter().skip(2) {
                    if arg == "--" {
                        after_sep = true;
                        continue;
                    }
                    if after_sep || !arg.starts_with('-') {
                        out.push(arg.clone());
                    }
                }
                out
            };
            cmd_test(&extra);
        }
        Some("lint") => {
            cmd_lint();
        }
        Some("fmt") => {
            let check = args.iter().skip(2).any(|a| a == "--check");
            cmd_fmt(check);
        }
        Some("add") => {
            let crate_name = match args.get(2) {
                Some(n) => n.clone(),
                None => {
                    eprintln!("Usage: rax add <crate-name>");
                    process::exit(1);
                }
            };
            cmd_add(&crate_name);
        }
        Some("--version") | Some("-V") => {
            println!("rax {}", VERSION);
        }
        Some("--help") | Some("-h") | None => {
            print_help();
        }
        Some(cmd) => {
            eprintln!("Unknown command: {}", cmd);
            eprintln!("Run 'rax --help' for usage.");
            process::exit(1);
        }
    }
}

fn print_help() {
    println!("rax {} — Rust-native mobile framework", VERSION);
    println!();
    println!("USAGE:");
    println!("    rax <COMMAND>");
    println!();
    println!("COMMANDS:");
    println!("    new <name>                Create a new rax iOS project");
    println!("    doctor                    Print environment diagnostic info");
    println!("    build [--target <TARGET>] Print the build command for a target");
    println!("    run   [--target <TARGET>] Print the run steps for a target");
    println!("    test  [-- <args>]         Run cargo test, forwarding extra args");
    println!("    lint                      Run cargo clippy --all-targets");
    println!("    fmt   [--check]           Run cargo fmt (or --check to only verify)");
    println!("    add   <crate-name>        Print the cargo add command for a crate");
    println!("    --version                 Print the rax version");
    println!("    --help                    Print this help message");
    println!();
    println!("TARGETS:");
    println!("    ios-sim   (default)  aarch64-apple-ios-sim");
    println!("    ios                  aarch64-apple-ios");
    println!("    android              aarch64-linux-android");
    println!("    macos                aarch64-apple-darwin");
    println!();
    println!("EXAMPLE:");
    println!("    rax new my-app");
    println!("    cd my-app");
    println!("    rax build --target ios-sim");
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

fn run_doctor() {
    println!("rax doctor");
    println!();

    // rustc
    match Command::new("rustc").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("  ✓ rustc found: {}", ver);
        }
        _ => println!("  ✗ rustc not found — install Rust from https://rustup.rs"),
    }

    // cargo
    match Command::new("cargo").arg("--version").output() {
        Ok(out) if out.status.success() => {
            println!("  ✓ cargo found");
        }
        _ => println!("  ✗ cargo not found"),
    }

    // rustup installed targets
    let installed_targets: Vec<String> =
        match Command::new("rustup").args(["target", "list", "--installed"]).output() {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .collect()
            }
            _ => Vec::new(),
        };

    let check_target = |triple: &str| {
        if installed_targets.iter().any(|t| t == triple) {
            println!("  ✓ {} target installed", triple);
        } else {
            println!(
                "  ✗ {} target NOT installed — run: rustup target add {}",
                triple, triple
            );
        }
    };

    check_target("aarch64-apple-ios-sim");
    check_target("aarch64-apple-ios");
    check_target("wasm32-unknown-unknown");

    // Xcode Command Line Tools
    match Command::new("xcode-select").arg("--print-path").output() {
        Ok(out) if out.status.success() => {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("  info: Xcode Command Line Tools: {}", path);
        }
        _ => {
            println!("  info: Xcode Command Line Tools: not found (run: xcode-select --install)");
        }
    }

    println!("  info: rax version: {}", VERSION);
}

// ---------------------------------------------------------------------------
// build
// ---------------------------------------------------------------------------

fn run_build(target: &str) {
    let cargo_triple = target_to_triple(target);
    if cargo_triple.is_empty() {
        eprintln!("Unknown target: {}", target);
        eprintln!("Valid targets: ios-sim, ios, android, macos");
        process::exit(1);
    }

    println!("rax build --target {}", target);
    println!();
    println!("→ cargo build --target {} --release", cargo_triple);
    println!();
    println!("Run this command in your project directory.");

    if target == "ios-sim" || target == "ios" {
        println!();
        println!("After the build succeeds, open your Xcode project and link the");
        println!("generated `.a` static library from `target/{}/release/`.`", cargo_triple);
    }
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

fn run_run(target: &str) {
    let cargo_triple = target_to_triple(target);
    if cargo_triple.is_empty() || (target != "ios-sim" && target != "ios") {
        if target == "android" || target == "macos" {
            eprintln!("'rax run' currently supports ios-sim and ios targets only.");
            eprintln!("For {} use 'rax build --target {}' and deploy manually.", target, target);
            process::exit(1);
        }
        eprintln!("Unknown target: {}", target);
        eprintln!("Valid targets for run: ios-sim, ios");
        process::exit(1);
    }

    println!("rax run --target {}", target);
    println!();
    println!("Step 1 — build the library:");
    println!("  cargo build --target {} --release", cargo_triple);
    println!();

    if target == "ios-sim" {
        println!("Step 2 — open your Xcode project and choose an iOS Simulator destination,");
        println!("         then press ▶ Run (or use xcodebuild):");
        println!("  xcodebuild -scheme <YourScheme> -destination 'platform=iOS Simulator,name=iPhone 16' build");
    } else {
        println!("Step 2 — open your Xcode project, select a connected device, then press ▶ Run:");
        println!("  xcodebuild -scheme <YourScheme> -destination 'platform=iOS,id=<DEVICE_UDID>' build");
    }

    println!();
    println!("Run the cargo command first, then rebuild/run in Xcode to pick up the new library.");
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Parse `--target <value>` from args, returning `default_target` if absent.
fn parse_target_flag(args: &[String], default_target: &str) -> String {
    let mut iter = args.iter().skip(2).peekable();
    while let Some(arg) = iter.next() {
        if arg == "--target" || arg == "-t" {
            if let Some(val) = iter.next() {
                return val.clone();
            }
        }
    }
    default_target.to_string()
}

/// Map a friendly target name to a Rust target triple.
fn target_to_triple(target: &str) -> &'static str {
    match target {
        "ios-sim" => "aarch64-apple-ios-sim",
        "ios" => "aarch64-apple-ios",
        "android" => "aarch64-linux-android",
        "macos" => "aarch64-apple-darwin",
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// create_project
// ---------------------------------------------------------------------------

fn create_project(name: &str) {
    let dir = Path::new(name);
    if dir.exists() {
        eprintln!("Error: directory '{}' already exists", name);
        process::exit(1);
    }

    println!("Creating rax project '{}'...", name);

    // Create directory structure
    fs::create_dir_all(dir.join("src")).expect("Failed to create src/");

    // Write Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[lib]
name = "{lib_name}"
crate-type = ["staticlib"]

[dependencies]
rax = {{ git = "https://github.com/1homsi/rax.git" }}
"#,
        name = name,
        lib_name = name.replace('-', "_"),
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml).expect("Failed to write Cargo.toml");

    // Write src/lib.rs
    let lib_rs = r#"use rax::prelude::*;

#[no_mangle]
pub extern "C" fn rax_main() {
    rax::run(|_host, _size| {
        let count = create_signal(0);

        column((
            text("Hello from rax! \u{1F980}")
                .font_size(24.0)
                .color(Color::rgb(0.1, 0.1, 0.1)),
            text("Build native iOS apps in Rust.")
                .font_size(16.0)
                .color(Color::rgba(0.0, 0.0, 0.0, 0.6)),
            button("Tap me", move || count.update(|n| n + 1)),
            dynamic(move || {
                text(format!("Tapped {} times", count.get()))
                    .font_size(14.0)
                    .color(Color::rgb(0.2, 0.5, 1.0))
            }),
        ))
        .padding(32.0)
        .gap(16.0)
        .align(AlignItems::Center)
    });
}
"#;
    fs::write(dir.join("src").join("lib.rs"), lib_rs).expect("Failed to write src/lib.rs");

    // Write .gitignore
    fs::write(dir.join(".gitignore"), "/target\n").expect("Failed to write .gitignore");

    println!("Created '{}'", name);
    println!();
    println!("Next steps:");
    println!("  cd {}", name);
    println!("  rax doctor");
    println!("  rax build --target ios-sim");
    println!();
    println!("To build and run on the iOS Simulator, use Xcode or xcodebuild.");
}

// ---------------------------------------------------------------------------
// test
// ---------------------------------------------------------------------------

fn cmd_test(extra_args: &[String]) {
    println!("Running: cargo test {}", extra_args.join(" "));
    println!();
    println!("For iOS integration tests, run on a simulator:");
    println!("  RUSTC=<path> cargo test --target aarch64-apple-ios-sim");
    println!();
    println!("rax includes a built-in test harness via rax-test:");
    println!("  • Unit tests: use #[test] as normal");
    println!("  • Widget tests: use rax_test::render() + finders");
    println!();

    let status = std::process::Command::new("cargo")
        .arg("test")
        .args(extra_args)
        .status()
        .expect("failed to run cargo test");
    std::process::exit(status.code().unwrap_or(1));
}

// ---------------------------------------------------------------------------
// lint
// ---------------------------------------------------------------------------

fn cmd_lint() {
    println!("Running: cargo clippy --all-targets");
    let status = std::process::Command::new("cargo")
        .args(["clippy", "--all-targets"])
        .status()
        .expect("failed to run cargo clippy");
    std::process::exit(status.code().unwrap_or(1));
}

// ---------------------------------------------------------------------------
// fmt
// ---------------------------------------------------------------------------

fn cmd_fmt(check: bool) {
    let args = if check { vec!["fmt", "--check"] } else { vec!["fmt"] };
    println!("Running: cargo {}", args.join(" "));
    let status = std::process::Command::new("cargo")
        .args(&args)
        .status()
        .expect("failed to run cargo fmt");
    std::process::exit(status.code().unwrap_or(1));
}

// ---------------------------------------------------------------------------
// add
// ---------------------------------------------------------------------------

fn cmd_add(crate_name: &str) {
    println!("To add a dependency:");
    println!("  cargo add {crate_name}");
    println!();
    println!("For rax plugins, check: https://github.com/1homsi/rax");
}
