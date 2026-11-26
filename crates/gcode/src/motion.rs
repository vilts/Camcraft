//! Motion execution (`G0`/`G1`/`G2`/`G3`, `G4`, `G28`/`G30`/`G53`, canned cycles).
//! Provenance: motion, reference return, and canned-cycle invocation semantics from RS-274/NGC (NISTIR 6556 §3.5) and ISO 6983.

use camcraft_cl::cl::MoveKind;
use crate::arc::resolve_arc;
use crate::block::{DecodedBlock, Group1G, LengthOp, NonModalG, PathControlG};
use crate::block_codes::check_unused_words;
use crate::domain::{check_dwell, check_len, check_point};
use crate::interp::Interpreter;
use crate::types::{DwellEvent, DwellUnit, GcodeError, MotionMode};

impl<'a> Interpreter<'a> {
    pub(crate) fn execute_motion(&mut self, b: &DecodedBlock) -> Result<(), GcodeError> {
        let line = b.line;
        let g64_consumed = b.path_control == Some(PathControlG::Continuous);
        let allow_h = b.length_op == Some(LengthOp::Apply);

        if b.non_modal == Some(NonModalG::Dwell) {
            return self.execute_dwell(b, allow_h);
        }

        if let Some(nm @ (NonModalG::HomeG28 | NonModalG::SecondaryG30)) = b.non_modal {
            check_unused_words(b, false, false, g64_consumed, g64_consumed, allow_h, false)?;
            if matches!(b.group1, Some(Group1G::ArcCw | Group1G::ArcCcw | Group1G::Cycle(_))) {
                return Err(GcodeError::InvalidModal {
                    line,
                    message: "G28/G30 cannot share a block with arcs or canned cycles".to_string(),
                });
            }
            if let Some(g1) = b.group1 {
                self.set_group1(g1);
            }
            return self.execute_reference_return(b, nm == NonModalG::HomeG28);
        }

        if b.g53 {
            check_unused_words(b, false, false, g64_consumed, g64_consumed, allow_h, false)?;
            if let Some(g1) = b.group1 {
                self.set_group1(g1);
            }
            return self.execute_g53(b);
        }

        if let Some(g1) = b.group1 {
            if g1 == Group1G::Cancel {
                check_unused_words(b, false, false, g64_consumed, g64_consumed, allow_h, false)?;
                if b.x.is_some() || b.y.is_some() || b.z.is_some() {
                    return Err(GcodeError::InvalidModal {
                        line,
                        message: "axis coordinates are invalid on a G80 cancel block".to_string(),
                    });
                }
                self.active_cycle = None;
                self.modal.motion = MotionMode::None;
                return Ok(());
            }
            self.set_group1(g1);
        }

        let has_coords = b.x.is_some()
            || b.y.is_some()
            || b.z.is_some()
            || b.i.is_some()
            || b.j.is_some()
            || b.k.is_some()
            || b.r.is_some()
            || (!g64_consumed && (b.p.is_some() || b.q.is_some()))
            || b.l.is_some();

        if !has_coords && b.group1.is_none() {
            return Ok(());
        }

        match self.modal.motion {
            MotionMode::None => {
                check_unused_words(b, false, false, g64_consumed, g64_consumed, allow_h, false)?;
                Err(GcodeError::InvalidModal {
                    line,
                    message: "axis motion or parameters with no active motion mode (G0/G1/G2/G3/cycle)"
                        .to_string(),
                })
            }
            MotionMode::Rapid => self.execute_linear(b, false, g64_consumed, allow_h),
            MotionMode::Linear => self.execute_linear(b, true, g64_consumed, allow_h),
            MotionMode::ArcCw | MotionMode::ArcCcw => {
                self.execute_arc(b, g64_consumed, allow_h)
            }
            MotionMode::Cycle(cycle_type) => {
                self.execute_cycle(b, cycle_type, allow_h)
            }
        }
    }

    fn execute_dwell(&mut self, b: &DecodedBlock, allow_h: bool) -> Result<(), GcodeError> {
        let line = b.line;
        check_unused_words(b, false, false, true, false, allow_h, false)?;
        if b.y.is_some() || b.z.is_some() {
            return Err(GcodeError::InvalidModal {
                line,
                message: "Y/Z words are invalid with G4 dwell".to_string(),
            });
        }
        if b.p.is_some() && b.x.is_some() {
            return Err(GcodeError::InvalidModal {
                line,
                message: "G4 dwell accepts P or X, not both".to_string(),
            });
        }
        let seconds = if let Some(p) = b.p {
            dwell_to_seconds(line, p, b.p_has_point, self.options.dialect.g4_dwell_unit)?
        } else if let Some(x) = b.x {
            dwell_to_seconds(line, x, b.p_has_point, DwellUnit::Seconds)?
        } else {
            return Err(GcodeError::Syntax {
                line,
                message: "G4 dwell requires a P or X duration".to_string(),
            });
        };
        self.dwells.push(DwellEvent {
            line,
            seconds,
            section: self.current.index,
            after_move: self.current.builder.moves_len().checked_sub(1),
        });
        Ok(())
    }

