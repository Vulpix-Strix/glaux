use super::fixture::Fixture;
use super::*;
use pretty_assertions::assert_eq;

fn names(listing: &[DirEntry]) -> Vec<String> {
    listing
        .iter()
        .map(|entry| match entry.kind {
            EntryKind::Directory => format!("{}/", entry.name),
            EntryKind::File => entry.name.clone(),
        })
        .collect()
}

fn ls(fs: &CartridgeFs, path: Option<&str>) -> Vec<String> {
    names(&fs.list(path).unwrap())
}

fn not_found(path: &str) -> FsError {
    FsError::NotFound(path.into())
}

#[test]
fn open_creates_the_cartridge_directory() {
    let fixture = Fixture::new("open-creates");
    let data = fixture.base().join("fresh");
    let fs = CartridgeFs::open(&data).unwrap();
    assert!(data.join(CARTRIDGE_DIR).is_dir());
    assert_eq!(fs.list(None).unwrap(), []);
}

#[test]
fn storage_without_a_data_directory_is_unavailable() {
    assert_eq!(CartridgeStorage::open(None), CartridgeStorage::Unavailable);
}

#[test]
fn storage_that_cannot_be_created_is_unavailable() {
    let fixture = Fixture::new("storage-blocked");
    let blocker = fixture.base().join("not-a-dir");
    fs::write(&blocker, "").unwrap();
    assert_eq!(
        CartridgeStorage::open(Some(&blocker)),
        CartridgeStorage::Unavailable
    );
}

#[test]
fn storage_opens_at_root() {
    let fixture = Fixture::new("storage-open");
    let storage = CartridgeStorage::open(Some(&fixture.data_dir()));
    assert_eq!(storage, fixture.storage());
    assert_eq!(storage.cwd(), Ok("/".into()));
}

#[test]
fn unavailable_storage_refuses_every_request() {
    let mut storage = CartridgeStorage::Unavailable;
    assert_eq!(storage.cwd(), Err(FsError::Unavailable));
    assert_eq!(storage.list(None), Err(FsError::Unavailable));
    assert_eq!(storage.change_dir("/"), Err(FsError::Unavailable));
    assert_eq!(
        FsError::Unavailable.to_string(),
        "Cartridge storage unavailable"
    );
}

#[test]
fn cd_relative_and_absolute() {
    let fixture = Fixture::new("cd-paths");
    let mut fs = fixture.open();
    fs.change_dir("games").unwrap();
    fs.change_dir("owl").unwrap();
    assert_eq!(fs.cwd(), "/games/owl");
    fs.change_dir("/tools").unwrap();
    assert_eq!(fs.cwd(), "/tools");
    fs.change_dir("/games/owl/").unwrap();
    assert_eq!(fs.cwd(), "/games/owl");
}

#[test]
fn dot_and_dot_dot() {
    let fixture = Fixture::new("dots");
    let mut fs = fixture.open();
    fs.change_dir("games/./owl/..").unwrap();
    assert_eq!(fs.cwd(), "/games");
    fs.change_dir(".").unwrap();
    assert_eq!(fs.cwd(), "/games");
    fs.change_dir("..").unwrap();
    assert_eq!(fs.cwd(), "/");
    fs.change_dir("games//owl").unwrap();
    assert_eq!(fs.cwd(), "/games/owl");
}

#[test]
fn dot_dot_never_leaves_root() {
    let fixture = Fixture::new("above-root");
    let mut fs = fixture.open();
    fs.change_dir("..").unwrap();
    assert_eq!(fs.cwd(), "/");
    fs.change_dir("../../..").unwrap();
    assert_eq!(fs.cwd(), "/");
    fs.change_dir("/../games").unwrap();
    assert_eq!(fs.cwd(), "/games");
    fs.change_dir("../../../tools").unwrap();
    assert_eq!(fs.cwd(), "/tools");
    assert_eq!(ls(&fs, Some("../../..")), ls(&fs, Some("/")));
    // The host directory above the root is not reachable by name either.
    assert_eq!(fs.change_dir("/../outside"), Err(not_found("/outside")));
}

#[test]
fn listing_puts_directories_first_and_ignores_case() {
    let fixture = Fixture::new("listing");
    let mut fs = fixture.open();
    let expected = ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"];
    assert_eq!(ls(&fs, None), expected);
    assert_eq!(ls(&fs, Some("/")), expected);
    assert_eq!(ls(&fs, Some("games")), ["owl/"]);
    assert_eq!(ls(&fs, Some("games/owl")), Vec::<String>::new());

    fs.change_dir("games").unwrap();
    assert_eq!(ls(&fs, Some("..")), expected);
    assert_eq!(fs.cwd(), "/games", "ls does not move");
}

#[test]
fn listing_order_is_total() {
    let fixture = Fixture::new("listing-ties");
    let fs = fixture.open();
    for name in ["readme", "README", "Readme"] {
        fs::write(fixture.root().join("tools").join(name), "").unwrap();
    }
    // Case-insensitive ties fall back to the exact name.
    assert_eq!(ls(&fs, Some("tools")), ["README", "Readme", "readme"]);
}

#[test]
fn nonexistent_paths_are_not_found() {
    let fixture = Fixture::new("missing");
    let mut fs = fixture.open();
    fs.change_dir("games").unwrap();
    assert_eq!(fs.change_dir("nope"), Err(not_found("/games/nope")));
    assert_eq!(
        fs.change_dir("/nope/deeper"),
        Err(not_found("/nope/deeper"))
    );
    assert_eq!(fs.list(Some("nope")), Err(not_found("/games/nope")));
    assert_eq!(fs.cwd(), "/games");
}

