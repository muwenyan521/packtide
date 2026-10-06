# Notices

## Project License

This Source Code Form is subject to the terms of the Mozilla Public License,
v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain
one at https://mozilla.org/MPL/2.0/.

Copyright (c) 2026 wangxianming and contributors.

This Rust workspace is distributed under MPL-2.0 in `LICENSE`. The
license declaration in the Cargo workspace and packages is intentionally
explicit; it applies to this repository's Rust source and release artifacts.

## Relationship To SHORiN-KiWATA Repositories

This repository is an independent Rust rewrite and multi-distribution package
management implementation. It is not a fork of either upstream repository:

- [`SHORiN-KiWATA/shorin-pac`](https://github.com/SHORiN-KiWATA/shorin-pac) is
  the upstream Arch-oriented shell package-management project. Its current
  `main` line is GPL-3.0-or-later and remains a separate project.
- [`SHORiN-KiWATA/shorin-contrib`](https://github.com/SHORiN-KiWATA/shorin-contrib)
  is the historical shell-contribution repository. Its public repository does
  not declare a repository-level license, so this project does not copy or
  redistribute its shell files and does not infer a license from the repository
  relationship.

The maintainer confirms that the historical projects were consulted only for
behavior, without copying or adapting their code or text. This source-provenance
declaration is the basis for the license choice; rewriting in Rust alone would
not establish independent authorship.

The historical projects informed compatibility goals and user-facing behavior;
they are not runtime dependencies of this workspace. If code from either
project is added in the future, its source file, copyright, and license terms
must be reviewed before distribution.
