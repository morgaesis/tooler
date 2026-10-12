use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use tempfile::TempDir;

// All release commands are shell functions. The bootstrap only touches this
// fixture's home, installation directory, and extraction directory.
const FIXTURE: &str = r#"
set -eo pipefail
cd "$1"
export FIXTURE_ROOT="$PWD"
export HOME="$PWD/home with space ' dollar $ backtick \` brackets [x]"
mkdir -p "$HOME" "$PWD/tmp"
touch "$HOME/.bashrc" "$HOME/.zshrc"
export TMPDIR="$PWD/tmp"
export TOOLER_CONFIG="$PWD/config.json"
export TOOLER_DATA_DIR="$PWD/data"
export PROCESSOR_ARCHITECTURE="$FIXTURE_NATIVE_ARCH"
export PROCESSOR_ARCHITEW6432="$FIXTURE_EMULATED_NATIVE_ARCH"
unset TOOLER_BIN_DIR GITHUB_TOKEN
export LOCALAPPDATA="$PWD/local app data"
export USERPROFILE="$PWD/profile"
expected="$LOCALAPPDATA/tooler/bin"
case "$FIXTURE_MODE" in
override) export TOOLER_BIN_DIR="$PWD/override with space ' dollar $ backtick \` brackets [x]"; expected="$TOOLER_BIN_DIR" ;;
relative) export TOOLER_BIN_DIR="relative bin"; expected="$PWD/$TOOLER_BIN_DIR" ;;
profile) unset LOCALAPPDATA; expected="$USERPROFILE/AppData/Local/tooler/bin" ;;
missing) unset LOCALAPPDATA USERPROFILE ;;
unix) expected="$HOME/.local/share/tooler/bin" ;;
esac
if [[ "$FIXTURE_MODE" == native* ]]; then
  if command -v cygpath >/dev/null; then
    export LOCALAPPDATA="$(cygpath -w "$LOCALAPPDATA")"
  else
    export LOCALAPPDATA="${LOCALAPPDATA//\//\\}"
  fi
