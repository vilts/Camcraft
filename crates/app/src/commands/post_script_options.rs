//! Per-post script option commands (#258): `SetPostScriptOptions`,
//! `SetSetupPostScriptOptions`, and composite `SetPostSettings`.
//!
//! Provenance: our own design; setup furniture (singleton and `SetupDef`
//! components), validate-early against the target script's `declaredOptions`,
//! reversible by snapshot restoration.

use std::collections::BTreeMap;

use camcraft_post::post::options::{self, OptionValue};
use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::World;

use crate::commands::{
    Command, CommandError, SetPostOptions, SetProgramInfo, SetSetupPostOptions,
};
use crate::model;
use crate::posting;
use crate::schema::{PostOptions, PostRef, PostScriptOptions, ProgramInfo, SetupDef};
use crate::setup_post_options;

fn post_ref_from_key(key: &str) -> Option<PostRef> {
    if let Some(name) = key.strip_prefix("builtin:") {
        return Some(PostRef::Named(name.to_owned()));
    }
    if let Some(path) = key.strip_prefix("file:") {
        return Some(PostRef::File(path.to_owned()));
    }
    None
}

fn check_finite(map: &BTreeMap<String, OptionValue>) -> Result<(), String> {
    for (k, v) in map {
        if !v.is_finite() {
            return Err(format!("post option '{k}' must be a finite number"));
        }
    }
    Ok(())
}

/// Validate `overrides` against `post`'s `declaredOptions`.
pub(crate) fn validate_for_post(
    post: &PostRef,
    overrides: &BTreeMap<String, OptionValue>,
) -> Result<(), String> {
    check_finite(overrides)?;
    let (_, src) = posting::script_of(post)?;
    let schema = options::declared_options(&src).map_err(|e| e.to_string())?;
    options::validate_overrides(&schema, overrides)
}

/// Validate every post override map that resolves cleanly (#258).
pub(crate) fn validate_post_map(map: &PostScriptOptions) -> Result<(), String> {
    for (key, ov) in &map.by_post {
        check_finite(ov)?;
        if let Some(post) = post_ref_from_key(key) {
            if let PostRef::Named(ref name) = post {
                if !posting::BUILTINS.iter().any(|(n, _)| n == name) {
                    return Err(format!("built-in post '{name}' is not in the registry"));
                }
                validate_for_post(&post, ov)?;
            } else if posting::script_of(&post).is_ok() {
                validate_for_post(&post, ov)?;
            }
        }
    }
    Ok(())
}

/// Replace or update document-level per-post script overrides (#258).
#[derive(Debug)]
pub struct SetPostScriptOptions {
    prev: PostScriptOptions,
    next: PostScriptOptions,
}

impl SetPostScriptOptions {
    /// Replace the document's `PostScriptOptions` singleton with `next`.
    pub fn new(world: &World, mut next: PostScriptOptions) -> Result<Self, CommandError> {
        next.normalize();
        validate_post_map(&next).map_err(CommandError::Invalid)?;
        let prev = setup_post_options::script_document(world);
        if prev == next {
            return Err(CommandError::Invalid(
                "those post script options are already set".to_owned(),
            ));
        }
        Ok(Self { prev, next })
    }

    /// Replace the document's overrides for `post` with `overrides`.
    pub fn for_post(
        world: &World,
        post: &PostRef,
        overrides: BTreeMap<String, OptionValue>,
    ) -> Result<Self, CommandError> {
        validate_for_post(post, &overrides).map_err(CommandError::Invalid)?;
        let mut next = setup_post_options::script_document(world);
        next.set_for_post(&setup_post_options::post_key(post), overrides);
        Self::new(world, next)
    }

