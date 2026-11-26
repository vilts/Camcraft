//! The whole-window picture the Report door asks for (#1696).
//!
//! egui's screenshot contract, our native answer: the frame loop sees a
//! `ViewportCommand::Screenshot` in the root viewport output, renders the
//! just-tessellated frame a SECOND time into a private texture (egui
//! draws the same clipped primitives on request; the 3D view is an image
//! widget, so the re-render reads the textures it already uploaded),
//! copies the texture to a CPU buffer, and on the map callback answers
//! with `Event::Screenshot` for every waiting token. The window keeps
//! repainting while a shot is outstanding; nothing blocks a frame.
//!
//! Provenance: our own code, specced from egui's public capture API
//! (`ViewportCommand`/`Event` in egui's docs) and wgpu's documented
//! copy-and-map flow with `COPY_BYTES_PER_ROW_ALIGNMENT`. egui-wgpu's
//! optional capture feature was read and dropped: its read-back refuses
//! the sRGB surface format this shell configures, and it would drag a
//! channel this module's plain mpsc pair already is. SolidCraft (#1696's
//! pointer) was read for behaviour only; their brand licence forbids
//! taking look or code and we took neither. No vendor material consulted.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, UserData};
use egui_wgpu::wgpu;

use crate::state::State;

/// How long a request may sit before we call it failed. A healthy
/// read-back is milliseconds; eight seconds means the GPU path is wedged
/// and the user was already promised a picture, so say so once.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(8);

/// Bytes in one pixel of every format this module reads back.
const BYTES_PER_PIXEL: u32 = 4;

/// The channel message from the map callback (any thread) to the frame
/// loop: the picture, and the tokens that were waiting when it began.
struct ShotDone {
    image: Arc<ColorImage>,
    tokens: Vec<UserData>,
}

/// Byte order of one pixel, as indices into the copied row. The surface
/// format decides it; the copied bytes of an sRGB format are already
/// sRGB-encoded, which is what egui's `Color32` stores, so there is no
/// gamma step here.
fn swizzle_for(format: wgpu::TextureFormat) -> Option<[usize; 4]> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb => Some([0, 1, 2, 3]),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb => Some([2, 1, 0, 3]),
        _ => None,
    }
}

/// A capture in flight between `begin` and its map callback: the private
/// texture to render into, the staging buffer to copy it to, and the
/// tokens this shot answers. The frame loop encodes the second egui pass
/// and `encode_copy` on it, then hands it to `finish` after the submit.
pub(super) struct Shot {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    buffer: wgpu::Buffer,
    size: [u32; 2],
    padded: u32,
    swizzle: [usize; 4],
    tokens: Vec<UserData>,
    tx: Sender<ShotDone>,
    in_flight: Arc<AtomicBool>,
}

impl Shot {
    /// The render target of the second egui pass.
    pub(super) fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Queue the texture-to-buffer copy on the frame's encoder, honouring
    /// the 256-byte row alignment the buffer layout was built with.
    pub(super) fn encode_copy(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            self.tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded),
                    rows_per_image: Some(self.size[1]),
                },
            },
            wgpu::Extent3d {
                width: self.size[0],
                height: self.size[1],
                depth_or_array_layers: 1,
            },
        );
    }

    /// After `queue.submit`: read the copied bytes back. The callback
    /// runs when the map completes - on the thread that polls, or in a
    /// driver thread; everything it captures moves in, and the mapped
    /// view unmaps itself on drop. It can never panic: a short buffer is
    /// a logged failure, and a lost picture is a status line, never a
    /// crash (the Prime Directive).
    pub(super) fn finish(self) {
        let Shot {
            buffer,
            size,
            padded,
            swizzle,
            tokens,
            tx,
            in_flight,
            ..
        } = self;
        let [width, height] = size;
        // A handle clone for the callback: the map call borrows the
        // buffer, the closure outlives this function (Buffer is an
        // Arc-backed handle - the clone keeps the resource alive until
        // the callback has unmapped and dropped it).
        let read_back = buffer.clone();
        buffer.map_async(wgpu::MapMode::Read, 0..buffer.size(), move |result| {
            if let Err(err) = result {
                diag_warn!("report capture (#1696): read-back map failed: {err}");
                in_flight.store(false, Ordering::Release);
                return;
            }
            let done = (|| -> Option<ShotDone> {
                let mapped = match read_back.get_mapped_range(..) {
                    Ok(view) => view,
                    Err(err) => {
                        diag_warn!("report capture (#1696): mapped range failed: {err}");
                        return None;
                    }
                };
                let row_bytes = width.checked_mul(BYTES_PER_PIXEL)? as usize;
                let padded = padded as usize;
                let need = padded.checked_mul(height.saturating_sub(1) as usize)?.checked_add(row_bytes)?;
                if mapped.len() < need {
                    diag_warn!(
                        "report capture (#1696): buffer held {} bytes, needed {need}",
                        mapped.len()
                    );
                    return None;
                }
                let mut pixels = Vec::with_capacity((width as usize).saturating_mul(height as usize));
                for row in 0..height as usize {
                    let start = row.checked_mul(padded)?.checked_add(row_bytes)?;
                    for px in mapped[start - row_bytes..start]
                        .as_chunks::<{ BYTES_PER_PIXEL as usize }>()
                        .0
                    {
                        pixels.push(Color32::from_rgba_premultiplied(
                            px[swizzle[0]],
                            px[swizzle[1]],
                            px[swizzle[2]],
                            px[swizzle[3]],
                        ));
                    }
                }
                Some(ShotDone {
                    image: Arc::new(ColorImage::new([width as usize, height as usize], pixels)),
                    tokens,
                })
            })();
            if let Some(done) = done {
                // A receiver outliving the shell means shutdown; the
                // picture then belongs to nobody - drop it quietly.
                let _ = tx.send(done);
            }
            in_flight.store(false, Ordering::Release);
        });
    }
}

