use std::{collections::BTreeSet, io::Cursor};

use expect_test::{expect, Expect};
use nix_remote::{
    serialize::{NixReadExt, NixWriteExt},
    worker_op::{BuildMode, BuildResult},
    DerivedPath, NixString, Realisation, StorePath, ValidPathInfoWithPath,
};
use serde::{de::DeserializeOwned, Serialize};

fn check<T: DeserializeOwned + Serialize + std::fmt::Debug>(data: &[u8], expect: Expect) {
    let mut read = Cursor::new(data);
    let actual: T = read.read_nix().unwrap();

    // We re-serialize in here, to check that it round-trips.
    let mut out = Vec::new();

    out.write_nix(&actual).unwrap();

    expect.assert_debug_eq(&actual);

    assert_eq!(&out, data);
}

#[test]
fn string() {
    // This is a bit different from the test in CppNix; they have
    // "oh no \0\0\0 what was that!", but since they're using a C
    // string literal it's the same as "oh no ".
    check::<(NixString, NixString, NixString, NixString, NixString)>(
        include_bytes!("data/worker-protocol/string.bin"),
        expect![[r#"
            (
                ,
                hi,
                white rabbit,
                大白兔,
                oh no ,
            )
        "#]],
    );
}

#[test]
fn store_path() {
    check::<(StorePath, StorePath)>(
        include_bytes!("data/worker-protocol/store-path.bin"),
        expect![[r#"
            (
                StorePath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo,
                ),
                StorePath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo-bar,
                ),
            )
        "#]],
    );
}

// CppNix has a ContentAddress test here, but we don't yet have a ContentAddress type.

#[test]
fn derived_path() {
    check::<(DerivedPath, DerivedPath, DerivedPath, DerivedPath)>(
        include_bytes!("data/worker-protocol/derived-path-1.30.bin"),
        expect![[r#"
            (
                DerivedPath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo,
                ),
                DerivedPath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo.drv,
                ),
                DerivedPath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar.drv!*,
                ),
                DerivedPath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar.drv!x,y,
                ),
            )
        "#]],
    );
}

