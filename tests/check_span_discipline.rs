//! Story 02.5: the ADR-0001 grep. Runs `scripts/check-span-discipline.sh` on the fixture
//! trees and on this crate's own `src/`.

use std::path::Path;
use std::process::Command;

fn run(args: &[&str]) -> (i32, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("sh")
        .arg(root.join("scripts/check-span-discipline.sh"))
        .args(args)
        .current_dir(root)
        .output()
        .expect("sh available");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn good_fixture_is_clean_even_in_strict_mode() {
    let (rc, out) = run(&["--strict", "scripts/fixtures/span-discipline/good"]);
    assert_eq!(rc, 0, "{out}");
}

#[test]
fn bad_fixture_fails_on_entered_guards_by_default() {
    let (rc, out) = run(&["scripts/fixtures/span-discipline/bad"]);
    assert_eq!(rc, 1, "{out}");
    assert_eq!(out.matches(": entered-span: ").count(), 3, "{out}");
    assert!(!out.contains("contextual-"), "not strict: {out}");
}

#[test]
fn bad_fixture_fails_on_contextual_macros_in_strict_mode() {
    let (rc, out) = run(&[
        "--strict",
        "--fix-hint",
        "scripts/fixtures/span-discipline/bad",
    ]);
    assert_eq!(rc, 1, "{out}");
    assert!(out.contains("contextual-span"), "{out}");
    assert!(out.contains("contextual-event"), "{out}");
    assert!(out.contains("hint:"), "{out}");
}

#[test]
fn own_src_is_clean_in_strict_mode() {
    let (rc, out) = run(&["--strict", "src"]);
    assert_eq!(rc, 0, "{out}");
}

#[test]
fn usage_error_is_2() {
    let (rc, _) = run(&[]);
    assert_eq!(rc, 2);
}
