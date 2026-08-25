.PHONY: install 

install:
	cargo build --release
	sudo mv ./target/release/omv /usr/local/bin/omv