/// The per-Shell capture state: the private texture (kept and reused
/// while the size holds), the waiting tokens, and the completion channel.
/// Main-thread data only - the wgpu objects are `Arc`-backed handles,
/// and the callback crossing threads touches only the channel and the
/// `in_flight` flag.
pub(super) struct Capture {
    tex: Option<wgpu::Texture>,
    view: Option<wgpu::TextureView>,
    size: [u32; 2],
    waiting: Vec<(UserData, Instant)>,
    in_flight: Arc<AtomicBool>,
    shot_since: Option<Instant>,
    tx: Sender<ShotDone>,
    rx: Receiver<ShotDone>,
}

impl Capture {
    /// An idle capture: no GPU object yet, nothing waiting.
    pub(super) fn new() -> Self {
        let (tx, rx) = channel();
        Self {
            tex: None,
            view: None,
            size: [0, 0],
            waiting: Vec::new(),
            in_flight: Arc::new(AtomicBool::new(false)),
            shot_since: None,
            tx,
            rx,
        }
    }

    /// The frame loop records one requested token here. A request that
    /// arrives while a shot is in flight rides the next one.
    pub(super) fn want(&mut self, token: UserData, now: Instant) {
        self.waiting.push((token, now));
    }

    /// True while any request is waiting or in flight: the frame loop
    /// keeps redrawing so a completion is picked up without an input
    /// event, and a wedged map still reaches its timeout.
    pub(super) fn busy(&self) -> bool {
        self.in_flight.load(Ordering::Acquire) || !self.waiting.is_empty()
    }

    /// Ready to start a capture this frame? Prunes timed-out waits and
    /// reports a map that never completed; both leave a status line so
    /// the promised picture is visibly refused, never silently dropped.
    pub(super) fn due(&mut self, state: &mut State, now: Instant) -> bool {
        if let Some(since) = self.shot_since
            && self.in_flight.load(Ordering::Acquire)
            && now.saturating_duration_since(since) > CAPTURE_TIMEOUT
        {
            diag_warn!("report capture (#1696): a read-back never completed; releasing it");
            state.status = "Screenshot: CamCraft could not read the window - copy a picture in yourself".to_owned();
            self.in_flight.store(false, Ordering::Release);
            self.shot_since = None;
            self.waiting.clear();
            return false;
        }
        if self.in_flight.load(Ordering::Acquire) {
            return false;
        }
        if let Some((_, oldest)) = self.waiting.first()
            && now.saturating_duration_since(*oldest) > CAPTURE_TIMEOUT
        {
            diag_warn!("report capture (#1696): {} request(s) timed out unread", self.waiting.len());
            state.status = "Screenshot: CamCraft could not read the window - copy a picture in yourself".to_owned();
            self.waiting.clear();
            return false;
        }
        !self.waiting.is_empty()
    }

    /// Build this frame's capture: a texture sized to the surface (made
    /// on first use, remade when the window resizes - never while a shot
    /// is in flight, `due` has that gated) and a staging buffer aligned
    /// to `COPY_BYTES_PER_ROW_ALIGNMENT`. An unreadable surface format is
    /// a logged refusal that drops the waiting tokens: the Prime
    /// Directive refuses visibly.
    pub(super) fn begin(
        &mut self,
        device: &wgpu::Device,
        size: [u32; 2],
        format: wgpu::TextureFormat,
    ) -> Option<Shot> {
        let swizzle = swizzle_for(format)?;
        let [width, height] = size;
        if width == 0 || height == 0 {
            return None;
        }
        if self.size != size || self.tex.is_none() || self.view.is_none() {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("report-capture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.view = Some(tex.create_view(&wgpu::TextureViewDescriptor::default()));
            self.tex = Some(tex);
            self.size = size;
        }
        let padded = wgpu::util::align_to(
            width.saturating_mul(BYTES_PER_PIXEL),
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
        );
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("report-capture-readback"),
            size: u64::from(padded.saturating_mul(height)),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let tokens = self.waiting.drain(..).map(|(token, _)| token).collect();
        self.in_flight.store(true, Ordering::Release);
        self.shot_since = Some(Instant::now());
        Some(Shot {
            tex: self.tex.clone()?,
            view: self.view.clone()?,
            buffer,
            size,
            padded,
            swizzle,
            tokens,
            tx: self.tx.clone(),
            in_flight: Arc::clone(&self.in_flight),
        })
    }

    /// Drain finished pictures into egui events for the next `ctx.run`.
    /// The token travels back untouched, so each door answers only its
    /// own request; `report::window` is the only door on the native
    /// shell's root viewport today (#1696).
    pub(super) fn drain_events(&mut self) -> Vec<egui::Event> {
        let mut out = Vec::new();
        while let Ok(done) = self.rx.try_recv() {
            self.shot_since = None;
            for token in done.tokens {
                out.push(egui::Event::Screenshot {
                    viewport_id: egui::ViewportId::ROOT,
                    user_data: token,
                    image: Arc::clone(&done.image),
                });
            }
        }
        out
    }

    /// Nudge the map callbacks. Non-blocking: `Poll` only looks at what
    /// already completed, so a frame never waits on the read-back.
    pub(super) fn pump(&self, device: &wgpu::Device) {
        if !self.in_flight.load(Ordering::Acquire) {
            return;
        }
        if let Err(err) = device.poll(wgpu::PollType::Poll) {
            diag_warn!("report capture (#1696): device poll failed: {err}");
        }
    }
}
