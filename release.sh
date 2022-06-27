#!/usr/bin/env bash

set -e

DIR=`dirname $0`

VERSION=`$DIR/scripts/version.sh`

rm -rf release

# Linux 64

mkdir -p release/virterm-$VERSION-linux64

TARGET_CC=x86_64-linux-musl-gcc \
RUSTFLAGS="-C linker=x86_64-linux-musl-gcc" \
cargo build --release --target=x86_64-unknown-linux-musl

cp target/x86_64-unknown-linux-musl/release/virterm \
  release/virterm-$VERSION-linux64/virterm

upx-head --brute release/virterm-$VERSION-linux64/virterm

tar -czvf release/virterm-$VERSION-linux64.tar.gz \
  -C release/virterm-$VERSION-linux64 \
  virterm

# Macos

mkdir -p release/virterm-$VERSION-macos64

cargo build --release --target=x86_64-apple-darwin

cp target/x86_64-apple-darwin/release/virterm \
  release/virterm-$VERSION-macos64/virterm

upx --brute release/virterm-$VERSION-macos64/virterm

tar -czvf release/virterm-$VERSION-macos64.tar.gz \
  -C release/virterm-$VERSION-macos64 \
  virterm
