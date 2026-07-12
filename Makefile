.PHONY: fmt build-loader image test-boot clean

fmt:
	cargo fmt --check

build-loader:
	cargo build -p pythos-boot --target x86_64-unknown-uefi

image: build-loader
	python scripts/build-image.py

test-boot:
	python scripts/test-boot.py --slice exit-boot-services-ok

clean:
	cargo clean
