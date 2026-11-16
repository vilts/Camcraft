//! SetProgramInfo: the document's program number and program comment
//! (#258) - what every post writes in place of the old fixed O0001 and
//! machine-name header, each in its own dialect.
//!
//! Setup furniture like `SetPostOptions`: no geometry changes, the
//! command writes only the `ProgramInfo` singleton and its inverse
//! writes the previous value back. Validated at creation (the number's
//! range, the comment's characters and length - refused by name); a
//! dialect's own narrower rule refuses by name at post time.
//! Provenance: our own (issue #258); the rules' sources are in
//! `posting_program.rs`.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::posting;
use crate::schema::ProgramInfo;

#[derive(Debug)]
pub struct SetProgramInfo {
    prev: ProgramInfo,
    next: ProgramInfo,
}

impl SetProgramInfo {
    /// State the program's number and comment (`None` for either posts
    /// that dialect's default). The comment is trimmed, an empty one is
    /// none; an invalid value, or the current one again (an empty undo
    /// step), is refused.
    pub fn new(world: &World, next: ProgramInfo) -> Result<Self, CommandError> {
        let next = posting::normalised_program_info(next);
        posting::check_program_info(&next).map_err(CommandError::Invalid)?;
        let prev = posting::program_info(world);
        if prev == next {
            return Err(CommandError::Invalid("that program number and comment are already set".to_owned()));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SetProgramInfo {
    fn apply(&self, world: &World) {
        posting::program_info_set(world, self.next.clone());
    }

    fn invert(&self, world: &World) {
        posting::program_info_set(world, self.prev.clone());
    }
}
