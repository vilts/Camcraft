//! Provenance: original deterministic regression pins for issue #2373.

use super::{Tokens, current, set_current};
use std::sync::Barrier;

#[test]
fn parallel_theme_writers_keep_their_own_frame_colours() {
    let dark_written = Barrier::new(2);
    let light_written = Barrier::new(2);
    let (dark, light) = std::thread::scope(|scope| {
        let dark = scope.spawn(|| {
            set_current(&Tokens::DARK);
            dark_written.wait();
            light_written.wait();
            current()
        });
        let light = scope.spawn(|| {
            dark_written.wait();
            set_current(&Tokens::LIGHT);
            light_written.wait();
            current()
        });
        (
            dark.join().expect("dark frame finishes"),
            light.join().expect("light frame finishes"),
        )
    });
    assert_eq!(
        dark,
        Tokens::DARK,
        "another frame must not change dark overlay colours"
    );
    assert_eq!(
        light,
        Tokens::LIGHT,
        "a light frame must keep its chosen colours"
    );
}

#[test]
fn a_fresh_test_thread_starts_with_the_default_dark_theme() {
    std::thread::spawn(|| {
        set_current(&Tokens::LIGHT);
        let fresh = std::thread::spawn(current)
            .join()
            .expect("fresh frame finishes");
        assert_eq!(
            fresh,
            Tokens::DARK,
            "a previous test must not change a fresh frame default"
        );
    })
    .join()
    .expect("previous frame finishes");
}
