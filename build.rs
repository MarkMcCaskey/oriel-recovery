use std::{env, fs, path::Path};
fn walk(root: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out);
        } else {
            out.push(p.to_str().unwrap().replace('\\', "/"));
        }
    }
}
fn main() {
    println!("cargo:rerun-if-changed=archive");
    let mut paths = vec![];
    walk(Path::new("archive"), &mut paths);
    let mut s = String::from("pub const ARCHIVE: &[(&str, &[u8])] = &[\n");
    let root = env::var("CARGO_MANIFEST_DIR").unwrap();
    for p in paths {
        s.push_str(&format!(
            "({p:?}, include_bytes!({:?})),\n",
            format!("{root}/{p}")
        ));
    }
    s.push_str("];\n");
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("archive.rs"),
        s,
    )
    .unwrap();
}
