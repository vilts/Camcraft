//! SelectPost: set the document's post script (M3.3 A).
//!
//! Setup furniture, like `LoadMachine`: no geometry changes, the
//! command writes only the world singleton and its inverse writes the
//! previous value back. A file script is validated at command
//! construction - read, then compiled and run on an empty stream with
//! no machine - so a broken `.cps` is a refusal at the front door,
//! never a surprise at post time (validate early).

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::post::engine;
use crate::posting;
use crate::schema::PostRef;

#[derive(Debug)]
pub struct SelectPost {
    prev: PostRef,
    next: PostRef,
}

impl SelectPost {
    /// Choose a post script. A `File` reference must be readable and
    /// must run: an unreadable path or a script that throws at open
    /// names itself in the refusal (the Prime Directive has no
    /// "we'll find out when we post" clause). A `Named` reference must
    /// exist in the shipped registry (M3.3 B): the picker only offers
    /// real names, so a stranger is a corrupt project, and a corrupt
    /// project's post choice is refused the same way.
    pub fn new(world: &World, next: PostRef) -> Result<Self, CommandError> {
        let next = posting::agie::current_ref(next);
        validate(&next)?;
        let prev = posting::document_post_of(world);
        if prev == next {
            return Err(CommandError::Invalid(
                "that post is already selected".to_owned(),
            ));
        }
        Ok(Self { prev, next })
    }
}

/// The front-door check every post choice passes (`SelectPost`, and a
/// setup's own post, #343): a file must read and run, a registry name
/// must exist.
pub(crate) fn validate(next: &PostRef) -> Result<(), CommandError> {
    match next {
        PostRef::File(path) => {
            let text = crate::session_files::read_post(path)
                .map_err(|e| CommandError::Invalid(format!("post script {path}: {e}")))?;
            engine::run(&text, &serde_json::Value::Null, "")
                .map_err(|e| CommandError::Invalid(format!("post script {path} does not run: {e}")))?;
        }
        PostRef::Named(name) => {
            if !posting::BUILTINS.iter().any(|(n, _)| n == name) {
                return Err(CommandError::Invalid(format!(
                    "built-in post {name} is not in the registry"
                )));
            }
        }
        PostRef::Builtin => {}
    }
    Ok(())
}

impl Command for SelectPost {
    fn apply(&self, world: &World) {
        posting::post_set(world, &self.next);
    }

    fn invert(&self, world: &World) {
        posting::post_set(world, &self.prev);
    }
}
