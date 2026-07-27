.PHONY: fmt build-loader build-core build-user-shell verify-user-shell image iso test-boot clean

fmt:
	cargo fmt --check

build-loader:
	cargo build -p pythos-boot --target x86_64-unknown-uefi

build-core:
	cargo build -p pythos-core --target x86_64-unknown-none --features verify

build-user-shell:
	python scripts/build-user-shell.py

verify-user-shell: build-user-shell
	python scripts/verify-user-elf.py

image: build-loader build-core verify-user-shell
	python scripts/build-image.py

iso: build-loader build-core verify-user-shell
	python scripts/build-iso.py

test-boot: verify-user-shell
	python scripts/test-boot.py --slice exit-boot-services-ok

clean:
	cargo clean
