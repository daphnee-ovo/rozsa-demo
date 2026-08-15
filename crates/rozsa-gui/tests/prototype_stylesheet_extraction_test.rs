// FrameworkTree
// prototype_stylesheet_extraction_test.rs
// ├── component_files_reconstruct_the_original_stylesheet_byte_for_byte()
// ├── extracted_inline_styles_reconstruct_the_original_html_byte_for_byte()
// ├── stylesheet_entries_are_external_acyclic_and_runtime_shaped()
// ├── repository_root()
// └── sha256_hex()

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const SOURCE_ORDER: &str = include_str!("../../../docs/gui/new-version/styles/source-order.json");

#[test]
fn component_files_reconstruct_the_original_stylesheet_byte_for_byte() {
    let manifest: Value = serde_json::from_str(SOURCE_ORDER).unwrap();
    let blocks = manifest["blocks"].as_array().unwrap();
    let mut reconstructed = Vec::new();
    let mut expected_start = 1_u64;
    let mut expected_imports = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        assert_eq!(block["order"].as_u64(), Some(index as u64 + 1));
        let range = block["source_lines"].as_array().unwrap();
        let start = range[0].as_u64().unwrap();
        let end = range[1].as_u64().unwrap();
        assert_eq!(start, expected_start, "CSS block 之间出现空洞或重叠");
        expected_start = end + 1;

        let target = block["target"].as_str().unwrap();
        let content = fs::read(repository_root().join(target)).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&content).lines().count() as u64,
            end - start + 1,
            "{target} 的行数与原始连续块不一致"
        );
        assert_eq!(
            sha256_hex(&content),
            block["sha256"].as_str().unwrap(),
            "{target} 的内容已偏离已审核的原始块"
        );
        assert!(
            !String::from_utf8_lossy(&content).contains("@import"),
            "组件文件不得形成隐藏 import 图：{target}"
        );
        reconstructed.extend_from_slice(&content);
        expected_imports.push(format!(
            "@import url(\"./{}\");",
            target.strip_prefix("docs/gui/new-version/styles/").unwrap()
        ));
    }

    let original = &manifest["original_stylesheet"];
    assert_eq!(expected_start - 1, original["line_count"].as_u64().unwrap());
    assert_eq!(
        sha256_hex(&reconstructed),
        original["sha256"].as_str().unwrap(),
        "有原始 rule block 丢失、重复、改写或跨块重排"
    );

    let main =
        fs::read_to_string(repository_root().join("docs/gui/new-version/styles/main.css")).unwrap();
    assert_eq!(main.lines().collect::<Vec<_>>(), expected_imports);
    assert!(
        !repository_root()
            .join("docs/gui/new-version/rozsa-gui.css")
            .exists(),
        "不得保留单体 CSS 或兼容入口"
    );
}

#[test]
fn extracted_inline_styles_reconstruct_the_original_html_byte_for_byte() {
    let manifest: Value = serde_json::from_str(SOURCE_ORDER).unwrap();

    for extraction in manifest["extracted_inline_styles"].as_array().unwrap() {
        let html_path = extraction["html"].as_str().unwrap();
        let css_path = extraction["target"].as_str().unwrap();
        let href = extraction["href"].as_str().unwrap();
        let html = fs::read_to_string(repository_root().join(html_path)).unwrap();
        let css = fs::read_to_string(repository_root().join(css_path)).unwrap();
        let link = format!("<link rel=\"stylesheet\" href=\"{href}\">");

        assert!(
            !html.contains("<style>"),
            "{html_path} 仍包含稳定 inline CSS"
        );
        assert_eq!(
            html.matches(&link).count(),
            1,
            "{html_path} 的外置入口不唯一"
        );
        assert_eq!(
            sha256_hex(css.as_bytes()),
            extraction["css_sha256"].as_str().unwrap(),
            "{css_path} 与原 inline style 不一致"
        );

        let direct_entry_reverted = html.replace("../styles/main.css", "../rozsa-gui.css");
        let reconstructed = direct_entry_reverted.replace(
            &link,
            &format!("<style>\n{}\n</style>", css.trim_end_matches('\n')),
        );
        let reconstructed_with_indented_close = direct_entry_reverted.replace(
            &link,
            &format!("<style>\n{}</style>", css.trim_end_matches('\n')),
        );
        let original_hash = extraction["original_html_sha256"].as_str().unwrap();
        assert!(
            sha256_hex(reconstructed.as_bytes()) == original_hash
                || sha256_hex(reconstructed.trim_end_matches('\n').as_bytes()) == original_hash
                || sha256_hex(reconstructed_with_indented_close.as_bytes()) == original_hash
                || sha256_hex(
                    reconstructed_with_indented_close
                        .trim_end_matches('\n')
                        .as_bytes(),
                ) == original_hash,
            "{html_path} 除入口替换、CSS 外置或末尾换行以外还发生了其他变化"
        );
    }
}

#[test]
fn stylesheet_entries_are_external_acyclic_and_runtime_shaped() {
    let root = repository_root();
    let main = fs::read_to_string(root.join("docs/gui/new-version/styles/main.css")).unwrap();
    let sidebar = fs::read_to_string(root.join("docs/gui/new-version/styles/sidebar.css")).unwrap();

    assert!(!main.contains("main.css"));
    assert!(
        !main
            .lines()
            .any(|line| line == "@import url(\"./sidebar.css\");")
    );
    assert!(!sidebar.contains("main.css"));
    assert!(
        !sidebar
            .lines()
            .any(|line| line == "@import url(\"./sidebar.css\");")
    );
    for required in [
        "tokens.css",
        "reset.css",
        "base.css",
        "layout/app-shell.css",
        "layout/sidebar-shell.css",
        "features/sidebar.css",
        "features/sidebar-window.css",
        "utilities.css",
    ] {
        assert!(sidebar.contains(required), "sidebar.css 缺少 {required}");
    }

    for html in [
        "docs/gui/new-version/scenes/ask-user-question.html",
        "docs/gui/new-version/scenes/complete-session.html",
        "docs/gui/new-version/scenes/dev-flow-runtime.html",
        "docs/gui/new-version/rozsa-visual-demo.html",
    ] {
        let source = fs::read_to_string(root.join(html)).unwrap();
        assert!(!source.contains("<style>"), "{html} 仍包含 style 标签");
    }

    for scene in fs::read_dir(root.join("docs/gui/new-version/scenes")).unwrap() {
        let path = scene.unwrap().path();
        if path.extension().and_then(|value| value.to_str()) != Some("html") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        assert!(source.contains("href=\"../styles/main.css\""));
        assert!(!source.contains("href=\"../rozsa-gui.css\""));
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = INITIAL;
    for block in padded.chunks_exact(64) {
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate() {
            words[index] = u32::from_be_bytes(chunk.try_into().unwrap());
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h].into_iter()) {
            *slot = slot.wrapping_add(value);
        }
    }

    state.iter().map(|value| format!("{value:08x}")).collect()
}
