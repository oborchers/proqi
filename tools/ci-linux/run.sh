#!/bin/sh
set -eux

export CARGO_HOME=/cache/cargo
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=/work/repository/target
export RUSTUP_HOME=/usr/local/rustup
export PATH=/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin

mkdir -p /work/repository "$CARGO_HOME" /cache/target
cd /source
tar --exclude=./.git --exclude=./target -cf - . | tar -xf - -C /work/repository
cd /work/repository
git init -q
git config user.name "Proqi Linux CI"
git config user.email "ci@proqi.invalid"
git add --all
git commit -qm baseline
printf '/target\n' >> .git/info/exclude

use_target() {
  rm -f "$CARGO_TARGET_DIR"
  mkdir -p "/cache/target/$1"
  ln -s "/cache/target/$1" "$CARGO_TARGET_DIR"
}

use_target stable

cargo xtask check
cargo xtask clean-worktree
cargo xtask test
cargo xtask clean-worktree
use_target msrv
cargo +1.88.0 xtask msrv
RUSTUP_TOOLCHAIN=1.88.0 cargo xtask crate-package
cargo xtask clean-worktree
cargo xtask audit
cargo xtask clean-worktree
use_target coverage
cargo xtask coverage
cargo xtask clean-worktree
use_target package
cargo xtask package
cargo xtask crate-package
cargo xtask clean-worktree
cargo xtask debian-package \
  target/package/proqi-x86_64-unknown-linux-gnu.tar.gz \
  target/debian-package
cargo xtask clean-worktree
