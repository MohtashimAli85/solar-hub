/// macOS has no public call for switching Bluetooth on, so this uses the
/// private IOBluetooth preference functions (the same ones `blueutil` uses),
/// looked up at runtime so a future macOS without them only loses this.
#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_int, c_void, CStr};

    extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    const RTLD_LAZY: c_int = 1;
    const FRAMEWORK: &CStr = c"/System/Library/Frameworks/IOBluetooth.framework/IOBluetooth";

    fn symbol(name: &CStr) -> Option<*mut c_void> {
        unsafe {
            let handle = dlopen(FRAMEWORK.as_ptr(), RTLD_LAZY);
            if handle.is_null() {
                return None;
            }
            let found = dlsym(handle, name.as_ptr());
            (!found.is_null()).then_some(found)
        }
    }

    pub fn is_on() -> Option<bool> {
        let found = symbol(c"IOBluetoothPreferenceGetControllerPowerState")?;
        let get: extern "C" fn() -> c_int = unsafe { std::mem::transmute(found) };
        Some(get() != 0)
    }

    pub fn request_on() -> bool {
        let Some(found) = symbol(c"IOBluetoothPreferenceSetControllerPowerState") else {
            return false;
        };
        let set: extern "C" fn(c_int) = unsafe { std::mem::transmute(found) };
        set(1);
        true
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn is_on() -> Option<bool> {
        None
    }

    pub fn request_on() -> bool {
        false
    }
}

pub use imp::{is_on, request_on};
