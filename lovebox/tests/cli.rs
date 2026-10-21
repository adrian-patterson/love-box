//! Exercise the shipped binary, including filesystem input/output and argument failures.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "lovebox-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lovebox"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn png(path: &Path, white: bool) {
    let img = image::open(path).unwrap().to_luma8();
    assert_eq!(img.dimensions(), (800, 480));
    assert_eq!(img.pixels().all(|p| p.0[0] == 255), white);
    let bytes = fs::read(path).unwrap();
    assert_eq!(bytes[24], 1, "PNG bit depth");
}
fn success(out: Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn all_card_commands_render_and_quote_file_matches_inline() {
    let dir = Scratch::new();
    success(dir.run(&[
        "render",
        "message",
        "--size",
        "48",
        "--font",
        "lora",
        "Good morning",
    ]));
    png(&dir.0.join("out/frame.png"), false);
    success(dir.run(&[
        "render",
        "quote",
        "Hold fast to dreams.",
        "--author",
        "A reader",
        "--align",
        "center",
        "--font",
        "lora",
        "-o",
        "inline.png",
    ]));
    fs::write(dir.0.join("quote.txt"), "Hold fast to dreams.").unwrap();
    success(dir.run(&[
        "render",
        "quote",
        "--file",
        "quote.txt",
        "--author",
        "A reader",
        "--align",
        "center",
        "--font",
        "lora",
        "-o",
        "from-file.png",
    ]));
    assert_eq!(
        fs::read(dir.0.join("inline.png")).unwrap(),
        fs::read(dir.0.join("from-file.png")).unwrap()
    );
    png(&dir.0.join("inline.png"), false);
    let image = image::RgbImage::from_fn(320, 240, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 100])
    });
    image.save(dir.0.join("input.png")).unwrap();
    success(dir.run(&[
        "render",
        "photo",
        "input.png",
        "--dither",
        "atkinson",
        "--focal",
        "0.5,0.3",
        "--no-enhance",
        "--keep-copy",
        "library/copy.jpg",
        "--max-alloc-mib",
        "160",
        "-o",
        "photo.png",
    ]));
    png(&dir.0.join("photo.png"), false);
    assert_eq!(
        image::image_dimensions(dir.0.join("library/copy.jpg")).unwrap(),
        (320, 240)
    );
    success(dir.run(&["render", "clear", "-o", "clear.png"]));
    png(&dir.0.join("clear.png"), true);
    success(dir.run(&["render", "message", "clear", "-o", "clear-message.png"]));
    png(&dir.0.join("clear-message.png"), true);
}

#[test]
fn fonts_and_client_errors_are_actionable() {
    let dir = Scratch::new();
    let out = dir.run(&["fonts"]);
    assert!(out.status.success());
    let fonts = String::from_utf8(out.stdout).unwrap();
    assert!(fonts.lines().count() >= 4);
    assert!(fonts.contains("eb-garamond\tEB Garamond"));
    let out = dir.run(&["render", "quote", &"a".repeat(2001)]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("maximum 2000"));
    assert!(!dir.0.join("out/frame.png").exists());
    for args in [
        vec!["render", "message", "--size", "40", "hi"],
        vec!["render", "quote"],
        vec!["render", "quote", "text", "--file", "q.txt"],
        vec!["render", "photo", "x", "--focal", "NaN,0.5"],
        vec!["render", "photo", "x", "--focal", "1.1,0.5"],
        vec!["render", "message", "hi", "--font", "missing"],
        vec!["render", "photo", "missing.jpg"],
    ] {
        assert!(!dir.run(&args).status.success(), "{args:?}");
    }
}
