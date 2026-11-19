// Provenance: Git's documented core.autocrlf and gitattributes checkout behavior.
use std::{fs, path::Path, process::Command, time::SystemTime};

struct Checkout(std::path::PathBuf);

impl Drop for Checkout {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-c")
        .arg("core.autocrlf=true")
        .arg("-c")
        .arg("core.safecrlf=false")
        .arg("-c")
        .arg("core.attributesfile=")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run Git for the example checkout test");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn all_example_nc_files_keep_lf_with_autocrlf() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let tracked = git(root, &["ls-files", "-z", "examples"]);
    let programs: Vec<_> = std::str::from_utf8(&tracked)
        .unwrap()
        .split('\0')
        .filter(|path| path.ends_with(".nc"))
        .collect();
    assert!(!programs.is_empty(), "no tracked example NC programs");

    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let checkout = Checkout(std::env::temp_dir().join(format!(
        "camcraft-example-lf-{}-{stamp}",
        std::process::id()
    )));
    fs::create_dir(&checkout.0).unwrap();
    git(&checkout.0, &["init", "--quiet", "--template="]);
    fs::copy(
        root.join(".gitattributes"),
        checkout.0.join(".gitattributes"),
    )
    .unwrap();
    for program in &programs {
        let target = checkout.0.join(program);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(root.join(program), &target).unwrap();
    }
    git(&checkout.0, &["add", "."]);
    for program in &programs {
        fs::remove_file(checkout.0.join(program)).unwrap();
    }
    git(&checkout.0, &["checkout-index", "--all", "--force"]);

    let mut changed = Vec::new();
    for program in programs {
        let actual = fs::read(checkout.0.join(program)).unwrap();
        let indexed = git(&checkout.0, &["show", &format!(":{program}")]);
        if actual.contains(&b'\r') || actual != indexed {
            changed.push(program);
        }
    }
    assert!(
        changed.is_empty(),
        "example NC checkout must preserve LF bytes with core.autocrlf=true: {changed:?}"
    );
}
