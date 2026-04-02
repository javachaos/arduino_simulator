use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rust_project::HostBoard;

#[derive(Debug, Clone)]
pub struct CompileArtifact {
    pub board: HostBoard,
    pub sketch_path: PathBuf,
    pub build_path: PathBuf,
    pub hex_path: PathBuf,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum ArduinoCliError {
    Io(std::io::Error),
    UnsupportedSource(PathBuf),
    MissingSketchParent(PathBuf),
    MissingSketchName(PathBuf),
    MissingHexOutput(PathBuf),
    CommandFailed {
        command: String,
        stdout: String,
        stderr: String,
    },
}

impl fmt::Display for ArduinoCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::UnsupportedSource(path) => write!(
                f,
                "expected a sketch directory or .ino file, got {}",
                path.display()
            ),
            Self::MissingSketchParent(path) => {
                write!(f, "missing parent directory for {}", path.display())
            }
            Self::MissingSketchName(path) => {
                write!(f, "could not determine sketch name for {}", path.display())
            }
            Self::MissingHexOutput(path) => write!(
                f,
                "arduino-cli finished but no .hex output was found in {}",
                path.display()
            ),
            Self::CommandFailed {
                command,
                stdout,
                stderr,
            } => write!(
                f,
                "{command} failed\nstdout:\n{stdout}\n\nstderr:\n{stderr}"
            ),
        }
    }
}

impl std::error::Error for ArduinoCliError {}

impl From<std::io::Error> for ArduinoCliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone)]
struct CompilePlan {
    board: HostBoard,
    sketch_path: PathBuf,
    sketch_name: String,
    build_path: PathBuf,
}

pub fn compile_ino(path: &Path, board: HostBoard) -> Result<CompileArtifact, ArduinoCliError> {
    let plan = resolve_compile_plan(path, board)?;
    fs::create_dir_all(&plan.build_path)?;
    let cli = resolve_arduino_cli_executable();

    let output = Command::new(&cli)
        .arg("compile")
        .arg("--fqbn")
        .arg(plan.board.fqbn())
        .arg("--build-path")
        .arg(&plan.build_path)
        .arg(&plan.sketch_path)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(ArduinoCliError::CommandFailed {
            command: format!(
                "arduino-cli compile --fqbn {} --build-path {} {}",
                plan.board.fqbn(),
                plan.build_path.display(),
                plan.sketch_path.display()
            ),
            stdout,
            stderr,
        });
    }

    let hex_path = find_hex_output(&plan.build_path, &plan.sketch_name)
        .ok_or_else(|| ArduinoCliError::MissingHexOutput(plan.build_path.clone()))?;

    Ok(CompileArtifact {
        board: plan.board,
        sketch_path: plan.sketch_path,
        build_path: plan.build_path,
        hex_path,
        stdout,
        stderr,
    })
}

fn resolve_arduino_cli_executable() -> PathBuf {
    if let Some(value) = std::env::var_os("ARDUINO_CLI") {
        let candidate = PathBuf::from(value);
        if candidate.is_file() {
            return candidate;
        }
    }

    if let Some(path) = lookup_command_on_path("arduino-cli") {
        return path;
    }

    let mut candidates = vec![
        PathBuf::from("/opt/homebrew/bin/arduino-cli"),
        PathBuf::from("/usr/local/bin/arduino-cli"),
    ];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".cargo/bin/arduino-cli"));
        candidates.push(home.join(".local/bin/arduino-cli"));
    }

    for candidate in candidates {
        if candidate.is_file() {
            return candidate;
        }
    }

    PathBuf::from("arduino-cli")
}