#[test]
fn files_are_not_directories() {
    let fixture = Fixture::new("files");
    let mut fs = fixture.open();
    let err = FsError::NotADirectory("/b.txt".into());
    assert_eq!(fs.change_dir("b.txt"), Err(err.clone()));
    assert_eq!(fs.list(Some("/b.txt")), Err(err));
    assert_eq!(fs.change_dir("b.txt/x"), Err(not_found("/b.txt/x")));
    assert_eq!(fs.cwd(), "/");
}

#[test]
fn names_with_host_separators_are_not_found() {
    let fixture = Fixture::new("separators");
    let mut fs = fixture.open();
    assert_eq!(fs.change_dir(r"games\owl"), Err(not_found(r"/games\owl")));
    assert_eq!(fs.change_dir(r"..\.."), Err(not_found(r"/..\..")));
    assert_eq!(fs.cwd(), "/");
}

#[test]
fn errors_show_virtual_paths() {
    let err = FsError::NotFound("/games/nope".into());
    assert_eq!(err.to_string(), "No such file or directory: /games/nope");
    let err = FsError::NotADirectory("/b.txt".into());
    assert_eq!(err.to_string(), "Not a directory: /b.txt");
}

#[test]
fn subdir_is_rooted_at_the_directory() {
    let fixture = Fixture::new("subdir");
    fs::write(fixture.root().join("games/owl/cart.json"), "{}").unwrap();
    let mut fs = fixture.open();
    fs.change_dir("games").unwrap();

    let (path, owl) = fs.subdir("owl").unwrap();
    assert_eq!(path, "/games/owl");
    assert_eq!(owl.cwd(), "/");
    assert_eq!(ls(&owl, None), ["cart.json"]);
    // `..` stops at the new root.
    assert_eq!(ls(&owl, Some("../..")), ["cart.json"]);
    assert_eq!(owl.read_file("/../../readme"), Err(not_found("/readme")));

    assert_eq!(fs.subdir("/").map(|(path, _)| path), Ok("/".into()));
    assert_eq!(fs.subdir("nope").map(drop), Err(not_found("/games/nope")));
    assert_eq!(
        fs.subdir("/b.txt").map(drop),
        Err(FsError::NotADirectory("/b.txt".into()))
    );
    assert_eq!(
        CartridgeStorage::Unavailable.subdir("/").map(drop),
        Err(FsError::Unavailable)
    );
}

#[test]
fn real_subdir_is_rooted_at_the_directory() {
    let fixture = Fixture::new("real-subdir");
    fs::write(fixture.root().join("games/owl/cart.json"), "{}").unwrap();
    let fs = fixture.open();

    let games = fs.real_subdir("games").unwrap();
    assert_eq!(ls(&games, None), ["owl/"]);
    assert_eq!(games.read_file("/../readme"), Err(not_found("/readme")));
    assert_eq!(fs.real_subdir("nope").map(drop), Err(not_found("/nope")));
    assert_eq!(
        fs.real_subdir("b.txt").map(drop),
        Err(FsError::NotADirectory("/b.txt".into()))
    );
    for name in ["", ".", "..", "games/owl", r"games\owl"] {
        assert!(
            matches!(fs.real_subdir(name), Err(FsError::NotFound(_))),
            "{name:?}"
        );
    }
}

#[test]
fn files_are_read_only_when_regular() {
    let fixture = Fixture::new("read-file");
    fs::write(fixture.root().join("tools/note"), "hello").unwrap();
    let fs = fixture.open();

    assert_eq!(fs.read_file("tools/note"), Ok(b"hello".to_vec()));
    assert_eq!(fs.ensure_file("/tools/note"), Ok(()));
    let not_a_file = FsError::NotAFile("/tools".into());
    assert_eq!(fs.read_file("tools"), Err(not_a_file.clone()));
    assert_eq!(fs.ensure_file("tools"), Err(not_a_file));
    assert_eq!(fs.read_file("/"), Err(FsError::NotAFile("/".into())));
    assert_eq!(fs.read_file("tools/nope"), Err(not_found("/tools/nope")));
    assert_eq!(fs.ensure_file("b.txt/x"), Err(not_found("/b.txt/x")));
    assert_eq!(not_found("/x").to_string(), "No such file or directory: /x");
    assert_eq!(FsError::NotAFile("/x".into()).to_string(), "Not a file: /x");
}

const TREE: &[(&str, &[u8])] = &[("top.txt", b"top"), ("code/main.luau", b"main")];

/// Creates `name` holding [`TREE`] in `fs`, accepting it unchecked.
fn create(fs: &CartridgeFs, name: &str) -> Result<String, CreateDirError<()>> {
    fs.create_dir_with(name, TREE, |_| Ok(()))
}

/// Every entry name directly in the host directory `dir`, sorted.
fn host_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn create_dir_with_publishes_the_tree_in_the_current_directory() {
    let fixture = Fixture::new("create-dir");
    let mut fs = fixture.open();
    fs.change_dir("games").unwrap();
    let mut seen = None;
    let created = fs.create_dir_with("new", TREE, |staged| {
        seen = Some(staged.read_file("/code/main.luau"));
        Ok::<(), ()>(())
    });
    assert_eq!(created, Ok("/games/new".into()));
    assert_eq!(seen, Some(Ok(b"main".to_vec())));
    assert_eq!(fs.read_file("new/top.txt"), Ok(b"top".to_vec()));
    assert_eq!(fs.read_file("new/code/main.luau"), Ok(b"main".to_vec()));
    assert_eq!(host_names(&fixture.root().join("games")), ["new", "owl"]);
}

