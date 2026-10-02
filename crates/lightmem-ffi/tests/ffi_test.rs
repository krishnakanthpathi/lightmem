use lightmem_ffi::c_api::*;
use lightmem_ffi::*;
use std::ffi::{CStr, CString};
use tempfile::tempdir;

#[test]
fn test_uniffi_lifecycle() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("uniffi_test.db");
    let db_str = db_path.to_str().unwrap().to_string();

    let lm = LightMem::new(Some(db_str.clone()), false).expect("open failed");

    // 1. Remember
    let rec = lm
        .remember(
            "ghp_secret_access_token_super_safe".to_string(),
            Some("password".to_string()),
            Some("GitHub Token".to_string()),
            vec!["auth".to_string(), "token".to_string()],
            Some(1.0),
        )
        .expect("remember failed");

    assert_eq!(rec.category, "password");
    assert_eq!(rec.title, "GitHub Token");

    // 2. Recall
    let recalled = lm
        .recall(
            "token".to_string(),
            Some("password".to_string()),
            None,
            Some(5),
            None,
        )
        .expect("recall failed");
    assert!(!recalled.is_empty());
    assert_eq!(recalled[0].memory.id, rec.id);

    // 3. Stats
    let stats = lm.stats().expect("stats failed");
    assert_eq!(stats.total_memories, 1);
    assert_eq!(stats.active_memories, 1);
    let pw_cat = stats
        .by_category
        .iter()
        .find(|c| c.category == "password")
        .map(|c| c.count)
        .unwrap_or(0);
    assert_eq!(pw_cat, 1);

    // 4. Forget
    let forgot = lm.forget(rec.id.clone(), false).expect("forget failed");
    assert!(forgot);

    let stats_after = lm.stats().expect("stats failed");
    assert_eq!(stats_after.active_memories, 0);
    assert_eq!(stats_after.expired_memories, 1);
}

#[test]
fn test_c_abi_lifecycle() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("c_abi_test.db");
    let db_str = db_path.to_str().unwrap();
    let db_c = CString::new(db_str).unwrap();

    unsafe {
        let handle = lmem_open(db_c.as_ptr(), 0);
        assert!(!handle.is_null());

        // 1. Remember
        let content_c = CString::new("PostgreSQL port is 5432").unwrap();
        let cat_c = CString::new("decision").unwrap();
        let title_c = CString::new("Database Port").unwrap();
        let tags_c = CString::new("db,postgres").unwrap();

        let json_ptr = lmem_remember(
            handle,
            content_c.as_ptr(),
            cat_c.as_ptr(),
            title_c.as_ptr(),
            tags_c.as_ptr(),
            0.95,
        );
        assert!(!json_ptr.is_null());
        let json_str = CStr::from_ptr(json_ptr).to_str().unwrap();
        assert!(json_str.contains("Database Port"));
        assert!(json_str.contains("5432"));
        lmem_free_string(json_ptr);

        // 2. Recall
        let query_c = CString::new("database port").unwrap();
        let recall_ptr = lmem_recall(
            handle,
            query_c.as_ptr(),
            cat_c.as_ptr(),
            std::ptr::null(),
            5,
            -1.0,
        );
        assert!(!recall_ptr.is_null());
        let recall_str = CStr::from_ptr(recall_ptr).to_str().unwrap();
        assert!(recall_str.contains("Database Port"));
        lmem_free_string(recall_ptr);

        // 3. Stats
        let stats_ptr = lmem_stats(handle);
        assert!(!stats_ptr.is_null());
        let stats_str = CStr::from_ptr(stats_ptr).to_str().unwrap();
        assert!(stats_str.contains("\"total_memories\":1"));
        lmem_free_string(stats_ptr);

        // 4. Close
        lmem_close(handle);
    }
}