fn lookup_command_on_path(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn resolve_compile_plan(path: &Path, board: HostBoard) -> Result<CompilePlan, ArduinoCliError> {
    let canonical = path.to_path_buf();
    if canonical.is_dir() {
        let sketch_name = canonical
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())
            .ok_or_else(|| ArduinoCliError::MissingSketchName(canonical.clone()))?;
        return Ok(CompilePlan {
            board,
            sketch_path: canonical.clone(),
            sketch_name: sketch_name.clone(),
            build_path: unique_build_path(&sketch_name, board),
        });
    }

    match canonical.extension().and_then(|value| value.to_str()) {
        Some("ino") => {
            let sketch_path = canonical
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| ArduinoCliError::MissingSketchParent(canonical.clone()))?;
            let sketch_name = canonical
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
                .ok_or_else(|| ArduinoCliError::MissingSketchName(canonical.clone()))?;
            Ok(CompilePlan {
                board,
                sketch_path,
                sketch_name: sketch_name.clone(),
                build_path: unique_build_path(&sketch_name, board),
            })
        }
        _ => Err(ArduinoCliError::UnsupportedSource(canonical)),
    }
}

fn unique_build_path(sketch_name: &str, board: HostBoard) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "arduino-simulator-{}-{}-{}",
        sanitize_name(sketch_name),
        board.short_name(),
        stamp
    ))
}

fn sanitize_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "sketch".to_string()
    } else {
        sanitized
    }
}