    /// Merge a partial patch into the document's overrides for `post` (`None` resets one key).
    pub fn merge_for_post(
        world: &World,
        post: &PostRef,
        patch: BTreeMap<String, Option<OptionValue>>,
    ) -> Result<Self, CommandError> {
        let key = setup_post_options::post_key(post);
        let mut current = setup_post_options::script_document(world)
            .for_post(&key)
            .clone();
        for (k, val) in patch {
            match val {
                Some(v) => {
                    current.insert(k, v);
                }
                None => {
                    current.remove(&k);
                }
            }
        }
        Self::for_post(world, post, current)
    }
}

impl Command for SetPostScriptOptions {
    fn apply(&self, world: &World) {
        setup_post_options::script_document_set(world, self.next.clone());
    }

    fn invert(&self, world: &World) {
        setup_post_options::script_document_set(world, self.prev.clone());
    }
}

/// Set or clear a setup's own per-post script overrides (`None` follows document) (#258).
#[derive(Debug)]
pub struct SetSetupPostScriptOptions {
    setup: String,
    prev: Option<PostScriptOptions>,
    next: Option<PostScriptOptions>,
}

impl SetSetupPostScriptOptions {
    /// Set `setup`'s `post_properties` (`None` follows the document).
    pub fn for_setup(
        world: &World,
        setup: &str,
        mut next: Option<PostScriptOptions>,
    ) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, setup)
            .ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.post_properties.clone())
            .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
        if let Some(ref mut map) = next {
            map.normalize();
            validate_post_map(map).map_err(CommandError::Invalid)?;
        }
        if prev == next {
            return Err(CommandError::Invalid(match &next {
                Some(_) => format!("setup '{setup}' already has those post script options"),
                None => format!("setup '{setup}' already follows the document's post script options"),
            }));
        }
        Ok(Self {
            setup: setup.to_owned(),
            prev,
            next,
        })
    }

    /// Replace one post's overrides on `setup` (copying document overrides for other posts if previously following).
    pub fn for_setup_post(
        world: &World,
        setup: &str,
        post: &PostRef,
        overrides: BTreeMap<String, OptionValue>,
    ) -> Result<Self, CommandError> {
        validate_for_post(post, &overrides).map_err(CommandError::Invalid)?;
        let mut next = setup_post_options::script_of(world, Some(setup));
        next.set_for_post(&setup_post_options::post_key(post), overrides);
        Self::for_setup(world, setup, Some(next))
    }
}

fn write_setup_script_options(world: &World, setup: &str, value: Option<PostScriptOptions>) {
    let Some(entity) = model::find_by_name(world, setup) else {
        return;
    };
    let view = entity.entity_view(world);
    view.get::<&mut SetupDef>(|def| {
        def.post_properties = value;
    });
}

impl Command for SetSetupPostScriptOptions {
    fn apply(&self, world: &World) {
        write_setup_script_options(world, &self.setup, self.next.clone());
    }

    fn invert(&self, world: &World) {
        write_setup_script_options(world, &self.setup, self.prev.clone());
    }
}

/// Fields submitted together from the Post dialog or `set_post_options` (#258).
#[derive(Debug, Clone, Default)]
pub struct PostSettingsEdit {
    pub setup: Option<String>,
    pub program_info: Option<ProgramInfo>,
    pub post_options: Option<PostOptions>,
    pub follow_document: bool,
    pub script_post: Option<PostRef>,
    pub script_overrides: Option<BTreeMap<String, OptionValue>>,
    pub script_replace: Option<PostScriptOptions>,
}

/// Composite single-undo command combining program info, built-in post options, and script options (#258).
#[derive(Debug)]
pub struct SetPostSettings {
    program_info: Option<SetProgramInfo>,
    doc_options: Option<SetPostOptions>,
    setup_options: Option<SetSetupPostOptions>,
    doc_script: Option<SetPostScriptOptions>,
    setup_script: Option<SetSetupPostScriptOptions>,
}

