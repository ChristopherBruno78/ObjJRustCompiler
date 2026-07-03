PREFIX ?= /usr/local
BINDIR := $(PREFIX)/bin
BINARY := objjc
RELEASE := target/release/$(BINARY)

.PHONY: build install uninstall clean

build:
	cargo build --release

$(RELEASE): build

install: $(RELEASE)
	install -d "$(DESTDIR)$(BINDIR)"
	install -m 0755 "$(RELEASE)" "$(DESTDIR)$(BINDIR)/$(BINARY)"
	@echo "Installed $(BINARY) to $(DESTDIR)$(BINDIR)/$(BINARY)"

uninstall:
	rm -f "$(DESTDIR)$(BINDIR)/$(BINARY)"
	@echo "Removed $(DESTDIR)$(BINDIR)/$(BINARY)"

clean:
	cargo clean
