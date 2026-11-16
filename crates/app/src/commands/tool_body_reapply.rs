//! Reapply an editor assembly, placing every piece and its CSYS together.
use super::*;
use crate::tool_body_assembly::{self, BodyPiece};
use std::sync::Arc;

impl SetToolBody {
    /// Place a spindle draft while retaining its modular records.
    pub fn with_pieces(
        world: &World,
        name: &str,
        spec: ToolBodySpec,
        pieces: Vec<BodyPiece>,
    ) -> Result<Self, CommandError> {
        let axis = spec.axis;
        let offset = spec.offset;
        let mut command = Self::new(world, name, Some(spec))?;
        command.place_pieces(pieces, |p| {
            Ok(BodyPiece {
                source: p.source,
                facets: Arc::from(tool_body::place(&p.facets, axis, offset)?),
                csys: tool_body_assembly::transform_csys_spindle(&p.csys, axis, offset)?,
            })
        })?;
        Ok(command)
    }

    /// Place a turning draft while retaining its modular records.
    pub fn lathe_with_pieces(
        world: &World,
        name: &str,
        spec: LatheBodySpec,
        pieces: Vec<BodyPiece>,
    ) -> Result<Self, CommandError> {
        let place = spec.place;
        let mut command = Self::lathe(world, name, Some(spec))?;
        command.place_pieces(pieces, |p| {
            Ok(BodyPiece {
                source: p.source,
                facets: Arc::from(crate::tool_body_lathe::place(&p.facets, &place)?),
                csys: tool_body_assembly::transform_csys_lathe(&p.csys, &place)?,
            })
        })?;
        Ok(command)
    }

    fn place_pieces(
        &mut self,
        pieces: Vec<BodyPiece>,
        place: impl Fn(BodyPiece) -> Result<BodyPiece, String>,
    ) -> Result<(), CommandError> {
        let placed = pieces
            .into_iter()
            .map(place)
            .collect::<Result<Vec<_>, _>>()
            .map_err(CommandError::Invalid)?;
        if let Some(body) = &mut self.next {
            if !placed
                .iter()
                .flat_map(|p| p.facets.iter())
                .eq(body.facets.iter())
            {
                return Err(CommandError::Invalid(
                    "assembly pieces differ from the combined body".into(),
                ));
            }
            body.pieces = placed;
        }
        Ok(())
    }
}
