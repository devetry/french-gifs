#!/bin/bash

cross build --target armv7-unknown-linux-gnueabihf --release

rm -rf binary
mkdir -p binary

cp -r binaries/ binary/binaries
cp ./target/armv7-unknown-linux-gnueabihf/release/tt binary/tt

zip -r binary.zip binary
