PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin
SHAREDIR := $(PREFIX)/share/objj
BINARY := objjc
CRATEDIR := src/Compiler
FRAMEWORKS := src/Frameworks
RELEASE := $(CRATEDIR)/target/release/$(BINARY)

.PHONY: build install uninstall clean

build:
	cargo build --release --manifest-path $(CRATEDIR)/Cargo.toml

$(RELEASE): build

install: $(RELEASE)
	install -d "$(DESTDIR)$(BINDIR)"
	install -m 0755 "$(RELEASE)" "$(DESTDIR)$(BINDIR)/$(BINARY)"
	@echo "Installed $(BINARY) to $(DESTDIR)$(BINDIR)/$(BINARY)"
	install -d "$(DESTDIR)$(SHAREDIR)"
	rm -rf "$(DESTDIR)$(SHAREDIR)/Frameworks"
	cp -R "$(FRAMEWORKS)" "$(DESTDIR)$(SHAREDIR)/Frameworks"
	@echo "Installed Frameworks to $(DESTDIR)$(SHAREDIR)/Frameworks"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/$(BINARY)"
	rm -rf "$(DESTDIR)$(SHAREDIR)/Frameworks"
	@echo "Removed $(DESTDIR)$(BINDIR)/$(BINARY)"

clean:
	cargo clean --manifest-path $(CRATEDIR)/Cargo.toml
