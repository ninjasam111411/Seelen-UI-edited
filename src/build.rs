use std::{fs::create_dir, path::PathBuf};

use slu_utils::{
    checksums::CheckSums,
    signature::{sign_minisign, sign_minisign_with_new_key},
};

fn main() {
    let _ = create_dir("gen");

    let mut checksums = CheckSums::new();
    read_folder_recursive(PathBuf::from("static"), &mut |path| {
        checksums.add(&path).unwrap();
    });

    let target_dir = target_dir();
    let sums_path = target_dir.join("SHA256SUMS");
    checksums.write(&sums_path).unwrap();

    if !cfg!(debug_assertions) {
        sign_sha256sums(&sums_path);
    } else {
        std::fs::write(
            sums_path.with_extension("sig"),
            "NOT SIGNED NEEDED FOR DEBUG",
        )
        .unwrap();
    }

    tauri_build::build();
}

fn target_dir() -> PathBuf {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // see <https://github.com/rust-lang/cargo/issues/5457>
    out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_folder_recursive<F>(path: PathBuf, cb: &mut F)
where
    F: FnMut(PathBuf),
{
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            read_folder_recursive(path, cb);
        } else {
            cb(path);
        }
    }
}

fn sign_sha256sums(path: &PathBuf) {
    println!("cargo:rerun-if-env-changed=TAURI_SIGNING_PRIVATE_KEY");
    println!("cargo:rerun-if-env-changed=TAURI_SIGNING_PRIVATE_KEY_PASSWORD");

    let data = std::fs::read(path).expect("Failed to read SHA256SUMS file");
    let sig_path = path.with_extension("sig");

    let official_key = std::env::var("TAURI_SIGNING_PRIVATE_KEY").ok();
    let password = std::env::var("TAURI_SIGNING_PRIVATE_KEY_PASSWORD").ok();
    let signature = match (official_key, password) {
        (Some(key_base64), Some(password)) => {
            sign_minisign(&data, &key_base64, password).expect("Failed to sign data")
        }
        _ => {
            // Builds made without the official key (e.g. personal forks) sign with a key generated
            // for this build, and embed its public key so the startup integrity check still works.
            println!(
                "cargo:warning=TAURI_SIGNING_PRIVATE_KEY not set: signing checksums with a key generated for this build"
            );
            let (signature, public_key) =
                sign_minisign_with_new_key(&data).expect("Failed to sign data");
            println!("cargo:rustc-env=SLU_CHECKSUMS_PUBLIC_KEY={public_key}");
            signature
        }
    };

    std::fs::write(&sig_path, signature).expect("Failed to write signature");
}
