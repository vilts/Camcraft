//! The default machine file (`assets/machines/3axis-vmc.mch`) read as the
//! post's machine for a mount with none, or refused by the file's name
//! (#2319), and its stated limits judged for such a mount (#2321).
//! Provenance: our own machine-file format (docs/MACHINE-ONBOARDING.md).

use crate::machine::{AxisLimits, Machine, MachineCheck};
use crate::model;
use flecs_ecs::prelude::{Entity, World};
use serde_json::Value;
use std::sync::OnceLock;

use super::files::{default_mch_text, DEFAULT_MCH_FILE};

/// The default machine: a plain 3-axis VMC with flood coolant. For the
/// readers that only time or show a machine (the sim's traverse clock):
/// a missing or malformed file reads as `Value::Null` here. The post
/// never reads this - it reads [`default_machine_checked`], which refuses
/// by the file's name (#2319).
pub fn default_machine() -> Value {
    default_machine_checked().unwrap_or(Value::Null)
}

/// The default machine, or the refusal that names its file (#2319). A
/// good file is parsed once (#2282). A refusal is not kept: the next post
/// reads the file again, so a repaired file posts without a restart.
pub fn default_machine_checked() -> Result<Value, String> {
    static KEPT: KeptMachine = KeptMachine::new();
    KEPT.get_or_read(default_mch_text).inspect_err(|why| tracing::error!("{why}"))
}

/// The cell behind [`default_machine_checked`]: it keeps only a machine
/// that passed [`parse_default_machine`]. A read that fails, or a text that
/// is refused, is not kept - the next call reads again. The reader is an
/// argument so a test can give a sequence of files (#2319).
pub struct KeptMachine(OnceLock<Value>);

impl KeptMachine {
    #[must_use]
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }

    /// The kept machine, or a new read through `read` and the check.
    pub fn get_or_read(&self, read: impl FnOnce() -> Result<String, String>) -> Result<Value, String> {
        if let Some(v) = self.0.get() {
            return Ok(v.clone());
        }
        let text = read();
        let v = parse_default_machine(text.as_deref().map_err(Clone::clone))?;
        Ok(self.0.get_or_init(|| v).clone())
    }
}

impl Default for KeptMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// The default machine file's text read as the post's machine, or a
/// plain-English refusal that names the file (#2319). Refused: a file that
/// does not read, is not JSON, is not a JSON object, or is an object that
/// is not a machine - no `name` (the program header) or no
/// `kinematics.static` object (the limits and the coolant words). Such a
/// machine would post with no machine name and no coolant words.
pub fn parse_default_machine(text: Result<&str, String>) -> Result<Value, String> {
    let file = DEFAULT_MCH_FILE.0;
    let advice = "mount a machine on the setup, or reinstall the file";
    let text = text.map_err(|why| format!("the default machine file {file} cannot be read ({why}) - {advice}"))?;
    let v = serde_json::from_str::<Value>(text)
        .map_err(|e| format!("the default machine file {file} is malformed ({e}) - {advice}"))?;
    if !v.is_object() {
        return Err(format!("the default machine file {file} is malformed (not a machine object) - {advice}"));
    }
    let named = v.get("name").and_then(Value::as_str).is_some_and(|n| !n.trim().is_empty());
    if !named {
        return Err(format!("the default machine file {file} is malformed (it has no machine name) - {advice}"));
    }
    if !v.pointer("/kinematics/static").is_some_and(Value::is_object) {
        return Err(format!("the default machine file {file} is malformed (it has no kinematics.static section) - {advice}"));
    }
    Ok(v)
}

/// The limits the default machine's text states (#2321), read as the
/// checker's [`Machine`]: its name, the spindle range and each linear
/// axis's feed and rapid rates. The text is the post grammar
/// (`kinematics.static`), not the `.mch` file form, so the `.mch` reader
/// cannot read it. Zero, negative or absent numbers are unstated - the
/// unstated-limit law - and an unstated limit checks nothing.
#[must_use]
pub fn default_limits(v: &Value) -> Machine {
    let rate = |p: Option<&Value>| p.and_then(Value::as_f64).filter(|x| x.is_finite() && *x > 0.0);
    let head = v.pointer("/kinematics/static/head/spindle");
    let axes = v.pointer("/kinematics/static/axes").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let axes = axes
        .iter()
        .filter_map(|a| {
            let name = a.get("letter").and_then(Value::as_str)?;
            ["X", "Y", "Z"].contains(&name).then(|| AxisLimits {
                name: name.to_owned(),
                feed: rate(a.pointer("/speeds/normal")),
                rapid: rate(a.pointer("/speeds/rapid")),
                ..Default::default()
            })
        })
        .collect();
    Machine {
        model: v.get("name").and_then(Value::as_str).unwrap_or_default().to_owned(),
        spindle_min: rate(head.and_then(|s| s.get("min"))),
        spindle_max: rate(head.and_then(|s| s.get("max"))),
        axes,
        ..Default::default()
    }
}

