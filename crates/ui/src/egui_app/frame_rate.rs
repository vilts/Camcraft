// The frame-rate reading of the Debug info panel and its log rule (#2262):
// one line when the rate moves to a new level, at most one line each 10 s
// when it keeps moving, nothing while the application is idle. Provenance:
// our own design; arithmetic on the frame stamps the shell already takes
// (the frame-cost rule, #2238), std::time only - no work grows with the
// model, and no repaint is requested.

use camcraft_app::clock::Instant;
use std::time::Duration;

/// A gap between two frames longer than this means the application sat
/// idle (it repaints on demand): the reading restarts and nothing logs.
pub const IDLE_GAP: Duration = Duration::from_millis(250);
/// The reading is the mean rate over one tick, so one slow frame is noise.
pub const TICK: Duration = Duration::from_millis(500);
/// At most one log line in this time (also the `gpu: slow frame` limit).
pub const LOG_EVERY: Duration = Duration::from_secs(10);
/// A new level needs a change of at least this share of the old level ...
pub const REL_STEP: f64 = 0.15;
/// ... and at least this many frames each second.
pub const ABS_STEP: f64 = 5.0;
/// A reading older than this shows as idle in the panel.
pub const STALE: Duration = Duration::from_secs(1);

/// Min, max, sum and time of the tick readings since the last log line.
#[derive(Default)]
struct Interval {
    min: f64,
    max: f64,
    sum: f64,
    n: u32,
    secs: f64,
}

impl Interval {
    fn push(&mut self, fps: f64, span: Duration) {
        if self.n == 0 {
            self.min = fps;
            self.max = fps;
        }
        self.min = self.min.min(fps);
        self.max = self.max.max(fps);
        self.sum += fps;
        self.n = self.n.saturating_add(1);
        self.secs += span.as_secs_f64();
    }
}

/// The frame-rate reading: fed one stamp per frame, answers with a log
/// line when the rule says so.
#[derive(Default)]
pub struct FrameRate {
    last: Option<Instant>,
    tick_start: Option<Instant>,
    frames: u32,
    level: Option<f64>,
    current: Option<(f64, Instant)>,
    last_log: Option<Instant>,
    pending: bool,
    /// The reading of the tick before, and how many ticks since the last
    /// line moved by a full step from it: one move is a new level.
    prev_tick: Option<f64>,
    moves: u32,
    iv: Interval,
}

impl FrameRate {
    /// Take the stamp of one drawn frame. `Some(line)` is a line to log.
    pub fn frame(&mut self, now: Instant) -> Option<String> {
        let prev = self.last.replace(now)?;
        if now.saturating_duration_since(prev) > IDLE_GAP {
            self.restart();
            return None;
        }
        self.frames = self.frames.saturating_add(1);
        let started = *self.tick_start.get_or_insert(prev);
        let span = now.saturating_duration_since(started);
        if span < TICK {
            return None;
        }
        let fps = f64::from(self.frames) / span.as_secs_f64().max(1e-6);
        self.tick_start = None;
        self.frames = 0;
        self.tick(fps, span, now)
    }

    /// Idle: forget the open tick and the open interval. Nothing logs.
    fn restart(&mut self) {
        self.tick_start = None;
        self.frames = 0;
        self.pending = false;
        self.prev_tick = None;
        self.moves = 0;
        self.iv = Interval::default();
    }

    fn tick(&mut self, fps: f64, span: Duration, now: Instant) -> Option<String> {
        self.current = Some((fps, now));
        self.iv.push(fps, span);
        if let Some(p) = self.prev_tick.replace(fps)
            && (fps - p).abs() >= (p * REL_STEP).max(ABS_STEP)
        {
            self.moves = self.moves.saturating_add(1);
        }
        let changed = self.level.is_none_or(|l| (fps - l).abs() >= (l * REL_STEP).max(ABS_STEP));
        let due = self.last_log.is_none_or(|t| now.saturating_duration_since(t) >= LOG_EVERY);
        let mut line = None;
        if changed {
            if due && !self.pending {
                line = Some(format!("debug info: frame rate {}", describe(fps)));
            } else {
                self.pending = true;
            }
        }
        if self.pending && due && self.moves <= 1 {
            // One step, then steady: that is a new level, not a jumping rate.
            line = Some(format!("debug info: frame rate {}", describe(fps)));
        } else if self.pending && due {
            let iv = &self.iv;
            let mean = iv.sum / f64::from(iv.n.max(1));
            line = Some(format!(
                "debug info: frame rate keeps changing - {:.0} to {:.0} fps, mean {:.0} fps over {:.0} s",
                iv.min, iv.max, mean, iv.secs
            ));
        }
        if line.is_some() {
            self.level = Some(fps);
            self.last_log = Some(now);
            self.pending = false;
            self.moves = 0;
            self.iv = Interval::default();
        }
        line
    }