#[test]
fn create_dir_with_never_replaces_an_entry() {
    let fixture = Fixture::new("create-dir-exists");
    let fs = fixture.open();
    fs::write(fixture.root().join("games/owl/keep"), "kept").unwrap();
    for name in ["games", "b.txt"] {
        assert_eq!(
            create(&fs, name),
            Err(CreateDirError::Fs(FsError::AlreadyExists(format!(
                "/{name}"
            ))))
        );
    }
    assert_eq!(
        fs::read(fixture.root().join("games/owl/keep")).unwrap(),
        b"kept"
    );
    assert_eq!(fs::read(fixture.root().join("b.txt")).unwrap(), b"");
    assert_eq!(host_names(&fixture.root().join("games")), ["owl"]);
}

#[test]
fn an_entry_appearing_during_creation_is_kept() {
    let fixture = Fixture::new("create-dir-race");
    let fs = fixture.open();
    let target = fixture.root().join("new");
    // Another creator takes the name while the tree is being staged.
    let created = fs.create_dir_with("new", TREE, |_| {
        fs::create_dir(&target).unwrap();
        fs::write(target.join("theirs"), "theirs").unwrap();
        Ok::<(), ()>(())
    });
    assert_eq!(
        created,
        Err(CreateDirError::Fs(FsError::AlreadyExists("/new".into())))
    );
    assert_eq!(host_names(&target), ["theirs"]);
    assert_eq!(fs::read(target.join("theirs")).unwrap(), b"theirs");
    assert!(
        !host_names(&fixture.root())
            .iter()
            .any(|n| n.starts_with(STAGING_PREFIX))
    );
}

#[test]
fn failed_creations_leave_nothing_behind() {
    let fixture = Fixture::new("create-dir-fail");
    let fs = fixture.open();
    let before = host_names(&fixture.root());

    let rejected = fs.create_dir_with("new", TREE, |_| Err("bad"));
    assert_eq!(rejected, Err(CreateDirError::Invalid("bad")));

    // `code` is written as a file, so `code/main.luau` cannot be.
    let clash: &[(&str, &[u8])] = &[("code", b""), ("code/main.luau", b"")];
    assert_eq!(
        fs.create_dir_with("new", clash, |_| Ok::<(), ()>(())),
        Err(CreateDirError::Fs(FsError::Unwritable("/new".into())))
    );
    assert_eq!(host_names(&fixture.root()), before);
}

#[test]
fn create_dir_with_rejects_names_that_are_not_one_component() {
    let fixture = Fixture::new("create-dir-names");
    let fs = fixture.open();
    for name in ["", ".", "..", "a/b", r"a\b", ".glaux-staging-x"] {
        assert!(
            matches!(
                create(&fs, name),
                Err(CreateDirError::Fs(FsError::Unwritable(_)))
            ),
            "{name:?}"
        );
    }
    let escaping: &[(&str, &[u8])] = &[("../out", b"")];
    assert!(matches!(
        fs.create_dir_with("new", escaping, |_| Ok::<(), ()>(())),
        Err(CreateDirError::Fs(FsError::Unwritable(_)))
    ));
    assert!(!fixture.root().join("new").exists());
    assert!(!fixture.root().join("../out").exists());
    assert_eq!(
        CartridgeStorage::Unavailable.create_dir_with("new", TREE, |_| Ok::<(), ()>(())),
        Err(CreateDirError::Fs(FsError::Unavailable))
    );
}

#[test]
fn staging_directories_are_hidden_and_stale_ones_are_swept() {
    let fixture = Fixture::new("create-dir-stale");
    let fs = fixture.open();
    let stale = fixture.root().join(format!("{STAGING_PREFIX}old-1-2-3"));
    let fresh = fixture.root().join(format!("{STAGING_PREFIX}busy-1-2-3"));
    fs::create_dir_all(stale.join("code")).unwrap();
    fs::create_dir(&fresh).unwrap();
    let old = SystemTime::now() - STALE_STAGING - Duration::from_secs(60);
    fs::File::open(&stale).unwrap().set_modified(old).unwrap();
    assert_eq!(
        ls(&fs, None),
        ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"]
    );

    create(&fs, "new").unwrap();
    assert!(!stale.exists());
    assert!(
        fresh.exists(),
        "a recent staging directory may still be in use"
    );
}

/// Names in the host directory `dir` that are staging entries.
fn staged(dir: &Path) -> Vec<String> {
    host_names(dir)
        .into_iter()
        .filter(|name| name.starts_with(STAGING_PREFIX))
        .collect()
}

#[test]
fn replace_file_swaps_contents_that_match_the_expectation() {
    let fixture = Fixture::new("replace");
    let fs = fixture.open();
    fs::write(fixture.root().join("tools/data"), b"old").unwrap();
    assert_eq!(
        fs.replace_file("tools/data", b"new contents", Some(b"old")),
        Ok(())
    );
    assert_eq!(fs.read_file("tools/data"), Ok(b"new contents".to_vec()));
    assert_eq!(staged(&fixture.root().join("tools")), Vec::<String>::new());
}

