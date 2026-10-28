//! Per-case/route JSON (ADV-01/ADV-02/ADV-03) and `report --merge-counters`
//! (ADV-03). No JSON crate is added (T-08-SC: no new dependency); this
//! module hand-serializes and hand-parses the flat, single-level object
//! shape the JSONL schema needs. Safe Rust throughout (D-13): nothing here
//! touches `sys`.

use crate::cases::CaseSpec;
use crate::routes::{RouteRow, RunOutcome};
use std::collections::HashMap;
use std::io;
use std::path::Path;

/// One JSON object per case or route row. Case rows populate
/// `class`/`case`/`oracle`; route rows populate `route`/`topology`/`family`;
/// every row carries `mode` and `ptrace_scope`. `agent_result` and
/// `deny_counter_delta` are filled in by `merge` from the agent-mode run and
/// the wrapper's deny-counters.txt; at Phase 8 (non-agent, off-host) they
/// stay `None`.
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub class: Option<String>,
    pub case: Option<String>,
    pub oracle: Option<String>,
    pub baseline_ok: Option<bool>,
    pub valid: Option<bool>,
    pub reach: Option<String>,
    pub errno: Option<i64>,
    pub closable_by: Option<String>,
    pub route: Option<String>,
    pub topology: Option<String>,
    pub family: Option<String>,
    pub mode: String,
    pub agent_result: Option<String>,
    pub deny_counter_delta: Option<i64>,
    pub ptrace_scope: String,
}

impl Record {
    /// Build a case row from one `CaseOutcome` (cases.rs): `valid` mirrors
    /// `baseline_ok` directly, the D-17 rule that a case whose side effect
    /// does not occur in the non-agent run is invalid.
    pub fn from_case(spec: &CaseSpec, baseline_ok: bool, mode: &str, ptrace_scope: &str) -> Record {
        Record {
            class: Some(spec.class.to_string()),
            case: Some(spec.case.to_string()),
            oracle: Some(spec.oracle.to_string()),
            baseline_ok: Some(baseline_ok),
            valid: Some(baseline_ok),
            closable_by: Some(String::new()),
            mode: mode.to_string(),
            ptrace_scope: ptrace_scope.to_string(),
            ..Default::default()
        }
    }

    /// Build a route row from one `RouteRow` and its `RunOutcome`
    /// (routes.rs): a `Skipped` row still carries its skip reason in
    /// `reach`, never marked invalid or failed (ADV-02 empty case 29).
    /// `errno` is the failing attempt's raw OS error, when the row ran and
    /// failed (D5: "failures are recorded with errno"); `None` for a
    /// reachable or skipped row. `payload_claim` is D-15's control route
    /// (iii): it is marked `oracle="control"` here so `report_text`'s S5
    /// tally (and any later phase's) excludes it by construction rather
    /// than by a name check scattered at each call site.
    pub fn from_route(
        row: &RouteRow,
        outcome: &RunOutcome,
        mode: &str,
        ptrace_scope: &str,
        errno: Option<i64>,
    ) -> Record {
        let reach = match outcome {
            RunOutcome::Ran(r) => r.as_str().to_string(),
            RunOutcome::Skipped(reason) => (*reason).to_string(),
        };
        Record {
            route: Some(row.route.to_string()),
            topology: Some(row.topology.as_str().to_string()),
            family: Some(row.family.as_str().to_string()),
            reach: Some(reach),
            errno,
            oracle: is_control_route(row.route).then(|| "control".to_string()),
            closable_by: Some(String::new()),
            mode: mode.to_string(),
            ptrace_scope: ptrace_scope.to_string(),
            ..Default::default()
        }
    }

    /// The merge/join key (ADV-03 encoding 36, ordering 37): route if
    /// present, else case; paired with topology (empty string for rows with
    /// no topology concept, e.g. case rows).
    pub fn join_key(&self) -> (String, String) {
        let primary = self
            .route
            .clone()
            .or_else(|| self.case.clone())
            .unwrap_or_default();
        let topology = self.topology.clone().unwrap_or_default();
        (primary, topology)
    }

