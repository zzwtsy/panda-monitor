use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn get_git_version() -> String {
    let version = env::var("CARGO_PKG_VERSION").unwrap().to_string();

    let child = Command::new("git").args(&["describe", "--always"]).output();
    return match child {
        Ok(child) => {
            let buf = String::from_utf8(child.stdout).expect("failed to read stdout");
            version + "-" + &buf
        }
        Err(err) => {
            eprintln!("`git describe` err: {}", err);
            version
        }
    };
}

fn main() {
    tonic_build::configure()
        .build_transport(true)
        .type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]")
        .compile_protos(&["proto/panda_monitor.proto"], &["proto"])
        .unwrap();
    let version = get_git_version();
    let mut f = File::create(Path::new(&env::var("OUT_DIR").unwrap()).join("VERSION")).unwrap();
    f.write_all(version.trim().as_bytes()).unwrap();
}