    /// The panel's line: the latest reading, or idle when no frame was
    /// drawn lately.
    #[must_use]
    pub fn report_line(&self, now: Instant) -> String {
        match self.current {
            Some((fps, at)) if now.saturating_duration_since(at) <= STALE => {
                format!("Frame rate: {}", describe(fps))
            }
            _ => "Frame rate: idle (the window is not repainting)".to_owned(),
        }
    }
}

fn describe(fps: f64) -> String {
    format!("{fps:.0} fps ({:.1} ms per frame)", 1000.0 / fps.max(1e-6))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed steady frames of `ms` each from `from`, for `secs` seconds.
    fn run(fr: &mut FrameRate, from: Instant, ms: u64, secs: u64) -> (Vec<String>, Instant) {
        let mut out = Vec::new();
        let mut t = from;
        let end = from + Duration::from_secs(secs);
        while t < end {
            t += Duration::from_millis(ms);
            out.extend(fr.frame(t));
        }
        (out, t)
    }

    #[test]
    fn one_level_change_logs_one_line_and_noise_logs_none() {
        let t0 = Instant::now();
        let mut fr = FrameRate::default();
        let _ = fr.frame(t0);
        let (a, t) = run(&mut fr, t0, 16, 3);
        assert_eq!(a.len(), 1, "the first level: {a:?}");
        assert!(a[0].contains("62 fps"), "{a:?}");
        // 16 ms to 17 ms is 62 to 59 fps: inside the threshold.
        let (b, t) = run(&mut fr, t, 17, 3);
        assert!(b.is_empty(), "noise logs nothing: {b:?}");
        // A real change (to 30 fps) logs once, not once per tick, even
        // though ten seconds have not passed since the first line.
        let (c, t) = run(&mut fr, t, 33, 12);
        assert_eq!(c.len(), 1, "one line for one change: {c:?}");
        assert!(c[0].contains("30 fps"), "{c:?}");
        let (d, _) = run(&mut fr, t, 33, 20);
        assert!(d.is_empty(), "a stable level stays quiet: {d:?}");
    }

    #[test]
    fn a_rate_that_keeps_moving_logs_once_each_10_s_with_min_max_mean() {
        let t0 = Instant::now();
        let mut fr = FrameRate::default();
        let _ = fr.frame(t0);
        let (first, mut t) = run(&mut fr, t0, 16, 1);
        assert_eq!(first.len(), 1);
        let mut lines = Vec::new();
        for i in 0..60 {
            let ms = if i % 2 == 0 { 10 } else { 33 };
            let (l, t2) = run(&mut fr, t, ms, 1);
            lines.extend(l);
            t = t2;
        }
        // 60 s of jumping: at most 6 lines, and each says min, max, mean.
        assert!((4..=6).contains(&lines.len()), "{} lines: {lines:?}", lines.len());
        for l in &lines {
            assert!(l.contains("keeps changing") && l.contains(" to ") && l.contains("mean"), "{l}");
        }
    }

    #[test]
    fn an_idle_application_logs_nothing_and_reads_idle() {
        let t0 = Instant::now();
        let mut fr = FrameRate::default();
        let _ = fr.frame(t0);
        let (a, t) = run(&mut fr, t0, 16, 2);
        assert_eq!(a.len(), 1);
        assert!(fr.report_line(t).starts_with("Frame rate: 6"), "{}", fr.report_line(t));
        // One frame each 5 s is not a frame rate: no line, shown as idle.
        let mut t = t;
        for _ in 0..10 {
            t += Duration::from_secs(5);
            assert_eq!(fr.frame(t), None);
        }
        assert!(fr.report_line(t + Duration::from_secs(2)).contains("idle"));
    }
}
