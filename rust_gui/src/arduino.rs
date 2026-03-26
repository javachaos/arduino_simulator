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

    let output = Command::new("arduino-cli")
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

    use tempfile::tempdir;

    use rust_project::HostBoard;

    use super::{find_hex_output, resolve_compile_plan};

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
}
