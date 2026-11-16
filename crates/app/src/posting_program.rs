//! The document's program identity (#258): the program number and the
//! program comment every post writes in its own dialect - an O-word on
//! the Fanuc family, Haas and Mazak, the program name on Heidenhain and
//! SINUMERIK, a header comment on PathPilot. This module owns the
//! singleton's read/write seam, the dialect-independent validation the
//! `SetProgramInfo` command runs at creation, and the `ProgramId` each
//! post call hands the script.
//!
//! Provenance: our own (issue #258). The number range is the shared
//! four-digit O0001-O9999 of the FANUC-style controls' published
//! operator's manuals (the narrowest O-word range among the shipped
//! dialects, so every numbering post can write any accepted number); the
//! refused comment characters are the ones that end or break a comment
//! or a block in a shipped dialect (RS-274 / ISO 6983 `( )`, the tape
//! mark `%`, the `;` comment of DIN controls, the TNC block end `*`).

use flecs_ecs::prelude::*;

use crate::post::ProgramId;
use crate::schema::ProgramInfo;

/// The highest program number a document may state: O9999.
pub const PROGRAM_NUMBER_MAX: u32 = 9999;
/// The longest program comment: the posts' comment width (#257).
pub const PROGRAM_COMMENT_MAX: usize = 40;
/// Characters no program comment may hold: each ends or breaks a
/// comment or a block in a shipped dialect.
pub const PROGRAM_COMMENT_REFUSED: &[char] = &['(', ')', '%', ';', '*'];

/// The document's program identity; the default when never set.
#[must_use]
pub fn program_info(world: &World) -> ProgramInfo {
    world.try_get::<&ProgramInfo>(|p| p.clone()).unwrap_or_default()
}

/// Write the singleton. For `SetProgramInfo` and the project reader only.
pub fn program_info_set(world: &World, next: ProgramInfo) {
    world.set(next);
}

/// The stated values cleaned to their stored shape: a comment is
/// trimmed and an empty one is no comment. Validation is separate
/// ([`check`]): a normalised value can still be refused.
#[must_use]
pub fn normalised(info: ProgramInfo) -> ProgramInfo {
    let comment = info.comment.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty());
    ProgramInfo { number: info.number, comment }
}

/// Every dialect-independent rule, refused by name: the number in
/// 1..=9999, the comment printable ASCII, at most 40 characters, and
/// free of the characters a shipped dialect cannot hold in a comment.
/// A dialect's own narrower rule (a SINUMERIK name's first letters)
/// refuses by name at post time.
pub fn check(info: &ProgramInfo) -> Result<(), String> {
    if let Some(n) = info.number
        && !(1..=PROGRAM_NUMBER_MAX).contains(&n)
    {
        return Err(format!("program number {n} is outside 1 to {PROGRAM_NUMBER_MAX} (O0001-O9999)"));
    }
    if let Some(c) = &info.comment {
        let len = c.chars().count();
        if len > PROGRAM_COMMENT_MAX {
            return Err(format!("program comment is {len} characters - at most {PROGRAM_COMMENT_MAX}"));
        }
        if let Some(bad) = c.chars().find(|ch| !(' '..='~').contains(ch)) {
            return Err(format!(
                "program comment holds {bad:?} - only printable ASCII (letters, digits, spaces, punctuation) posts"
            ));
        }
        if let Some(bad) = c.chars().find(|ch| PROGRAM_COMMENT_REFUSED.contains(ch)) {
            return Err(format!("program comment holds '{bad}', which ends a comment or a block on a control"));
        }
    }
    Ok(())
}

/// A typed program number: blank is none (O0001), a leading `O` is
/// the word's own letter and is dropped, anything else must be a whole
/// number - refused by name. The range is [`check`]'s.
pub fn parse_number(text: &str) -> Result<Option<u32>, String> {
    let t = text.trim();
    let digits = t.strip_prefix(['O', 'o']).unwrap_or(t);
    if digits.is_empty() {
        return if t.is_empty() { Ok(None) } else { Err("program number \"O\" has no digits".to_owned()) };
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("program number \"{t}\" is not a whole number"));
    }
    digits
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("program number \"{t}\" is outside 1 to {PROGRAM_NUMBER_MAX} (O0001-O9999)"))
}

/// What the script reads for program `index` (1-based) of a post: the
/// stated number (1 when none) counting up from it, one file per
/// operation (#111). A count past O9999 is refused by name, never
/// wrapped onto another program's number.
pub fn program_id(info: &ProgramInfo, index: usize) -> Result<ProgramId, String> {
    let base = info.number.unwrap_or(1);
    let step = u32::try_from(index.saturating_sub(1)).map_err(|_| "too many programs to number".to_owned())?;
    let number = base.checked_add(step).ok_or_else(|| "too many programs to number".to_owned())?;
    if info.number.is_some() && number > PROGRAM_NUMBER_MAX {
        return Err(format!(
            "program {index} would be number {number}, past O{PROGRAM_NUMBER_MAX} - state a lower program number"
        ));
    }
    Ok(ProgramId { number, numbered: info.number.is_some(), comment: info.comment.clone().unwrap_or_default() })
}

/// [`program_id`] of the document's own identity.
pub fn document_program_id(world: &World, index: usize) -> Result<ProgramId, String> {
    program_id(&program_info(world), index)
}