fn find_hex_output(build_path: &Path, sketch_name: &str) -> Option<PathBuf> {
    let preferred = build_path.join(format!("{sketch_name}.ino.hex"));
    if preferred.is_file() {
        return Some(preferred);
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(build_path).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.ends_with(".ino.hex") && !value.ends_with(".with_bootloader.hex"))
            .unwrap_or(false)
        {
            candidates.push(path);
        }
    }
    candidates.sort();
    candidates.into_iter().next()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::tempdir;

    use rust_project::HostBoard;

    use super::{
        find_hex_output, lookup_command_on_path, resolve_arduino_cli_executable,
        resolve_compile_plan, sanitize_name, ArduinoCliError,
    };

    #[test]
    fn resolve_compile_plan_accepts_ino_file() {
        let temp = tempdir().expect("tempdir");
        let sketch_dir = temp.path().join("hello");
        fs::create_dir_all(&sketch_dir).expect("mkdir");
        let ino = sketch_dir.join("hello.ino");
        fs::write(&ino, "void setup() {}\nvoid loop() {}\n").expect("write ino");

        let plan = resolve_compile_plan(&ino, HostBoard::NanoV3).expect("plan");
        assert_eq!(plan.board, HostBoard::NanoV3);
        assert_eq!(plan.sketch_path, sketch_dir);
        assert_eq!(plan.sketch_name, "hello");
        assert!(plan.build_path.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn resolve_compile_plan_accepts_sketch_directory() {
        let temp = tempdir().expect("tempdir");
        let sketch_dir = temp.path().join("controller");
        fs::create_dir_all(&sketch_dir).expect("mkdir");

        let plan = resolve_compile_plan(&sketch_dir, HostBoard::Mega2560Rev3).expect("plan");
        assert_eq!(plan.sketch_path, sketch_dir);
        assert_eq!(plan.sketch_name, "controller");
    }

    #[test]
    fn find_hex_output_prefers_plain_application_hex() {
        let temp = tempdir().expect("tempdir");
        let build = temp.path();
        let preferred = build.join("hello.ino.hex");
        let boot = build.join("hello.ino.with_bootloader.hex");
        fs::write(&preferred, ":00000001FF\n").expect("write preferred");
        fs::write(&boot, ":00000001FF\n").expect("write boot");

        let found = find_hex_output(build, "hello").expect("hex");
        assert_eq!(found, preferred);
    }

    #[test]
    fn sanitize_name_replaces_invalid_characters_and_defaults_when_empty() {
        assert_eq!(sanitize_name("hello world!"), "hello-world-");
        assert_eq!(sanitize_name("already_ok"), "already_ok");
        assert_eq!(sanitize_name(""), "sketch");
    }

    #[test]
    fn resolve_compile_plan_rejects_unsupported_sources() {
        let temp = tempdir().expect("tempdir");
        let source = temp.path().join("notes.txt");
        fs::write(&source, "not a sketch").expect("write text");

        assert!(matches!(
            resolve_compile_plan(&source, HostBoard::NanoV3),
            Err(ArduinoCliError::UnsupportedSource(path)) if path == source
        ));
    }

    #[test]
    fn find_hex_output_falls_back_to_sorted_candidates() {
        let temp = tempdir().expect("tempdir");
        let build = temp.path();
        let alpha = build.join("alpha.ino.hex");
        let zeta = build.join("zeta.ino.hex");
        let boot = build.join("alpha.ino.with_bootloader.hex");
        fs::write(&alpha, ":00000001FF\n").expect("write alpha");
        fs::write(&zeta, ":00000001FF\n").expect("write zeta");
        fs::write(&boot, ":00000001FF\n").expect("write boot");

        let found = find_hex_output(build, "missing").expect("fallback hex");
        assert_eq!(found, alpha);
    }

    #[test]
    fn error_display_messages_are_human_readable() {
        let io_error = ArduinoCliError::Io(std::io::Error::other("tool missing"));
        assert_eq!(io_error.to_string(), "tool missing");

        let unsupported = ArduinoCliError::UnsupportedSource(PathBuf::from("notes.txt"));
        assert_eq!(
            unsupported.to_string(),
            "expected a sketch directory or .ino file, got notes.txt"
        );

        let missing_parent = ArduinoCliError::MissingSketchParent(PathBuf::from("hello.ino"));
        assert_eq!(
            missing_parent.to_string(),
            "missing parent directory for hello.ino"
        );

        let missing_name = ArduinoCliError::MissingSketchName(PathBuf::from("/tmp/.ino"));
        assert_eq!(
            missing_name.to_string(),
            "could not determine sketch name for /tmp/.ino"
        );

        let missing_hex = ArduinoCliError::MissingHexOutput(PathBuf::from("/tmp/build"));
        assert_eq!(
            missing_hex.to_string(),
            "arduino-cli finished but no .hex output was found in /tmp/build"
        );

        let failed = ArduinoCliError::CommandFailed {
            command: "arduino-cli compile".to_string(),
            stdout: "build output".to_string(),
            stderr: "bad exit".to_string(),
        };
        assert!(failed
            .to_string()
            .contains("arduino-cli compile failed\nstdout:\nbuild output\n\nstderr:\nbad exit"));
    }

    #[test]
    fn command_lookup_checks_path_entries() {
        let temp = tempdir().expect("tempdir");
        let bin = temp.path().join("bin");
        fs::create_dir_all(&bin).expect("mkdir");
        let cli = bin.join("arduino-cli");
        fs::write(&cli, "#!/bin/sh\n").expect("write");

        let original_path = std::env::var_os("PATH");
        std::env::set_var("PATH", bin.as_os_str());
        let found = lookup_command_on_path("arduino-cli");
        if let Some(path) = original_path {
            std::env::set_var("PATH", path);
        } else {
            std::env::remove_var("PATH");
        }

        assert_eq!(found, Some(cli));
    }

    #[test]
    fn arduino_cli_resolution_prefers_explicit_environment_override() {
        let temp = tempdir().expect("tempdir");
        let cli = temp.path().join("arduino-cli");
        fs::write(&cli, "#!/bin/sh\n").expect("write");

        let previous = std::env::var_os("ARDUINO_CLI");
        std::env::set_var("ARDUINO_CLI", &cli);
        let resolved = resolve_arduino_cli_executable();
        if let Some(value) = previous {
            std::env::set_var("ARDUINO_CLI", value);
        } else {
            std::env::remove_var("ARDUINO_CLI");
        }

        assert_eq!(resolved, cli);
    }
}
