fn main() {
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let commit = String::from_utf8_lossy(&commit.stdout);

    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    let branch = String::from_utf8_lossy(&branch.stdout);

    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET").unwrap());
    println!("cargo:rustc-env=COMMIT={}", commit);
    println!("cargo:rustc-env=BRANCH={}", branch);
}
