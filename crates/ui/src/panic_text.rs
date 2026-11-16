// Turns a caught panic payload into text for the Windows start-up error box (#1325).
// Spec: std::panic::catch_unwind documents the payload as `Box<dyn Any + Send>`, in
// practice a `&'static str` or a `String`. Pure, so it is tested on every OS.
#![cfg_attr(not(windows), allow(dead_code))]

use std::any::Any;

/// Text of a panic payload; never panics. Anything but `&str` / `String` gets a generic text.
pub fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "an unknown error".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::payload_text;

    #[test]
    fn payload_text_reads_str_string_and_other() {
        assert_eq!(payload_text(&"boom"), "boom");
        assert_eq!(payload_text(&String::from("bang")), "bang");
        assert_eq!(payload_text(&42_u32), "an unknown error");
        let caught = std::panic::catch_unwind(|| {
            let n = 3;
            if n > 2 {
                std::panic::panic_any(String::from("formatted 3"));
            }
        });
        if let Err(p) = caught {
            assert_eq!(payload_text(&*p), "formatted 3");
        }
    }
}