    pub fn to_json_line(&self) -> String {
        let parts = [
            field_str("class", &self.class),
            field_str("case", &self.case),
            field_str("oracle", &self.oracle),
            field_bool("baseline_ok", &self.baseline_ok),
            field_bool("valid", &self.valid),
            field_str("reach", &self.reach),
            field_i64("errno", &self.errno),
            field_str("closable_by", &self.closable_by),
            field_str("route", &self.route),
            field_str("topology", &self.topology),
            field_str("family", &self.family),
            format!("\"mode\":\"{}\"", json_escape(&self.mode)),
            field_str("agent_result", &self.agent_result),
            field_i64("deny_counter_delta", &self.deny_counter_delta),
            format!("\"ptrace_scope\":\"{}\"", json_escape(&self.ptrace_scope)),
        ];
        format!("{{{}}}", parts.join(","))
    }

    fn from_fields(fields: &[(String, JsonValue)]) -> Record {
        let mut r = Record::default();
        for (key, value) in fields {
            match key.as_str() {
                "class" => r.class = value.as_opt_str(),
                "case" => r.case = value.as_opt_str(),
                "oracle" => r.oracle = value.as_opt_str(),
                "baseline_ok" => r.baseline_ok = value.as_opt_bool(),
                "valid" => r.valid = value.as_opt_bool(),
                "reach" => r.reach = value.as_opt_str(),
                "errno" => r.errno = value.as_opt_i64(),
                "closable_by" => r.closable_by = value.as_opt_str(),
                "route" => r.route = value.as_opt_str(),
                "topology" => r.topology = value.as_opt_str(),
                "family" => r.family = value.as_opt_str(),
                "mode" => r.mode = value.as_opt_str().unwrap_or_default(),
                "agent_result" => r.agent_result = value.as_opt_str(),
                "deny_counter_delta" => r.deny_counter_delta = value.as_opt_i64(),
                "ptrace_scope" => r.ptrace_scope = value.as_opt_str().unwrap_or_default(),
                _ => {}
            }
        }
        r
    }
}

/// D-15 route (iii) is a control: it must be reachable only when the stub's
/// reply ignored the claimed id (`attempt_payload_claim`'s own check), and
/// it never counts toward the S5 reachable-route tally (D5).
fn is_control_route(route: &str) -> bool {
    route == "payload_claim"
}

fn field_str(key: &str, v: &Option<String>) -> String {
    match v {
        Some(s) => format!("\"{key}\":\"{}\"", json_escape(s)),
        None => format!("\"{key}\":null"),
    }
}

fn field_bool(key: &str, v: &Option<bool>) -> String {
    match v {
        Some(b) => format!("\"{key}\":{b}"),
        None => format!("\"{key}\":null"),
    }
}

fn field_i64(key: &str, v: &Option<i64>) -> String {
    match v {
        Some(n) => format!("\"{key}\":{n}"),
        None => format!("\"{key}\":null"),
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A parsed value from this module's minimal flat-object JSON reader. Only
/// the shapes `to_json_line` ever writes (string, number, bool, null; no
/// nesting, no arrays) need to round-trip.
enum JsonValue {
    Str(String),
    Num(i64),
    Bool(bool),
    Null,
}

impl JsonValue {
    fn as_opt_str(&self) -> Option<String> {
        match self {
            JsonValue::Str(s) => Some(s.clone()),
            _ => None,
        }
    }
    fn as_opt_bool(&self) -> Option<bool> {
        match self {
            JsonValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
    fn as_opt_i64(&self) -> Option<i64> {
        match self {
            JsonValue::Num(n) => Some(*n),
            _ => None,
        }
    }
}

fn unquote(s: &str) -> io::Result<String> {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        Ok(json_unescape(&s[1..s.len() - 1]))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected a quoted string: {s}"),
        ))
    }
}

fn parse_value(s: &str) -> io::Result<JsonValue> {
    let s = s.trim();
    match s {
        "null" => Ok(JsonValue::Null),
        "true" => Ok(JsonValue::Bool(true)),
        "false" => Ok(JsonValue::Bool(false)),
        _ if s.starts_with('"') => Ok(JsonValue::Str(unquote(s)?)),
        _ => s
            .parse::<i64>()
            .map(JsonValue::Num)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("bad value: {s}"))),
    }
}

