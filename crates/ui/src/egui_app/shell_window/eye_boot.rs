//! The traced-stock eye is built off the UI thread. Building it compiles
//! the composite and trace shaders, which took about 450 ms on one Windows
//! DX12 adapter, while the window was shown but did not answer. The shell
//! draws without an eye until it lands, as the browser shell does
//! (`StockEye::new_async`). Provenance: our own code over std threads and
//! the public wgpu docs (`Device` is `Send + Sync + Clone` on native).

use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;

use egui_wgpu::wgpu;
use winit::window::Window;

use crate::trace::eye::StockEye;

pub(super) type EyeRx = Receiver<Result<StockEye, String>>;

/// Build the eye on a new thread; ask `window` for a frame when it is done.
/// `None` when the thread cannot start: the shell then builds it here.
pub(super) fn spawn(device: wgpu::Device, format: wgpu::TextureFormat, window: Arc<Window>) -> Option<EyeRx> {
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("stock-eye".into())
        .spawn(move || {
            let built = StockEye::new(&device, format);
            if tx.send(built).is_ok() {
                window.request_redraw();
            }
        })
        .ok()?;
    Some(rx)
}

/// The eye, once: taken when built, `Err` when the build refused.
pub(super) fn take(rx: &mut Option<EyeRx>) -> Option<Result<StockEye, String>> {
    let got = match rx.as_ref()?.try_recv() {
        Ok(r) => Some(r),
        Err(TryRecvError::Empty) => return None,
        Err(TryRecvError::Disconnected) => Some(Err("the stock eye thread ended without a result".to_owned())),
    };
    *rx = None;
    got
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_yet_keeps_the_receiver() {
        let (_tx, rx) = channel::<Result<StockEye, String>>();
        let mut slot = Some(rx);
        assert!(take(&mut slot).is_none());
        assert!(slot.is_some(), "an empty channel must be polled again on the next frame");
    }

    #[test]
    fn a_refusal_is_handed_over_once() {
        let (tx, rx) = channel::<Result<StockEye, String>>();
        tx.send(Err("no composite".into())).unwrap();
        let mut slot = Some(rx);
        assert!(matches!(take(&mut slot), Some(Err(e)) if e == "no composite"));
        assert!(slot.is_none() && take(&mut slot).is_none());
    }

    #[test]
    fn a_thread_that_ended_without_a_result_is_named() {
        let (tx, rx) = channel::<Result<StockEye, String>>();
        drop(tx);
        let mut slot = Some(rx);
        assert!(matches!(take(&mut slot), Some(Err(e)) if e.contains("ended")));
        assert!(slot.is_none());
    }

    #[test]
    fn the_eye_can_cross_threads() {
        fn send<T: Send>() {}
        send::<StockEye>();
    }
}