fi
uname() {
  case "$1" in
  -s) printf '%s\n' "$FIXTURE_OS" ;;
  -m) printf '%s\n' "$FIXTURE_ARCH" ;;
  *) return 1 ;;
  esac
}
gh() {
  [[ "$*" == "api repos/morgaesis/tooler/releases/latest --jq .tag_name" ]]
  printf 'gh\n' >> "$FIXTURE_ROOT/calls"
  printf 'v0.7.9\n'
}
curl() {
  [[ $# == 4 && "$1" == -fsSL && "$2" == -o ]]
  printf '%s\n%s\n' "$3" "$4" >> "$FIXTURE_ROOT/asset"
  printf 'offline archive\n' > "$3"
}
extract_fixture() {
  printf '%s\n' '#!/bin/bash' 'printf "%s\n" "$@" >> "$FIXTURE_ROOT/bootstrap"' 'printf "%s\n" "$TOOLER_BIN_DIR" > "$FIXTURE_ROOT/bootstrap-bin-dir"' > "$FIXTURE_BINARY"
}
unzip() { [[ $# == 2 && "$1" == -q && "$2" == *.zip ]]; extract_fixture; }
tar() { [[ $# == 2 && "$1" == -xzf && "$2" == *.tar.gz ]]; extract_fixture; }
if [[ "$FIXTURE_MODE" == native ]]; then
  cygpath() {
    [[ $# == 3 && "$2" == -- ]]
    if [[ "$1" == -u ]]; then
      [[ "$3" == "$LOCALAPPDATA/tooler/bin" ]]
    else
      [[ "$1" == -w && "$3" == "$expected" ]]
    fi
    printf 'cygpath\n' >> "$FIXTURE_ROOT/calls"
    printf '%s\n' "$expected"
  }
elif [[ "$FIXTURE_MODE" == native-fallback ]]; then
  command() {
    if [[ "$1" == -v && "$2" == cygpath ]]; then return 1; fi
    builtin command "$@"
  }
fi
source "$2"
[[ -x "$expected/$FIXTURE_BINARY" ]]
[[ "$(< "$FIXTURE_ROOT/bootstrap-bin-dir")" -ef "$expected" ]]
for installed in "$expected"/*; do
  [[ "${installed##*/}" == "$FIXTURE_BINARY" ]]
done
# Source both RC files to verify that metacharacters stay literal and PATH is retained.
for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
  PATH=/fixture/original
  source "$rc"
  [[ "$PATH" == *:/fixture/original ]]
  [[ "${PATH%:/fixture/original}" -ef "$expected" ]]
done
"#;

fn run_fixture(
    os: &str,
    arch: &str,
    native_arch: &str,
    emulated_native_arch: &str,
    mode: &str,
    target: &str,
    latest: bool,
) {
    let root = TempDir::new().unwrap();
    let windows = os != "Linux" && os != "Darwin";
    let binary = if windows { "tooler.exe" } else { "tooler" };
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh");
    let bash = std::env::var_os("TOOLER_TEST_BASH").unwrap_or_else(|| "bash".into());
    let mut command = Command::new(bash);
    // Only the executable search path is inherited; credentials and Tooler state
    // are excluded from the child environment.
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap());
    #[cfg(windows)]
    command.env("SystemRoot", std::env::var_os("SystemRoot").unwrap());
    let mut child = command
        .args(["--noprofile", "--norc", "-s", "--"])
        .arg(root.path())
        .arg(script)
        .env("FIXTURE_OS", os)
        .env("FIXTURE_ARCH", arch)
        .env("FIXTURE_NATIVE_ARCH", native_arch)
        .env("FIXTURE_EMULATED_NATIVE_ARCH", emulated_native_arch)
        .env("FIXTURE_MODE", mode)
        .env("FIXTURE_BINARY", binary)
        .env("TOOLER_VERSION", if latest { "" } else { "v0.7.9" })
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Bash is required for offline installer tests");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(FIXTURE.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if mode == "missing" {
        assert!(!output.status.success());
        assert!(stdout.contains("Set LOCALAPPDATA, USERPROFILE, or TOOLER_BIN_DIR"));
        assert!(!root.path().join("asset").exists());
        return;
    }
    assert!(output.status.success(), "{os}/{mode}: {stdout}\n{stderr}");
    let extension = if windows { "zip" } else { "tar.gz" };
    let asset = format!("tooler-v0.7.9-{target}.{extension}");
    assert_eq!(
        fs::read_to_string(root.path().join("asset")).unwrap(),
        format!("{asset}\nhttps://github.com/morgaesis/tooler/releases/download/v0.7.9/{asset}\n")
    );
    assert_eq!(
        fs::read_to_string(root.path().join("bootstrap")).unwrap(),
        "pull\nmorgaesis/tooler\n"
    );
    assert_eq!(fs::read_dir(root.path().join("tmp")).unwrap().count(), 0);
    if latest || mode == "native" {
        let calls = fs::read_to_string(root.path().join("calls")).unwrap();
        if latest {
            assert!(calls.contains("gh\n"));
        }
        if mode == "native" {
            assert!(calls.contains("cygpath\n"));
        }
    }
}

#[test]
fn windows_platform_and_native_architecture() {
    for os in ["MINGW64_NT-10.0", "MSYS_NT-10.0", "CYGWIN_NT-10.0"] {
        for (native, emulated, target) in [
            ("AMD64", "ARM64", "aarch64-pc-windows-msvc"),
            ("ARM64", "", "aarch64-pc-windows-msvc"),
            ("AMD64", "", "x86_64-pc-windows-msvc"),
            ("", "", "x86_64-pc-windows-msvc"),
        ] {
            run_fixture(os, "x86_64", native, emulated, "native", target, false);
        }
    }
    run_fixture(
        "MINGW64_NT-10.0",
        "aarch64",
        "",
        "",
        "native",
        "aarch64-pc-windows-msvc",
        false,
    );
}

#[test]
fn windows_paths_and_latest_release() {
    for mode in [
        "native-fallback",
        "override",
        "relative",
        "profile",
        "missing",
    ] {
        run_fixture(
            "MINGW64_NT-10.0",
            "x86_64",
            "AMD64",
            "ARM64",
            mode,
            "aarch64-pc-windows-msvc",
            mode == "override",
        );
    }
}

#[test]
fn unix_assets_and_installation_paths() {
    for (os, arch, target) in [
        ("Linux", "x86_64", "x86_64-unknown-linux-gnu"),
        ("Linux", "aarch64", "aarch64-unknown-linux-gnu"),
        ("Linux", "armv7l", "armv7-unknown-linux-gnueabihf"),
        ("Darwin", "x86_64", "x86_64-apple-darwin"),
        ("Darwin", "arm64", "aarch64-apple-darwin"),
    ] {
        for mode in ["unix", "override", "relative"] {
            run_fixture(os, arch, "AMD64", "ARM64", mode, target, false);
        }
    }
}
