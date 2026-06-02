# Changelog

All notable changes to this project will be documented in this file.

## [0.0.7] - 2026-06-02

### Bug Fixes

- Broken command mode — [`eb09ae4`](https://github.com/emilien-jegou/oyui/commit/eb09ae412ef6437c6ba0535ae47ce6947ac1bf60) by Emilien, 2026-06-02

## [0.0.3] - 2026-06-02

### Bug Fixes

- Removed some dead options — [`87a67cc`](https://github.com/emilien-jegou/oyui/commit/87a67cc83e76fbc7f4c6417b8206627477f925e1) by Emilien, 2026-05-20
- Wrong files picked in merge + empty dir skipped — [`ff6ab25`](https://github.com/emilien-jegou/oyui/commit/ff6ab25dee59cc2676864100b7f572e94ad1e11a) by Emilien, 2026-05-22
- Deleted/added file were not being displayed in file view — [`bd49e56`](https://github.com/emilien-jegou/oyui/commit/bd49e56c9332f723e7e095be7dc7724c75cef378) by Emilien, 2026-05-22
- Outdated keybinds helper — [`29e0dd2`](https://github.com/emilien-jegou/oyui/commit/29e0dd2fac67ce2681f7c881ffa763ad689d897c) by Emilien, 2026-05-26
- Duplicate fold keybind — [`b64adcc`](https://github.com/emilien-jegou/oyui/commit/b64adcc4d2bbde9c0ea27577ef013d74c01caa5a) by Emilien, 2026-05-26
- Regression on hunk staging — [`48d896f`](https://github.com/emilien-jegou/oyui/commit/48d896fcc6f4370894383c721ff720b5ab354960) by Emilien, 2026-06-02
- Tree invert action ignored partial hunk staging — [`b3be203`](https://github.com/emilien-jegou/oyui/commit/b3be203bc5a0ecd140605f2c93e9dedc4bcc0d95) by Emilien, 2026-06-02
- Broken 'G' keybind on tree and file view — [`52f5b10`](https://github.com/emilien-jegou/oyui/commit/52f5b10e885c5ef37b3f0c0f8a55696c174e67f7) by Emilien, 2026-06-02

### Documentation

- Description of goals in readme — [`ac3cf30`](https://github.com/emilien-jegou/oyui/commit/ac3cf30bf5f106d2245f76607161750019727cb8) by Emilien, 2026-05-20
- Just inproved feature copy in readme — [`769444b`](https://github.com/emilien-jegou/oyui/commit/769444b677e2bf5f728f26721ab75b276f204583) by Emilien, 2026-05-20
- Updated readme — [`d2eb90e`](https://github.com/emilien-jegou/oyui/commit/d2eb90e543b637f3dfb85685ddad4158c6343778) by Emilien, 2026-05-24
- Wider screenshots for theme.md — [`6fae970`](https://github.com/emilien-jegou/oyui/commit/6fae970e6ac498c53f8a251c9d3ba30018eb3bd5) by Emilien, 2026-05-26
- Re-arrange feature list — [`ce688f7`](https://github.com/emilien-jegou/oyui/commit/ce688f7a81e6451c44c744253306f4669e655a74) by Emilien, 2026-05-26
- Update LSP information in readme — [`1149b84`](https://github.com/emilien-jegou/oyui/commit/1149b845baec5e2e65a4e98da216d30fe88908fe) by Emilien, 2026-06-01
- Update feature & bug tracking — [`b28fd2e`](https://github.com/emilien-jegou/oyui/commit/b28fd2e7bc9ffc8b44dfd2358e870d08d89f9e45) by Emilien, 2026-06-02
- Gif in readme — [`568da8c`](https://github.com/emilien-jegou/oyui/commit/568da8cb7b7a2aea0715fb16a57e09d51c265b94) by Emilien, 2026-06-02
- Add a language badge in readme — [`580d67a`](https://github.com/emilien-jegou/oyui/commit/580d67ab120d8c499f164e7e2c1b8c47647698b4) by Emilien, 2026-06-02

### Features

- Bootstrap — [`d6b2517`](https://github.com/emilien-jegou/oyui/commit/d6b2517500710a3df0d1ab8426dba189a9fb0ecd) by Emilien, 2026-05-19
- Add shortcut for tree inversion — [`770ffd0`](https://github.com/emilien-jegou/oyui/commit/770ffd032a5d921d2fb0f59b7fee354a8174cd97) by Emilien, 2026-05-19
- Shortcuts, file view changes and app refactoring — [`3e84ce7`](https://github.com/emilien-jegou/oyui/commit/3e84ce75582bf12b7e375ed45a69905036b4d17b) by Emilien, 2026-05-20
- Dynamic +/- colors based on modification weight — [`7a88dbb`](https://github.com/emilien-jegou/oyui/commit/7a88dbb04716ab9358770ee5ba62f4aa2b54dc22) by Emilien, 2026-05-20
- Improve initial stats performance via rayon + tracing — [`db64278`](https://github.com/emilien-jegou/oyui/commit/db642785a50e7c9ecddb2c8e1b6134210a0a5a88) by Emilien, 2026-05-21
- Better handling of binary files — [`0a7b0a3`](https://github.com/emilien-jegou/oyui/commit/0a7b0a39f7ad1ceb66709fc4b31c960b70620ea4) by Emilien, 2026-05-23
- Experimental syntax aware diffing — [`5e374a2`](https://github.com/emilien-jegou/oyui/commit/5e374a2d62f8191d5cca6915c0f0d8551d8c75c9) by Emilien, 2026-05-24
- Add scrolloff option — [`b433902`](https://github.com/emilien-jegou/oyui/commit/b433902d801af2f2aa691418a7fafc1c1210fb0e) by Emilien, 2026-05-24
- Collapse long directory chains. — [`f1db4ed`](https://github.com/emilien-jegou/oyui/commit/f1db4ed2ca806691a2c71367483dc61718a019cd) by Emilien, 2026-05-24
- 'n' and 'N' shortcuts for hunk navigation — [`24f34ea`](https://github.com/emilien-jegou/oyui/commit/24f34ea1c0464ebc54da8b993c61061a175c5411) by Emilien, 2026-05-24
- Hunk staging — [`a5d8824`](https://github.com/emilien-jegou/oyui/commit/a5d88249fe0ca79ecf1722ce0669ee185665a3d1) by Emilien, 2026-05-24
- Config.toml and theming (+30 themes) — [`75f7212`](https://github.com/emilien-jegou/oyui/commit/75f72126cd169da969f7b208374e2e49f80c1158) by Emilien, 2026-05-25
- Add --config option in cli — [`2a7aa4a`](https://github.com/emilien-jegou/oyui/commit/2a7aa4ac7436887c677877d2a40d8f3962d978d9) by Emilien, 2026-05-26
- Automate themes.md creation — [`0f4dc71`](https://github.com/emilien-jegou/oyui/commit/0f4dc71fae2147a0909c2b5f6314576f599958c8) by Emilien, 2026-05-26
- Dynamic fallback based on theme light/dark mode — [`85eabf1`](https://github.com/emilien-jegou/oyui/commit/85eabf15d1bd47346cbc12d4563654fa80c777d5) by Emilien, 2026-05-26
- Highlight configuration for theming — [`4ee536b`](https://github.com/emilien-jegou/oyui/commit/4ee536b24ad05bc4dba5a2bc68519df9656e49d8) by Emilien, 2026-05-26
- Horizontal scroll in file view — [`1fcdff9`](https://github.com/emilien-jegou/oyui/commit/1fcdff920a72008d49fee6a908254f05bc74cc13) by Emilien, 2026-05-26
- Rune config and keybinds — [`c9baa8b`](https://github.com/emilien-jegou/oyui/commit/c9baa8b6ace7651d73c7d2acb7b0bc3a8c4fe8f4) by Emilien, 2026-05-28
- Sub-splitting hunks with keybinds — [`944d9ff`](https://github.com/emilien-jegou/oyui/commit/944d9ff57a9258a3f2ca417ebc1d5ecb81b94582) by Emilien, 2026-06-02
- Invert action for file view — [`42a54cd`](https://github.com/emilien-jegou/oyui/commit/42a54cd96e9bb0b59f63504206f06cd32eb025f7) by Emilien, 2026-06-02
- Add devicon integration — [`1ce9dc5`](https://github.com/emilien-jegou/oyui/commit/1ce9dc55cca44305b62dbd0b2937e52c164da3cd) by Emilien, 2026-06-02
- Cargo packaging — [`90c7d66`](https://github.com/emilien-jegou/oyui/commit/90c7d663c3787f338716e52c724682f5843cda7d) by Emilien, 2026-06-02

### Refactoring

- Simplify async task creation logic — [`36e27a5`](https://github.com/emilien-jegou/oyui/commit/36e27a512b8e76c8a1de003bdaac2b7e68b1e3bf) by Emilien, 2026-05-21
- Divide file view in logical parts — [`b0f301e`](https://github.com/emilien-jegou/oyui/commit/b0f301eb7147218fd488dcd1b676c8d88655d66a) by Emilien, 2026-05-27
- Remove manual define of action handler — [`d6cb855`](https://github.com/emilien-jegou/oyui/commit/d6cb855db9f47f5ee3028d20d22380cd2001c49a) by Emilien, 2026-06-02


