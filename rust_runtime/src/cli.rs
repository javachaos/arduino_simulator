use std::fmt;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::firmware::load_hex_file;
use crate::runtime::{MegaRuntime, NanoRuntime, RuntimeExit};
use crate::tui::{
    default_chunk_size as default_monitor_chunk_size,
    default_refresh_ms as default_monitor_refresh_ms, monitor_mega, monitor_nano, MonitorConfig,
};

#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Io(io::Error),
    Firmware(crate::firmware::HexLoadError),
    Cpu(rust_cpu::CpuError),
    NoSerialOutput,
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Usage(message) => write!(f, "{message}"),
            CliError::Io(error) => write!(f, "{error}"),
            CliError::Firmware(error) => write!(f, "{error}"),
            CliError::Cpu(error) => write!(f, "{error}"),
            CliError::NoSerialOutput => write!(f, "no serial output captured"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<crate::firmware::HexLoadError> for CliError {
    fn from(value: crate::firmware::HexLoadError) -> Self {
        Self::Firmware(value)
    }
}

impl From<rust_cpu::CpuError> for CliError {
    fn from(value: rust_cpu::CpuError) -> Self {
        Self::Cpu(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunConfig {
    pub firmware_path: PathBuf,
    pub max_instructions: Option<usize>,
    pub until_serial: bool,
    pub chunk_size: usize,
    pub out_path: Option<PathBuf>,
}

pub fn run_cli(args: impl IntoIterator<Item = String>) -> Result<i32, CliError> {
    let mut iter = args.into_iter();
    let _program_name = iter.next();
    let Some(command) = iter.next() else {
        return Err(CliError::Usage(usage()));
    };

    match command.as_str() {
        "run-nano" => {
            let config = parse_run_config(iter)?;
            run_nano(config)
        }
        "run-mega" => {
            let config = parse_run_config(iter)?;
            run_mega(config)
        }
        "monitor-nano" => {
            let config = parse_monitor_config(iter)?;
            monitor_nano(config)
        }
        "monitor-mega" => {
            let config = parse_monitor_config(iter)?;
            monitor_mega(config)
        }
        "--help" | "-h" | "help" => {
            print!("{}", usage());
            Ok(0)
        }
        other => Err(CliError::Usage(format!(
            "unknown command `{other}`\n\n{}",
            usage()
        ))),
    }
}

fn parse_run_config(args: impl IntoIterator<Item = String>) -> Result<RunConfig, CliError> {
    let mut firmware_path: Option<PathBuf> = None;
    let mut max_instructions: Option<usize> = None;
    let mut until_serial = false;
    let mut chunk_size: usize = 10_000;
    let mut out_path: Option<PathBuf> = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-instructions" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "--max-instructions requires a numeric value".to_string(),
                    ));
                };
                max_instructions = Some(parse_usize("--max-instructions", &value)?);
            }
            "--until-serial" => until_serial = true,
            "--chunk-size" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "--chunk-size requires a numeric value".to_string(),
                    ));
                };
                chunk_size = parse_usize("--chunk-size", &value)?.max(1);
            }
            "--out" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage("--out requires a file path".to_string()));
                };
                out_path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => return Err(CliError::Usage(usage())),
            value if value.starts_with('-') => {
                return Err(CliError::Usage(format!(
                    "unknown option `{value}`\n\n{}",
                    usage()
                )));
            }
            value => {
                if firmware_path.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected extra positional argument `{value}`"
                    )));
                }
                firmware_path = Some(PathBuf::from(value));
            }
        }
    }

    let Some(firmware_path) = firmware_path else {
        return Err(CliError::Usage(format!(
            "missing firmware hex path\n\n{}",
            usage()
        )));
    };

    Ok(RunConfig {
        firmware_path,
        max_instructions,
        until_serial,
        chunk_size,
        out_path,
    })
}