#[test]
fn drv_output() {
    // CppNix uses DrvOutput as the type, but we don't have it and it's just a NixString
    // on the wire anyway.
    check::<(NixString, NixString)>(
        include_bytes!("data/worker-protocol/drv-output.bin"),
        expect![[r#"
            (
                sha256:15e3c560894cbb27085cf65b5a2ecb18488c999497f4531b6907a7581ce6d527!baz,
                sha256:6f869f9ea2823bda165e06076fd0de4366dead2c0e8d2dbbad277d4f15c373f5!quux,
            )
        "#]],
    );
}

#[test]
fn realisation() {
    check::<(Realisation, Realisation)>(
        include_bytes!("data/worker-protocol/realisation.bin"),
        expect![[r#"
            (
                Realisation(
                    {"dependentRealisations":{},"id":"sha256:15e3c560894cbb27085cf65b5a2ecb18488c999497f4531b6907a7581ce6d527!baz","outPath":"g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo","signatures":["asdf","qwer"]},
                ),
                Realisation(
                    {"dependentRealisations":{"sha256:6f869f9ea2823bda165e06076fd0de4366dead2c0e8d2dbbad277d4f15c373f5!quux":"g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo"},"id":"sha256:15e3c560894cbb27085cf65b5a2ecb18488c999497f4531b6907a7581ce6d527!baz","outPath":"g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo","signatures":["asdf","qwer"]},
                ),
            )
        "#]],
    );
}

#[test]
fn build_result() {
    check::<(BuildResult, BuildResult, BuildResult)>(
        include_bytes!("data/worker-protocol/build-result-1.29.bin"),
        expect![[r#"
            (
                BuildResult {
                    status: OutputRejected,
                    error_msg: no idea why,
                    times_built: 0,
                    is_non_deterministic: false,
                    start_time: 0,
                    stop_time: 0,
                    built_outputs: DrvOutputs(
                        [],
                    ),
                },
                BuildResult {
                    status: NotDeterministic,
                    error_msg: no idea why,
                    times_built: 3,
                    is_non_deterministic: true,
                    start_time: 30,
                    stop_time: 50,
                    built_outputs: DrvOutputs(
                        [],
                    ),
                },
                BuildResult {
                    status: Built,
                    error_msg: ,
                    times_built: 1,
                    is_non_deterministic: false,
                    start_time: 30,
                    stop_time: 50,
                    built_outputs: DrvOutputs(
                        [
                            (
                                sha256:6f869f9ea2823bda165e06076fd0de4366dead2c0e8d2dbbad277d4f15c373f5!bar,
                                Realisation(
                                    {"dependentRealisations":{},"id":"sha256:6f869f9ea2823bda165e06076fd0de4366dead2c0e8d2dbbad277d4f15c373f5!bar","outPath":"g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar","signatures":[]},
                                ),
                            ),
                            (
                                sha256:6f869f9ea2823bda165e06076fd0de4366dead2c0e8d2dbbad277d4f15c373f5!foo,
                                Realisation(
                                    {"dependentRealisations":{},"id":"sha256:6f869f9ea2823bda165e06076fd0de4366dead2c0e8d2dbbad277d4f15c373f5!foo","outPath":"g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo","signatures":[]},
                                ),
                            ),
                        ],
                    ),
                },
            )
        "#]],
    );
}

type KeyedBuildResult = (DerivedPath, BuildResult);
#[test]
fn keyed_build_result() {
    check::<(KeyedBuildResult, KeyedBuildResult)>(
        include_bytes!("data/worker-protocol/keyed-build-result-1.29.bin"),
        expect![[r#"
            (
                (
                    DerivedPath(
                        /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-xxx,
                    ),
                    BuildResult {
                        status: OutputRejected,
                        error_msg: no idea why,
                        times_built: 0,
                        is_non_deterministic: false,
                        start_time: 0,
                        stop_time: 0,
                        built_outputs: DrvOutputs(
                            [],
                        ),
                    },
                ),
                (
                    DerivedPath(
                        /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar.drv!out,
                    ),
                    BuildResult {
                        status: NotDeterministic,
                        error_msg: no idea why,
                        times_built: 3,
                        is_non_deterministic: true,
                        start_time: 30,
                        stop_time: 50,
                        built_outputs: DrvOutputs(
                            [],
                        ),
                    },
                ),
            )
        "#]],
    );
}

#[test]
fn valid_path_info() {
    check::<(
        ValidPathInfoWithPath,
        ValidPathInfoWithPath,
        ValidPathInfoWithPath,
    )>(
        include_bytes!("data/worker-protocol/valid-path-info-1.16.bin"),
        expect![[r#"
            (
                ValidPathInfoWithPath {
                    path: StorePath(
                        /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar,
                    ),
                    info: ValidPathInfo {
                        deriver: StorePath(
                            ,
                        ),
                        hash: NarHash {
                            data: [
                                49,
                                53,
                                101,
                                51,
                                99,
                                53,
                                54,
                                48,
                                56,
                                57,
                                52,
                                99,
                                98,
                                98,
                                50,
                                55,
                                48,
                                56,
                                53,
                                99,
                                102,
                                54,
                                53,
                                98,
                                53,
                                97,
                                50,
                                101,
                                99,
                                98,
                                49,
                                56,
                                52,
                                56,
                                56,
                                99,
                                57,
                                57,
                                57,
                                52,
                                57,
                                55,
                                102,
                                52,
                                53,
                                51,
                                49,
                                98,
                                54,
                                57,
                                48,
                                55,
                                97,
                                55,
                                53,
                                56,
                                49,
                                99,
                                101,
                                54,
                                100,
                                53,
                                50,
                                55,
                            ],
                        },
                        references: StorePathSet {
                            paths: [],
                        },
                        registration_time: 23423,
                        nar_size: 34878,
                        ultimate: true,
                        sigs: StringSet {
                            paths: [],
                        },
                        content_address: ,
                    },
                },
                ValidPathInfoWithPath {
                    path: StorePath(
                        /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar,
                    ),
                    info: ValidPathInfo {
                        deriver: StorePath(
                            /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar.drv,
                        ),
                        hash: NarHash {
                            data: [
                                49,
                                53,
                                101,
                                51,
                                99,
                                53,
                                54,
                                48,
                                56,
                                57,
                                52,
                                99,
                                98,
                                98,
                                50,
                                55,
                                48,
                                56,
                                53,
                                99,
                                102,
                                54,
                                53,
                                98,
                                53,
                                97,
                                50,
                                101,
                                99,
                                98,
                                49,
                                56,
                                52,
                                56,
                                56,
                                99,
                                57,
                                57,
                                57,
                                52,
                                57,
                                55,
                                102,
                                52,
                                53,
                                51,
                                49,
                                98,
                                54,
                                57,
                                48,
                                55,
                                97,
                                55,
                                53,
                                56,
                                49,
                                99,
                                101,
                                54,
                                100,
                                53,
                                50,
                                55,
                            ],
                        },
                        references: StorePathSet {
                            paths: [
                                StorePath(
                                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar,
                                ),
                                StorePath(
                                    /nix/store/g1w7hyyyy1w7hy3qg1w7hy3qgqqqqy3q-foo,
                                ),
                            ],
                        },
                        registration_time: 23423,
                        nar_size: 34878,
                        ultimate: false,
                        sigs: StringSet {
                            paths: [
                                fake-sig-1,
                                fake-sig-2,
                            ],
                        },
                        content_address: ,
                    },
                },
                ValidPathInfoWithPath {
                    path: StorePath(
                        /nix/store/n5wkd9frr45pa74if5gpz9j7mifg27fh-foo,
                    ),
                    info: ValidPathInfo {
                        deriver: StorePath(
                            ,
                        ),
                        hash: NarHash {
                            data: [
                                49,
                                53,
                                101,
                                51,
                                99,
                                53,
                                54,
                                48,
                                56,
                                57,
                                52,
                                99,
                                98,
                                98,
                                50,
                                55,
                                48,
                                56,
                                53,
                                99,
                                102,
                                54,
                                53,
                                98,
                                53,
                                97,
                                50,
                                101,
                                99,
                                98,
                                49,
                                56,
                                52,
                                56,
                                56,
                                99,
                                57,
                                57,
                                57,
                                52,
                                57,
                                55,
                                102,
                                52,
                                53,
                                51,
                                49,
                                98,
                                54,
                                57,
                                48,
                                55,
                                97,
                                55,
                                53,
                                56,
                                49,
                                99,
                                101,
                                54,
                                100,
                                53,
                                50,
                                55,
                            ],
                        },
                        references: StorePathSet {
                            paths: [
                                StorePath(
                                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-bar,
                                ),
                                StorePath(
                                    /nix/store/n5wkd9frr45pa74if5gpz9j7mifg27fh-foo,
                                ),
                            ],
                        },
                        registration_time: 23423,
                        nar_size: 34878,
                        ultimate: false,
                        sigs: StringSet {
                            paths: [],
                        },
                        content_address: fixed:r:sha256:1lr187v6dck1rjh2j6svpikcfz53wyl3qrlcbb405zlh13x0khhh,
                    },
                },
            )
        "#]],
    );
}

#[test]
fn build_mode() {
    check::<(BuildMode, BuildMode, BuildMode)>(
        include_bytes!("data/worker-protocol/build-mode.bin"),
        expect![[r#"
            (
                Normal,
                Repair,
                Check,
            )
        "#]],
    );
}

// TODO: TrustedFlag (which is needed for protocol version 35)

#[test]
fn vector() {
    check::<(
        Vec<NixString>,
        Vec<NixString>,
        Vec<NixString>,
        Vec<Vec<NixString>>,
    )>(
        include_bytes!("data/worker-protocol/vector.bin"),
        expect![[r#"
        (
            [],
            [
                ,
            ],
            [
                ,
                foo,
                bar,
            ],
            [
                [],
                [
                    ,
                ],
                [
                    ,
                    1,
                    2,
                ],
            ],
        )
    "#]],
    );
}

#[test]
fn set() {
    check::<(
        BTreeSet<NixString>,
        BTreeSet<NixString>,
        BTreeSet<NixString>,
        BTreeSet<BTreeSet<NixString>>,
    )>(
        include_bytes!("data/worker-protocol/set.bin"),
        expect![[r#"
        (
            {},
            {
                ,
            },
            {
                ,
                bar,
                foo,
            },
            {
                {},
                {
                    ,
                },
                {
                    ,
                    1,
                    2,
                },
            },
        )
    "#]],
    );
}

#[test]
fn optional_store_path() {
    check::<(StorePath, StorePath)>(
        include_bytes!("data/worker-protocol/optional-store-path.bin"),
        expect![[r#"
            (
                StorePath(
                    ,
                ),
                StorePath(
                    /nix/store/g1w7hy3qg1w7hy3qg1w7hy3qg1w7hy3q-foo-bar,
                ),
            )
        "#]],
    );
}
