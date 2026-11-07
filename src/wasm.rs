use crate::{console::BRIEFING, Console};
use std::cell::RefCell;
thread_local! {static SESSION:RefCell<Option<Console>>=const{RefCell::new(None)};static OUTPUT:RefCell<Vec<u8>>=const{RefCell::new(Vec::new())};}
fn output(bytes: Vec<u8>) -> u32 {
    let n = bytes.len() as u32;
    OUTPUT.with(|o| *o.borrow_mut() = bytes);
    n
}
#[no_mangle]
pub extern "C" fn alloc(n: u32) -> *mut u8 {
    let b = vec![0u8; n as usize].into_boxed_slice();
    Box::into_raw(b) as *mut u8
}
#[no_mangle]
pub unsafe extern "C" fn dealloc(p: *mut u8, n: u32) {
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
        p, n as usize,
    )));
}
#[no_mangle]
pub extern "C" fn output_ptr() -> *const u8 {
    OUTPUT.with(|o| o.borrow().as_ptr())
}
#[no_mangle]
pub extern "C" fn init(hardcore: u32) -> u32 {
    match Console::new(hardcore != 0) {
        Ok(c) => {
            SESSION.with(|s| *s.borrow_mut() = Some(c));
            output(BRIEFING.as_bytes().to_vec())
        }
        Err(e) => output(format!("error: {e}").into_bytes()),
    }
}
#[no_mangle]
pub unsafe extern "C" fn command(p: *const u8, n: u32) -> u32 {
    let bytes = std::slice::from_raw_parts(p, n as usize);
    let line = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return output(b"error: invalid UTF-8".to_vec()),
    };
    SESSION.with(|s| match s.borrow_mut().as_mut() {
        Some(c) => output(c.exec(line).into_bytes()),
        None => output(b"error: not initialized".to_vec()),
    })
}
#[no_mangle]
pub extern "C" fn status() -> u32 {
    SESSION.with(|s| {
        output(
            s.borrow()
                .as_ref()
                .map_or("{}".into(), Console::status_json)
                .into_bytes(),
        )
    })
}
#[no_mangle]
pub extern "C" fn save() -> u32 {
    SESSION.with(|s| output(s.borrow().as_ref().map_or(vec![], Console::save)))
}
#[no_mangle]
pub unsafe extern "C" fn load(p: *const u8, n: u32) -> u32 {
    if SESSION.with(|s| s.borrow().as_ref().is_some_and(Console::hardcore)) {
        return output(b"error: session import disabled in hardcore mode".to_vec());
    }
    match Console::load(std::slice::from_raw_parts(p, n as usize)) {
        Ok(c) => {
            SESSION.with(|s| *s.borrow_mut() = Some(c));
            output(b"Session resumed.\n".to_vec())
        }
        Err(e) => output(format!("error: {e}\n").into_bytes()),
    }
}
#[no_mangle]
pub unsafe extern "C" fn get_file(p: *const u8, n: u32) -> u32 {
    let name = std::str::from_utf8(std::slice::from_raw_parts(p, n as usize)).unwrap_or("");
    SESSION.with(|s| {
        output(
            s.borrow()
                .as_ref()
                .and_then(|c| c.file(name).ok())
                .unwrap_or(&[])
                .to_vec(),
        )
    })
}