fn parse_monitor_config(args: impl IntoIterator<Item = String>) -> Result<MonitorConfig, CliError> {
    let mut firmware_path: Option<PathBuf> = None;
    let mut max_instructions: Option<usize> = None;
    let mut chunk_size = default_monitor_chunk_size();
    let mut refresh_ms = default_monitor_refresh_ms();

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--max-instructions" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "--max-instructions requires a numeric value".to_string(),
                    ));
                };
                max_instructions = Some(parse_usize("--max-instructions", &value)?);
            }
            "--chunk-size" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "--chunk-size requires a numeric value".to_string(),
                    ));
                };
                chunk_size = parse_usize("--chunk-size", &value)?.max(1);
            }
            "--refresh-ms" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "--refresh-ms requires a numeric value".to_string(),
                    ));
                };
                refresh_ms = parse_usize("--refresh-ms", &value)? as u64;
            }
            "--help" | "-h" => return Err(CliError::Usage(usage())),
            value if value.starts_with('-') => {
                return Err(CliError::Usage(format!(
                    "unknown option `{value}`\n\n{}",
                    usage()
                )));
            }
            value => {
                if firmware_path.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected extra positional argument `{value}`"
                    )));
                }
                firmware_path = Some(PathBuf::from(value));
            }
        }
    }

    let Some(firmware_path) = firmware_path else {
        return Err(CliError::Usage(format!(
            "missing firmware hex path\n\n{}",
            usage()
        )));
    };

    Ok(MonitorConfig {
        firmware_path,
        max_instructions,
        chunk_size,
        refresh_ms,
    })
}

fn parse_usize(flag: &str, value: &str) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|_| CliError::Usage(format!("{flag} expects an unsigned integer, got `{value}`")))
}

fn run_nano(config: RunConfig) -> Result<i32, CliError> {
    let mut runtime = NanoRuntime::new();
    load_hex_file(&mut runtime.cpu, &config.firmware_path)?;
    run_loop(
        runtime,
        config.max_instructions,
        config.until_serial,
        config.chunk_size,
        config.out_path.as_deref(),
    )
}

fn run_mega(config: RunConfig) -> Result<i32, CliError> {
    let mut runtime = MegaRuntime::new();
    load_hex_file(&mut runtime.cpu, &config.firmware_path)?;
    run_loop(
        runtime,
        config.max_instructions,
        config.until_serial,
        config.chunk_size,
        config.out_path.as_deref(),
    )
}

trait RunnableRuntime {
    fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), rust_cpu::CpuError>;
    fn take_new_serial_bytes(&mut self) -> &[u8];
    fn serial_output_bytes(&self) -> &[u8];
}

impl RunnableRuntime for NanoRuntime {
    fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), rust_cpu::CpuError> {
        NanoRuntime::run_chunk(self, instruction_budget, until_serial)
    }

    fn take_new_serial_bytes(&mut self) -> &[u8] {
        NanoRuntime::take_new_serial_bytes(self)
    }

    fn serial_output_bytes(&self) -> &[u8] {
        NanoRuntime::serial_output_bytes(self)
    }
}

impl RunnableRuntime for MegaRuntime {
    fn run_chunk(
        &mut self,
        instruction_budget: usize,
        until_serial: bool,
    ) -> Result<(usize, Option<RuntimeExit>), rust_cpu::CpuError> {
        MegaRuntime::run_chunk(self, instruction_budget, until_serial)
    }

    fn take_new_serial_bytes(&mut self) -> &[u8] {
        MegaRuntime::take_new_serial_bytes(self)
    }

    fn serial_output_bytes(&self) -> &[u8] {
        MegaRuntime::serial_output_bytes(self)
    }
}

fn run_loop<R: RunnableRuntime>(
    mut runtime: R,
    max_instructions: Option<usize>,
    until_serial: bool,
    chunk_size: usize,
    out_path: Option<&Path>,
) -> Result<i32, CliError> {
    let mut sink = OutputSink::new(out_path)?;
    let mut executed_total = 0usize;
    let mut remaining = max_instructions;

    loop {
        let budget = remaining
            .map(|limit| limit.min(chunk_size))
            .unwrap_or(chunk_size);
        if budget == 0 {
            break;
        }

        let (executed, exit) = runtime.run_chunk(budget, until_serial)?;
        executed_total += executed;
        if let Some(limit) = remaining.as_mut() {
            *limit = limit.saturating_sub(executed);
        }

        let new_bytes = runtime.take_new_serial_bytes().to_vec();
        if !new_bytes.is_empty() {
            sink.write_all(&new_bytes)?;
            if until_serial {
                sink.flush()?;
                return Ok(0);
            }
        }

        match exit {
            Some(RuntimeExit::BreakHit | RuntimeExit::Sleeping) => break,
            Some(RuntimeExit::UntilSerialSatisfied) => {
                sink.flush()?;
                return Ok(0);
            }
            Some(RuntimeExit::MaxInstructionsReached) => {
                if remaining == Some(0) {
                    break;
                }
            }
            None => {}
        }

        if executed == 0 {
            break;
        }
    }

    sink.flush()?;
    if until_serial && runtime.serial_output_bytes().is_empty() {
        return Err(CliError::NoSerialOutput);
    }
    let _ = executed_total;
    Ok(0)
}