/// Split a flat `{...}` JSON object into `(key, value)` pairs, splitting on
/// top-level commas only (commas inside quoted strings do not split a
/// field). No nesting is supported; this schema has none.
fn parse_flat_object(line: &str) -> io::Result<Vec<(String, JsonValue)>> {
    let line = line.trim();
    let inner = line
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "not a JSON object line"))?;

    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut prev_escape = false;
    for c in inner.chars() {
        if in_quotes {
            current.push(c);
            if prev_escape {
                prev_escape = false;
            } else if c == '\\' {
                prev_escape = true;
            } else if c == '"' {
                in_quotes = false;
            }
        } else if c == '"' {
            in_quotes = true;
            current.push(c);
        } else if c == ',' {
            pieces.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    if !current.trim().is_empty() {
        pieces.push(current);
    }

    let mut fields = Vec::new();
    for piece in pieces {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let colon = piece.find(':').ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("malformed field: {piece}"),
            )
        })?;
        let key = unquote(piece[..colon].trim())?;
        let value = parse_value(piece[colon + 1..].trim())?;
        fields.push((key, value));
    }
    Ok(fields)
}

pub fn write_jsonl(records: &[Record], path: &Path) -> io::Result<()> {
    let mut out = String::new();
    for r in records {
        out.push_str(&r.to_json_line());
        out.push('\n');
    }
    std::fs::write(path, out)
}

pub fn read_jsonl(path: &Path) -> io::Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)?;
    let mut records = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields = parse_flat_object(line)?;
        records.push(Record::from_fields(&fields));
    }
    Ok(records)
}

/// One `route=<r> topology=<t> hook=<h> before=<n> after=<n>` line from the
/// wrapper's `deny-counters.txt` (ADV-03 encoding 36). `hook` is parsed and
/// kept on the struct for the per-hook breakdown a later plan may report;
/// `merge` only sums `after - before` per `(route, topology)` today.
#[derive(Clone, Debug)]
pub struct DenyCounterLine {
    pub route: String,
    pub topology: String,
    #[allow(dead_code)]
    pub hook: String,
    pub before: i64,
    pub after: i64,
}

pub fn parse_deny_counters(path: &Path) -> io::Result<Vec<DenyCounterLine>> {
    let text = std::fs::read_to_string(path)?;
    let mut lines = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut route = None;
        let mut topology = None;
        let mut hook = None;
        let mut before = None;
        let mut after = None;
        for part in line.split_whitespace() {
            if let Some(v) = part.strip_prefix("route=") {
                route = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("topology=") {
                topology = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("hook=") {
                hook = Some(v.to_string());
            } else if let Some(v) = part.strip_prefix("before=") {
                before = v.parse::<i64>().ok();
            } else if let Some(v) = part.strip_prefix("after=") {
                after = v.parse::<i64>().ok();
            }
        }
        match (route, topology, hook, before, after) {
            (Some(route), Some(topology), Some(hook), Some(before), Some(after)) => {
                lines.push(DenyCounterLine {
                    route,
                    topology,
                    hook,
                    before,
                    after,
                });
            }
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("malformed deny-counters line: {line}"),
                ));
            }
        }
    }
    Ok(lines)
}

/// `report --merge-counters`: join the non-agent and agent runs on
/// `(route|case, topology)`, taking `baseline_ok`/`valid`/`reach` from the
/// non-agent run and `agent_result` from the agent run, and sum
/// `deny-counters.txt`'s `after - before` per hook into
/// `deny_counter_delta` by the same key (ADV-03 encoding 36, ordering 37).
pub fn merge(nonagent: &[Record], agent: &[Record], counters: &[DenyCounterLine]) -> Vec<Record> {
    let mut agent_by_key: HashMap<(String, String), &Record> = HashMap::new();
    for r in agent {
        agent_by_key.insert(r.join_key(), r);
    }

    let mut delta_by_key: HashMap<(String, String), i64> = HashMap::new();
    for c in counters {
        let key = (c.route.clone(), c.topology.clone());
        *delta_by_key.entry(key).or_insert(0) += c.after - c.before;
    }

    nonagent
        .iter()
        .map(|base| {
            let mut merged = base.clone();
            let key = base.join_key();
            if let Some(agent_rec) = agent_by_key.get(&key) {
                merged.agent_result = agent_rec.agent_result.clone();
            }
            if let Some(delta) = delta_by_key.get(&key) {
                merged.deny_counter_delta = Some(*delta);
            }
            merged
        })
        .collect()
}

