// Frame-time budget for the windowed shell (#74): names the phase of a
// frame that blew the budget, so a stall the reporter sees (and we cannot
// reproduce headlessly) shows up in their log. Provenance: our own design;
// std::time only.

use camcraft_app::clock::Instant;
use std::time::Duration;

/// A frame slower than this is reported (10 fps; vsync frames are 7-17 ms).
pub const BUDGET: Duration = Duration::from_millis(100);
/// After one report, stay quiet this long so a stuck loop cannot flood.
pub const QUIET: Duration = Duration::from_secs(5);

/// Per-frame phase stopwatch plus the rate-limited verdict.
pub struct FrameWatch {
    start: Instant,
    last: Instant,
    phases: Vec<(&'static str, Duration)>,
    last_report: Option<Instant>,
    slow_frames: u64,
}

impl FrameWatch {
    pub fn new(now: Instant) -> Self {
        FrameWatch { start: now, last: now, phases: Vec::with_capacity(8), last_report: None, slow_frames: 0 }
    }

    /// Open a frame.
    pub fn begin(&mut self, now: Instant) {
        self.start = now;
        self.last = now;
        self.phases.clear();
    }

    /// Take `waited` off the frame (#866): time the UI thread spent in a
    /// modal dialog is the user choosing a file, not the app working, so
    /// it never reads as a slow phase. Call before the `mark` closing the
    /// phase it happened in (every boundary, since panels open dialogs in
    /// the "ui" phase). Clamped so neither stamp moves past `now`.
    pub fn excuse(&mut self, waited: Duration, now: Instant) {
        let shift = waited.min(now.saturating_duration_since(self.last));
        if let (Some(start), Some(last)) = (self.start.checked_add(shift), self.last.checked_add(shift)) {
            self.start = start;
            self.last = last;
        }
    }

    /// Close the phase that ran since the previous mark.
    pub fn mark(&mut self, phase: &'static str, now: Instant) {
        self.phases.push((phase, now.saturating_duration_since(self.last)));
        self.last = now;
    }

    /// Close the frame: a warning line when it overran the budget and the
    /// quiet window has passed, naming the slowest phase and every phase.
    pub fn end(&mut self, now: Instant) -> Option<String> {
        let total = now.saturating_duration_since(self.start);
        if total <= BUDGET {
            return None;
        }
        self.slow_frames = self.slow_frames.saturating_add(1);
        if self.last_report.is_some_and(|t| now.saturating_duration_since(t) < QUIET) {
            return None;
        }
        self.last_report = Some(now);
        let (worst, wt) = self
            .phases
            .iter()
            .copied()
            .max_by_key(|(_, d)| *d)
            .unwrap_or(("unmarked", total));
        let all: Vec<String> = self.phases.iter().map(|(p, d)| format!("{p} {} ms", d.as_millis())).collect();
        Some(format!(
            "gpu: slow frame {} ms (budget {} ms, {} slow so far) - slowest phase \"{worst}\" {} ms [{}]",
            total.as_millis(),
            BUDGET.as_millis(),
            self.slow_frames,
            wt.as_millis(),
            all.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_fast_frame_says_nothing() {
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.mark("ui", t0 + ms(5));
        w.mark("present", t0 + ms(16));
        assert_eq!(w.end(t0 + ms(16)), None);
    }

    #[test]
    fn a_slow_frame_names_its_slowest_phase() {
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.mark("ui", t0 + ms(4));
        w.mark("acquire", t0 + ms(1504));
        w.mark("present", t0 + ms(1510));
        let line = w.end(t0 + ms(1510)).expect("over budget");
        assert!(line.contains("1510 ms"), "{line}");
        assert!(line.contains("slowest phase \"acquire\" 1500 ms"), "{line}");
        assert!(line.contains("ui 4 ms"), "{line}");
    }

    #[test]
    fn time_in_a_modal_dialog_is_not_a_slow_frame() {
        // #866: 35 s browsing for a STEP read as a 35 s "commands" freeze.
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.excuse(ms(35_000), t0 + ms(35_010));
        w.mark("commands", t0 + ms(35_010));
        w.mark("ui", t0 + ms(35_016));
        assert_eq!(w.end(t0 + ms(35_016)), None, "16 ms of work is a fast frame");
        // Work beyond the wait is still caught, and only the work is named.
        w.begin(t0);
        w.excuse(ms(35_000), t0 + ms(35_400));
        w.mark("commands", t0 + ms(35_400));
        let line = w.end(t0 + ms(35_400)).expect("400 ms of work is slow");
        assert!(line.contains("slow frame 400 ms") && line.contains("\"commands\" 400 ms"), "{line}");
    }

    #[test]
    fn a_wait_inside_the_ui_phase_is_excused_and_the_next_frame_is_clean() {
        // #866 review: panels open dialogs while drawing, after "commands".
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.mark("commands", t0 + ms(2));
        w.excuse(ms(30_000), t0 + ms(30_010)); // collected at the ui boundary
        w.mark("ui", t0 + ms(30_010));
        assert_eq!(w.end(t0 + ms(30_010)), None, "10 ms of work, 30 s of dialog");
        // The next frame is untouched: nothing carried over to excuse.
        let t1 = t0 + ms(30_100);
        w.begin(t1);
        w.excuse(Duration::ZERO, t1);
        w.mark("commands", t1 + ms(250));
        let line = w.end(t1 + ms(250)).expect("real slow work is still reported");
        assert!(line.contains("\"commands\" 250 ms"), "{line}");
        // Real slow work in the ui phase beside a wait is still named.
        let t2 = t1 + ms(60_000);
        w.begin(t2);
        w.excuse(ms(30_000), t2 + ms(30_300));
        w.mark("ui", t2 + ms(30_300));
        let line = w.end(t2 + ms(30_300)).expect("300 ms of ui work is slow");
        assert!(line.contains("\"ui\" 300 ms"), "{line}");
        // A wait longer than the phase never moves a stamp past now.
        let t3 = t2 + ms(1_000);
        w.begin(t3);
        w.excuse(ms(5_000), t3 + ms(10));
        w.mark("ui", t3 + ms(10));
        assert_eq!(w.end(t3 + ms(10)), None);
    }

    #[test]
    fn an_excuse_is_clamped_to_the_time_since_the_last_mark() {
        // #879: a 5 s dialog excused at +210 ms may only take the 10 ms
        // since the last mark off the frame. Without the clamp the stamps
        // jump past `now`, the frame reads as 0 ms and the 200 ms of
        // commands work is lost.
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        w.begin(t0);
        w.mark("commands", t0 + ms(200));
        w.excuse(ms(5_000), t0 + ms(210));
        let line = w.end(t0 + ms(210)).expect("200 ms of commands work is slow");
        assert!(line.contains("slow frame 200 ms"), "{line}");
        assert!(line.contains("\"commands\" 200 ms"), "{line}");
    }

    #[test]
    fn reports_are_rate_limited_but_counted() {
        let t0 = Instant::now();
        let mut w = FrameWatch::new(t0);
        let frame = |w: &mut FrameWatch, at: Instant| {
            w.begin(at);
            w.mark("ui", at + ms(200));
            w.end(at + ms(200))
        };
        assert!(frame(&mut w, t0).is_some());
        assert!(frame(&mut w, t0 + ms(300)).is_none(), "inside the quiet window");
        let again = frame(&mut w, t0 + QUIET + ms(400)).expect("quiet window over");
        assert!(again.contains("3 slow so far"), "{again}");
    }
}