#[test]
fn replace_file_creates_a_file_expected_to_be_absent() {
    let fixture = Fixture::new("replace-create");
    let fs = fixture.open();
    assert_eq!(fs.replace_file("/tools/fresh", b"hi", None), Ok(()));
    assert_eq!(fs.read_file("/tools/fresh"), Ok(b"hi".to_vec()));
    assert_eq!(
        fs.replace_file("/nope/fresh", b"hi", None),
        Err(ReplaceError::Fs(not_found("/nope")))
    );
}

#[test]
fn replace_file_refuses_contents_that_changed() {
    let fixture = Fixture::new("replace-changed");
    let fs = fixture.open();
    let path = fixture.root().join("tools/data");
    fs::write(&path, b"theirs").unwrap();
    assert_eq!(
        fs.replace_file("tools/data", b"mine", Some(b"old")),
        Err(ReplaceError::Changed(Some(b"theirs".to_vec())))
    );
    // Absent and empty are different states.
    assert_eq!(
        fs.replace_file("tools/data", b"mine", None),
        Err(ReplaceError::Changed(Some(b"theirs".to_vec())))
    );
    fs::write(&path, b"").unwrap();
    assert_eq!(
        fs.replace_file("tools/data", b"mine", None),
        Err(ReplaceError::Changed(Some(Vec::new())))
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        fs.replace_file("tools/data", b"mine", Some(b"")),
        Err(ReplaceError::Changed(None))
    );
    assert!(!path.exists());
    assert_eq!(staged(&fixture.root().join("tools")), Vec::<String>::new());
}

#[test]
fn replace_file_refuses_directories_and_staging_names() {
    let fixture = Fixture::new("replace-kinds");
    let fs = fixture.open();
    assert_eq!(
        fs.replace_file("tools", b"x", None),
        Err(ReplaceError::Fs(FsError::NotAFile("/tools".into())))
    );
    assert_eq!(
        fs.replace_file("/", b"x", None),
        Err(ReplaceError::Fs(FsError::NotAFile("/".into())))
    );
    let staging = format!("{STAGING_PREFIX}x");
    assert!(matches!(
        fs.replace_file(&staging, b"x", None),
        Err(ReplaceError::Fs(FsError::Unwritable(_)))
    ));
    assert!(!fixture.root().join(staging).exists());
}

#[test]
fn stale_staging_files_are_swept_by_the_next_replace() {
    let fixture = Fixture::new("replace-stale");
    let fs = fixture.open();
    let stale = fixture.root().join(format!("{STAGING_PREFIX}data-1-2-3"));
    let fresh = fixture.root().join(format!("{STAGING_PREFIX}data-4-5-6"));
    fs::write(&stale, b"").unwrap();
    fs::write(&fresh, b"").unwrap();
    let old = SystemTime::now() - STALE_STAGING - Duration::from_secs(60);
    fs::File::options()
        .write(true)
        .open(&stale)
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert!(!ls(&fs, None).iter().any(|n| n.starts_with(STAGING_PREFIX)));

    fs.replace_file("data", b"x", None).unwrap();
    assert!(!stale.exists());
    assert!(fresh.exists());
}

const WALK: WalkLimits = WalkLimits {
    max_depth: 16,
    max_files: 100,
};

fn walk(fs: &CartridgeFs, limits: WalkLimits) -> Walk {
    fs.walk_files(limits, |_| true).unwrap()
}

/// Writes empty files at each of `paths` below the fixture's root.
fn touch(fixture: &Fixture, paths: &[&str]) {
    for path in paths {
        let host = fixture.root().join(path);
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(host, "").unwrap();
    }
}

#[test]
fn walking_finds_files_in_every_directory() {
    let fixture = Fixture::new("walk-all");
    touch(&fixture, &["games/owl/code/main.luau", "tools/x/y/z.txt"]);
    let found = walk(&fixture.open(), WALK);
    assert_eq!(
        found.files,
        [
            "games/owl/code/main.luau",
            "tools/x/y/z.txt",
            "A.txt",
            "b.txt",
            "readme",
        ]
    );
    assert_eq!(found.gaps, []);
}

#[test]
fn walking_keeps_only_what_the_filter_accepts() {
    let fixture = Fixture::new("walk-filter");
    touch(&fixture, &["tools/a.luau", "tools/b.png"]);
    let found = fixture
        .open()
        .walk_files(WALK, |path| path.ends_with(".luau"))
        .unwrap();
    assert_eq!(found.files, ["tools/a.luau"]);
}

#[test]
fn walking_reports_where_the_file_limit_stopped_it() {
    let fixture = Fixture::new("walk-file-limit");
    let limits = WalkLimits {
        max_files: 2,
        ..WALK
    };
    let found = walk(&fixture.open(), limits);
    assert_eq!(found.files, ["A.txt", "b.txt"]);
    assert_eq!(found.gaps, [WalkGap::FileLimit(2)]);

    // Exactly as many files as the limit is complete.
    let limits = WalkLimits {
        max_files: 3,
        ..WALK
    };
    assert_eq!(walk(&fixture.open(), limits).gaps, []);
}

#[test]
fn walking_reports_directories_below_the_depth_limit() {
    let fixture = Fixture::new("walk-depth");
    touch(&fixture, &["tools/top.luau", "tools/deep/hidden.luau"]);
    let limits = WalkLimits {
        max_depth: 1,
        ..WALK
    };
    let found = fixture
        .open()
        .walk_files(limits, |path| path.ends_with(".luau"))
        .unwrap();
    assert_eq!(found.files, ["tools/top.luau"]);
    assert_eq!(
        found.gaps,
        [
            WalkGap::TooDeep("/games/owl".into()),
            WalkGap::TooDeep("/tools/deep".into()),
        ]
    );
}