impl SetPostSettings {
    /// Validate all provided fields first; emit only the sub-commands that actually change state.
    pub fn new(world: &World, edit: PostSettingsEdit) -> Result<Self, CommandError> {
        let program_info = match edit.program_info {
            Some(info) => match SetProgramInfo::new(world, info) {
                Ok(cmd) => Some(cmd),
                Err(e) if e.to_string().contains("already set") => None,
                Err(e) => return Err(e),
            },
            None => None,
        };

        let mut doc_options = None;
        let mut setup_options = None;
        let mut doc_script = None;
        let mut setup_script = None;

        match edit.setup.as_deref() {
            None => {
                if let Some(opts) = edit.post_options
                    && opts != setup_post_options::document(world)
                {
                    doc_options = Some(SetPostOptions::new(world, opts)?);
                }
                if let Some(map) = edit.script_replace {
                    if let Ok(cmd) = SetPostScriptOptions::new(world, map) {
                        doc_script = Some(cmd);
                    }
                } else if let (Some(post), Some(ov)) = (&edit.script_post, edit.script_overrides) {
                    validate_for_post(post, &ov).map_err(CommandError::Invalid)?;
                    let cur = setup_post_options::script_document(world)
                        .for_post(&setup_post_options::post_key(post))
                        .clone();
                    if cur != ov {
                        doc_script = Some(SetPostScriptOptions::for_post(world, post, ov)?);
                    }
                }
            }
            Some(name) => {
                if edit.follow_document {
                    if setup_post_options::own(world, Some(name)).is_some() {
                        setup_options = Some(SetSetupPostOptions::new(world, name, None)?);
                    }
                    if setup_post_options::script_own(world, Some(name)).is_some() {
                        setup_script = Some(SetSetupPostScriptOptions::for_setup(world, name, None)?);
                    }
                } else {
                    if let Some(opts) = edit.post_options
                        && setup_post_options::own(world, Some(name)) != Some(opts)
                    {
                        setup_options = Some(SetSetupPostOptions::new(world, name, Some(opts))?);
                    }
                    if let Some(map) = edit.script_replace {
                        if setup_post_options::script_own(world, Some(name)).as_ref() != Some(&map) {
                            setup_script =
                                Some(SetSetupPostScriptOptions::for_setup(world, name, Some(map))?);
                        }
                    } else if let (Some(post), Some(ov)) =
                        (&edit.script_post, edit.script_overrides)
                    {
                        validate_for_post(post, &ov).map_err(CommandError::Invalid)?;
                        let mut next = setup_post_options::script_of(world, Some(name));
                        next.set_for_post(&setup_post_options::post_key(post), ov);
                        next.normalize();
                        if setup_post_options::script_own(world, Some(name)).as_ref()
                            != Some(&next)
                        {
                            setup_script = Some(SetSetupPostScriptOptions::for_setup(
                                world,
                                name,
                                Some(next),
                            )?);
                        }
                    }
                }
            }
        }

        Ok(Self {
            program_info,
            doc_options,
            setup_options,
            doc_script,
            setup_script,
        })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.program_info.is_none()
            && self.doc_options.is_none()
            && self.setup_options.is_none()
            && self.doc_script.is_none()
            && self.setup_script.is_none()
    }
}

impl Command for SetPostSettings {
    fn apply(&self, world: &World) {
        if let Some(c) = &self.program_info {
            c.apply(world);
        }
        if let Some(c) = &self.doc_options {
            c.apply(world);
        }
        if let Some(c) = &self.setup_options {
            c.apply(world);
        }
        if let Some(c) = &self.doc_script {
            c.apply(world);
        }
        if let Some(c) = &self.setup_script {
            c.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        if let Some(c) = &self.setup_script {
            c.invert(world);
        }
        if let Some(c) = &self.doc_script {
            c.invert(world);
        }
        if let Some(c) = &self.setup_options {
            c.invert(world);
        }
        if let Some(c) = &self.doc_options {
            c.invert(world);
        }
        if let Some(c) = &self.program_info {
            c.invert(world);
        }
    }
}
