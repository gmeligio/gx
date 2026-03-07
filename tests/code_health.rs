use std::fs;
use std::path::Path;

fn count_ignore_attributes(dir: &Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += count_ignore_attributes(&path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                if let Ok(content) = fs::read_to_string(&path) {
                    count += content.matches("#[ignore").count();
                }
            }
        }
    }
    count
}

#[test]
fn ignore_attribute_budget() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let count = count_ignore_attributes(&src_dir);
    let max_ignored = 10;
    assert!(
        count <= max_ignored,
        "Too many #[ignore] attributes in src/: found {count}, budget is {max_ignored}. \
         Remove #[ignore] from tests that can now run, or increase the budget if justified."
    );
}