#[test]
fn creating_directories_makes_every_missing_level_and_keeps_existing_ones() {
    let fixture = Fixture::new("create-dirs");
    let root = fixture.root();
    fs::create_dir(root.join("code")).unwrap();
    fs::write(root.join("code/keep.luau"), "x").unwrap();
    let fs = fixture.open();
    assert_eq!(fs.create_dirs("/code/enemies/bosses"), Ok(()));
    assert!(root.join("code/enemies/bosses").is_dir());
    assert_eq!(fs.create_dirs("code/enemies"), Ok(()), "already there");
    assert_eq!(
        fs::read_to_string(root.join("code/keep.luau")).unwrap(),
        "x"
    );
    assert_eq!(
        fs.create_dirs("/code/keep.luau/inner"),
        Err(FsError::NotADirectory("/code/keep.luau".into()))
    );
    assert!(!root.join("code/keep.luau/inner").exists());
}

#[test]
fn walking_unavailable_storage_fails() {
    let fixture = Fixture::new("walk-gone");
    let fs = fixture.open();
    std::fs::remove_dir_all(fixture.root()).unwrap();
    assert!(fs.walk_files(WALK, |_| true).is_err());
}

#[cfg(unix)]
mod symlinks {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::os::unix::fs::symlink;

    #[test]
    fn creating_directories_never_follows_a_link_out_of_the_root() {
        let fixture = Fixture::new("create-dirs-link");
        let root = fixture.root();
        symlink(fixture.outside(), root.join("out")).unwrap();
        symlink("missing", root.join("dangling")).unwrap();
        fs::create_dir(root.join("inside")).unwrap();
        symlink("inside", root.join("alias")).unwrap();
        let fs = fixture.open();
        assert_eq!(
            fs.create_dirs("/out/new"),
            Err(FsError::Unwritable("/out".into()))
        );
        assert_eq!(
            fs.create_dirs("/dangling"),
            Err(FsError::Unwritable("/dangling".into()))
        );
        assert_eq!(host_names(&fixture.outside()), ["secret"]);
        // A link inside the root is followed like any directory.
        assert_eq!(fs.create_dirs("/alias/deep"), Ok(()));
        assert!(root.join("inside/deep").is_dir());
    }

    #[test]
    fn creation_never_replaces_a_link() {
        let fixture = Fixture::new("create-dir-link");
        let root = fixture.root();
        symlink(fixture.outside(), root.join("out")).unwrap();
        symlink("missing", root.join("dangling")).unwrap();
        let fs = fixture.open();
        for name in ["out", "dangling"] {
            assert_eq!(
                create(&fs, name),
                Err(CreateDirError::Fs(FsError::AlreadyExists(format!(
                    "/{name}"
                ))))
            );
        }
        assert_eq!(host_names(&fixture.outside()), ["secret"]);
        assert!(
            fs::symlink_metadata(root.join("dangling"))
                .unwrap()
                .is_symlink()
        );
    }

    #[test]
    fn links_leaving_the_root_are_hidden_and_unreachable() {
        let fixture = Fixture::new("links-escape");
        let root = fixture.root();
        symlink(fixture.outside(), root.join("abs-out")).unwrap();
        symlink("../../outside", root.join("rel-out")).unwrap();
        symlink("..", root.join("parent")).unwrap();
        symlink(fixture.outside().join("secret"), root.join("secret")).unwrap();
        let mut fs = fixture.open();

        for name in ["abs-out", "rel-out", "parent", "secret"] {
            let path = format!("/{name}");
            assert_eq!(fs.change_dir(name), Err(not_found(&path)), "{name}");
            assert_eq!(fs.list(Some(name)), Err(not_found(&path)), "{name}");
        }
        assert_eq!(
            fs.change_dir("abs-out/.."),
            Ok(()),
            "lexical `..` cancels the link before it is used"
        );
        assert_eq!(fs.cwd(), "/");
        assert_eq!(
            ls(&fs, None),
            ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"]
        );
    }

    #[test]
    fn dangling_and_looping_links_are_hidden_and_unreachable() {
        let fixture = Fixture::new("links-broken");
        let root = fixture.root();
        symlink("missing", root.join("dangling")).unwrap();
        symlink("loop-b", root.join("loop-a")).unwrap();
        symlink("loop-a", root.join("loop-b")).unwrap();
        let mut fs = fixture.open();

        for name in ["dangling", "loop-a", "loop-b"] {
            assert_eq!(fs.change_dir(name), Err(not_found(&format!("/{name}"))));
        }
        assert_eq!(
            ls(&fs, None),
            ["games/", "tools/", "Zeta/", "A.txt", "b.txt", "readme"]
        );
    }

