use crate::ebur128;

use std::{mem, ptr};

// ABI compatible with ebur128_state
#[repr(C)]
pub struct State {
    mode: i32,
    channels: u32,
    samplerate: std::os::raw::c_ulong,
    internal: *mut ebur128::EbuR128,
}

/// # Safety
///
/// `state` must be a valid, non-null pointer to a `State` whose `internal`
/// field is a valid, non-null pointer to an `EbuR128`.  The resulting
/// references must not alias any mutable reference for their entire lifetime.
unsafe fn state_ref<'a>(state: *const State) -> (&'a State, &'a ebur128::EbuR128) {
    let s = unsafe { &*state };
    let e = unsafe { &*s.internal };
    (s, e)
}

/// # Safety
///
/// `state` must be a valid, non-null pointer to a `State` whose `internal`
/// field is a valid, non-null pointer to an `EbuR128`.  The caller must
/// guarantee exclusive access for the lifetime of the returned references.
unsafe fn state_mut<'a>(state: *mut State) -> (&'a mut State, &'a mut ebur128::EbuR128) {
    // Read the internal pointer before creating the &mut State reference
    // to avoid overlapping mutable borrows.
    let internal = unsafe { (*state).internal };
    let s = unsafe { &mut *state };
    let e = unsafe { &mut *internal };
    (s, e)
}

