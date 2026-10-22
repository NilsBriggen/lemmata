//! How long the walks after every save take on a thesis-sized project:
//! `cargo test --release -p lemmata-bib --test scale -- --ignored --nocapture`

use std::time::Instant;

#[test]
#[ignore]
fn walks_after_a_save_on_a_big_project() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    let mut main = String::from(
        "\\documentclass{book}\n\\usepackage{biblatex}\n\\addbibresource{refs.bib}\n\\begin{document}\n",
    );
    std::fs::create_dir(root.join("chapters")).unwrap();
    for c in 0..60 {
        main.push_str(&format!("\\input{{chapters/c{c}}}\n"));
        let mut ch = format!("\\chapter{{Chapter {c}}}\\label{{ch:{c}}}\n");
        for l in 0..400 {
            ch.push_str(&match l % 40 {
                0 => format!("\\section{{S {c}.{l}}}\\label{{s:{c}:{l}}}\n"),
                7 => format!(
                    "As shown in \\cref{{s:{c}:0}} and by \\cite{{k{}}}, it holds.\n",
                    (c * 50 + l) % 3000
                ),
                _ => {
                    "Some prose of a thesis, with words enough to count and a bit of $x^2$ maths.\n"
                        .into()
                }
            });
        }
        std::fs::write(root.join(format!("chapters/c{c}.tex")), ch).unwrap();
    }
    main.push_str("\\printbibliography\n\\end{document}\n");
    std::fs::write(root.join("main.tex"), main).unwrap();
    let bib: String = (0..3000)
        .map(|k| {
            format!(
                "@article{{k{k},\n  author = {{A. Author and B. Other}},\n  title = {{Title number {k}}},\n  journal = {{J}},\n  year = {{2001}},\n  doi = {{10.1/{k}}}\n}}\n\n"
            )
        })
        .collect();
    std::fs::write(root.join("refs.bib"), bib).unwrap();

    let p = lemmata_project::Project::open(root).unwrap();
    let time = |name: &str, f: &dyn Fn()| {
        f();
        let t = Instant::now();
        for _ in 0..5 {
            f();
        }
        eprintln!("{name}: {:?}", t.elapsed() / 5);
    };
    time("open", &|| {
        drop(lemmata_project::Project::open(root).unwrap())
    });
    time("outline", &|| drop(lemmata_project::outline(&p)));
    time("labels", &|| drop(lemmata_project::extras::labels(&p)));
    time("counts", &|| drop(lemmata_project::extras::counts(&p)));
    time("symbols", &|| drop(lemmata_project::symbols::scan(&p)));
    time("tree", &|| {
        drop(lemmata_project::tree(root, &p.config.outdir))
    });
    time("bib entries", &|| {
        let files = lemmata_bib::bib_files(&p);
        drop(lemmata_bib::entries(&p.root, &files));
    });
    time("cited keys", &|| drop(lemmata_bib::cited_keys(&p)));
}