    #[test]
    fn subdir_confines_links_to_the_directory() {
        let fixture = Fixture::new("links-subdir");
        let root = fixture.root();
        symlink(root.join("b.txt"), root.join("games/owl/b-abs")).unwrap();
        symlink("../../b.txt", root.join("games/owl/b-rel")).unwrap();
        symlink("../../tools", root.join("games/owl/tools")).unwrap();
        fs::write(root.join("games/owl/own"), "").unwrap();
        symlink("own", root.join("games/owl/own-link")).unwrap();
        symlink("games/owl", root.join("owl-link")).unwrap();
        let fs = fixture.open();

        // Reached through a link, the subdirectory is rooted at its target.
        let (path, owl) = fs.subdir("owl-link").unwrap();
        assert_eq!(path, "/owl-link");
        assert_eq!(owl, fs.subdir("games/owl").unwrap().1);

        assert_eq!(ls(&owl, None), ["own", "own-link"]);
        assert_eq!(owl.ensure_file("own-link"), Ok(()));
        for name in ["b-abs", "b-rel", "tools"] {
            assert_eq!(owl.ensure_file(name), Err(not_found(&format!("/{name}"))));
        }
        assert_eq!(owl.read_file("tools/b.txt"), Err(not_found("/tools/b.txt")));
    }

    #[test]
    fn real_subdir_does_not_follow_links() {
        let fixture = Fixture::new("links-real-subdir");
        let root = fixture.root();
        symlink("games", root.join("games-link")).unwrap();
        symlink(fixture.outside(), root.join("out")).unwrap();
        let fs = fixture.open();

        for name in ["games-link", "out"] {
            assert_eq!(
                fs.real_subdir(name).map(drop),
                Err(not_found(&format!("/{name}")))
            );
        }
    }

    #[test]
    fn special_files_are_not_files() {
        let fixture = Fixture::new("socket");
        let _listener =
            std::os::unix::net::UnixListener::bind(fixture.root().join("sock")).unwrap();
        let fs = fixture.open();
        assert_eq!(fs.read_file("sock"), Err(FsError::NotAFile("/sock".into())));
        assert_eq!(
            fs.ensure_file("sock"),
            Err(FsError::NotAFile("/sock".into()))
        );
    }

    #[test]
    fn replace_file_writes_through_links_inside_the_root() {
        let fixture = Fixture::new("replace-link");
        let root = fixture.root();
        fs::write(root.join("tools/real"), b"old").unwrap();
        symlink("tools/real", root.join("alias")).unwrap();
        let fs = fixture.open();
        fs.replace_file("alias", b"new", Some(b"old")).unwrap();
        assert!(
            fs::symlink_metadata(root.join("alias"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(fs::read(root.join("tools/real")).unwrap(), b"new");
    }

    #[test]
    fn replace_file_never_replaces_unreachable_links() {
        let fixture = Fixture::new("replace-escape");
        let root = fixture.root();
        symlink(fixture.outside().join("secret"), root.join("out")).unwrap();
        symlink("missing", root.join("dangling")).unwrap();
        let fs = fixture.open();
        for name in ["out", "dangling"] {
            assert_eq!(
                fs.replace_file(name, b"x", None),
                Err(ReplaceError::Fs(FsError::Unwritable(format!("/{name}")))),
                "{name}"
            );
            assert!(fs::symlink_metadata(root.join(name)).unwrap().is_symlink());
        }
        assert_eq!(fs::read(fixture.outside().join("secret")).unwrap(), b"");
        assert_eq!(staged(&root), Vec::<String>::new());
    }

    /// Sets `path`'s Unix mode, restoring `restore` on drop so the
    /// fixture can be removed.
    struct Mode {
        path: PathBuf,
        restore: u32,
    }

    impl Mode {
        fn set(path: &Path, mode: u32, restore: u32) -> Self {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
            Self {
                path: path.to_owned(),
                restore,
            }
        }
    }

    impl Drop for Mode {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(self.restore));
        }
    }

    /// Whether this process can create files in `dir` despite its mode,
    /// as root can. Permission tests are meaningless then.
    fn bypasses_permissions(dir: &Path) -> bool {
        let probe = dir.join("probe");
        let created = fs::write(&probe, b"").is_ok();
        let _ = fs::remove_file(&probe);
        created
    }

    #[test]
    fn a_read_only_file_is_never_replaced() {
        use std::os::unix::fs::MetadataExt;
        let fixture = Fixture::new("replace-read-only");
        let path = fixture.root().join("tools/data");
        fs::write(&path, b"old").unwrap();
        let inode = fs::metadata(&path).unwrap().ino();
        let _mode = Mode::set(&path, 0o444, 0o644);
        let fs = fixture.open();
        // The directory stays writable, so a rename could replace the
        // file; the read-only flag alone keeps it.
        assert_eq!(
            fs.replace_file("tools/data", b"new", Some(b"old")),
            Err(ReplaceError::Fs(FsError::ReadOnly("/tools/data".into())))
        );
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        assert_eq!(staged(&fixture.root().join("tools")), Vec::<String>::new());
    }

    #[test]
    fn a_read_only_directory_is_unwritable() {
        let fixture = Fixture::new("replace-read-only-dir");
        let dir = fixture.root().join("tools");
        fs::write(dir.join("data"), b"old").unwrap();
        let _mode = Mode::set(&dir, 0o555, 0o755);
        if bypasses_permissions(&dir) {
            return;
        }
        let fs = fixture.open();
        assert_eq!(
            fs.replace_file("tools/data", b"new", Some(b"old")),
            Err(ReplaceError::Fs(FsError::Unwritable("/tools/data".into())))
        );
        assert_eq!(fs::read(dir.join("data")).unwrap(), b"old");
        assert_eq!(staged(&dir), Vec::<String>::new());
    }

    #[test]
    fn links_inside_the_root_are_followed() {
        let fixture = Fixture::new("links-inside");
        let root = fixture.root();
        symlink("games/owl", root.join("owl-link")).unwrap();
        symlink(root.join("b.txt"), root.join("tools/b-link")).unwrap();
        symlink("../games", root.join("tools/games-link")).unwrap();
        let mut fs = fixture.open();

        assert_eq!(ls(&fs, Some("tools")), ["games-link/", "b-link"]);
        assert_eq!(ls(&fs, Some("tools/games-link")), ["owl/"]);
        fs.change_dir("owl-link").unwrap();
        // The virtual path is the one navigated, not the link target.
        assert_eq!(fs.cwd(), "/owl-link");
        assert_eq!(
            fs.change_dir("/tools/b-link"),
            Err(FsError::NotADirectory("/tools/b-link".into()))
        );
    }

    #[test]
    fn walking_skips_a_directory_reached_again_through_a_link() {
        let fixture = Fixture::new("walk-cycle");
        let root = fixture.root();
        touch(&fixture, &["tools/a.luau"]);
        // A link back up the tree would otherwise be walked forever.
        symlink("..", root.join("tools/up")).unwrap();
        let found = walk(&fixture.open(), WALK);
        assert_eq!(found.files, ["tools/a.luau", "A.txt", "b.txt", "readme"]);
        assert_eq!(found.gaps, [WalkGap::AlreadyWalked("/tools/up".into())]);
    }

    #[test]
    fn walking_never_leaves_the_root() {
        let fixture = Fixture::new("walk-escape");
        symlink(fixture.outside(), fixture.root().join("tools/out")).unwrap();
        let found = walk(&fixture.open(), WALK);
        assert!(!found.files.iter().any(|file| file.contains("secret")));
        assert_eq!(found.gaps, []);
    }

    #[test]
    fn walking_reports_an_unreadable_directory_and_lists_the_rest() {
        let fixture = Fixture::new("walk-unreadable");
        touch(&fixture, &["tools/locked/a.luau", "Zeta/z.luau"]);
        let locked = fixture.root().join("tools/locked");
        let _mode = Mode::set(&locked, 0o000, 0o755);
        if fs::read_dir(&locked).is_ok() {
            return;
        }
        let found = fixture
            .open()
            .walk_files(WALK, |path| path.ends_with(".luau"))
            .unwrap();
        assert_eq!(found.files, ["Zeta/z.luau"]);
        assert_eq!(found.gaps, [WalkGap::Unreadable("/tools/locked".into())]);
    }
}

mod removing_and_moving {
    use super::*;
    use pretty_assertions::assert_eq;

