use crate::LightMem;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Arc;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn set_last_error(err: String) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = Some(err);
    });
}

pub struct LMemHandle {
    pub inner: Arc<LightMem>,
}

#[no_mangle]
pub unsafe extern "C" fn lmem_get_last_error() -> *mut c_char {
    LAST_ERROR.with(|cell| {
        if let Some(ref err) = *cell.borrow() {
            CString::new(err.as_str()).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut())
        } else {
            std::ptr::null_mut()
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn lmem_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

unsafe fn to_string_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_open(db_path: *const c_char, global_db: c_int) -> *mut LMemHandle {
    let path_opt = to_string_opt(db_path);
    match LightMem::new(path_opt, global_db != 0) {
        Ok(lm) => Box::into_raw(Box::new(LMemHandle { inner: lm })),
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_close(handle: *mut LMemHandle) {
    if !handle.is_null() {
        drop(Box::from_raw(handle));
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_remember(
    handle: *mut LMemHandle,
    content: *const c_char,
    category: *const c_char,
    title: *const c_char,
    tags_csv: *const c_char,
    confidence: f32,
) -> *mut c_char {
    if handle.is_null() || content.is_null() {
        set_last_error("Null pointer provided to lmem_remember".to_string());
        return std::ptr::null_mut();
    }

    let lm = &(*handle).inner;
    let content_str = match CStr::from_ptr(content).to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_last_error(e.to_string());
            return std::ptr::null_mut();
        }
    };

    let cat_opt = to_string_opt(category);
    let title_opt = to_string_opt(title);
    let tags_vec: Vec<String> = to_string_opt(tags_csv)
        .map(|s| s.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default();

    let conf_opt = if confidence >= 0.0 { Some(confidence) } else { None };

    match lm.remember(content_str, cat_opt, title_opt, tags_vec, conf_opt) {
        Ok(rec) => match serde_json::to_string(&rec) {
            Ok(json) => CString::new(json).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_recall(
    handle: *mut LMemHandle,
    query: *const c_char,
    category: *const c_char,
    as_of: *const c_char,
    limit: u32,
    min_similarity: f32,
) -> *mut c_char {
    if handle.is_null() || query.is_null() {
        set_last_error("Null pointer provided to lmem_recall".to_string());
        return std::ptr::null_mut();
    }

    let lm = &(*handle).inner;
    let query_str = match CStr::from_ptr(query).to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_last_error(e.to_string());
            return std::ptr::null_mut();
        }
    };

    let cat_opt = to_string_opt(category);
    let as_of_opt = to_string_opt(as_of);
    let min_sim_opt = if min_similarity >= 0.0 { Some(min_similarity) } else { None };

    match lm.recall(query_str, cat_opt, as_of_opt, Some(limit), min_sim_opt) {
        Ok(results) => match serde_json::to_string(&results) {
            Ok(json) => CString::new(json).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_answer(
    handle: *mut LMemHandle,
    question: *const c_char,
    needle: c_int,
    category: *const c_char,
    as_of: *const c_char,
    limit: u32,
) -> *mut c_char {
    if handle.is_null() || question.is_null() {
        set_last_error("Null pointer provided to lmem_answer".to_string());
        return std::ptr::null_mut();
    }

    let lm = &(*handle).inner;
    let q_str = match CStr::from_ptr(question).to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_last_error(e.to_string());
            return std::ptr::null_mut();
        }
    };

    let cat_opt = to_string_opt(category);
    let as_of_opt = to_string_opt(as_of);

    match lm.answer(q_str, Some(needle != 0), cat_opt, as_of_opt, Some(limit)) {
        Ok(ans) => match serde_json::to_string(&ans) {
            Ok(json) => CString::new(json).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_list(
    handle: *mut LMemHandle,
    category: *const c_char,
    status: *const c_char,
    as_of: *const c_char,
    limit: u32,
) -> *mut c_char {
    if handle.is_null() {
        set_last_error("Null handle provided to lmem_list".to_string());
        return std::ptr::null_mut();
    }

    let lm = &(*handle).inner;
    let cat_opt = to_string_opt(category);
    let stat_opt = to_string_opt(status);
    let as_of_opt = to_string_opt(as_of);

    match lm.list(cat_opt, stat_opt, as_of_opt, Some(limit)) {
        Ok(records) => match serde_json::to_string(&records) {
            Ok(json) => CString::new(json).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_forget(
    handle: *mut LMemHandle,
    id: *const c_char,
    hard: c_int,
) -> c_int {
    if handle.is_null() || id.is_null() {
        set_last_error("Null pointer provided to lmem_forget".to_string());
        return 0;
    }

    let lm = &(*handle).inner;
    let id_str = match CStr::from_ptr(id).to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_last_error(e.to_string());
            return 0;
        }
    };

    match lm.forget(id_str, hard != 0) {
        Ok(success) => if success { 1 } else { 0 },
        Err(e) => {
            set_last_error(e.to_string());
            0
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_stats(handle: *mut LMemHandle) -> *mut c_char {
    if handle.is_null() {
        set_last_error("Null handle provided to lmem_stats".to_string());
        return std::ptr::null_mut();
    }

    let lm = &(*handle).inner;
    match lm.stats() {
        Ok(stats) => match serde_json::to_string(&stats) {
            Ok(json) => CString::new(json).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
            Err(e) => {
                set_last_error(e.to_string());
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_export_okf(
    handle: *mut LMemHandle,
    output_path: *const c_char,
) -> *mut c_char {
    if handle.is_null() {
        set_last_error("Null handle provided to lmem_export_okf".to_string());
        return std::ptr::null_mut();
    }

    let lm = &(*handle).inner;
    let path_opt = to_string_opt(output_path);

    match lm.export_okf(path_opt) {
        Ok(path) => CString::new(path).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(e.to_string());
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn lmem_import_file(
    handle: *mut LMemHandle,
    file_path: *const c_char,
) -> i64 {
    if handle.is_null() || file_path.is_null() {
        set_last_error("Null pointer provided to lmem_import_file".to_string());
        return -1;
    }

    let lm = &(*handle).inner;
    let file_str = match CStr::from_ptr(file_path).to_str() {
        Ok(s) => s.to_string(),
        Err(e) => {
            set_last_error(e.to_string());
            return -1;
        }
    };

    match lm.import_file(file_str) {
        Ok(count) => count as i64,
        Err(e) => {
            set_last_error(e.to_string());
            -1
        }
    }
}