/// The human-readable per-route report (08-04's S5 table copies this): one
/// line per route row, then the D-17 completeness boundary and the
/// read-then-act race named explicitly (RESEARCH), not hidden.
pub fn report_text(records: &[Record]) -> String {
    let mut lines = vec!["aok-adv S5 route report".to_string()];
    for r in records {
        if let Some(route) = &r.route {
            lines.push(format!(
                "route={route} topology={} reach={} ptrace_scope={} agent_result={} deny_counter_delta={}",
                r.topology.as_deref().unwrap_or("-"),
                r.reach.as_deref().unwrap_or("-"),
                r.ptrace_scope,
                r.agent_result.as_deref().unwrap_or("-"),
                r.deny_counter_delta
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }
    }

    let s5_total = records
        .iter()
        .filter(|r| r.route.is_some() && r.oracle.as_deref() != Some("control"))
        .count();
    let s5_reachable = records
        .iter()
        .filter(|r| {
            r.route.is_some()
                && r.oracle.as_deref() != Some("control")
                && r.reach.as_deref() == Some("reachable")
        })
        .count();
    lines.push(format!(
        "S5 routes: {s5_reachable} of {s5_total} reachable (payload_claim excluded: \
         control route, D-15/D5, never counted toward S5)"
    ));

    for r in records {
        if let Some(class) = &r.class {
            lines.push(format!(
                "class={class} case={} valid={} agent_result={}",
                r.case.as_deref().unwrap_or("-"),
                r.valid
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                r.agent_result.as_deref().unwrap_or("untested"),
            ));
        }
    }

    let case_count = records.iter().filter(|r| r.case.is_some()).count();
    let valid_count = records
        .iter()
        .filter(|r| r.case.is_some() && r.valid == Some(true))
        .count();
    lines.push(format!(
        "S1 skeleton: {valid_count} of {case_count} cases valid; 0 of about 8 is not evidence \
         of completeness (D-17): a case with no valid instance proves nothing about whether \
         that class is reachable."
    ));

    let case_untested = records
        .iter()
        .filter(|r| r.case.is_some() && r.agent_result.is_none())
        .count();
    lines.push(format!(
        "S1 agent column: {case_untested} of {case_count} cases untested (agent-mode S1 needs \
         enforcement hooks that arrive with ADV-04 and Phases 11-13; untested is not a failure \
         and is never fabricated as a denial or a success)."
    ));
    lines.push(
        "Read-then-act race: PIDFD_GET_INFO reads the peer's cgroup id at the moment of the \
         call; a peer that moves cgroups between that read and the caller acting on the reply \
         is reported as whatever it held at read time, not at act time (RESEARCH)."
            .to_string(),
    );
    lines.join("\n")
}

/// `report --merge-counters ... --out FILE.json`: the merged records plus
/// the human-readable text, as one JSON object.
pub fn write_report_json(records: &[Record], text: &str, path: &Path) -> io::Result<()> {
    let items: Vec<String> = records.iter().map(Record::to_json_line).collect();
    let body = format!(
        "{{\"report_text\":\"{}\",\"records\":[{}]}}",
        json_escape(text),
        items.join(",")
    );
    std::fs::write(path, body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::{Family, Reach, RunOutcome, Topology};

    fn sample_route_record(route: &str, topology: Topology, mode: &str) -> Record {
        let row = RouteRow {
            family: Family::Kill,
            route: match route {
                "kill" => "kill",
                "cgroup.kill" => "cgroup.kill",
                other => panic!("unexpected test route {other}"),
            },
            topology,
        };
        Record::from_route(&row, &RunOutcome::Ran(Reach::Reachable), mode, "1", None)
    }

    #[test]
    fn record_round_trips_through_json_line() {
        let r = sample_route_record("kill", Topology::Sibling, "nonagent");
        let line = r.to_json_line();
        let parsed = Record::from_fields(&parse_flat_object(&line).expect("parse"));
        assert_eq!(parsed.route, r.route);
        assert_eq!(parsed.topology, r.topology);
        assert_eq!(parsed.reach, r.reach);
        assert_eq!(parsed.mode, r.mode);
        assert_eq!(parsed.ptrace_scope, r.ptrace_scope);
    }

    #[test]
    fn merge_joins_on_route_and_topology_and_sums_deny_counters() {
        let nonagent = vec![sample_route_record("kill", Topology::Sibling, "nonagent")];
        let mut agent_record = sample_route_record("kill", Topology::Sibling, "agent");
        agent_record.agent_result = Some("denied".to_string());
        let agent = vec![agent_record];
        let counters = vec![
            DenyCounterLine {
                route: "kill".to_string(),
                topology: "sibling".to_string(),
                hook: "task_kill".to_string(),
                before: 3,
                after: 5,
            },
            DenyCounterLine {
                route: "kill".to_string(),
                topology: "sibling".to_string(),
                hook: "task_kill_retry".to_string(),
                before: 0,
                after: 1,
            },
        ];

        let merged = merge(&nonagent, &agent, &counters);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].agent_result, Some("denied".to_string()));
        assert_eq!(merged[0].deny_counter_delta, Some(3));
        assert_eq!(merged[0].reach, Some("reachable".to_string()));
    }

    #[test]
    fn merge_does_not_cross_topology_keys() {
        let nonagent = vec![
            sample_route_record("kill", Topology::Sibling, "nonagent"),
            sample_route_record("kill", Topology::Ancestor, "nonagent"),
        ];
        let mut sibling_agent = sample_route_record("kill", Topology::Sibling, "agent");
        sibling_agent.agent_result = Some("denied".to_string());
        let agent = vec![sibling_agent];
        let merged = merge(&nonagent, &agent, &[]);
        let sibling = merged
            .iter()
            .find(|r| r.topology == Some("sibling".to_string()))
            .unwrap();
        let ancestor = merged
            .iter()
            .find(|r| r.topology == Some("ancestor".to_string()))
            .unwrap();
        assert_eq!(sibling.agent_result, Some("denied".to_string()));
        assert_eq!(ancestor.agent_result, None);
    }

    #[test]
    fn report_text_names_completeness_boundary_and_race() {
        let text = report_text(&[sample_route_record("kill", Topology::Sibling, "nonagent")]);
        assert!(text.contains("not evidence of completeness"));
        assert!(text.contains("Read-then-act race"));
    }

    /// C.3: when the agent file carries route rows only (no case rows), the
    /// nonagent file's 8 S1 cases merge with `agent_result: None` ("untested"
    /// in `report_text`), never a fabricated denial or success, and never
    /// reported as a failure.
    #[test]
    fn merge_leaves_cases_untested_when_agent_file_has_no_case_rows() {
        let case_row = Record {
            class: Some("kill-impersonation".to_string()),
            case: Some("case-1".to_string()),
            oracle: Some("pidfd".to_string()),
            baseline_ok: Some(true),
            valid: Some(true),
            mode: "nonagent".to_string(),
            ptrace_scope: "1".to_string(),
            ..Default::default()
        };
        let nonagent = vec![case_row];
        let agent = vec![sample_route_record("kill", Topology::Sibling, "agent")];

        let merged = merge(&nonagent, &agent, &[]);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].agent_result, None,
            "a case with no agent-file counterpart must stay untested, not denied/succeeded"
        );

        let text = report_text(&merged);
        assert!(
            text.contains("agent_result=untested"),
            "report_text must render the untested case column: {text}"
        );
        assert!(
            text.contains("S1 agent column: 1 of 1 cases untested"),
            "report_text must summarize the untested count: {text}"
        );
    }

    /// D5: `payload_claim` is a control and must never count toward the S5
    /// reachable-route tally, even when it itself is reachable.
    #[test]
    fn payload_claim_is_excluded_from_the_s5_tally() {
        let row = RouteRow {
            family: Family::Impersonation,
            route: "payload_claim",
            topology: Topology::Sibling,
        };
        let claim_record = Record::from_route(
            &row,
            &RunOutcome::Ran(Reach::Reachable),
            "nonagent",
            "1",
            None,
        );
        assert_eq!(claim_record.oracle, Some("control".to_string()));

        let kill_record = sample_route_record("kill", Topology::Sibling, "nonagent");
        let text = report_text(&[claim_record, kill_record]);
        assert!(
            text.contains("S5 routes: 1 of 1 reachable"),
            "expected payload_claim excluded from the S5 tally, got: {text}"
        );
    }
}