    fn write(fixture: &Fixture, path: &str, text: &str) {
        let host = fixture.root().join(path);
        fs::create_dir_all(host.parent().unwrap()).unwrap();
        fs::write(host, text).unwrap();
    }

    fn read(fixture: &Fixture, path: &str) -> String {
        fs::read_to_string(fixture.root().join(path)).unwrap()
    }

    #[test]
    fn removing_a_file_leaves_its_directory_and_neighbours() {
        let fixture = Fixture::new("remove-file");
        write(&fixture, "tools/a.luau", "a");
        write(&fixture, "tools/b.luau", "b");
        fixture.open().remove_file("/tools/a.luau").unwrap();
        assert_eq!(ls(&fixture.open(), Some("/tools")), ["b.luau"]);
    }

    #[test]
    fn removing_refuses_what_is_not_a_plain_file() {
        let fixture = Fixture::new("remove-refused");
        write(&fixture, "tools/a.luau", "a");
        let fs = fixture.open();
        assert_eq!(fs.remove_file("/tools/gone"), Err(not_found("/tools/gone")));
        assert_eq!(
            fs.remove_file("/tools"),
            Err(FsError::NotAFile("/tools".into()))
        );
        assert_eq!(
            fs.remove_file("/tools/.glaux-staging-x"),
            Err(FsError::Unwritable("/tools/.glaux-staging-x".into()))
        );
        assert_eq!(read(&fixture, "tools/a.luau"), "a");
    }

    #[test]
    fn removing_a_read_only_file_leaves_it() {
        let fixture = Fixture::new("remove-read-only");
        write(&fixture, "tools/a.luau", "a");
        let host = fixture.root().join("tools/a.luau");
        let mut permissions = fs::metadata(&host).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&host, permissions).unwrap();
        assert_eq!(
            fixture.open().remove_file("/tools/a.luau"),
            Err(FsError::ReadOnly("/tools/a.luau".into()))
        );
        assert!(host.exists());
    }

    #[test]
    fn moving_creates_directories_and_keeps_the_emptied_one() {
        let fixture = Fixture::new("move-nested");
        write(&fixture, "tools/old/a.luau", "contents");
        fixture
            .open()
            .rename_file("/tools/old/a.luau", "/tools/new/deeper/b.luau")
            .unwrap();
        assert_eq!(read(&fixture, "tools/new/deeper/b.luau"), "contents");
        assert!(!fixture.root().join("tools/old/a.luau").exists());
        assert!(fixture.root().join("tools/old").is_dir());
    }

    #[test]
    fn moving_never_replaces_an_existing_file() {
        let fixture = Fixture::new("move-exists");
        write(&fixture, "tools/a.luau", "a");
        write(&fixture, "tools/b.luau", "b");
        assert_eq!(
            fixture.open().rename_file("/tools/a.luau", "/tools/b.luau"),
            Err(FsError::AlreadyExists("/tools/b.luau".into()))
        );
        assert_eq!(read(&fixture, "tools/a.luau"), "a");
        assert_eq!(read(&fixture, "tools/b.luau"), "b");
    }

