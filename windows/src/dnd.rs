// OLE drop target for the notch: unlike WM_DROPFILES it tells us when a
// file drag *enters* the notch, so the notch can open on the Shelf while
// the file is still being dragged. A tiny hand-written IDropTarget (one
// static object, no allocation per drag).

use std::ffi::c_void;
use std::ptr::null_mut;

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Shell::{DragQueryFileW, HDROP};

type HRESULT = i32;
const S_OK: HRESULT = 0;
const E_NOINTERFACE: HRESULT = 0x8000_4002u32 as i32;
const CF_HDROP: u16 = 15;
const TYMED_HGLOBAL: u32 = 1;
const DROPEFFECT_NONE: u32 = 0;
const DROPEFFECT_COPY: u32 = 1;
const DROPEFFECT_LINK: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PointL {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
struct FormatEtc {
    cf: u16,
    ptd: *mut c_void,
    aspect: u32,
    lindex: i32,
    tymed: u32,
}

#[repr(C)]
struct StgMedium {
    tymed: u32,
    handle: *mut c_void,
    unk_release: *mut c_void,
}

#[link(name = "ole32")]
extern "system" {
    fn OleInitialize(reserved: *mut c_void) -> HRESULT;
    fn RegisterDragDrop(hwnd: HWND, target: *mut c_void) -> HRESULT;
    fn ReleaseStgMedium(m: *mut StgMedium);
}

#[repr(C)]
struct Vtbl {
    query_interface: unsafe extern "system" fn(*mut DropTarget, *const GUID, *mut *mut c_void) -> HRESULT,
    add_ref: unsafe extern "system" fn(*mut DropTarget) -> u32,
    release: unsafe extern "system" fn(*mut DropTarget) -> u32,
    drag_enter: unsafe extern "system" fn(*mut DropTarget, *mut c_void, u32, PointL, *mut u32) -> HRESULT,
    drag_over: unsafe extern "system" fn(*mut DropTarget, u32, PointL, *mut u32) -> HRESULT,
    drag_leave: unsafe extern "system" fn(*mut DropTarget) -> HRESULT,
    drop: unsafe extern "system" fn(*mut DropTarget, *mut c_void, u32, PointL, *mut u32) -> HRESULT,
}

#[repr(C)]
struct DropTarget {
    vtbl: *const Vtbl,
}

/// What the notch does with a drag (set by main).
pub struct Handlers {
    pub enter: unsafe fn(),
    pub leave: unsafe fn(),
    pub drop: unsafe fn(Vec<String>),
}

static VTBL: Vtbl = Vtbl { query_interface, add_ref, release, drag_enter, drag_over, drag_leave, drop };
static mut TARGET: DropTarget = DropTarget { vtbl: &VTBL };
static mut HANDLERS: Option<Handlers> = None;
static mut ACCEPT: bool = false;

const IID_IUNKNOWN: GUID = GUID { data1: 0, data2: 0, data3: 0, data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };
const IID_IDROPTARGET: GUID = GUID { data1: 0x122, data2: 0, data3: 0, data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46] };

