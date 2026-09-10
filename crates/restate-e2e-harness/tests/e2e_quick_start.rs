//! Copy the README quick start into an unrelated crate: the documented
//! dependencies must suffice without workspace feature unification, and the
//! exact example must invoke its service and inspect its retained run.

#![cfg(unix)]

use std::process::Command;

use restate_e2e_harness::{Launcher, ReusePolicy, launcher_or_skip};

fn block<'a>(section: &'a str, language: &str) -> &'a str {
    let start = format!("```{language}");
    section
        .split_once(&start)
        .expect("quick-start code block")
        .1
        .split_once('\n')
        .expect("code fence line")
        .1
        .split_once("```")
        .expect("closing code fence")
        .0
}

#[test]
#[ignore = "needs RESTATE_SERVER_BIN; compiles and runs the README in an isolated crate"]
fn e2e_quick_start() {
    let Some(Launcher::Binary { binary, .. }) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    // The child Cargo runs in a different directory; preserve relative binary
    // selections by resolving paths before changing its working directory.
    // A bare executable name keeps Command's usual PATH lookup.
    let binary = if binary.components().count() > 1 {
        std::fs::canonicalize(binary).expect("server binary")
    } else {
        binary
    };
    let section = include_str!("../README.md")
        .split_once("## Quick start\n")
        .expect("README quick start")
        .1
        .split("\n## ")
        .next()
        .expect("quick-start section");
    let dependencies = block(section, "toml");
    let example = block(section, "rust");

    // Exclusive creation; never reuse a previous failed run's source or lockfile.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let project = std::env::temp_dir().join(format!(
        "restate-quick-start-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&project).expect("exclusive quick-start directory");
    eprintln!("standalone quick-start crate: {}", project.display());
    std::fs::create_dir(project.join("tests")).expect("test directory");

    // Only redirect the harness package to the code under test. All dependency
    // versions and features come verbatim from the README, resolved afresh.
    let harness = env!("CARGO_MANIFEST_DIR");
    let manifest = format!(
        "[package]\nname = \"harness-quick-start\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\
         [workspace]\n\n{dependencies}\n\
         [patch.crates-io]\nrestate-e2e-harness = {{ path = {harness:?} }}\n"
    );
    std::fs::write(project.join("Cargo.toml"), manifest).expect("standalone manifest");
    std::fs::write(project.join("tests/e2e.rs"), example).expect("README example");
    let output = Command::new(env!("CARGO"))
        .current_dir(&project)
        .env("RESTATE_SERVER_BIN", binary)
        // Never inherit the outer Cargo's target directory: it holds that
        // directory's build lock while waiting for this test to complete.
        .env("CARGO_TARGET_DIR", project.join("target"))
        .args([
            "test",
            "--color",
            "never",
            "--test",
            "e2e",
            "--",
            "--include-ignored",
            "--nocapture",
        ])
        .output()
        .expect("run standalone cargo test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    eprintln!("{stdout}");
    assert!(
        output.status.success(),
        "standalone quick start failed; sources and build retained at {}",
        project.display()
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("test result: ok. 1 passed;")),
        "expected one executed README test; sources and build retained at {}",
        project.display()
    );
    std::fs::remove_dir_all(&project).expect("remove successful standalone build");
}