/// The limit clause for the mounts with no machine (#2321): their ops
/// post with the default machine file, so they are judged against the
/// limits that file states. A file that does not read refuses here by
/// its name (#2319). With nothing to post, the program's own refusal
/// answers, as for a mounted machine.
pub(super) fn default_machine_problems(world: &World) -> Vec<String> {
    let mut mounts = crate::setup_mount::program_mounts(world, false);
    if mounts.is_empty() {
        mounts.push(super::mounts::active(world));
    }
    let bare: Vec<String> = mounts.into_iter().filter(|m| m.machine.1.is_none()).flat_map(|m| m.setups).collect();
    let Ok(program) = super::program_ops(world) else { return Vec::new() };
    let ops: Vec<(String, Entity)> = program.into_iter().filter(|(_, e)| super::mounts::member(world, e, &bare)).collect();
    if ops.is_empty() {
        return Vec::new();
    }
    let v = match default_machine_checked() {
        Ok(v) => v,
        Err(why) => {
            // #2319: the refusal names the setups that post with it.
            let named: Vec<&str> = bare.iter().map(String::as_str).filter(|n| !n.is_empty()).collect();
            let who = if named.is_empty() { "the setup".to_owned() } else { named.join(", ") };
            return vec![format!("machine of {who}: {why}")];
        }
    };
    let m = default_limits(&v);
    let advice = "no machine is mounted, so the program is judged against the default machine; \
                  mount a machine on the setup, or change the operation";
    match default_limit_check(world, &m, &ops) {
        Ok(c) if !c.is_ok() => {
            let more = if c.more > 0 { format!(" (+{} more)", c.more) } else { String::new() };
            vec![format!(
                "default machine {}: {} limit problem(s): {}{more} - {advice}",
                m.model,
                c.problems.len(),
                c.problems.join("; ")
            )]
        }
        Ok(_) => Vec::new(),
        Err(err) => vec![format!("default machine: {err}")],
    }
}

/// The ops of a mount with no machine judged against the default
/// machine's stated limits (#2321): each op's spindle speed against the
/// spindle range, and each feed move against the slowest axis it drives.
/// The same two judgements a mounted machine's check makes, in the same
/// words. Only these: the default file states no stroke, no tool table
/// and no rotaries, so nothing else is claimed for it.
pub(super) fn default_limit_check(world: &World, m: &Machine, ops: &[(String, Entity)]) -> Result<MachineCheck, String> {
    let mut out = MachineCheck::default();
    let mut say = |s: String| {
        if out.problems.len() < REPORT_CAP {
            out.problems.push(s);
        } else {
            out.more += 1;
        }
    };
    for (name, e) in ops {
        let op = model::operation_of(world, e).ok_or_else(|| format!("{name}: lost its operation"))?;
        let tool = model::find_by_name(world, &op.tool)
            .and_then(|t| model::tool_of(world, &t))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let rpm = super::posted_rpm(&tool);
        if rpm > 0.0 {
            if let Some(hi) = m.spindle_max
                && rpm > hi
            {
                say(format!("{name}: tool {} runs {rpm:.0} rpm, above the machine's {hi:.0}", op.tool));
            }
            if let Some(lo) = m.spindle_min
                && rpm < lo
            {
                say(format!("{name}: tool {} runs {rpm:.0} rpm, below the machine's {lo:.0}", op.tool));
            }
        }
        let cache = model::toolpath_of(world, e).ok_or_else(|| format!("{name}: lost its toolpath"))?;
        for (i, mv) in cache.path.moves.iter().enumerate() {
            let Some(feed) = mv.feed else { continue };
            let d = [mv.to.x - mv.from.x, mv.to.y - mv.from.y, mv.to.z - mv.from.z];
            if let Some(ceil) = m.feed_ceiling(d)
                && feed > ceil
            {
                let axis = m.axes.iter().find(|a| a.feed == Some(ceil)).map_or("an axis", |a| a.name.as_str());
                say(format!("{name}: move {i} feeds {feed:.0} mm/min, {axis} tops out at {ceil:.0}"));
            }
        }
    }
    Ok(out)
}

/// The most problems one check lists; the rest are counted.
const REPORT_CAP: usize = 40;
