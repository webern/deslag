//! The as-built docs in `docs/design/` share out the modules under `src/`: each module is named in
//! the `subsystems:` of exactly one of them, the one that describes it.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use deslag::Document;
use deslag::document::{BlockKind, Body};
use serde::Deserialize;

/// The frontmatter of an as-built doc, as far as this test reads it.
#[derive(Deserialize)]
struct Frontmatter {
    subsystems: Vec<String>,
}

/// The `subsystems:` of the as-built doc `name`, whose text is `text`.
fn subsystems(name: &str, text: &str) -> Vec<String> {
    let document = Document::markdown(text);
    let yaml: String = document
        .blocks
        .iter()
        .find(|block| block.kind == BlockKind::Frontmatter)
        .and_then(|block| match &block.body {
            Body::Raw(pieces) => Some(pieces.iter().map(|piece| piece.text.as_ref()).collect()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} has no frontmatter"));
    let frontmatter: Frontmatter = serde_saphyr::from_str(&yaml)
        .unwrap_or_else(|error| panic!("{name} has no list of subsystems: {error}"));
    frontmatter.subsystems
}

/// The modules under `src`: each directory, and each file but `lib.rs` and `main.rs`.
fn modules(src: &Path) -> Vec<String> {
    let mut modules = Vec::new();
    for entry in fs::read_dir(src).expect("src/ is readable") {
        let path = entry.expect("src/ is readable").path();
        let name = path.file_name().expect("a name").to_string_lossy();
        let module = if path.is_dir() {
            Some(name.as_ref())
        } else {
            name.strip_suffix(".rs")
                .filter(|stem| !matches!(*stem, "lib" | "main"))
        };
        modules.extend(module.map(str::to_string));
    }
    modules.sort();
    modules
}

#[test]
fn each_module_is_named_in_one_asbuilt_doc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));

    let mut docs = fs::read_dir(root.join("docs/design"))
        .expect("docs/design/ is readable")
        .map(|entry| entry.expect("docs/design/ is readable").path())
        .filter(|path| path.to_string_lossy().ends_with(".asbuilt.md"))
        .collect::<Vec<_>>();
    docs.sort();

    // Each subsystem, and the docs that name it.
    let mut named: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in &docs {
        let name = path.file_name().expect("a name").to_string_lossy();
        let text = fs::read_to_string(path).expect("an as-built doc is readable");
        for subsystem in subsystems(&name, &text) {
            named.entry(subsystem).or_default().push(name.to_string());
        }
    }

    let wrong = modules(&root.join("src"))
        .into_iter()
        .filter_map(|module| match named.get(&module).map(Vec::as_slice) {
            Some([_]) => None,
            None | Some([]) => Some(format!("{module}: in no doc")),
            Some(names) => Some(format!("{module}: in {}", names.join(", "))),
        })
        .collect::<Vec<_>>();
    assert!(
        wrong.is_empty(),
        "each module under src/ must be named in the subsystems: of exactly one \
         docs/design/*.asbuilt.md, the one that describes it:\n{}",
        wrong.join("\n")
    );
}