enum OutputSink {
    Stdout(io::Stdout),
    File(BufWriter<File>),
}

impl OutputSink {
    fn new(path: Option<&Path>) -> Result<Self, io::Error> {
        match path {
            Some(path) => Ok(Self::File(BufWriter::new(File::create(path)?))),
            None => Ok(Self::Stdout(io::stdout())),
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), io::Error> {
        match self {
            OutputSink::Stdout(stdout) => stdout.write_all(bytes),
            OutputSink::File(file) => file.write_all(bytes),
        }
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        match self {
            OutputSink::Stdout(stdout) => stdout.flush(),
            OutputSink::File(file) => file.flush(),
        }
    }
}

fn usage() -> String {
    [
        "Usage:",
        "  arduino-simulator run-nano <firmware.hex> [--max-instructions N] [--until-serial] [--chunk-size N] [--out PATH]",
        "  arduino-simulator run-mega <firmware.hex> [--max-instructions N] [--until-serial] [--chunk-size N] [--out PATH]",
        "  arduino-simulator monitor-nano <firmware.hex> [--max-instructions N] [--chunk-size N] [--refresh-ms N]",
        "  arduino-simulator monitor-mega <firmware.hex> [--max-instructions N] [--chunk-size N] [--refresh-ms N]",
        "",
        "By default the simulator keeps running until the firmware breaks, sleeps, or you interrupt it.",
        "Monitor controls: Space pause/resume, S step, C clear serial, Q quit.",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use rust_cpu::CpuError;

    use super::{
        parse_monitor_config, parse_run_config, parse_usize, run_cli, run_loop, usage, CliError,
        RunnableRuntime,
    };
    use crate::runtime::RuntimeExit;

    #[derive(Debug)]
    struct FakeRuntime {
        results: VecDeque<Result<(usize, Option<RuntimeExit>), CpuError>>,
        serial_batches: VecDeque<Vec<u8>>,
        last_taken: Vec<u8>,
        total_serial: Vec<u8>,
    }

    impl FakeRuntime {
        fn new(
            results: impl IntoIterator<Item = Result<(usize, Option<RuntimeExit>), CpuError>>,
            serial_batches: impl IntoIterator<Item = Vec<u8>>,
        ) -> Self {
            Self {
                results: results.into_iter().collect(),
                serial_batches: serial_batches.into_iter().collect(),
                last_taken: Vec::new(),
                total_serial: Vec::new(),
            }
        }
    }

    impl RunnableRuntime for FakeRuntime {
        fn run_chunk(
            &mut self,
            _instruction_budget: usize,
            _until_serial: bool,
        ) -> Result<(usize, Option<RuntimeExit>), CpuError> {
            self.results.pop_front().unwrap_or(Ok((0, None)))
        }

        fn take_new_serial_bytes(&mut self) -> &[u8] {
            self.last_taken = self.serial_batches.pop_front().unwrap_or_default();
            self.total_serial.extend_from_slice(&self.last_taken);
            &self.last_taken
        }

        fn serial_output_bytes(&self) -> &[u8] {
            &self.total_serial
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("arduino-simulator-cli-{name}-{unique}.txt"))
    }

    #[test]
    fn usage_mentions_monitor_commands() {
        let text = usage();
        assert!(text.contains("monitor-nano"));
        assert!(text.contains("monitor-mega"));
        assert!(text.contains("Space pause/resume"));
    }

    #[test]
    fn parse_monitor_config_uses_expected_defaults() {
        let config = parse_monitor_config(vec!["fixture.hex".to_string()]).unwrap();
        assert_eq!(config.firmware_path.to_string_lossy(), "fixture.hex");
        assert_eq!(config.max_instructions, None);
        assert_eq!(config.chunk_size, 10_000);
        assert_eq!(config.refresh_ms, 50);
    }

    #[test]
    fn parse_run_config_accepts_flags_and_clamps_chunk_size() {
        let config = parse_run_config(vec![
            "firmware.hex".to_string(),
            "--max-instructions".to_string(),
            "123".to_string(),
            "--until-serial".to_string(),
            "--chunk-size".to_string(),
            "0".to_string(),
            "--out".to_string(),
            "serial.log".to_string(),
        ])
        .expect("run config");

        assert_eq!(config.firmware_path, PathBuf::from("firmware.hex"));
        assert_eq!(config.max_instructions, Some(123));
        assert!(config.until_serial);
        assert_eq!(config.chunk_size, 1);
        assert_eq!(config.out_path, Some(PathBuf::from("serial.log")));
    }

    #[test]
    fn parse_run_config_reports_usage_errors() {
        assert!(matches!(
            parse_run_config(Vec::<String>::new()),
            Err(CliError::Usage(message)) if message.contains("missing firmware hex path")
        ));

        assert!(matches!(
            parse_run_config(vec!["--max-instructions".to_string()]),
            Err(CliError::Usage(message))
                if message == "--max-instructions requires a numeric value"
        ));

        assert!(matches!(
            parse_run_config(vec!["firmware.hex".to_string(), "--out".to_string()]),
            Err(CliError::Usage(message)) if message == "--out requires a file path"
        ));

        assert!(matches!(
            parse_run_config(vec!["firmware.hex".to_string(), "--bogus".to_string()]),
            Err(CliError::Usage(message)) if message.contains("unknown option `--bogus`")
        ));

        assert!(matches!(
            parse_run_config(vec![
                "firmware.hex".to_string(),
                "extra.hex".to_string(),
            ]),
            Err(CliError::Usage(message))
                if message == "unexpected extra positional argument `extra.hex`"
        ));

        assert!(matches!(
            parse_run_config(vec!["--help".to_string()]),
            Err(CliError::Usage(message)) if message.contains("Usage:")
        ));
    }

    #[test]
    fn parse_monitor_config_accepts_overrides_and_reports_errors() {
        let config = parse_monitor_config(vec![
            "monitor.hex".to_string(),
            "--max-instructions".to_string(),
            "55".to_string(),
            "--chunk-size".to_string(),
            "0".to_string(),
            "--refresh-ms".to_string(),
            "250".to_string(),
        ])
        .expect("monitor config");

        assert_eq!(config.firmware_path, PathBuf::from("monitor.hex"));
        assert_eq!(config.max_instructions, Some(55));
        assert_eq!(config.chunk_size, 1);
        assert_eq!(config.refresh_ms, 250);

        assert!(matches!(
            parse_monitor_config(vec!["--refresh-ms".to_string()]),
            Err(CliError::Usage(message))
                if message == "--refresh-ms requires a numeric value"
        ));

        assert!(matches!(
            parse_monitor_config(vec!["monitor.hex".to_string(), "--oops".to_string()]),
            Err(CliError::Usage(message)) if message.contains("unknown option `--oops`")
        ));
    }

    #[test]
    fn parse_usize_rejects_non_numeric_values() {
        assert!(matches!(
            parse_usize("--chunk-size", "abc"),
            Err(CliError::Usage(message))
                if message == "--chunk-size expects an unsigned integer, got `abc`"
        ));
    }

    #[test]
    fn run_loop_writes_serial_output_to_file_and_returns_zero() {
        let out_path = temp_path("serial-output");
        let runtime = FakeRuntime::new(
            [Ok((3, Some(RuntimeExit::UntilSerialSatisfied)))],
            [b"OK".to_vec()],
        );

        let status = run_loop(runtime, None, true, 64, Some(&out_path)).expect("status");
        assert_eq!(status, 0);
        assert_eq!(fs::read(&out_path).expect("output"), b"OK");

        let _ = fs::remove_file(out_path);
    }

    #[test]
    fn run_loop_returns_no_serial_output_when_requested() {
        let runtime = FakeRuntime::new([Ok((1, Some(RuntimeExit::BreakHit)))], [Vec::new()]);

        assert!(matches!(
            run_loop(runtime, None, true, 64, None),
            Err(CliError::NoSerialOutput)
        ));
    }

    #[test]
    fn run_loop_short_circuits_when_instruction_budget_is_zero() {
        let runtime = FakeRuntime::new(
            Vec::<Result<(usize, Option<RuntimeExit>), CpuError>>::new(),
            Vec::<Vec<u8>>::new(),
        );
        let status = run_loop(runtime, Some(0), false, 64, None).expect("status");
        assert_eq!(status, 0);
    }

    #[test]
    fn run_loop_propagates_cpu_errors() {
        let runtime = FakeRuntime::new([Err(CpuError::Sleeping)], [Vec::new()]);

        assert!(matches!(
            run_loop(runtime, None, false, 64, None),
            Err(CliError::Cpu(CpuError::Sleeping))
        ));
    }

    #[test]
    fn run_cli_handles_help_and_unknown_commands() {
        assert_eq!(
            run_cli(["arduino-simulator".to_string(), "help".to_string()]).expect("help"),
            0
        );

        assert!(matches!(
            run_cli(["arduino-simulator".to_string(), "mystery".to_string()]),
            Err(CliError::Usage(message))
                if message.contains("unknown command `mystery`") && message.contains("Usage:")
        ));
    }
}
