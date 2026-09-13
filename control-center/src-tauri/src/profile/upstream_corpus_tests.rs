use super::{hash, ProfileDocument, TextEncoding};
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const CORPUS: &str = "tests/fixtures/mactype-ini-luantu-f3e926f";

fn collect_ini_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).expect("fixture directory should be readable") {
        let path = entry.expect("fixture entry should be readable").path();
        if path.is_dir() {
            collect_ini_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "ini") {
            files.push(path);
        }
    }
}

#[test]
fn real_world_profile_corpus_round_trips_with_the_expected_encoding() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
    let mut files = Vec::new();
    collect_ini_files(&corpus, &mut files);
    files.sort();
    assert_eq!(
        files.len(),
        70,
        "the pinned corpus must not change silently"
    );

    let legacy_gb18030 = [
        "CRT.ini",
        "CandyTypeSharpFix.ini",
        "LCD.ini",
        "luantu - 副本.ini",
        "luantu.ini",
        "new.ini",
    ];
    let expected_hashes = [
        (
            "CRT.ini",
            "4fd96a61c16a3ac463b08298aa2f69b62859a86a9a457c3abf861c9bf24601fe",
        ),
        (
            "CandyTypeSharpFix.ini",
            "6e53d3424f08f71f2b8c94e7dd5f9bf3072bd28bffaa23141514d5bf9ccaa2b6",
        ),
        (
            "LCD.ini",
            "6a0bf9dfcbce4967be5b8ec7f53f30a8e4d4ebe7857eecca586b71c7e581a67b",
        ),
        (
            "luantu - 副本.ini",
            "5d027c6d5abdb1546c589196a93649d05e863549c0dab2f94e275248c942369a",
        ),
        (
            "luantu.ini",
            "ff4c14e3f4fe10ad96351f5be315b6fdd3b5be6a8faa2eba838ccff82f639135",
        ),
        (
            "new.ini",
            "b21cedc1fa066262ee425591b666b0aa644be6df5e2c76d9798df65b1e96a91f",
        ),
    ];

    for path in files {
        let original = fs::read(&path).expect("fixture should be readable");
        let document = ProfileDocument::open(&path).expect("fixture should decode");
        assert_eq!(
            document.encoded().expect("fixture should encode"),
            original,
            "unchanged profile did not round-trip: {}",
            path.display()
        );

        let relative = path
            .strip_prefix(&corpus)
            .expect("fixture should be under the corpus")
            .to_string_lossy()
            .replace('\\', "/");
        if legacy_gb18030.contains(&relative.as_str()) {
            let actual_hash = hash(&original)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let expected_hash = expected_hashes
                .iter()
                .find_map(|(name, expected)| (*name == relative).then_some(*expected))
                .expect("every legacy fixture should have a pinned hash");
            assert_eq!(actual_hash, expected_hash, "fixture changed: {relative}");
            assert_eq!(
                document.snapshot().encoding,
                TextEncoding::Gb18030,
                "legacy Chinese profile was decoded with the wrong codec: {relative}"
            );
        }
    }
}

#[test]
fn legacy_chinese_profiles_survive_cjk_edits_and_reopening() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
    let legacy_gb18030 = [
        "CRT.ini",
        "CandyTypeSharpFix.ini",
        "LCD.ini",
        "luantu - 副本.ini",
        "luantu.ini",
        "new.ini",
    ];
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "mactype-legacy-corpus-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&directory).expect("temporary fixture directory should be created");

    for file_name in legacy_gb18030 {
        let path = directory.join(file_name);
        fs::copy(corpus.join(file_name), &path).expect("fixture should be copied");
        let mut document = ProfileDocument::open(&path).expect("fixture should decode");
        document
            .set_list(
                "excludeFonts",
                vec!["微软雅黑".to_owned(), "宋体".to_owned()],
            )
            .expect("CJK font list should be editable");
        document.save().expect("edited fixture should save");

        let reopened = ProfileDocument::open(&path).expect("saved fixture should reopen");
        let snapshot = reopened.snapshot();
        assert_eq!(snapshot.encoding, TextEncoding::Gb18030, "{file_name}");
        assert_eq!(
            snapshot.lists.exclude_fonts,
            vec!["微软雅黑".to_owned(), "宋体".to_owned()],
            "CJK font names changed after round-trip: {file_name}"
        );
        assert_eq!(
            reopened.encoded().expect("reopened fixture should encode"),
            fs::read(&path).expect("saved fixture should be readable"),
            "saved bytes did not stabilize after reopening: {file_name}"
        );
    }

    fs::remove_dir_all(directory).expect("temporary fixture directory should be removed");
}
