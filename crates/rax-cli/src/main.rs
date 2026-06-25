//! rax CLI — project scaffolding tool.
//!
//! Usage:
//!   rax new <project-name>    Create a new rax iOS project
//!   rax --version             Print the rax version
//!   rax --help                Print help

use std::env;
use std::fs;
use std::path::Path;
use std::process;

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
    println!("    new <name>    Create a new rax iOS project");
    println!("    --version     Print the rax version");
    println!("    --help        Print this help message");
    println!();
    println!("EXAMPLE:");
    println!("    rax new my-app");
    println!("    cd my-app");
    println!("    cargo build --target aarch64-apple-ios-sim");
}

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
    println!("  cargo check --target aarch64-apple-ios-sim");
    println!();
    println!("To build and run on the iOS Simulator, use Xcode or xcodebuild.");
}