fn same(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

unsafe extern "system" fn query_interface(this: *mut DropTarget, riid: *const GUID, out: *mut *mut c_void) -> HRESULT {
    if !riid.is_null() && (same(&*riid, &IID_IUNKNOWN) || same(&*riid, &IID_IDROPTARGET)) {
        *out = this as *mut c_void;
        return S_OK;
    }
    *out = null_mut();
    E_NOINTERFACE
}
// static object: reference counting is a no-op
unsafe extern "system" fn add_ref(_: *mut DropTarget) -> u32 {
    2
}
unsafe extern "system" fn release(_: *mut DropTarget) -> u32 {
    1
}

fn hdrop_format() -> FormatEtc {
    FormatEtc { cf: CF_HDROP, ptd: null_mut(), aspect: 1, lindex: -1, tymed: TYMED_HGLOBAL }
}

// IDataObject vtable: 0..2 IUnknown, 3 GetData, 5 QueryGetData
unsafe fn data_has_files(obj: *mut c_void) -> bool {
    if obj.is_null() {
        return false;
    }
    let vt = *(obj as *const *const usize);
    let qgd: unsafe extern "system" fn(*mut c_void, *const FormatEtc) -> HRESULT = std::mem::transmute(*vt.add(5));
    let f = hdrop_format();
    qgd(obj, &f) == S_OK
}

unsafe fn data_files(obj: *mut c_void) -> Vec<String> {
    let mut out = Vec::new();
    if obj.is_null() {
        return out;
    }
    let vt = *(obj as *const *const usize);
    let get: unsafe extern "system" fn(*mut c_void, *const FormatEtc, *mut StgMedium) -> HRESULT = std::mem::transmute(*vt.add(3));
    let f = hdrop_format();
    let mut m = StgMedium { tymed: 0, handle: null_mut(), unk_release: null_mut() };
    if get(obj, &f, &mut m) != S_OK {
        return out;
    }
    let hdrop = m.handle as HDROP;
    let count = DragQueryFileW(hdrop, 0xFFFF_FFFF, null_mut(), 0);
    for i in 0..count {
        let len = DragQueryFileW(hdrop, i, null_mut(), 0);
        let mut buf = vec![0u16; len as usize + 1];
        DragQueryFileW(hdrop, i, buf.as_mut_ptr(), len + 1);
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    ReleaseStgMedium(&mut m);
    out
}

// Never offer "move": the Shelf only keeps a link to the file.
unsafe fn effect(allowed: u32) -> u32 {
    if !ACCEPT {
        DROPEFFECT_NONE
    } else if allowed & DROPEFFECT_LINK != 0 {
        DROPEFFECT_LINK
    } else if allowed & DROPEFFECT_COPY != 0 {
        DROPEFFECT_COPY
    } else {
        DROPEFFECT_NONE
    }
}

unsafe extern "system" fn drag_enter(_: *mut DropTarget, obj: *mut c_void, _keys: u32, _pt: PointL, eff: *mut u32) -> HRESULT {
    ACCEPT = data_has_files(obj);
    if !eff.is_null() {
        *eff = effect(*eff);
    }
    if ACCEPT {
        if let Some(h) = (*(&raw const HANDLERS)).as_ref() {
            (h.enter)();
        }
    }
    S_OK
}

unsafe extern "system" fn drag_over(_: *mut DropTarget, _keys: u32, _pt: PointL, eff: *mut u32) -> HRESULT {
    if !eff.is_null() {
        *eff = effect(*eff);
    }
    S_OK
}

unsafe extern "system" fn drag_leave(_: *mut DropTarget) -> HRESULT {
    if ACCEPT {
        ACCEPT = false;
        if let Some(h) = (*(&raw const HANDLERS)).as_ref() {
            (h.leave)();
        }
    }
    S_OK
}

unsafe extern "system" fn drop(_: *mut DropTarget, obj: *mut c_void, _keys: u32, _pt: PointL, eff: *mut u32) -> HRESULT {
    if !eff.is_null() {
        *eff = effect(*eff);
    }
    if ACCEPT {
        ACCEPT = false;
        let files = data_files(obj);
        if let Some(h) = (*(&raw const HANDLERS)).as_ref() {
            (h.drop)(files);
        }
    }
    S_OK
}

/// Make `hwnd` an OLE drop target. Returns false if OLE refused (the caller
/// keeps WM_DROPFILES as the fallback).
pub unsafe fn register(hwnd: HWND, handlers: Handlers) -> bool {
    *(&raw mut HANDLERS) = Some(handlers);
    let hr = OleInitialize(null_mut());
    if hr < 0 {
        return false;
    }
    RegisterDragDrop(hwnd, (&raw mut TARGET) as *mut c_void) == S_OK
}
