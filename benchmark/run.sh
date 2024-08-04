#!/usr/bin/bash
set -euo pipefail

# adapted from https://stackoverflow.com/a/246128/65678
SOURCE="${BASH_SOURCE[0]}"
while [[ -h "$SOURCE" ]]; do # resolve $SOURCE until the file is no longer a symlink
	DIR="$(cd -P "$( dirname "$SOURCE" )" && pwd)"
	SOURCE="$(readlink "$SOURCE")"
	[[ $SOURCE != /* ]] && SOURCE="$DIR/$SOURCE" # if $SOURCE was a relative symlink, we need to resolve it relative to the path where the symlink file was located
done
DIR="$(cd -P "$(dirname "$SOURCE")" && pwd)"
cd "$DIR"

rm -f bin/{emma-clean,emma-tls-clean}
if git status --porcelain | grep '^ M src' ; then
	git stash push ../src

	pushd harness
	cargo build --release --features=emma
	popd

	mv harness/target/release/harness bin/emma-clean
	command time -v bin/emma-clean

	pushd harness
	cargo build --release --features=emma-tls
	popd

	mv harness/target/release/harness bin/emma-tls-clean
	command time -v bin/emma-tls-clean

	git stash pop
fi

for alloc in emma emma-tls std jemalloc mimalloc ; do
	pushd harness
	cargo build --release --features=$alloc
	popd
	rm -f bin/$alloc
	mv harness/target/release/harness bin/$alloc
	command time -v bin/$alloc
done

hyperfine --warmup 3 --shell=none "$@" bin/*
