use std::collections::BTreeSet;
use std::path::Path;

use rust_board::load_built_in_board_model;

use crate::{BindingMode, HostBoard, SignalBinding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetSuggestion {
    pub net_name: String,
    pub score: i32,
    pub reason: &'static str,
}

impl NetSuggestion {
    pub fn confidence_label(&self) -> &'static str {
        match self.score {
            900.. => "exact",
            450.. => "high",
            250.. => "medium",
            _ => "low",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum NumericHintKind {
    Unknown,
    Digital,
    Analog,
}

pub fn inferred_host_board_from_source(path: impl AsRef<Path>) -> Option<HostBoard> {
    let normalized = path.as_ref().to_string_lossy().trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if normalized.contains("atmega328") || normalized.contains("328p") {
        return Some(HostBoard::NanoV3);
    }
    if normalized.contains("atmega2560") || normalized.contains("2560") {
        return Some(HostBoard::Mega2560Rev3);
    }

    let looks_like_nano = normalized.contains("nano");
    let looks_like_mega = normalized.contains("/mega")
        || normalized.contains("_mega")
        || normalized.contains("mega_");

    match (looks_like_nano, looks_like_mega) {
        (true, false) => Some(HostBoard::NanoV3),
        (false, true) => Some(HostBoard::Mega2560Rev3),
        _ => None,
    }
}

pub fn default_project_name(source_path: impl AsRef<Path>, pcb_path: impl AsRef<Path>) -> String {
    for candidate in [source_path.as_ref(), pcb_path.as_ref()] {
        if let Some(stem) = display_stem_for_path(candidate) {
            return stem;
        }
    }
    "Untitled Simulation".to_string()
}

pub fn host_signals_for_board(board: HostBoard) -> Vec<String> {
    load_built_in_board_model(board.builtin_board_model())
        .map(|board| board.nets.into_iter().map(|net| net.name).collect())
        .unwrap_or_default()
}

pub fn infer_binding_mode(signal: &str) -> BindingMode {
    if signal.starts_with('A') || signal.contains("_PWM") || signal == "AREF" {
        return BindingMode::Analog;
    }
    if signal.contains("_SDA")
        || signal.contains("_SCL")
        || signal.contains("_MISO")
        || signal.contains("_MOSI")
        || signal.contains("_SCK")
        || signal.ends_with("_SS")
        || signal.ends_with("_RX")
        || signal.ends_with("_TX")
    {
        return BindingMode::Bus;
    }
    if signal == "GND"
        || signal == "+5V"
        || signal == "+3V3"
        || signal == "VIN"
        || signal == "IOREF"
        || signal == "RESET"
    {
        return BindingMode::Power;
    }
    BindingMode::Digital
}

pub fn controller_signal_suggestions(
    board: HostBoard,
    signal: &str,
    available_nets: &BTreeSet<String>,
) -> Vec<NetSuggestion> {
    suggest_pcb_nets(signal, &candidate_pcb_nets(board, signal), available_nets)
}

pub fn auto_bind_host_board(
    board: HostBoard,
    available_nets: &BTreeSet<String>,
) -> Vec<SignalBinding> {
    let mut bindings = Vec::new();

    for signal in host_signals_for_board(board) {
        let suggestions = controller_signal_suggestions(board, &signal, available_nets);
        if !should_auto_apply_suggestion(&suggestions) {
            continue;
        }
        let Some(best) = suggestions.first() else {
            continue;
        };

        bindings.push(SignalBinding {
            board_signal: signal.clone(),
            pcb_net: best.net_name.clone(),
            mode: infer_binding_mode(&signal),
            note: Some(format!(
                "Auto-bound by arduino_simulator ({}, {})",
                best.confidence_label(),
                best.reason
            )),
        });
    }

    bindings
}

pub fn sync_host_board_bindings(
    board: HostBoard,
    available_nets: &BTreeSet<String>,
    existing: &[SignalBinding],
) -> Vec<SignalBinding> {
    let valid_signals = host_signals_for_board(board)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut merged = existing
        .iter()
        .filter(|binding| {
            valid_signals.contains(&binding.board_signal)
                && available_nets.contains(&binding.pcb_net)
        })
        .cloned()
        .collect::<Vec<_>>();
    let bound_signals = merged
        .iter()
        .map(|binding| binding.board_signal.clone())
        .collect::<BTreeSet<_>>();

    for binding in auto_bind_host_board(board, available_nets) {
        if !bound_signals.contains(&binding.board_signal) {
            merged.push(binding);
        }
    }

    merged.sort_by(|left, right| left.board_signal.cmp(&right.board_signal));
    merged
}

fn display_stem_for_path(path: &Path) -> Option<String> {
    let file_name = path.file_name().and_then(|value| value.to_str())?;
    let stem = strip_avrsim_suffix(file_name);
    if stem != file_name && !stem.trim().is_empty() {
        return Some(stem.to_string());
    }
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
}

fn strip_avrsim_suffix(name: &str) -> &str {
    name.strip_suffix(".board.avrsim.json")
        .or_else(|| name.strip_suffix(".avrsim.json"))
        .or_else(|| name.strip_suffix(".board.avrsim"))
        .or_else(|| name.strip_suffix(".avrsim"))
        .or_else(|| name.strip_suffix(".json"))
        .unwrap_or(name)
}

fn candidate_pcb_nets(board: HostBoard, signal: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push_unique = |candidate: String| {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    };

    push_unique(signal.to_string());

    if let Some(number) = signal
        .strip_prefix('D')
        .and_then(|rest| rest.split('_').next())
        .filter(|value| {
            !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
        })
    {
        push_unique(format!("/{number}"));
        push_unique(format!("/*{number}"));
    }

    if let Some(number) = signal.strip_prefix('A').filter(|value| {
        !value.is_empty() && value.chars().all(|character| character.is_ascii_digit())
    }) {
        push_unique(format!("A{number}"));
        push_unique(format!("/A{number}"));
    }

    for alias in controller_signal_aliases(board, signal) {
        push_unique(alias);
    }

    candidates
}

fn controller_signal_aliases(board: HostBoard, signal: &str) -> Vec<String> {
    let mut aliases = controller_port_aliases(board, signal);
    aliases.extend(controller_board_label_aliases(board, signal));
    if let Some(adc_alias) = analog_channel_alias(signal) {
        aliases.push(adc_alias.clone());
        aliases.push(format!("/{adc_alias}"));
    }
    aliases
}

fn analog_channel_alias(signal: &str) -> Option<String> {
    parse_board_signal_number(signal, 'A').map(|number| format!("ADC{number}"))
}

fn controller_port_aliases(board: HostBoard, signal: &str) -> Vec<String> {
    match board {
        HostBoard::Mega2560Rev3 => mega_controller_port_aliases(signal),
        HostBoard::NanoV3 => nano_controller_port_aliases(signal),
    }
}

fn controller_board_label_aliases(board: HostBoard, signal: &str) -> Vec<String> {
    match board {
        HostBoard::Mega2560Rev3 => Vec::new(),
        HostBoard::NanoV3 => nano_controller_board_label_aliases(signal),
    }
}

fn parse_board_signal_number(signal: &str, prefix: char) -> Option<u8> {
    let rest = signal.strip_prefix(prefix)?;
    let digits = rest.split('_').next()?;
    if digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn push_port_alias(aliases: &mut Vec<String>, port: &str) {
    aliases.push(port.to_string());
    aliases.push(format!("/{port}"));
}

fn mega_controller_port_aliases(signal: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    if let Some(number) = parse_board_signal_number(signal, 'D') {
        let port = match number {
            0 => Some("PE0"),
            1 => Some("PE1"),
            2 => Some("PE4"),
            3 => Some("PE5"),
            4 => Some("PG5"),
            5 => Some("PE3"),
            6 => Some("PH3"),
            7 => Some("PH4"),
            8 => Some("PH5"),
            9 => Some("PH6"),
            10 => Some("PB4"),
            11 => Some("PB5"),
            12 => Some("PB6"),
            13 => Some("PB7"),
            14 => Some("PJ1"),
            15 => Some("PJ0"),
            16 => Some("PH1"),
            17 => Some("PH0"),
            18 => Some("PD3"),
            19 => Some("PD2"),
            20 => Some("PD1"),
            21 => Some("PD0"),
            22 => Some("PA0"),
            23 => Some("PA1"),
            24 => Some("PA2"),
            25 => Some("PA3"),
            26 => Some("PA4"),
            27 => Some("PA5"),
            28 => Some("PA6"),
            29 => Some("PA7"),
            30 => Some("PC7"),
            31 => Some("PC6"),
            32 => Some("PC5"),
            33 => Some("PC4"),
            34 => Some("PC3"),
            35 => Some("PC2"),
            36 => Some("PC1"),
            37 => Some("PC0"),
            38 => Some("PD7"),
            39 => Some("PG2"),
            40 => Some("PG1"),
            41 => Some("PG0"),
            42 => Some("PL7"),
            43 => Some("PL6"),
            44 => Some("PL5"),
            45 => Some("PL4"),
            46 => Some("PL3"),
            47 => Some("PL2"),
            48 => Some("PL1"),
            49 => Some("PL0"),
            50 => Some("PB3"),
            51 => Some("PB2"),
            52 => Some("PB1"),
            53 => Some("PB0"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    if let Some(number) = parse_board_signal_number(signal, 'A') {
        let port = match number {
            0 => Some("PF0"),
            1 => Some("PF1"),
            2 => Some("PF2"),
            3 => Some("PF3"),
            4 => Some("PF4"),
            5 => Some("PF5"),
            6 => Some("PF6"),
            7 => Some("PF7"),
            8 => Some("PK0"),
            9 => Some("PK1"),
            10 => Some("PK2"),
            11 => Some("PK3"),
            12 => Some("PK4"),
            13 => Some("PK5"),
            14 => Some("PK6"),
            15 => Some("PK7"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    aliases
}

fn nano_controller_port_aliases(signal: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    if let Some(number) = parse_board_signal_number(signal, 'D') {
        let port = match number {
            0 => Some("PD0"),
            1 => Some("PD1"),
            2 => Some("PD2"),
            3 => Some("PD3"),
            4 => Some("PD4"),
            5 => Some("PD5"),
            6 => Some("PD6"),
            7 => Some("PD7"),
            8 => Some("PB0"),
            9 => Some("PB1"),
            10 => Some("PB2"),
            11 => Some("PB3"),
            12 => Some("PB4"),
            13 => Some("PB5"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    if let Some(number) = parse_board_signal_number(signal, 'A') {
        let port = match number {
            0 => Some("PC0"),
            1 => Some("PC1"),
            2 => Some("PC2"),
            3 => Some("PC3"),
            4 => Some("PC4"),
            5 => Some("PC5"),
            _ => None,
        };
        if let Some(port) = port {
            push_port_alias(&mut aliases, port);
        }
    }

    aliases
}

fn nano_controller_board_label_aliases(signal: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut push_label_aliases = |label: &str| {
        aliases.push(label.to_string());
        aliases.push(format!("/{label}"));
        aliases.push(format!("/{}", label.replace('/', "{slash}")));
    };

    match signal {
        "D0_RX" | "D0" => push_label_aliases("D0/RX"),
        "D1_TX" | "D1" => push_label_aliases("D1/TX"),
        "D10_SS" | "D10" => push_label_aliases("D10/SS"),
        "D11_MOSI" | "D11" => push_label_aliases("D11/MOSI"),
        "D12_MISO" | "D12" => push_label_aliases("D12/MISO"),
        "D13_SCK" | "D13" => push_label_aliases("D13/SCK"),
        "A4_SDA" | "A4" => push_label_aliases("A4/SDA"),
        "A5_SCL" | "A5" => push_label_aliases("A5/SCL"),
        _ => {}
    }

    aliases
}

fn suggest_pcb_nets(
    signal: &str,
    candidates: &[String],
    available_nets: &BTreeSet<String>,
) -> Vec<NetSuggestion> {
    let exact_candidates = candidates
        .iter()
        .map(|candidate| candidate.to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let canonical_candidates = candidates
        .iter()
        .map(|candidate| canonical_signal_name(candidate))
        .collect::<BTreeSet<_>>();
    let candidate_tokens = collect_match_tokens(candidates);
    let candidate_numbers = collect_number_hints(signal, candidates);
    let signal_mode = infer_binding_mode(signal);

    let mut suggestions = available_nets
        .iter()
        .filter_map(|net| {
            let mut score = 0i32;
            let mut strongest_reason = "";
            let mut strongest_reason_score = 0i32;
            let mut bump = |points: i32, reason: &'static str| {
                if points <= 0 {
                    return;
                }
                score += points;
                if points > strongest_reason_score {
                    strongest_reason_score = points;
                    strongest_reason = reason;
                }
            };

            let net_upper = net.to_ascii_uppercase();
            let net_canonical = canonical_signal_name(net);
            if exact_candidates.contains(&net_upper) {
                bump(1200, "exact alias");
            }
            if canonical_candidates.contains(&net_canonical) {
                bump(950, "normalized name");
            }
            if canonical_candidates.iter().any(|candidate| {
                candidate.len() > 2
                    && !candidate
                        .chars()
                        .all(|character| character.is_ascii_digit())
                    && net_canonical.contains(candidate)
            }) {
                bump(180, "shared name");
            }

            let net_tokens = match_tokens(net);
            let role_overlap = candidate_tokens
                .iter()
                .filter(|token| {
                    !token.chars().all(|character| character.is_ascii_digit())
                        && net_tokens.contains(*token)
                })
                .count();
            if role_overlap > 0 {
                bump((role_overlap.min(2) as i32) * 140, "shared signal role");
            }

            let net_numbers = extract_number_hints(net);
            let mut best_number_points = 0i32;
            for (candidate_number, candidate_kind) in &candidate_numbers {
                for (net_number, net_kind) in &net_numbers {
                    if candidate_number != net_number {
                        continue;
                    }
                    let points = match (candidate_kind, net_kind) {
                        (left, right) if left == right && *left != NumericHintKind::Unknown => 320,
                        (NumericHintKind::Unknown, NumericHintKind::Unknown) => 220,
                        (NumericHintKind::Unknown, _) | (_, NumericHintKind::Unknown) => 260,
                        _ => 0,
                    };
                    best_number_points = best_number_points.max(points);
                }
            }
            if best_number_points > 0 {
                bump(best_number_points, "pin number");
            }

            let net_mode = infer_net_binding_mode(net);
            if signal_mode == net_mode {
                let points = match signal_mode {
                    BindingMode::Power => 120,
                    BindingMode::Bus => 100,
                    BindingMode::Analog => 90,
                    BindingMode::Digital => 60,
                    BindingMode::Auto => 0,
                };
                bump(points, "signal class");
            }

            if score > 0 {
                Some(NetSuggestion {
                    net_name: net.clone(),
                    score,
                    reason: if strongest_reason.is_empty() {
                        "heuristic match"
                    } else {
                        strongest_reason
                    },
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    suggestions.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.net_name.cmp(&right.net_name))
    });
    suggestions
}

fn should_auto_apply_suggestion(suggestions: &[NetSuggestion]) -> bool {
    let Some(best) = suggestions.first() else {
        return false;
    };
    let second_score = suggestions
        .get(1)
        .map(|suggestion| suggestion.score)
        .unwrap_or(0);
    best.score >= 900
        || (best.score >= 450 && (best.score - second_score) >= 80)
        || (best.score >= 320 && (best.score - second_score) >= 140)
}

fn split_match_tokens(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_is_alpha: Option<bool> = None;

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            let is_alpha = character.is_ascii_alphabetic();
            if current_is_alpha.is_some_and(|previous| previous != is_alpha) && !current.is_empty()
            {
                tokens.push(std::mem::take(&mut current));
            }
            current.push(character.to_ascii_uppercase());
            current_is_alpha = Some(is_alpha);
        } else {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            current_is_alpha = None;
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn normalize_match_token(token: &str) -> String {
    match token {
        "SS" => "CS".to_string(),
        "CLK" => "SCK".to_string(),
        "SDI" | "SI" => "MOSI".to_string(),
        "SDO" | "SO" => "MISO".to_string(),
        "IRQ" => "INT".to_string(),
        "GROUND" | "GRND" | "VSS" => "GND".to_string(),
        "VDD" => "VCC".to_string(),
        _ => token.to_string(),
    }
}

fn is_noise_match_token(token: &str) -> bool {
    matches!(
        token,
        "D" | "A" | "GPIO" | "IO" | "PIN" | "PAD" | "NET" | "SIG" | "SIGNAL" | "PORT"
    )
}

fn match_tokens(value: &str) -> BTreeSet<String> {
    split_match_tokens(value)
        .into_iter()
        .map(|token| normalize_match_token(&token))
        .filter(|token| !token.is_empty() && !is_noise_match_token(token))
        .collect()
}

fn collect_match_tokens(candidates: &[String]) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for candidate in candidates {
        tokens.extend(match_tokens(candidate));
    }
    tokens
}

fn extract_number_hints(value: &str) -> Vec<(String, NumericHintKind)> {
    let tokens = split_match_tokens(value);
    let mut hints = Vec::new();
    let mut seen = BTreeSet::new();

    for (index, token) in tokens.iter().enumerate() {
        if !token.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let previous = index
            .checked_sub(1)
            .map(|offset| normalize_match_token(&tokens[offset]));
        let kind = match previous.as_deref() {
            Some("A") | Some("ADC") | Some("AN") => NumericHintKind::Analog,
            Some("D") | Some("GPIO") | Some("IO") | Some("PIN") => NumericHintKind::Digital,
            _ => NumericHintKind::Unknown,
        };
        if seen.insert((token.clone(), kind)) {
            hints.push((token.clone(), kind));
        }
    }

    hints
}

fn collect_number_hints(signal: &str, candidates: &[String]) -> Vec<(String, NumericHintKind)> {
    let mut hints = extract_number_hints(signal);
    let mut seen = hints.iter().cloned().collect::<BTreeSet<_>>();
    for candidate in candidates {
        for hint in extract_number_hints(candidate) {
            if seen.insert(hint.clone()) {
                hints.push(hint);
            }
        }
    }
    hints
}

fn canonical_signal_name(value: &str) -> String {
    let upper = value.to_ascii_uppercase().replace("{SLASH}", "_");
    let mut normalized = String::with_capacity(upper.len());
    let mut last_was_sep = false;
    for character in upper.chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
            last_was_sep = false;
        } else if !last_was_sep {
            normalized.push('_');
            last_was_sep = true;
        }
    }
    normalized.trim_matches('_').to_string()
}

fn infer_net_binding_mode(name: &str) -> BindingMode {
    let upper = name.trim_start_matches('/').to_ascii_uppercase();
    if upper == "GND" || upper.contains("GROUND") {
        return BindingMode::Power;
    }
    if upper.starts_with('+')
        || upper.contains("VCC")
        || upper.contains("VDD")
        || upper.contains("VIN")
        || upper.contains("24V")
        || upper.contains("12V")
        || upper.contains("5V")
        || upper.contains("3V3")
        || upper.contains("IOREF")
    {
        return BindingMode::Power;
    }
    if upper.starts_with('A') || upper.contains("ADC") || upper.contains("_RAW") {
        return BindingMode::Analog;
    }
    if upper.contains("SDA")
        || upper.contains("SCL")
        || upper.contains("MISO")
        || upper.contains("MOSI")
        || upper.contains("SCK")
        || upper.contains("CLK")
        || upper.contains("SPI")
        || upper.contains("I2C")
        || upper.contains("UART")
        || upper.contains("CAN")
        || upper.contains("IRQ")
        || upper.contains("INT")
        || upper.contains("CS")
        || upper.ends_with("_RX")
        || upper.ends_with("_TX")
    {
        return BindingMode::Bus;
    }
    BindingMode::Digital
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rust_board::board_from_kicad_pcb;

    use super::{
        auto_bind_host_board, controller_signal_suggestions, default_project_name,
        infer_binding_mode, inferred_host_board_from_source,
    };
    use crate::{BindingMode, HostBoard};

    fn example_pcb_path(file_name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../examples/pcbs")
            .join(file_name)
    }

    #[test]
    fn infers_host_board_from_common_source_names() {
        assert_eq!(
            inferred_host_board_from_source("/tmp/nano_pin_sweep.ino"),
            Some(HostBoard::NanoV3)
        );
        assert_eq!(
            inferred_host_board_from_source("/tmp/mega_pin_sweep.ino"),
            Some(HostBoard::Mega2560Rev3)
        );
        assert_eq!(
            inferred_host_board_from_source("/tmp/atmega328p_blink.hex"),
            Some(HostBoard::NanoV3)
        );
        assert_eq!(
            inferred_host_board_from_source("/tmp/atmega2560_blink.hex"),
            Some(HostBoard::Mega2560Rev3)
        );
    }

    #[test]
    fn default_project_name_prefers_source_then_pcb() {
        assert_eq!(
            default_project_name("/tmp/dewpoint.ino", "/tmp/board.kicad_pcb"),
            "dewpoint"
        );
        assert_eq!(
            default_project_name("", "/tmp/main-controller.board.avrsim.json"),
            "main-controller"
        );
    }

    #[test]
    fn infer_binding_mode_covers_common_signal_types() {
        assert_eq!(infer_binding_mode("D27"), BindingMode::Digital);
        assert_eq!(infer_binding_mode("D44_PWM"), BindingMode::Analog);
        assert_eq!(infer_binding_mode("D50_MISO"), BindingMode::Bus);
        assert_eq!(infer_binding_mode("+5V"), BindingMode::Power);
    }

    #[test]
    fn controller_signal_suggestions_find_exact_mega_port_aliases() {
        let board = board_from_kicad_pcb(example_pcb_path(
            "mega_r3_sidecar_controller_rev_a.kicad_pcb",
        ))
        .expect("board");
        let available_nets = board.nets.into_iter().map(|net| net.name).collect();
        let suggestions =
            controller_signal_suggestions(HostBoard::Mega2560Rev3, "D27", &available_nets);
        assert_eq!(
            suggestions
                .first()
                .map(|suggestion| suggestion.net_name.as_str()),
            Some("/PA5")
        );
    }

    #[test]
    fn auto_bind_host_board_produces_useful_bindings_for_example_board() {
        let board = board_from_kicad_pcb(example_pcb_path(
            "mega_r3_sidecar_controller_rev_a.kicad_pcb",
        ))
        .expect("board");
        let available_nets = board.nets.into_iter().map(|net| net.name).collect();
        let bindings = auto_bind_host_board(HostBoard::Mega2560Rev3, &available_nets);
        assert!(bindings
            .iter()
            .any(|binding| binding.board_signal == "+5V" && binding.pcb_net == "+5V"));
        assert!(bindings
            .iter()
            .any(|binding| binding.board_signal == "D27" && binding.pcb_net == "/PA5"));
    }
}