    fn execute_linear(
        &mut self,
        b: &DecodedBlock,
        is_feed: bool,
        g64_consumed: bool,
        allow_h: bool,
    ) -> Result<(), GcodeError> {
        let line = b.line;
        check_unused_words(b, false, false, g64_consumed, g64_consumed, allow_h, false)?;
        if b.x.is_none() && b.y.is_none() && b.z.is_none() {
            return Ok(());
        }
        let unit = self.require_unit(line)?;
        let (kind, feed) = if is_feed {
            (MoveKind::Feed, Some(self.require_feed(line, "G1")?))
        } else {
            (MoveKind::Rapid, None)
        };
        let to = self.target_point(b, unit)?;
        self.current.push_move(line, kind, to, feed);
        self.pos = to;
        Ok(())
    }

    fn execute_arc(
        &mut self,
        b: &DecodedBlock,
        g64_consumed: bool,
        allow_h: bool,
    ) -> Result<(), GcodeError> {
        let line = b.line;
        check_unused_words(b, true, true, g64_consumed, g64_consumed, allow_h, false)?;
        if b.group1.is_none()
            && b.x.is_none()
            && b.y.is_none()
            && b.z.is_none()
            && b.i.is_none()
            && b.j.is_none()
            && b.k.is_none()
            && b.r.is_none()
        {
            return Ok(());
        }
        let unit = self.require_unit(line)?;
        let ccw = self.modal.motion == MotionMode::ArcCcw;
        let feed = self.require_feed(line, if ccw { "G3" } else { "G2" })?;
        let to = self.target_point(b, unit)?;
        let ijk = [
            self.len_word(line, 'I', b.i, unit)?,
            self.len_word(line, 'J', b.j, unit)?,
            self.len_word(line, 'K', b.k, unit)?,
        ];
        let r_mm = self.len_word(line, 'R', b.r, unit)?;
        let resolved = resolve_arc(
            line,
            self.pos,
            to,
            self.modal.plane,
            ccw,
            self.modal.arc_center,
            ijk,
            r_mm,
            self.options.dialect.arc_radius_tolerance_mm,
        )?;
        // A resolved centre or mid point outside the domain is refused here: the
        // builder would replace an arc with a non-finite centre by a straight move.
        let c = resolved.center_words;
        check_len(line, "arc centre", c.x)?;
        check_len(line, "arc centre", c.y)?;
        if let Some(mid) = resolved.split_mid {
            check_point(line, "arc mid point", mid)?;
            self.current.push_arc(
                line,
                mid,
                feed,
                resolved.center_words,
                ccw,
                self.modal.plane,
            );
        }
        self.current.push_arc(
            line,
            to,
            feed,
            resolved.center_words,
            ccw,
            self.modal.plane,
        );
        self.pos = to;
        Ok(())
    }

    pub(crate) fn set_group1(&mut self, g1: Group1G) {
        let next = match g1 {
            Group1G::Rapid => MotionMode::Rapid,
            Group1G::Linear => MotionMode::Linear,
            Group1G::ArcCw => MotionMode::ArcCw,
            Group1G::ArcCcw => MotionMode::ArcCcw,
            Group1G::Cancel => MotionMode::None,
            Group1G::Cycle(ct) => MotionMode::Cycle(ct),
        };
        // Contiguous canned cycles keep their state, also across cycle codes
        // (#3657 review 6): only a non-cycle motion code ends it.
        if !matches!(next, MotionMode::Cycle(_)) {
            self.active_cycle = None;
        }
        self.modal.motion = next;
    }
}

/// Scale a dwell word to seconds by the dialect rule and refuse it outside the domain.
/// `has_point` is the written form of the `P` word (Haas reads `P10.` as seconds).
pub(crate) fn dwell_to_seconds(line: usize, val: f64, has_point: bool, unit: DwellUnit) -> Result<f64, GcodeError> {
    if !val.is_finite() || val < 0.0 {
        return Err(GcodeError::InvalidModal {
            line,
            message: format!("dwell duration ({val}) must be non-negative"),
        });
    }
    check_dwell(line, unit.to_seconds(val, has_point))
}
