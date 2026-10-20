//! M1 spike: RaTeX against latex-rust as the KaTeX replacement.
//!
//! `cargo run --release -- OUT_DIR` renders every symbol-palette entry and the formulas in
//! formulas.txt with both, times them, and writes PNGs of the formulas for a side-by-side look.

use std::time::Instant;

const SYMBOLS: &str = include_str!("../symbols.txt");
const FORMULAS: &str = include_str!("../formulas.txt");
/// Preamble macros arrive as `\newcommand` definitions in the source (writing/math.ts `collectMacros`).
const MACRO: &str = r"\def\R{\mathbb{R}}\def\norm#1{\left\lVert #1 \right\rVert} f: \R \to \R, \norm{x}";

fn ratex(tex: &str, display: bool) -> Result<Vec<u8>, String> {
    use ratex_layout::{LayoutOptions, layout, to_display_list, validate_layout};
    use ratex_types::math_style::MathStyle;
    let ast = ratex_parser::parse(tex).map_err(|e| e.to_string())?;
    let style = if display { MathStyle::Display } else { MathStyle::Text };
    let lbox = layout(&ast, &LayoutOptions::default().with_style(style));
    validate_layout(&lbox).map_err(|e| e.to_string())?;
    let list = to_display_list(&lbox);
    let opts = ratex_render::RenderOptions { font_size: 24.0, padding: 6.0, device_pixel_ratio: 2.0, ..Default::default() };
    ratex_render::render_to_png(&list, &opts)
}

fn latex_rust(font: &latex_rust::MathFont, tex: &str, display: bool) -> Result<Vec<u8>, String> {
    let mut opts = latex_rust::PngOptions::new();
    opts.display = display;
    opts.dpi = latex_rust::Dim::from_i64(192);
    opts.font_size_pt = latex_rust::Dim::from_i64(18);
    latex_rust::latex_to_png(tex, font, &opts).map_err(|e| e.to_string())
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("OUT_DIR"));
    for d in ["ratex", "latex-rust"] {
        std::fs::create_dir_all(out.join(d)).unwrap();
    }
    let t = Instant::now();
    let font = latex_rust::MathFont::stix_two_math().unwrap();
    println!("latex-rust font load: {:.1} ms", t.elapsed().as_secs_f64() * 1e3);

    let symbols: Vec<&str> = SYMBOLS.lines().filter(|l| !l.is_empty()).collect();
    let formulas: Vec<&str> = FORMULAS.lines().filter(|l| !l.is_empty()).collect();

    for (engine, run) in [
        ("ratex", &(|tex: &str, d: bool| ratex(tex, d)) as &dyn Fn(&str, bool) -> Result<Vec<u8>, String>),
        ("latex-rust", &|tex: &str, d: bool| latex_rust(&font, tex, d)),
    ] {
        let t = Instant::now();
        let mut fails = Vec::new();
        for s in &symbols {
            if let Err(e) = run(s, false) {
                fails.push(format!("{s}: {}", e.chars().take(70).collect::<String>()));
            }
        }
        let sym_time = t.elapsed().as_secs_f64() * 1e3 / symbols.len() as f64;
        println!("\n== {engine}: symbols {}/{} ok, {:.2} ms each", symbols.len() - fails.len(), symbols.len(), sym_time);
        for f in fails.iter().take(40) {
            println!("   fail {f}");
        }
        let mut ok = 0;
        let t = Instant::now();
        for (i, f) in formulas.iter().enumerate() {
            match run(f, true) {
                Ok(png) => {
                    ok += 1;
                    std::fs::write(out.join(engine).join(format!("{:02}.png", i + 1)), png).unwrap();
                }
                Err(e) => println!("   formula {} fail: {}", i + 1, e.chars().take(90).collect::<String>()),
            }
        }
        println!("   formulas {ok}/{} ok, {:.2} ms each", formulas.len(), t.elapsed().as_secs_f64() * 1e3 / formulas.len() as f64);
        match run(MACRO, false) {
            Ok(png) => {
                std::fs::write(out.join(engine).join("macro.png"), png).unwrap();
                println!("   \\newcommand: ok");
            }
            Err(e) => println!("   \\newcommand: {}", e.chars().take(90).collect::<String>()),
        }
    }
}