    #[test]
    fn moving_refuses_missing_sources_directories_and_staging_names() {
        let fixture = Fixture::new("move-refused");
        write(&fixture, "tools/a.luau", "a");
        let fs = fixture.open();
        assert_eq!(
            fs.rename_file("/tools/gone.luau", "/tools/b.luau"),
            Err(not_found("/tools/gone.luau"))
        );
        assert_eq!(
            fs.rename_file("/tools", "/other"),
            Err(FsError::NotAFile("/tools".into()))
        );
        assert_eq!(
            fs.rename_file("/tools/a.luau", "/tools/.glaux-staging-b"),
            Err(FsError::Unwritable("/tools/.glaux-staging-b".into()))
        );
        assert_eq!(read(&fixture, "tools/a.luau"), "a");
    }

    #[test]
    fn a_name_that_appears_during_the_move_is_not_replaced() {
        // `rename_file` looks before it moves; the primitive itself must
        // still refuse when the name is taken in between.
        let fixture = Fixture::new("move-race");
        write(&fixture, "tools/a.luau", "a");
        write(&fixture, "tools/b.luau", "b");
        let (from, to) = (
            fixture.root().join("tools/a.luau"),
            fixture.root().join("tools/b.luau"),
        );
        for result in [move_no_replace(&from, &to), link_then_unlink(&from, &to)] {
            assert!(matches!(
                result,
                Err(MoveError::Io(err)) if err.kind() == ErrorKind::AlreadyExists
            ));
        }
        assert_eq!(read(&fixture, "tools/a.luau"), "a");
        assert_eq!(read(&fixture, "tools/b.luau"), "b");
    }

    #[test]
    fn the_linking_fallback_moves_the_file() {
        let fixture = Fixture::new("move-link");
        write(&fixture, "tools/a.luau", "a");
        let (from, to) = (
            fixture.root().join("tools/a.luau"),
            fixture.root().join("tools/b.luau"),
        );
        assert!(link_then_unlink(&from, &to).is_ok());
        assert!(!from.exists());
        assert_eq!(read(&fixture, "tools/b.luau"), "a");
    }

    #[test]
    fn dot_dot_cannot_move_a_file_out_of_the_root() {
        let fixture = Fixture::new("move-dotdot");
        write(&fixture, "tools/a.luau", "a");
        fixture
            .open()
            .rename_file("/tools/a.luau", "/../../escaped.luau")
            .unwrap();
        // `..` stops at the root, so the file landed directly inside it.
        assert_eq!(read(&fixture, "escaped.luau"), "a");
        assert!(!fixture.outside().join("escaped.luau").exists());
    }

    #[cfg(unix)]
    mod symlinks {
        use super::*;
        use pretty_assertions::assert_eq;
        use std::os::unix::fs::symlink;

        #[test]
        fn a_link_is_neither_removed_nor_moved_nor_followed() {
            let fixture = Fixture::new("move-links");
            let root = fixture.root();
            write(&fixture, "tools/real.luau", "real");
            symlink("real.luau", root.join("tools/alias.luau")).unwrap();
            symlink(
                fixture.outside().join("secret"),
                root.join("tools/out.luau"),
            )
            .unwrap();
            let fs = fixture.open();
            for link in ["/tools/alias.luau", "/tools/out.luau"] {
                assert_eq!(
                    fs.remove_file(link),
                    Err(FsError::Unwritable(link.into())),
                    "{link}"
                );
                assert_eq!(
                    fs.rename_file(link, "/tools/moved.luau"),
                    Err(FsError::Unwritable(link.into())),
                    "{link}"
                );
            }
            assert!(fs::symlink_metadata(root.join("tools/alias.luau")).is_ok());
            assert!(fs::symlink_metadata(root.join("tools/out.luau")).is_ok());
            assert_eq!(read(&fixture, "tools/real.luau"), "real");
            assert_eq!(
                fs::read_to_string(fixture.outside().join("secret")).unwrap(),
                ""
            );
            assert!(!root.join("tools/moved.luau").exists());
        }

        #[test]
        fn a_link_holding_the_destination_name_counts_as_taken() {
            let fixture = Fixture::new("move-dest-link");
            let root = fixture.root();
            write(&fixture, "tools/a.luau", "a");
            symlink("missing", root.join("tools/dangling.luau")).unwrap();
            symlink(fixture.outside().join("new"), root.join("tools/out.luau")).unwrap();
            for name in ["dangling", "out"] {
                assert_eq!(
                    fixture
                        .open()
                        .rename_file("/tools/a.luau", &format!("/tools/{name}.luau")),
                    Err(FsError::AlreadyExists(format!("/tools/{name}.luau")))
                );
            }
            assert_eq!(read(&fixture, "tools/a.luau"), "a");
            assert!(!fixture.outside().join("new").exists());
        }

        #[test]
        fn a_link_leading_out_of_the_root_never_receives_the_file() {
            let fixture = Fixture::new("move-out-dir");
            write(&fixture, "tools/a.luau", "a");
            symlink(fixture.outside(), fixture.root().join("out")).unwrap();
            assert_eq!(
                fixture.open().rename_file("/tools/a.luau", "/out/a.luau"),
                Err(FsError::Unwritable("/out".into()))
            );
            assert_eq!(read(&fixture, "tools/a.luau"), "a");
            assert!(!fixture.outside().join("a.luau").exists());
        }
    }
}
