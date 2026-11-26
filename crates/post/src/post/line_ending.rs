//! The program's line ending (#1882): the `lineEnding` manifest entry,
//! `"lf"` (the default) or `"crlf"` for a control that reads only
//! `\r\n` lines. Provenance: our own manifest entry; CR LF is the line
//! end of the RS-232 and DOS-hosted controls that ISO 6983 programs
//! are sent to (general practice).

use super::js::Script;
use super::PostError;

/// The line end the script asks for. The engine reads it when it
/// assembles the program, after `onClose`, so a script may set it in
/// `onOpen` from a declared option. Any word but `lf` or `crlf` is
/// refused by name.
pub fn read(s: &mut Script) -> Result<&'static str, PostError> {
    let stated = !super::js::is_true(&s.eval_src("typeof lineEnding === \"undefined\"")?);
    match s.string_global("lineEnding")?.as_deref() {
        None if !stated => Ok("\n"),
        Some("lf") => Ok("\n"),
        Some("crlf") => Ok("\r\n"),
        Some(other) => Err(PostError::Refused(format!(
            "this post script states lineEnding {other:?} - the engine writes \"lf\" or \"crlf\" only"
        ))),
        None => Err(PostError::Refused(
            "this post script states a lineEnding that is not a word - the engine writes \"lf\" or \"crlf\" only".into(),
        )),
    }
}
