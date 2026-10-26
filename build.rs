//! Antes de compilar: el commit del que sale el ejecutable, para saber siempre de qué código es
//! (`gesshtor --version`). El de `$GESSHTOR_COMMIT` si viene de fuera; si no, el de git, con «-sucio» si hay
//! cambios sin commit.
use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let o = Command::new("git").args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn main() {
    let commit = std::env::var("GESSHTOR_COMMIT").ok().filter(|c| !c.trim().is_empty()).unwrap_or_else(|| match git(&["rev-parse", "--short=12", "HEAD"]) {
        Some(c) if git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty()) => format!("{c}-sucio"),
        Some(c) => c,
        None => "sin-commit".into(),
    });
    println!("cargo::rustc-env=GESSHTOR_COMMIT={commit}");
    println!("cargo::rerun-if-env-changed=GESSHTOR_COMMIT");
    // Se vuelve a mirar si cambia el código o git (otro commit, algo añadido); solo lo que existe, para no
    // recompilar siempre.
    for p in ["build.rs", "Cargo.toml", "Cargo.lock", "src", ".git/HEAD", ".git/index", ".git/refs", ".git/packed-refs"] {
        if Path::new(p).exists() {
            println!("cargo::rerun-if-changed={p}");
        }
    }
}