/// Write `val` through a raw output pointer.
///
/// # Safety
///
/// `out` must be a valid, non-null, properly aligned pointer to `f64`.
unsafe fn write_out(out: *mut f64, val: f64) {
    unsafe { *out = val };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_get_version(major: *mut i32, minor: *mut i32, patch: *mut i32) {
    // We're based on 1.2.6 so let's return that for now
    unsafe {
        *major = 1;
        *minor = 2;
        *patch = 6;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_init(
    channels: u32,
    samplerate: std::os::raw::c_ulong,
    // Same values as our Mode enum
    mode: i32,
) -> *mut State {
    let e = match ebur128::EbuR128::new(
        channels,
        samplerate as u32,
        ebur128::Mode::from_bits_truncate(mode as u8),
    ) {
        Err(_) => return ptr::null_mut(),
        Ok(e) => e,
    };

    let s = State {
        mode,
        channels,
        samplerate,
        internal: Box::into_raw(Box::new(e)),
    };

    Box::into_raw(Box::new(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_destroy(state: *mut *mut State) {
    if state.is_null() || unsafe { (*state).is_null() } {
        return;
    }

    unsafe {
        // Read the inner pointer before taking ownership of State,
        // so we don't access Box<State> fields after it could be dropped.
        let inner = (**state).internal;
        let _s = Box::from_raw(*state);
        let _e = Box::from_raw(inner);
        // _e (EbuR128) drops first, then _s (State) — correct order.

        *state = ptr::null_mut();
    }
}

impl From<ebur128::Error> for i32 {
    fn from(v: ebur128::Error) -> i32 {
        match v {
            ebur128::Error::NoMem => 1,
            ebur128::Error::InvalidMode => 2,
            ebur128::Error::InvalidChannelIndex => 3,
        }
    }
}

// Same channel representation
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_set_channel(
    state: *mut State,
    channel_number: u32,
    value: i32,
) -> i32 {
    let (_, e) = unsafe { state_mut(state) };

    // Safety: ebur128::Channel must be #[repr(i32)] (or equivalent) and the
    // caller is responsible for passing only valid discriminant values.
    // Passing an out-of-range value is UB — this matches the C library contract.
    match e.set_channel(channel_number, unsafe {
        mem::transmute::<i32, ebur128::Channel>(value)
    }) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_change_parameters(
    state: *mut State,
    channels: u32,
    samplerate: std::os::raw::c_ulong,
) -> i32 {
    let (s, e) = unsafe { state_mut(state) };

    if s.channels == channels && s.samplerate == samplerate {
        return 4; // EBUR128_ERROR_NO_CHANGE
    }

    match e.change_parameters(channels, samplerate as u32) {
        Err(err) => err.into(),
        Ok(_) => {
            s.channels = channels;
            s.samplerate = samplerate;
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_set_max_window(
    state: *mut State,
    window: std::os::raw::c_ulong,
) -> i32 {
    if window > u32::MAX as std::os::raw::c_ulong {
        return ebur128::Error::NoMem.into();
    }

    let (_, e) = unsafe { state_mut(state) };

    if e.max_window() == window as usize {
        return 4; // EBUR128_ERROR_NO_CHANGE
    }

    match e.set_max_window(window as u32) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_set_max_history(
    state: *mut State,
    history: std::os::raw::c_ulong,
) -> i32 {
    if history > u32::MAX as std::os::raw::c_ulong {
        return ebur128::Error::NoMem.into();
    }

    let (_, e) = unsafe { state_mut(state) };

    if e.max_history() == history as usize {
        return 4; // EBUR128_ERROR_NO_CHANGE
    }

    match e.set_max_history(history as u32) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_add_frames_short(
    state: *mut State,
    src: *const i16,
    frames: usize,
) -> i32 {
    let (s, e) = unsafe { state_mut(state) };

    let samples = match frames.checked_mul(s.channels as usize) {
        None => return crate::ebur128::Error::NoMem.into(),
        Some(samples) => samples,
    };

    match e.add_frames_i16(unsafe { std::slice::from_raw_parts(src, samples) }) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_add_frames_int(
    state: *mut State,
    src: *const i32,
    frames: usize,
) -> i32 {
    let (s, e) = unsafe { state_mut(state) };

    let samples = match frames.checked_mul(s.channels as usize) {
        None => return crate::ebur128::Error::NoMem.into(),
        Some(samples) => samples,
    };

    match e.add_frames_i32(unsafe { std::slice::from_raw_parts(src, samples) }) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_add_frames_float(
    state: *mut State,
    src: *const f32,
    frames: usize,
) -> i32 {
    let (s, e) = unsafe { state_mut(state) };

    let samples = match frames.checked_mul(s.channels as usize) {
        None => return crate::ebur128::Error::NoMem.into(),
        Some(samples) => samples,
    };

    match e.add_frames_f32(unsafe { std::slice::from_raw_parts(src, samples) }) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_add_frames_double(
    state: *mut State,
    src: *const f64,
    frames: usize,
) -> i32 {
    let (s, e) = unsafe { state_mut(state) };

    let samples = match frames.checked_mul(s.channels as usize) {
        None => return crate::ebur128::Error::NoMem.into(),
        Some(samples) => samples,
    };

    match e.add_frames_f64(unsafe { std::slice::from_raw_parts(src, samples) }) {
        Err(err) => err.into(),
        Ok(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_global(state: *mut State, out: *mut f64) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.loudness_global() {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_global_multiple(
    state: *mut *mut State,
    size: usize,
    out: *mut f64,
) -> i32 {
    let s = unsafe { std::slice::from_raw_parts(state, size) };
    let iter = s
        .iter()
        .copied()
        .map(|s: *mut State| unsafe { &*(*s).internal });

    match ebur128::EbuR128::loudness_global_multiple(iter) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_momentary(state: *mut State, out: *mut f64) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.loudness_momentary() {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_shortterm(state: *mut State, out: *mut f64) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.loudness_shortterm() {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_window(
    state: *mut State,
    window: std::os::raw::c_ulong,
    out: *mut f64,
) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    if window > u32::MAX as std::os::raw::c_ulong {
        return ebur128::Error::NoMem.into();
    }

    match e.loudness_window(window as u32) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_range(state: *mut State, out: *mut f64) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.loudness_range() {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_loudness_range_multiple(
    state: *mut *mut State,
    size: usize,
    out: *mut f64,
) -> i32 {
    let s = unsafe { std::slice::from_raw_parts(state, size) };
    let iter = s
        .iter()
        .copied()
        .map(|s: *mut State| unsafe { &*(*s).internal });

    match ebur128::EbuR128::loudness_range_multiple(iter) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_sample_peak(
    state: *mut State,
    channel_number: u32,
    out: *mut f64,
) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.sample_peak(channel_number) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_prev_sample_peak(
    state: *mut State,
    channel_number: u32,
    out: *mut f64,
) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.prev_sample_peak(channel_number) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_true_peak(
    state: *mut State,
    channel_number: u32,
    out: *mut f64,
) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.true_peak(channel_number) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_prev_true_peak(
    state: *mut State,
    channel_number: u32,
    out: *mut f64,
) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.prev_true_peak(channel_number) {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ebur128_relative_threshold(state: *mut State, out: *mut f64) -> i32 {
    let (_, e) = unsafe { state_ref(state) };

    match e.relative_threshold() {
        Err(err) => err.into(),
        Ok(val) => {
            unsafe { write_out(out, val) };
            0
        }
    }
}
