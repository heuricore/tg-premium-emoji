use std::fs;
use std::sync::Arc;
use std::thread;

use serde::{Deserialize, Serialize};
use tge::error::Error;
use tge::store::atomic::{update_json, write_atomic};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Counter {
    n: u32,
}

#[test]
fn write_atomic_replaces_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/data.txt");
    write_atomic(&path, b"first").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"first");
    write_atomic(&path, b"second").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"second");
}

#[test]
fn no_temp_files_left() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.txt");
    for i in 0..5 {
        write_atomic(&path, format!("v{i}").as_bytes()).unwrap();
    }
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["data.txt".to_string()]);
}

#[test]
fn concurrent_updates_lose_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = Arc::new(dir.path().join("counter.json"));
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let path = Arc::clone(&path);
            thread::spawn(move || {
                for _ in 0..25 {
                    update_json::<Counter>(&path, |cur| {
                        Ok(Counter {
                            n: cur.map_or(0, |c| c.n) + 1,
                        })
                    })
                    .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let text = fs::read_to_string(&*path).unwrap();
    assert!(text.ends_with('\n'));
    let counter: Counter = serde_json::from_str(&text).unwrap();
    assert_eq!(counter, Counter { n: 200 });
}

#[test]
fn failing_update_leaves_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("counter.json");
    update_json::<Counter>(&path, |_| Ok(Counter { n: 7 })).unwrap();
    let before = fs::read(&path).unwrap();
    let err = update_json::<Counter>(&path, |_| Err(Error::Runtime("nope".into()))).unwrap_err();
    assert!(matches!(err, Error::Runtime(_)));
    assert_eq!(fs::read(&path).unwrap(), before);
}
